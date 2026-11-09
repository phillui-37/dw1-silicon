//! The window host: the game's display shown through `render` (wgpu on Metal), paced to the
//! PS1's field rate, with the keyboard and gamepads as the pad. SDL3 keeps the window, input and
//! sound.
//!
//! Keys (physical positions, so other layouts work) are configurable in host settings. Defaults:
//! arrows = D-pad, Z/X/A/S = Cross/Circle/Square/Triangle, Enter = Start, right Shift = Select,
//! Q/W = L1/R1, 1/2 = L2/R2.
//! Gamepads use SDL's standard layout (south = Cross, east = Circle, west = Square,
//! north = Triangle), the D-pad or left stick for directions, the triggers for L2/R2.
//! Closing the window or Cmd-Q exits 0. `DW1_WINDOW_LOG=1` logs frames presented and the
//! display size (first frame, on size changes, then every 60 frames).
//!
//! Host settings: F1 opens the volume, 4:3 window/display mode, keyboard/gamepad choice and
//! configurable bindings menu. Settings persist between runs. F2 render scale (auto, 1, 2, 3, 4,
//! 6, 8), F3 no wobble, F4 fit, F5 full colour and F11 fullscreen remain available in play.
//! F12 writes a bug snapshot (`snapshot.rs`).
//!
//! Before the game starts, `Window::splash` shows a short text screen (about 2.8 s; any key,
//! button or click skips it) saying this is the native Mac build. `DW1_NO_SPLASH=1` skips it;
//! `DW1_SPLASH_SHOT=<file.png>` saves it as a picture. Headless and replay runs never show it.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use render::{Fit, Picture, Renderer};
use sdl3::pixels::PixelFormat;
use sdl3::render::Canvas;
use sdl3::surface::Surface;

use crate::settings::{InputMode, Scale, Settings, WindowMode};
use sdl3::EventPump;
use sdl3::audio::{AudioFormat, AudioSpec, AudioStreamOwner};
use sdl3::event::Event;
use sdl3::gamepad::{Axis, Button, Gamepad};
use sdl3::keyboard::{Mod, Scancode};

mod disc_picker;
mod snapshot;
mod splash;
mod settings_ui;

/// The PS1's NTSC non-interlaced field rate: 53.693175 MHz / (3413 x 263) = 59.817 Hz
/// [doc: psx-spx GPU timings; PLAN.md's "Timing is measured", docs/s08-oracle.md]. Redux, the
/// oracle, counts at 60.009 Hz, which is a quirk of its counter and not what to match.
/// In thousandths of a hertz, so the arithmetic stays integer.
pub const FIELD_MILLIHZ: u64 = 59_817;

/// The rate the SPU's samples are played at: what libspu renders in a VBlank (the emulator's
/// 564,398 CPU cycles at 768 a sample, 734.89 samples) times the field rate, so the sound keeps
/// pace with the picture. That's 0.3% under the SPU's 44.1 kHz (the oracle's VBlank is a little
/// shorter than the PS1's); SDL resamples to the device, 5 cents flat.
pub const AUDIO_HZ: i32 = ((psyq::libspu::CYCLES_PER_VBLANK * FIELD_MILLIHZ)
    / (psyq::libspu::CYCLES_PER_SAMPLE * 1000)) as i32;
/// Most audio queued before the queue is dropped (a quarter second): after a stall the sound
/// catches up instead of lagging behind the picture for good.
const AUDIO_MAX_QUEUED_BYTES: i32 = AUDIO_HZ * 4 / 4;

/// How many fields behind the clock may fall before it resyncs instead of racing to catch up.
pub const MAX_LAG_FIELDS: u64 = 4;

/// Fixed-step field clock. Field `n` is due at `base + n / FIELD_HZ` (computed from the count,
/// not accumulated, so there is no drift). Times are nanoseconds on any monotonic origin.
pub struct Pacer {
    base_ns: u64,
    fields: u64,
}

impl Pacer {
    pub fn new(now_ns: u64) -> Pacer {
        Pacer {
            base_ns: now_ns,
            fields: 0,
        }
    }

    fn due_ns(&self, fields: u64) -> u64 {
        self.base_ns + (fields as u128 * 1_000_000_000_000 / FIELD_MILLIHZ as u128) as u64
    }

    /// A field is finished at `now_ns`: how long to wait until the next one is due. If the
    /// host is more than `MAX_LAG_FIELDS` late, restarts the clock from now (no wait).
    pub fn next(&mut self, now_ns: u64) -> u64 {
        self.fields += 1;
        let due = self.due_ns(self.fields);
        let late_by = now_ns.saturating_sub(due);
        if late_by > self.due_ns(MAX_LAG_FIELDS) - self.due_ns(0) {
            *self = Pacer::new(now_ns);
            return 0;
        }
        due.saturating_sub(now_ns)
    }
}

// libetc's PadRead bits (libetc.h).
const L2: u32 = 1 << 0;
const R2: u32 = 1 << 1;
const L1: u32 = 1 << 2;
const R1: u32 = 1 << 3;
const TRIANGLE: u32 = 1 << 4;
const CIRCLE: u32 = 1 << 5;
const CROSS: u32 = 1 << 6;
const SQUARE: u32 = 1 << 7;
const SELECT: u32 = 1 << 8;
const START: u32 = 1 << 11;
const UP: u32 = 1 << 12;
const RIGHT: u32 = 1 << 13;
const DOWN: u32 = 1 << 14;
const LEFT: u32 = 1 << 15;
const PAD_ACTIONS: [u32; 14] = [CROSS, CIRCLE, SQUARE, TRIANGLE, START, SELECT, L1, R1, L2, R2, UP, RIGHT, DOWN, LEFT];

#[cfg(test)]
pub fn key_bit(key: Scancode) -> u32 {
    match key {
        Scancode::Up => UP,
        Scancode::Down => DOWN,
        Scancode::Left => LEFT,
        Scancode::Right => RIGHT,
        Scancode::Z => CROSS,
        Scancode::X => CIRCLE,
        Scancode::A => SQUARE,
        Scancode::S => TRIANGLE,
        Scancode::Return | Scancode::KpEnter => START,
        Scancode::RShift => SELECT,
        Scancode::Q => L1,
        Scancode::W => R1,
        Scancode::_1 => L2,
        Scancode::_2 => R2,
        _ => 0,
    }
}

fn binding_bits(bindings: &[i32; 16], matches: impl Fn(i32) -> bool) -> u32 {
    bindings.iter().take(PAD_ACTIONS.len()).enumerate().fold(0, |bits, (index, &code)| {
        if code != 0 && matches(code) { bits | PAD_ACTIONS[index] } else { bits }
    })
}

fn axis_binding_bits(axis: Axis, value: i16, bindings: &[i32; 16]) -> u32 {
    let index = axis as i32;
    binding_bits(bindings, |code| {
        let custom_axis = code == 100 + index * 2 && value < -AXIS_THRESHOLD
            || code == 101 + index * 2 && value > AXIS_THRESHOLD;
        let default_stick = match (axis, value < -AXIS_THRESHOLD, value > AXIS_THRESHOLD) {
            (Axis::LeftX, true, _) => code == Button::DPadLeft as i32 + 1,
            (Axis::LeftX, _, true) => code == Button::DPadRight as i32 + 1,
            (Axis::LeftY, true, _) => code == Button::DPadUp as i32 + 1,
            (Axis::LeftY, _, true) => code == Button::DPadDown as i32 + 1,
            _ => false,
        };
        custom_axis || default_stick
    })
}

fn key_binding_bits(key: Scancode, bindings: &[i32; 16]) -> u32 {
    binding_bits(bindings, |code| code == key as i32)
}

#[cfg(test)]
pub fn button_bit(button: Button) -> u32 {
    match button {
        Button::DPadUp => UP,
        Button::DPadDown => DOWN,
        Button::DPadLeft => LEFT,
        Button::DPadRight => RIGHT,
        Button::South => CROSS,
        Button::East => CIRCLE,
        Button::West => SQUARE,
        Button::North => TRIANGLE,
        Button::Start => START,
        Button::Back => SELECT,
        Button::LeftShoulder => L1,
        Button::RightShoulder => R1,
        _ => 0,
    }
}

const AXIS_THRESHOLD: i16 = 16_000;

/// Bits an axis contributes at `value` (both directions of a stick axis are reported, so the
/// result is the whole set for that axis).
#[cfg(test)]
pub fn axis_bits(axis: Axis, value: i16) -> u32 {
    let hi = value > AXIS_THRESHOLD;
    let lo = value < -AXIS_THRESHOLD;
    match axis {
        Axis::LeftX => (if lo { LEFT } else { 0 }) | (if hi { RIGHT } else { 0 }),
        Axis::LeftY => (if lo { UP } else { 0 }) | (if hi { DOWN } else { 0 }),
        Axis::TriggerLeft if hi => L2,
        Axis::TriggerRight if hi => R2,
        _ => 0,
    }
}

/// The destination rectangle `(x, y, w, h)` for a `src_w x src_h` picture in a
/// `win_w x win_h` window: the largest whole-number scale (at least 1 when it fits; when the
/// window is smaller than the picture, the fit scale, so it is never cropped), centred.
pub fn letterbox(src_w: u32, src_h: u32, win_w: u32, win_h: u32) -> (f32, f32, f32, f32) {
    let (sw, sh) = (src_w.max(1) as f32, src_h.max(1) as f32);
    let (ww, wh) = (win_w as f32, win_h as f32);
    let fit = (ww / sw).min(wh / sh);
    let scale = if fit >= 1.0 { fit.floor() } else { fit };
    let (w, h) = (sw * scale, sh * scale);
    (((ww - w) / 2.0).floor(), ((wh - h) / 2.0).floor(), w, h)
}

/// One gamepad's held inputs: buttons and each axis kept apart, so a stick returning to centre
/// doesn't release a D-pad direction (or the other way round).
#[derive(Default)]
pub struct PadState {
    buttons: u32,
    axes: BTreeMap<i32, u32>,
}

impl PadState {
    pub fn button(&mut self, button: Button, down: bool, bindings: &[i32; 16]) {
        let code = button as i32 + 1;
        let mapped = binding_bits(bindings, |binding| binding == code);
        if down {
            self.buttons |= mapped;
        } else {
            self.buttons &= !mapped;
        }
    }

    pub fn axis(&mut self, axis: Axis, value: i16, bindings: &[i32; 16]) {
        self.axes.insert(axis as i32, axis_binding_bits(axis, value, bindings));
    }

    pub fn bits(&self) -> u32 {
        self.axes.values().fold(self.buttons, |a, b| a | b)
    }
}

pub struct Window {
    /// Declared before `window`, so it goes first (it draws into the window's Metal layer).
    renderer: Renderer,
    window: sdl3::video::Window,
    /// The settings in use, and as kept in the file (without one run's environment overrides).
    settings: Settings,
    kept: Settings,
    settings_path: Option<std::path::PathBuf>,
    /// What the corner note says and until when, and a message note's text.
    note: Option<(Note, Instant)>,
    message: String,
    /// The last minute of pad input, for bug snapshots.
    pad_log: snapshot::PadLog,
    note_buf: Vec<u32>,
    pump: EventPump,
    gamepad_sys: sdl3::GamepadSubsystem,
    gamepads: Vec<Gamepad>,
    keys: u32,
    /// Per gamepad, by joystick id.
    pads: BTreeMap<sdl3::joystick::JoystickId, PadState>,
    buf: Vec<u32>,
    pacer: Option<Pacer>,
    t0: Instant,
    frames: u64,
    last_size: Option<(usize, usize)>,
    log: bool,
    /// Pad bits still held from before the game took over (the splash's skip press): not
    /// reported until released.
    held_over: u32,
    /// The sound output (kept open; libspu pushes into it). None if there's no audio device.
    _audio: Option<AudioStreamOwner>,
    /// The title says a save failed.
    save_failed_shown: bool,
    /// The display's refresh period (0 if unknown) and when a frame was last shown: on a
    /// display slower than the field rate, frames are dropped rather than waited for.
    refresh_ns: u64,
    gate: ShowGate,
}

/// The display's refresh period in nanoseconds, 0 if SDL doesn't know it.
fn refresh_ns(window: &sdl3::video::Window) -> u64 {
    match window.get_display().and_then(|d| d.get_mode()) {
        Ok(m) if m.refresh_rate > 1.0 => (1e9 / m.refresh_rate as f64) as u64,
        _ => 0,
    }
}

/// Lets frames through no faster than the display refreshes, so the window never waits on the
/// display and the game keeps the field rate. On a display at 60 Hz or faster (or of unknown
/// rate) every field is shown; at 30 Hz every other one.
#[derive(Default)]
pub struct ShowGate {
    /// When the next frame may be shown, in nanoseconds on the caller's clock.
    next_ns: Option<u64>,
}

/// Timer jitter allowed: a frame may come this much before its slot. The slots themselves
/// advance by a whole refresh per frame shown, so over time no more frames are shown than the
/// display refreshes.
const SHOW_SLACK_NS: u64 = 1_000_000;

impl ShowGate {
    /// Whether a frame at `now_ns` may be shown on a display refreshing every `refresh_ns`
    /// (0 = unknown: always).
    pub fn admit(&mut self, now_ns: u64, refresh_ns: u64) -> bool {
        if refresh_ns == 0 {
            self.next_ns = None;
            return true;
        }
        match self.next_ns {
            Some(next) if now_ns + SHOW_SLACK_NS < next => false,
            // Behind by more than a refresh (a stall, or the first frame): start again from now.
            next => {
                let slot = next.filter(|&n| now_ns < n + refresh_ns).unwrap_or(now_ns);
                self.next_ns = Some(slot + refresh_ns);
                true
            }
        }
    }
}

/// How long a note stays in the corner.
const NOTE_TIME: Duration = Duration::from_millis(2500);

/// What a hotkey changed.
#[derive(Clone, Copy)]
enum Note {
    Scale,
    Wobble,
    FullColour,
    Fit,
    Fullscreen,
    /// `Window::message`.
    Message,
}

impl Note {
    /// The note's line, with whether the setting is the original or an addition. `scale`: the
    /// render scale in use.
    fn text(self, s: &Settings, scale: u32) -> String {
        let label = |original: bool| if original { "original" } else { "addition" };
        match self {
            Note::Scale => {
                let auto = if s.scale == Scale::Auto {
                    " (auto)"
                } else {
                    ""
                };
                format!("Render scale {scale}x{auto} - {}", label(scale == 1))
            }
            Note::Wobble if s.no_wobble && scale > 1 => "No wobble: on - addition".into(),
            Note::Wobble if s.no_wobble => "No wobble: on (needs render scale 2x+)".into(),
            Note::Wobble => "No wobble: off - original".into(),
            Note::FullColour if s.full_colour && scale > 1 => "Full colour: on - addition".into(),
            Note::FullColour if s.full_colour => "Full colour: on (needs render scale 2x+)".into(),
            Note::FullColour => "Full colour: off - original".into(),
            Note::Fit if s.fit == Fit::Fill => "Fit: fill window - addition".into(),
            Note::Fit => "Fit: whole multiples - original".into(),
            Note::Fullscreen if s.fullscreen => "Fullscreen (F11 for a window)".into(),
            Note::Fullscreen => "Window (F11 for fullscreen)".into(),
            Note::Message => String::new(),
        }
    }
}

/// Draws `text` in SDL's 8x8 debug font at `k` window pixels per font pixel, light on dark
/// with a margin, into `out`; returns its size.
fn draw_note(text: &str, k: u32, out: &mut Vec<u32>) -> Result<(u32, u32), String> {
    let pad = 4 * k;
    let (w, h) = (text.len() as u32 * 8 * k + 2 * pad, 8 * k + 2 * pad);
    let surface = Surface::new(w, h, PixelFormat::XRGB8888).map_err(|e| e.to_string())?;
    let mut canvas: Canvas<Surface> = surface.into_canvas().map_err(|e| e.to_string())?;
    canvas.set_draw_color((16, 18, 26));
    canvas.clear();
    canvas.set_draw_color((235, 238, 245));
    canvas.set_scale(k as f32, k as f32).ok();
    canvas
        .draw_debug_text(text, ((pad / k) as f32, (pad / k) as f32))
        .map_err(|e| e.to_string())?;
    canvas.present();
    let surface = canvas.into_surface();
    let pitch = surface.pitch() as usize;
    out.clear();
    surface.with_lock(|px| {
        for row in 0..h as usize {
            out.extend(
                px[row * pitch..][..w as usize * 4]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|&b| u32::from_ne_bytes(b) & 0x00ff_ffff),
            );
        }
    });
    Ok((w, h))
}

/// The audio stream as libspu's sink holds it. SDL's stream calls are thread-safe [doc: SDL3
/// SDL_PutAudioStreamData], and the sink only runs on the game's (main) thread anyway.
struct SinkStream(*mut sdl3::sys::audio::SDL_AudioStream);
// SAFETY: see above.
unsafe impl Send for SinkStream {}

/// Opens the default playback device and points libspu's samples at it. DW1_MUTE=1 skips it.
fn open_audio(sdl: &sdl3::Sdl) -> Option<AudioStreamOwner> {
    if std::env::var("DW1_MUTE").is_ok_and(|v| !v.is_empty() && v != "0") {
        return None;
    }
    let spec = AudioSpec {
        freq: Some(AUDIO_HZ),
        channels: Some(2),
        format: Some(AudioFormat::s16_sys()),
    };
    let stream = match sdl
        .audio()
        .and_then(|a| a.default_playback_device().open_device_stream(Some(&spec)))
    {
        Ok(s) => s,
        Err(e) => {
            eprintln!("dw1: no sound: {e}");
            return None;
        }
    };
    if let Err(e) = stream.resume() {
        eprintln!("dw1: no sound: {e}");
        return None;
    }
    let sink = SinkStream(stream.raw());
    psyq::libspu::set_audio_sink(Box::new(move |samples: &[i16]| {
        let sink = &sink;
        // SAFETY: the window owns the stream and takes this sink away before closing it
        // (`Drop for Window`).
        unsafe {
            // Behind by more than the limit (a stall): drop what's queued and start again
            // from now, rather than play stale sound late for good.
            if sdl3::sys::audio::SDL_GetAudioStreamQueued(sink.0) > AUDIO_MAX_QUEUED_BYTES {
                sdl3::sys::audio::SDL_ClearAudioStream(sink.0);
            }
            sdl3::sys::audio::SDL_PutAudioStreamData(
                sink.0,
                samples.as_ptr().cast(),
                (samples.len() * 2) as i32,
            );
        }
    }));
    Some(stream)
}

impl Drop for Window {
    /// The audio stream goes with the window: take libspu's sink (which holds its pointer)
    /// away first.
    fn drop(&mut self) {
        psyq::libspu::clear_audio_sink();
    }
}

// SAFETY: SDL objects must stay on the main thread, and the host is only ever called from the
// game's thread, which is main (`dw1_game_main` is called from `main`). `Host: Send` is
// required only because the clock lives in a static.
unsafe impl Send for Window {}

impl Window {
    pub fn new() -> Result<Window, String> {
        let sdl = sdl3::init().map_err(|e| e.to_string())?;
        let video = sdl.video().map_err(|e| e.to_string())?;
        let window = video
            .window("Digimon World (dw1-silicon)", 960, 720)
            .resizable()
            .high_pixel_density()
            .metal_view()
            .build()
            .map_err(|e| e.to_string())?;
        let (pw, ph) = window.size_in_pixels();
        // SDL expects min/max WIDTH ? HEIGHT. Passing width and height here (4, 3)
        // accidentally clamps the window to 4:1.
        unsafe {
            sdl3::sys::video::SDL_SetWindowAspectRatio(window.raw(), 4.0 / 3.0, 4.0 / 3.0);
        }
        // SAFETY: the renderer is dropped before the window (field order in `Window`).
        let renderer = unsafe { Renderer::for_window(&window, pw, ph)? };
        let settings_path = Settings::path();
        let (kept, settings) = Settings::load(settings_path.as_deref());
        let gamepad_sys = sdl.gamepad().map_err(|e| e.to_string())?;
        let pump = sdl.event_pump().map_err(|e| e.to_string())?;
        let audio = open_audio(&sdl);
        let mut w = Window {
            renderer,
            window,
            settings,
            kept,
            settings_path,
            note: None,
            message: String::new(),
            pad_log: snapshot::PadLog::default(),
            note_buf: Vec::new(),
            pump,
            gamepad_sys,
            gamepads: Vec::new(),
            keys: 0,
            pads: BTreeMap::new(),
            buf: vec![0; 640 * 480],
            pacer: None,
            t0: Instant::now(),
            frames: 0,
            last_size: None,
            log: std::env::var_os("DW1_WINDOW_LOG").is_some(),
            held_over: 0,
            _audio: audio,
            save_failed_shown: false,
            refresh_ns: 0,
            gate: ShowGate::default(),
        };
        w.refresh_ns = refresh_ns(&w.window);
        w.apply_window_settings();
        w.apply_audio_settings();
        if w.settings.fullscreen {
            w.set_fullscreen(true);
        }
        for id in w.gamepad_sys.gamepads().unwrap_or_default() {
            w.open_gamepad(id);
        }
        Ok(w)
    }

    fn apply_window_settings(&mut self) {
        let sizes = [(640, 480), (800, 600), (960, 720), (1280, 960), (1600, 1200)];
        let (sw, sh) = sizes[self.settings.window_size.min(4) as usize];
        let _ = self.window.set_minimum_size(320, 240);
        match self.settings.window_mode {
            WindowMode::Windowed => {
                let _ = self.window.set_fullscreen(false);
                let _ = self.window.set_bordered(true);
                let _ = self.window.restore();
                let _ = self.window.set_size(sw, sh);
                unsafe { sdl3::sys::video::SDL_SetWindowAspectRatio(self.window.raw(), 4.0 / 3.0, 4.0 / 3.0); }
            }
            WindowMode::Borderless => {
                let _ = self.window.set_fullscreen(false);
                let _ = self.window.set_bordered(false);
                unsafe { sdl3::sys::video::SDL_SetWindowAspectRatio(self.window.raw(), 0.0, 0.0); }
                let _ = self.window.maximize();
            }
            WindowMode::Exclusive => {
                let _ = self.window.set_bordered(true);
                unsafe {
                    sdl3::sys::video::SDL_SetWindowAspectRatio(self.window.raw(), 0.0, 0.0);
                    // Prefer a real 4:3 exclusive mode on displays that expose one.
                    if let Ok(display) = self.window.get_display() {
                        if let Ok(modes) = display.get_fullscreen_modes() {
                            if let Some(mode) = modes.into_iter()
                                .filter(|m| m.w > 0 && m.h > 0 && (m.w as f32 / m.h as f32 - 4.0 / 3.0).abs() < 0.01)
                                .min_by_key(|m| (m.w - 960).abs() + (m.h - 720).abs())
                            {
                                let raw = mode.to_ll();
                                let _ = sdl3::sys::video::SDL_SetWindowFullscreenMode(self.window.raw(), &raw);
                            } else {
                                let _ = sdl3::sys::video::SDL_SetWindowFullscreenMode(self.window.raw(), std::ptr::null());
                            }
                        }
                    }
                }
                self.set_fullscreen(true);
            }
        }
    }

    fn apply_audio_settings(&self) {
        if let Some(stream) = &self._audio {
            unsafe { sdl3::sys::audio::SDL_SetAudioStreamGain(stream.raw(), self.settings.volume as f32 / 100.0); }
        }
    }

    pub fn settings_screen(&mut self) { self.settings_ui(); }

    fn open_gamepad(&mut self, id: sdl3::joystick::JoystickId) {
        if let Ok(g) = self.gamepad_sys.open(id) {
            self.gamepads.push(g);
        }
    }

    fn events(&mut self) {
        let events: Vec<Event> = self.pump.poll_iter().collect();
        for ev in events {
            self.handle(ev);
        }
    }

    fn handle(&mut self, ev: Event) {
        match ev {
            Event::Quit { .. } => {
                if self.log {
                    eprintln!("dw1: window closed after {} frames", self.frames);
                }
                crate::applog::clean_exit();
                std::process::exit(0);
            }
            Event::KeyDown {
                scancode: Some(k),
                keymod,
                repeat: false,
                ..
            } => {
                let cmd_ctrl = keymod.intersects(Mod::LGUIMOD | Mod::RGUIMOD)
                    && keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD);
                match k {
                    Scancode::F1 => self.settings_ui(),
                    Scancode::F2 => self.change(Note::Scale, |s| s.next_scale()),
                    Scancode::F3 => self.change(Note::Wobble, |s| s.no_wobble = !s.no_wobble),
                    Scancode::F5 => {
                        self.change(Note::FullColour, |s| s.full_colour = !s.full_colour)
                    }
                    Scancode::F4 => self.change(Note::Fit, |s| {
                        s.fit = if s.fit == Fit::Fill {
                            Fit::Integer
                        } else {
                            Fit::Fill
                        }
                    }),
                    Scancode::F11 => self.toggle_fullscreen(),
                    Scancode::F12 => self.bug_snapshot(),
                    Scancode::F if cmd_ctrl => self.toggle_fullscreen(),
                    _ => {
                        if self.settings.input_mode != InputMode::Gamepad {
                            self.keys |= key_binding_bits(k, &self.settings.keyboard_bindings);
                        }
                    }
                }
            }
            Event::KeyUp { scancode: Some(k), .. } => {
                self.keys &= !key_binding_bits(k, &self.settings.keyboard_bindings);
            }
            Event::Window {
                win_event: sdl3::event::WindowEvent::DisplayChanged(_),
                ..
            } => self.refresh_ns = refresh_ns(&self.window),
            // A display's mode changed (its refresh rate, say): ours may be the one.
            Event::Display {
                display_event:
                    sdl3::event::DisplayEvent::CurrentModeChanged
                    | sdl3::event::DisplayEvent::DesktopModeChanged,
                ..
            } => self.refresh_ns = refresh_ns(&self.window),
            Event::GamepadAdded { which, .. } => self.open_gamepad(which),
            Event::GamepadRemoved { which, .. } => {
                self.gamepads.retain(|g| g.id().ok() != Some(which));
                self.pads.remove(&which);
            }
            Event::GamepadButtonDown { which, button, .. } => {
                self.pads.entry(which).or_default().button(button, true, &self.settings.gamepad_bindings)
            }
            Event::GamepadButtonUp { which, button, .. } => {
                self.pads.entry(which).or_default().button(button, false, &self.settings.gamepad_bindings)
            }
            Event::GamepadAxisMotion { which, axis, value, .. } => {
                // Keep physical state fresh even in keyboard-only mode, so switching devices
                // while a stick is held cannot reintroduce a stale direction.
                self.pads.entry(which).or_default().axis(axis, value, &self.settings.gamepad_bindings)
            }
            _ => {}
        }
    }

    fn held_pad(&self) -> u32 {
        let keyboard = if self.settings.input_mode == InputMode::Gamepad { 0 } else { self.keys };
        if self.settings.input_mode == InputMode::Keyboard { keyboard }
        else { self.pads.values().fold(keyboard, |a, p| a | p.bits()) }
    }

    /// A hotkey changed a setting: keep it (that setting only: the others keep what the file
    /// says, whatever this run's environment overrides) and say so in the corner.
    fn change(&mut self, note: Note, f: impl FnOnce(&mut Settings)) {
        f(&mut self.settings);
        let (s, k) = (&self.settings, &mut self.kept);
        match note {
            Note::Scale => k.scale = s.scale,
            Note::Wobble => k.no_wobble = s.no_wobble,
            Note::FullColour => k.full_colour = s.full_colour,
            Note::Fit => k.fit = s.fit,
            Note::Fullscreen => { k.fullscreen = s.fullscreen; k.window_mode = s.window_mode; },
            Note::Message => {}
        }
        self.kept.save(self.settings_path.as_deref());
        self.note = Some((note, Instant::now() + NOTE_TIME));
    }

    /// Shows `text` in the corner for a while.
    fn show_message(&mut self, text: String) {
        self.message = text;
        self.note = Some((Note::Message, Instant::now() + NOTE_TIME));
    }

    fn toggle_fullscreen(&mut self) {
        let on = !matches!(
            self.window.fullscreen_state(),
            sdl3::video::FullscreenType::True | sdl3::video::FullscreenType::Desktop
        );
        self.set_fullscreen(on);
        self.settings.fullscreen = on;
        self.settings.window_mode = if on { WindowMode::Exclusive } else { WindowMode::Windowed };
        self.change(Note::Fullscreen, |_| {});
    }

    /// Fullscreen on or off. SDL changes it asynchronously on macOS (an animation); waiting for
    /// it to finish means a second press always sees where the first one ended. The field clock
    /// resyncs after the wait.
    fn set_fullscreen(&mut self, on: bool) {
        if let Err(e) = self.window.set_fullscreen(on) {
            eprintln!("dw1: fullscreen: {e}");
        }
        self.window.sync();
        self.refresh_ns = refresh_ns(&self.window);
    }

    /// Sets the GPU's render scale (and no wobble, full colour) for a window `pw x ph` pixels: changes only
    /// when the settings or the window's size call for another scale.
    fn apply_scale(&self, pw: u32, ph: u32) -> u32 {
        let (_, _, _, shown_h) = render::place(self.settings.fit, 320, 240, pw, ph);
        let scale = self.settings.scale_for(shown_h);
        let wobble_off = scale > 1 && self.settings.no_wobble;
        psyq::gpu::with(|g| {
            if g.scale() != scale {
                g.set_scale(scale);
            }
            if g.full_colour() != self.settings.full_colour {
                g.set_full_colour(self.settings.full_colour);
            }
        });
        if backend::pgxp::enabled() != wobble_off {
            backend::pgxp::set_enabled(wobble_off);
        }
        scale
    }

    /// Shows `picture` (or black) and keeps the renderer at the window's pixel size. Returns the
    /// window's size in pixels. (Takes the fields, so a picture may borrow another field.)
    fn show(
        renderer: &mut Renderer,
        window: &sdl3::video::Window,
        picture: Option<Picture>,
        fit: Fit,
        note: Option<(Picture, u32, u32)>,
    ) -> (u32, u32) {
        let (pw, ph) = window.size_in_pixels();

        renderer.resize(pw, ph);
        renderer.present(picture, fit, note);
        (pw, ph)
    }

    fn present(&mut self) {
        if !self
            .gate
            .admit(self.t0.elapsed().as_nanos() as u64, self.refresh_ns)
        {
            return;
        }
        let (pw, ph) = self.renderer.drawable_size(
            self.window.size_in_pixels().0,
            self.window.size_in_pixels().1,
        );
        let scale = self.apply_scale(pw, ph);
        // At a render scale above 1, the HD surface's picture.
        let shown = psyq::gpu::with(|g| {
            if g.scale() > 1 {
                g.read_display_hd(&mut self.buf)
            } else {
                g.read_display(&mut self.buf)
            }
        });
        let picture = shown.map(|(w, h)| Picture {
            pixels: &self.buf[..w * h],
            w: w as u32,
            h: h as u32,
        });
        let note = match self.note {
            Some((n, until)) if Instant::now() < until => {
                let text = match n {
                    Note::Message => self.message.clone(),
                    n => n.text(&self.settings, scale),
                };
                // Glyphs of 8 window pixels at 360 lines, smaller if the line wouldn't fit across;
                // cut short if it doesn't at the smallest. A margin of one glyph.
                let width_for = |k: u32| (text.len() as u32 + 3) * 8 * k;
                let k = (1..=(ph / 360).max(1))
                    .rev()
                    .find(|&k| width_for(k) <= pw)
                    .unwrap_or(1);
                let fits = (pw / 8).saturating_sub(3) as usize;
                let text: String = text.chars().take(fits).collect();
                draw_note(&text, k, &mut self.note_buf)
                    .map(|(w, h)| (w, h, 8 * k, 8 * k))
                    .map_err(|e| eprintln!("dw1: note: {e}"))
                    .ok()
            }
            _ => {
                self.note = None;
                None
            }
        };
        let note = note.map(|(w, h, x, y)| {
            (
                Picture {
                    pixels: &self.note_buf,
                    w,
                    h,
                },
                x,
                y,
            )
        });
        let (pw, ph) = Self::show(
            &mut self.renderer,
            &self.window,
            picture,
            self.settings.fit,
            note,
        );
        self.frames += 1;
        if self.log
            && (self.frames == 1 || self.last_size != shown || self.frames.is_multiple_of(60))
        {
            match shown {
                Some((w, h)) => eprintln!(
                    "dw1: window: {} frames presented, display {w}x{h}, window {pw}x{ph} \
                     ({:?}, {} skipped)",
                    self.frames,
                    self.renderer.present_mode(),
                    self.renderer.skipped
                ),
                None => eprintln!("dw1: window: {} frames presented, display off", self.frames),
            }
        }
        self.last_size = shown;
    }

    /// A save the game thinks it made but that didn't reach the disk shows in the window's
    /// title until a later save succeeds.
    fn show_save_state(&mut self) {
        let failed = psyq::libmcrd::save_failed();
        if failed != self.save_failed_shown {
            self.save_failed_shown = failed;
            let title = if failed {
                "Digimon World (dw1-silicon) - SAVE NOT WRITTEN TO DISK (see the terminal)"
            } else {
                "Digimon World (dw1-silicon)"
            };
            let _ = self.window.set_title(title);
        }
    }

    fn wait_for_field(&mut self) {
        let now = self.t0.elapsed().as_nanos() as u64;
        let pacer = self.pacer.get_or_insert_with(|| Pacer::new(now));
        let wait = pacer.next(now);
        if wait == 0 {
            return;
        }
        // Sleep most of it, spin the last millisecond: the OS sleep overshoots.
        let end = self.t0 + Duration::from_nanos(now + wait);
        let spin = Duration::from_millis(1);
        if let Some(coarse) = end
            .checked_duration_since(Instant::now())
            .and_then(|d| d.checked_sub(spin))
        {
            std::thread::sleep(coarse);
        }
        while Instant::now() < end {
            std::hint::spin_loop();
        }
    }
}

impl psyq::libetc::Host for Window {
    fn vblank(&mut self, _vblank: u32) {
        self.events();
        self.wait_for_field();
        self.present();
        self.show_save_state();
    }

    fn pad(&mut self, _caller: usize) -> u32 {
        let held = self.held_pad();
        self.held_over &= held;
        let bits = held & !self.held_over;
        self.pad_log.push(psyq::libetc::vblank_count(), bits);
        bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_map_to_libetc_bits() {
        let all = [
            (Scancode::Return, 0x800), // PADstart, as main.c masks it
            (Scancode::Up, 1 << 12),
            (Scancode::Right, 1 << 13),
            (Scancode::Down, 1 << 14),
            (Scancode::Left, 1 << 15),
            (Scancode::Z, 0x40),
            (Scancode::X, 0x20),
            (Scancode::A, 0x80),
            (Scancode::S, 0x10),
            (Scancode::RShift, 0x100),
            (Scancode::Q, 0x4),
            (Scancode::W, 0x8),
            (Scancode::_1, 0x1),
            (Scancode::_2, 0x2),
        ];
        for (k, bit) in all {
            assert_eq!(key_bit(k), bit, "{k:?}");
        }
        assert_eq!(key_bit(Scancode::P), 0);
        // No two keys share a bit.
        let or = all.iter().fold(0, |m, (_, b)| m | b);
        assert_eq!(or.count_ones() as usize, all.len());
    }

    #[test]
    fn gamepad_maps() {
        assert_eq!(button_bit(Button::South), 0x40);
        assert_eq!(button_bit(Button::Start), 0x800);
        assert_eq!(axis_bits(Axis::LeftX, -20000), LEFT);
        assert_eq!(axis_bits(Axis::LeftY, 20000), DOWN);
        assert_eq!(axis_bits(Axis::LeftX, 100), 0);
        assert_eq!(axis_bits(Axis::TriggerRight, 30000), R2);

        // A centred stick doesn't release a held D-pad direction, nor the other way round.
        let bindings = Settings::default().gamepad_bindings;
        let mut p = PadState::default();
        p.button(Button::DPadLeft, true, &bindings);
        p.axis(Axis::LeftX, 100, &bindings);
        assert_eq!(p.bits(), LEFT);
        p.axis(Axis::LeftX, -20000, &bindings);
        p.button(Button::DPadLeft, false, &bindings);
        assert_eq!(p.bits(), LEFT);
        p.axis(Axis::LeftX, 0, &bindings);
        assert_eq!(p.bits(), 0);

        // Default face buttons and triggers keep their expected PS1 actions.
        assert_eq!(binding_bits(&bindings, |code| code == Button::South as i32 + 1), CROSS);
        assert_eq!(axis_binding_bits(Axis::TriggerLeft, 20_000, &bindings), L2);
        assert_eq!(axis_binding_bits(Axis::TriggerRight, 20_000, &bindings), R2);

        // A user remap is what PadRead receives, not the hard-coded face-button layout.
        let mut custom = bindings;
        custom[0] = Button::East as i32 + 1;
        let mut remapped = PadState::default();
        remapped.button(Button::East, true, &custom);
        assert_eq!(remapped.bits(), CROSS | CIRCLE);
        remapped.button(Button::East, false, &custom);
        assert_eq!(remapped.bits(), 0);
    }

    #[test]
    fn keyboard_bindings_are_configurable() {
        let defaults = Settings::default();
        assert_eq!(key_binding_bits(Scancode::Z, &defaults.keyboard_bindings), CROSS);
        let mut keys = defaults.keyboard_bindings;
        keys[0] = Scancode::P as i32;
        assert_eq!(key_binding_bits(Scancode::P, &keys), CROSS);
        assert_eq!(key_binding_bits(Scancode::Z, &keys), 0);
    }

    #[test]
    fn pacer_holds_the_field_rate_without_drift() {
        let mut p = Pacer::new(0);
        let mut now = 0;
        for _ in 0..5982 {
            now += p.next(now);
        }
        let expect = 5982u128 * 1_000_000_000_000 / FIELD_MILLIHZ as u128;
        assert_eq!(now as u128, expect);
        assert!((100_000_000_000..100_030_000_000).contains(&now));
    }

    #[test]
    fn pacer_resyncs_after_a_long_stall_and_tolerates_small_ones() {
        let mut p = Pacer::new(0);
        let field = p.next(0); // ~16.7 ms
        assert!((16_700_000..16_730_000).contains(&field));
        // 2 fields late: no resync, no wait (the next deadline is still on the old grid).
        assert_eq!(p.next(field + 2 * field), 0);
        // A 20-field stall: resync; the following wait is a whole field again.
        let now = 25 * field;
        assert_eq!(p.next(now), 0);
        let w = p.next(now);
        assert!((16_700_000..16_730_000).contains(&w));
    }

    /// Shows frames at the field rate (with up to 0.3 ms of jitter) for 10 s on a display
    /// at `hz`; returns how many were shown.
    fn shown_on(hz: f64) -> u64 {
        let field = 1_000_000_000_000 / FIELD_MILLIHZ;
        let refresh = if hz > 0.0 { (1e9 / hz) as u64 } else { 0 };
        let mut gate = ShowGate::default();
        let mut shown = 0;
        for n in 0..598u64 {
            let jitter = (n * 7919 % 7) * 50_000;
            if gate.admit(n * field + jitter, refresh) {
                shown += 1;
            }
        }
        shown
    }

    #[test]
    fn frames_drop_only_on_displays_slower_than_the_fields() {
        // 60 Hz and faster, 59.94 Hz, unknown: every field.
        for hz in [0.0, 59.94, 60.0, 75.0, 120.0, 240.0] {
            assert_eq!(shown_on(hz), 598, "{hz} Hz");
        }
        // Slower: never more than the display shows (10 s), and close to it.
        for hz in [58.0, 50.0, 30.0] {
            let n = shown_on(hz);
            let most = (hz * 10.0) as u64 + 1;
            assert!(n <= most && n + 2 >= most - 1, "{hz} Hz: {n}");
        }
    }

    #[test]
    fn letterbox_scales_by_whole_numbers_and_centres() {
        assert_eq!(letterbox(320, 240, 960, 720), (0.0, 0.0, 960.0, 720.0));
        assert_eq!(letterbox(320, 240, 1000, 720), (20.0, 0.0, 960.0, 720.0));
        assert_eq!(
            letterbox(320, 240, 1920, 1080),
            (320.0, 60.0, 1280.0, 960.0)
        );
        assert_eq!(letterbox(480, 240, 960, 720), (0.0, 120.0, 960.0, 480.0));
        let (_, _, w, h) = letterbox(320, 240, 160, 200);
        assert_eq!((w, h), (160.0, 120.0));
    }
}
