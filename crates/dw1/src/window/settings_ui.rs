use render::{Fit, Picture};
use sdl3::{event::Event, gamepad::Button, keyboard::Scancode};

use super::{splash::{draw_lines, Rgb}, Window};
use crate::settings::{InputMode, WindowMode};

const ACTIONS: [&str; 14] = [
    "Cross", "Circle", "Square", "Triangle", "Start", "Select", "L1", "R1",
    "L2 / left trigger", "R2 / right trigger", "Up", "Right", "Down", "Left",
];
const SIZES: [(u32, u32); 5] = [(640, 480), (800, 600), (960, 720), (1280, 960), (1600, 1200)];

fn button_name(code: i32) -> &'static str {
    match Button::from_ll(sdl3::sys::gamepad::SDL_GamepadButton(code - 1)) {
        Some(Button::South) => "South", Some(Button::East) => "East",
        Some(Button::West) => "West", Some(Button::North) => "North",
        Some(Button::Start) => "Start", Some(Button::Back) => "Back",
        Some(Button::LeftShoulder) => "L1", Some(Button::RightShoulder) => "R1",
        Some(Button::DPadUp) => "D-pad Up", Some(Button::DPadRight) => "D-pad Right",
        Some(Button::DPadDown) => "D-pad Down", Some(Button::DPadLeft) => "D-pad Left",
        Some(Button::LeftStick) => "L3", Some(Button::RightStick) => "R3",
        _ if code >= 100 => {
            let axis = (code - 100) / 2;
            match (axis, code % 2) {
                (0,0) => "Left stick left", (0,1) => "Left stick right",
                (1,0) => "Left stick up", (1,1) => "Left stick down",
                (2,0) => "Right stick left", (2,1) => "Right stick right",
                (3,0) => "Right stick up", (3,1) => "Right stick down",
                (4,1) => "Left trigger", (5,1) => "Right trigger", _ => "Unbound",
            }
        }
        _ => "Unbound",
    }
}

fn input_mode_name(mode: InputMode) -> &'static str {
    match mode { InputMode::Auto => "Keyboard + gamepad", InputMode::Keyboard => "Keyboard only", InputMode::Gamepad => "Gamepad only" }
}

impl Window {
    pub(super) fn settings_ui(&mut self) {
        // Main menu rows: volume, display mode, window size, input device, two binding maps,
        // reset bindings, close. A child menu captures one physical input per selected action.
        let mut row = 0usize;
        let mut editor: Option<(bool, usize)> = None;
        let mut capture = false;
        let mut pixels = Vec::new();
        loop {
            let title = if let Some((keyboard, _)) = editor { if keyboard { "Keyboard mapping" } else { "Gamepad mapping" } } else { "Settings" };
            let mut lines: Vec<(String, f32, f32, Rgb)> = vec![(title.into(), 2.0, 35.0, (235,238,245))];
            if let Some((keyboard, selected)) = editor {
                for (index, label) in ACTIONS.iter().enumerate() {
                    let code = if keyboard { self.settings.keyboard_bindings[index] } else { self.settings.gamepad_bindings[index] };
                    let binding = if capture && index == selected { "Press input (Esc cancels)".to_string() }
                        else if keyboard { Scancode::from_i32(code).map(|key| key.name().to_string()).unwrap_or_else(|| "Unbound".into()) }
                        else { button_name(code).to_string() };
                    let y = 56.0 + index as f32 * 11.0;
                    let prefix = if index == selected { "> " } else { "  " };
                    lines.push((format!("{prefix}{label}: {binding}"), 0.72, y, if index == selected {(170,205,255)} else {(140,148,168)}));
                }
                lines.push(("Enter: rebind | Backspace: default | Delete: clear | Esc: back".into(),0.62,221.0,(105,116,140)));
            } else {
                let display = match self.settings.window_mode { WindowMode::Windowed => "Windowed", WindowMode::Borderless => "Borderless", WindowMode::Exclusive => "Fullscreen" };
                let (sw, sh) = SIZES[self.settings.window_size.min(4) as usize];
                let labels = [format!("Volume: {}%", self.settings.volume), format!("Display: {display}"), format!("Window size: {sw}x{sh} (4:3)"), format!("Input: {}", input_mode_name(self.settings.input_mode)), "Keyboard mappings...".into(), "Gamepad mappings...".into(), "Reset all input mappings".into(), "Close settings (Esc / F1)".into()];
                for (index, label) in labels.iter().enumerate() {
                    let y = 77.0 + index as f32 * 19.0;
                    let prefix = if row == index { "> " } else { "  " };
                    lines.push((format!("{prefix}{label}"), 0.86, y, if row == index {(170,205,255)} else {(140,148,168)}));
                }
                lines.push(("Up/Down choose | Left/Right change | Enter select".into(),0.66,221.0,(105,116,140)));
            }
            let refs: Vec<(&str, f32, f32, Rgb)> = lines.iter().map(|(t,s,y,c)|(t.as_str(),*s,*y,*c)).collect();
            let (w,h) = self.window.size_in_pixels();
            let (w,h) = self.renderer.drawable_size(w,h);
            if w > 0 && h > 0 && draw_lines(w,h,&refs,1.0,&mut pixels).is_ok() {
                Self::show(&mut self.renderer,&self.window,Some(Picture { pixels: &pixels, w, h }),Fit::Exact,None);
            }

            let mut close = false;
            let mut change = 0i32;
            for event in self.pump.poll_iter().collect::<Vec<_>>() {
                if capture {
                    match event {
                        Event::KeyDown { scancode: Some(Scancode::Escape), repeat:false, .. } => capture = false,
                        Event::KeyDown { scancode: Some(key), repeat:false, .. } if editor.is_some_and(|(keyboard,_)| keyboard) && key != Scancode::F1 => {
                            if let Some((_,index)) = editor { self.settings.keyboard_bindings[index] = key as i32; }
                            capture = false;
                            self.save_input_settings();
                        }
                        Event::GamepadButtonDown { button, .. } if editor.is_some_and(|(keyboard,_)| !keyboard) => {
                            if let Some((_,index)) = editor { self.settings.gamepad_bindings[index] = button as i32 + 1; }
                            capture = false;
                            self.save_input_settings();
                        }
                        Event::GamepadAxisMotion { axis, value, .. } if editor.is_some_and(|(keyboard,_)| !keyboard) && value.unsigned_abs() > 16_000 => {
                            if let Some((_,index)) = editor { self.settings.gamepad_bindings[index] = 100 + axis as i32 * 2 + i32::from(value > 0); }
                            capture = false;
                            self.save_input_settings();
                        }
                        Event::GamepadAdded { .. } | Event::GamepadRemoved { .. } => self.handle(event),
                        Event::Quit { .. } => { crate::applog::clean_exit(); std::process::exit(0); }
                        _ => {}
                    }
                    continue;
                }
                match event {
                    Event::Quit { .. } => { crate::applog::clean_exit(); std::process::exit(0); }
                    Event::KeyDown { scancode:Some(Scancode::Escape|Scancode::F1), repeat:false, .. } => {
                        if editor.is_some() { editor = None; } else { close = true; }
                    }
                    Event::KeyDown { scancode:Some(Scancode::Up), repeat:false, .. } => {
                        if let Some((keyboard,index))=editor { editor=Some((keyboard,(index+ACTIONS.len()-1)%ACTIONS.len())); }
                        else { row=(row+8-1)%8; }
                    }
                    Event::KeyDown { scancode:Some(Scancode::Down), repeat:false, .. } => {
                        if let Some((keyboard,index))=editor { editor=Some((keyboard,(index+1)%ACTIONS.len())); }
                        else { row=(row+1)%8; }
                    }
                    Event::KeyDown { scancode:Some(Scancode::Left), repeat:false, .. } => change=-1,
                    Event::KeyDown { scancode:Some(Scancode::Right), repeat:false, .. } => change=1,
                    Event::KeyDown { scancode:Some(Scancode::Return|Scancode::KpEnter|Scancode::Space), repeat:false, .. } => change=1,
                    Event::KeyDown { scancode:Some(Scancode::Backspace), repeat:false, .. } if editor.is_some() => {
                        if let Some((keyboard,index))=editor {
                            let defaults=WindowSettingsDefaults::default();
                            if keyboard { self.settings.keyboard_bindings[index]=defaults.keyboard[index]; }
                            else { self.settings.gamepad_bindings[index]=defaults.gamepad[index]; }
                            self.save_input_settings();
                        }
                    }
                    Event::KeyDown { scancode:Some(Scancode::Delete), repeat:false, .. } if editor.is_some() => {
                        if let Some((keyboard,index))=editor { if keyboard {self.settings.keyboard_bindings[index]=0;} else {self.settings.gamepad_bindings[index]=0;} self.save_input_settings(); }
                    }
                    Event::GamepadButtonDown { button:Button::DPadUp,.. } => {
                        if let Some((keyboard,index))=editor {editor=Some((keyboard,(index+ACTIONS.len()-1)%ACTIONS.len()));} else {row=(row+7)%8;}
                    }
                    Event::GamepadButtonDown { button:Button::DPadDown,.. } => {
                        if let Some((keyboard,index))=editor {editor=Some((keyboard,(index+1)%ACTIONS.len()));} else {row=(row+1)%8;}
                    }
                    Event::GamepadButtonDown { button:Button::East,.. } => { if editor.is_some() {editor=None;} else {close=true;} }
                    Event::GamepadButtonDown { button:Button::West|Button::South,.. } => change=1,
                    Event::MouseButtonDown { y,.. } if editor.is_some() => {
                        if let Some((keyboard,_)) = editor {
                            let index = (((y as usize).saturating_mul(240) / h.max(1) as usize).saturating_sub(56) / 11).min(ACTIONS.len()-1);
                            editor = Some((keyboard,index));
                            change = 1;
                        }
                    }
                    Event::MouseButtonDown { y,.. } if editor.is_none() => {row=(((y as usize).saturating_mul(240)/h.max(1) as usize).saturating_sub(77)/19).min(7);change=1;}
                    Event::GamepadAdded { .. } | Event::GamepadRemoved { .. } => self.handle(event),
                    _ => {}
                }
            }

            if change != 0 {
                let old_mode = self.settings.window_mode;
                let old_size = self.settings.window_size;
                let old_volume = self.settings.volume;
                if let Some((keyboard,index))=editor {
                    if change > 0 { capture=true; }
                    editor=Some((keyboard,index));
                } else { match row {
                    0 => self.settings.volume=if change < 0 {self.settings.volume.saturating_sub(5)} else {self.settings.volume.saturating_add(5).min(100)},
                    1 => self.settings.window_mode=match (self.settings.window_mode,change>0) {
                        (WindowMode::Windowed,true)=>WindowMode::Borderless,(WindowMode::Borderless,true)=>WindowMode::Exclusive,(WindowMode::Exclusive,true)=>WindowMode::Windowed,
                        (WindowMode::Windowed,false)=>WindowMode::Exclusive,(WindowMode::Exclusive,false)=>WindowMode::Borderless,(WindowMode::Borderless,false)=>WindowMode::Windowed,
                    },
                    2 => self.settings.window_size=if change<0 {self.settings.window_size.saturating_sub(1)} else {(self.settings.window_size+1).min(4)},
                    3 => self.settings.input_mode=match self.settings.input_mode {InputMode::Auto=>if change>0 {InputMode::Keyboard} else {InputMode::Gamepad},InputMode::Keyboard=>if change>0 {InputMode::Gamepad} else {InputMode::Auto},InputMode::Gamepad=>if change>0 {InputMode::Auto} else {InputMode::Keyboard}},
                    4 => {editor=Some((true,0));row=0;},
                    5 => {editor=Some((false,0));row=0;},
                    6 => {let d=WindowSettingsDefaults::default();self.settings.keyboard_bindings=d.keyboard;self.settings.gamepad_bindings=d.gamepad;self.save_input_settings();},
                    _ => close=true,
                }}
                self.settings.fullscreen=self.settings.window_mode==WindowMode::Exclusive;
                if editor.is_none() {
                    if old_mode != self.settings.window_mode || old_size != self.settings.window_size { self.apply_window_settings(); }
                    if old_volume != self.settings.volume { self.apply_audio_settings(); }
                    self.save_input_settings();
                } else {self.held_over=self.held_pad();}
            }
            if close {break;}
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        self.held_over=self.held_pad();
    }

    fn save_input_settings(&mut self) {
        self.kept.keyboard_bindings=self.settings.keyboard_bindings;
        self.kept.gamepad_bindings=self.settings.gamepad_bindings;
        self.kept.input_mode=self.settings.input_mode;
        self.kept.volume=self.settings.volume;
        self.kept.window_mode=self.settings.window_mode;
        self.kept.fullscreen=self.settings.fullscreen;
        self.kept.window_size=self.settings.window_size;
        self.kept.save(self.settings_path.as_deref());
    }
}

struct WindowSettingsDefaults { keyboard: [i32;16], gamepad: [i32;16] }
impl Default for WindowSettingsDefaults {
    fn default() -> Self {
        let s=crate::settings::Settings::default(); Self {keyboard:s.keyboard_bindings,gamepad:s.gamepad_bindings}
    }
}
