//! The window build's picture settings (Phase 8.4), kept in
//! `~/Library/Application Support/dw1-silicon/settings.txt` and changed in play with hotkeys.
//! Every option says whether it's the original or an addition: with `render_scale = 1` and
//! `fit = integer` the picture is the PS1's, pixel for pixel.
//!
//! The file is `key = value` lines (`#` starts a comment); unknown keys and bad values are
//! reported and left at their defaults. `DW1_SCALE`, `DW1_PGXP`, `DW1_FULL_COLOUR` and `DW1_FIT`
//! override it for one run. Headless runs never read it.

use std::path::{Path, PathBuf};

use render::Fit;

/// The render scale: the window's size or a fixed one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    /// Enough to fill the picture's place in the window pixel for pixel (at most 8).
    Auto,
    /// 1 (the original picture) to 8.
    Fixed(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub scale: Scale,
    pub no_wobble: bool,
    pub full_colour: bool,
    pub fit: Fit,
    pub fullscreen: bool,
    pub volume: u8,
    pub window_mode: WindowMode,
    pub window_size: u8,
    pub keyboard_bindings: [i32; 16],
    pub gamepad_bindings: [i32; 16],
    pub input_mode: InputMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowMode { Windowed, Borderless, Exclusive }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputMode { Auto, Keyboard, Gamepad }

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            scale: Scale::Auto,
            no_wobble: true,
            full_colour: true,
            fit: Fit::Integer,
            fullscreen: false,
            volume: 100,
            window_mode: WindowMode::Windowed,
            window_size: 2,
            keyboard_bindings: [29, 27, 4, 22, 40, 229, 20, 26, 30, 31, 82, 79, 81, 80, 0, 0],
            gamepad_bindings: [1, 2, 3, 4, 7, 5, 10, 11, 109, 111, 12, 15, 13, 14, 0, 0],
            input_mode: InputMode::Auto,
        }
    }
}

/// The scales F2 steps through, after `Auto`.
const STEPS: [u32; 6] = [1, 2, 3, 4, 6, 8];

/// The render scale that draws a picture shown `shown_h` window pixels tall at least one pixel
/// per window pixel: the shown height over the PS1's 240 lines, rounded up, 1 to 8.
pub fn auto_scale(shown_h: u32) -> u32 {
    shown_h.div_ceil(render::BASE.1).clamp(1, 8)
}

fn parse_bindings(value: &str, key: &str) -> Result<[i32; 16], String> {
    let values: Vec<i32> = value.split(',').map(|v| v.trim().parse::<i32>())
        .collect::<Result<_, _>>().map_err(|_| format!("{key} must contain 16 integer codes"))?;
    values.try_into().map_err(|_| format!("{key} must contain 16 integer codes"))
}

impl Settings {
    /// The settings file, or None without a home directory.
    pub fn path() -> Option<PathBuf> {
        crate::default_save_dir().map(|d| d.join("settings.txt"))
    }

    /// The settings in `path` (defaults where the file says nothing or doesn't exist), and the
    /// same with the environment's overrides: `(kept, in use)`. Only the first is saved, so an
    /// override lasts one run.
    pub fn load(path: Option<&Path>) -> (Settings, Settings) {
        let mut s = Settings::default();
        if let Some(text) = path.and_then(|p| std::fs::read_to_string(p).ok()) {
            let has_window_mode = text.lines().filter_map(|line| line.split('#').next())
                .filter_map(|line| line.split_once('='))
                .any(|(key, _)| key.trim() == "window_mode");
            for problem in s.parse(&text) {
                eprintln!("dw1: settings: {problem}");
            }
            // Read settings made by older builds, where fullscreen was the only display flag.
            if !has_window_mode && s.fullscreen { s.window_mode = WindowMode::Exclusive; }
        }
        let mut used = s;
        used.apply_env(|k| std::env::var(k).ok());
        (s, used)
    }

    /// Reads `key = value` lines over `self`; returns what it couldn't use.
    fn parse(&mut self, text: &str) -> Vec<String> {
        let mut problems = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                problems.push(format!("line {}: not `key = value`: {line}", n + 1));
                continue;
            };
            if let Err(e) = self.set(key.trim(), value.trim()) {
                problems.push(format!("line {}: {e}", n + 1));
            }
        }
        problems
    }

    fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        let bool_of = |v: &str| match v {
            "true" | "on" | "1" => Ok(true),
            "false" | "off" | "0" => Ok(false),
            _ => Err(format!("{key} is true or false, not {v:?}")),
        };
        match key {
            "render_scale" => {
                self.scale = match value {
                    "auto" => Scale::Auto,
                    v => match v.parse::<u32>() {
                        Ok(s @ 1..=8) => Scale::Fixed(s),
                        _ => return Err(format!("render_scale is auto or 1 to 8, not {v:?}")),
                    },
                }
            }
            "volume" => self.volume = value.parse::<u8>().ok().filter(|v| *v <= 100).ok_or_else(|| format!("volume is 0 to 100, not {value:?}"))?,
            "window_size" => self.window_size = value.parse::<u8>().ok().filter(|v| *v < 5).ok_or_else(|| format!("window_size is 0 to 4, not {value:?}"))?,
            "keyboard_bindings" => self.keyboard_bindings = parse_bindings(value, key)?,
            "gamepad_bindings" => self.gamepad_bindings = parse_bindings(value, key)?,
            "input_mode" => self.input_mode = match value { "auto" => InputMode::Auto, "keyboard" => InputMode::Keyboard, "gamepad" => InputMode::Gamepad, _ => return Err(format!("input_mode is auto, keyboard or gamepad, not {value:?}")) },
            "window_mode" => self.window_mode = match value { "windowed" => WindowMode::Windowed, "borderless" => WindowMode::Borderless, "exclusive" => WindowMode::Exclusive, _ => return Err(format!("invalid window_mode {value:?}")) },
            "no_wobble" => self.no_wobble = bool_of(value)?,
            "full_colour" => self.full_colour = bool_of(value)?,
            "fullscreen" => self.fullscreen = bool_of(value)?,
            "fit" => {
                self.fit = match value {
                    "integer" => Fit::Integer,
                    "fill" => Fit::Fill,
                    v => return Err(format!("fit is integer or fill, not {v:?}")),
                }
            }
            _ => return Err(format!("unknown setting {key:?}")),
        }
        Ok(())
    }

    /// `DW1_SCALE=<n>` (a fixed scale), `DW1_PGXP=0/1`, `DW1_FULL_COLOUR=0/1`,
    /// `DW1_FIT=integer/fill`.
    fn apply_env(&mut self, var: impl Fn(&str) -> Option<String>) {
        for (env, key) in [
            ("DW1_SCALE", "render_scale"),
            ("DW1_PGXP", "no_wobble"),
            ("DW1_FULL_COLOUR", "full_colour"),
            ("DW1_FIT", "fit"),
        ] {
            if let Some(v) = var(env).filter(|v| !v.is_empty())
                && let Err(e) = self.set(key, v.trim())
            {
                eprintln!("dw1: {env}: {e}");
            }
        }
    }

    pub fn to_text(self) -> String {
        let scale = match self.scale {
            Scale::Auto => "auto".to_string(),
            Scale::Fixed(s) => s.to_string(),
        };
        let fit = match self.fit {
            Fit::Fill => "fill",
            _ => "integer",
        };
        let keyboard = self.keyboard_bindings.iter().map(i32::to_string).collect::<Vec<_>>().join(",");
        let gamepad = self.gamepad_bindings.iter().map(i32::to_string).collect::<Vec<_>>().join(",");
        format!(
            "# dw1-silicon picture settings. Changed in the game: F2 render scale, F3 no wobble,\n\
             # F4 fit, F5 full colour, F11 fullscreen. Delete this file for the defaults.\n\
             \n\
             # The resolution the game is drawn at: 1 is the original PS1 picture; 2 to 8 draw it\n\
             # that many times sharper (an addition); auto picks from the window's size.\n\
             render_scale = {scale}\n\
             \n\
             # 3D models at their exact positions instead of the PS1's whole pixels, so they\n\
             # don't wobble (an addition; only with render_scale above 1).\n\
             no_wobble = {}\n\
             \n\
             # Smooth colour: shading and see-through effects at full precision, without the\n\
             # PS1's 15-bit colour and its dither pattern (an addition; only with render_scale\n\
             # above 1).\n\
             full_colour = {}\n\
             \n\
             # integer: the largest whole multiple of the PS1's picture, black around it (the\n\
             # original look). fill: as large as the window allows at 4:3 (an addition).\n\
             fit = {fit}\n\
             \n\
             fullscreen = {}\n\
             volume = {}\n\
             window_mode = {}\n\
             window_size = {}\n\
             keyboard_bindings = {keyboard}\n\
             gamepad_bindings = {gamepad}\n\
             input_mode = {}\n\
",
            self.no_wobble, self.full_colour, self.fullscreen, self.volume,
            match self.window_mode { WindowMode::Windowed => "windowed", WindowMode::Borderless => "borderless", WindowMode::Exclusive => "exclusive" }, self.window_size,
            match self.input_mode { InputMode::Auto => "auto", InputMode::Keyboard => "keyboard", InputMode::Gamepad => "gamepad" }
        )
    }

    /// Writes the settings (a temporary file renamed over the old one); a failure is reported
    /// and otherwise ignored.
    pub fn save(&self, path: Option<&Path>) {
        let Some(path) = path else {
            return;
        };
        let result = (|| -> std::io::Result<()> {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
            std::fs::write(&tmp, self.to_text())?;
            std::fs::rename(&tmp, path)
        })();
        if let Err(e) = result {
            eprintln!("dw1: settings not saved to {}: {e}", path.display());
        }
    }

    /// F2: auto, 1, 2, 3, 4, 6, 8, auto...
    pub fn next_scale(&mut self) {
        self.scale = match self.scale {
            Scale::Auto => Scale::Fixed(STEPS[0]),
            Scale::Fixed(s) => match STEPS.iter().position(|&t| t == s) {
                Some(i) if i + 1 < STEPS.len() => Scale::Fixed(STEPS[i + 1]),
                _ => Scale::Auto,
            },
        };
    }

    /// The render scale for a picture shown `shown_h` window pixels tall.
    pub fn scale_for(&self, shown_h: u32) -> u32 {
        match self.scale {
            Scale::Auto => auto_scale(shown_h),
            Scale::Fixed(s) => s,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_round_trips() {
        let s = Settings {
            scale: Scale::Fixed(3),
            no_wobble: false,
            full_colour: false,
            fit: Fit::Fill,
            fullscreen: true,
            ..Settings::default()
        };
        let mut t = Settings::default();
        let problems = t.parse(&s.to_text());
        assert!(problems.is_empty(), "{problems:?}\n{}", s.to_text());
        assert_eq!(t, s);
        let mut device = Settings::default();
        assert!(device.parse("input_mode = gamepad\nvolume = 35\n").is_empty());
        assert_eq!(device.input_mode, InputMode::Gamepad);
        assert_eq!(device.volume, 35);
        let mut d = Settings::default();
        assert!(d.parse(&Settings::default().to_text()).is_empty());
        assert_eq!(d, Settings::default());
    }

    #[test]
    fn bad_lines_are_reported_and_skipped() {
        let mut s = Settings::default();
        let problems =
            s.parse("render_scale = 9\nfit = stretch\nwobble = no\nno_wobble = off\nnonsense\n");
        assert_eq!(problems.len(), 4, "{problems:?}");
        assert_eq!(s.scale, Scale::Auto);
        assert!(!s.no_wobble);
    }

    #[test]
    fn the_environment_overrides() {
        let mut s = Settings::default();
        s.apply_env(|k| match k {
            "DW1_SCALE" => Some("1".into()),
            "DW1_FIT" => Some("fill".into()),
            _ => None,
        });
        assert_eq!(s.scale, Scale::Fixed(1));
        assert_eq!(s.fit, Fit::Fill);
        assert!(s.no_wobble);
    }

    #[test]
    fn scales_step_and_follow_the_window() {
        let mut s = Settings::default();
        let mut seen = vec![];
        for _ in 0..8 {
            s.next_scale();
            seen.push(s.scale);
        }
        assert_eq!(seen[0], Scale::Fixed(1));
        assert_eq!(seen[5], Scale::Fixed(8));
        assert_eq!(seen[6], Scale::Auto);
        assert_eq!(auto_scale(720), 3);
        assert_eq!(auto_scale(1440), 6);
        assert_eq!(auto_scale(1441), 7);
        assert_eq!(auto_scale(100), 1);
        assert_eq!(auto_scale(5000), 8);
    }
}
