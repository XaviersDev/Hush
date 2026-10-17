use crate::keys::{Binding, ACTIONS, MOD_ALT, MOD_CTRL};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    All,
    Default,
    Device(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Overlay {
    pub enabled: bool,
    pub style: usize,
    /// 0 none, 1 left, 2 right, 3 middle
    pub click: u8,
    /// 0 always, 1 only muted, 2 only live
    pub show_when: u8,
    pub locked: bool,
    pub opacity: f32,
    pub hide_capture: bool,
    pub placed: bool,
    pub x: i32,
    pub y: i32,
    pub size: i32,
}

impl Default for Overlay {
    fn default() -> Self {
        Self {
            enabled: true,
            style: 3,
            click: 1,
            show_when: 0,
            locked: false,
            opacity: 1.0,
            hide_capture: false,
            placed: false,
            x: 0,
            y: 0,
            size: 0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub target: Target,
    pub overlay: Overlay,
    pub keys: [Option<Binding>; ACTIONS],
    pub autostart: bool,
    pub sound: bool,
    /// 0 toggles mic, 1 opens window
    pub tray_click: u8,
    /// 0 badge, 1 glyph
    pub tray_style: u8,
    pub hold_state: bool,
    pub tray_hint_shown: bool,
    /// empty = follow Windows
    pub lang: String,
}

impl Default for Config {
    fn default() -> Self {
        let mut keys = [None; ACTIONS];
        keys[0] = Some(Binding { mods: MOD_CTRL | MOD_ALT, vk: b'M' as u16 });
        Self {
            target: Target::All,
            overlay: Overlay::default(),
            keys,
            autostart: true,
            sound: true,
            tray_click: 0,
            tray_style: 0,
            hold_state: false,
            tray_hint_shown: false,
            lang: String::new(),
        }
    }
}

fn dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("Hush")
}

pub fn load() -> Config {
    let path = dir().join("config.json");
    match std::fs::read_to_string(&path) {
        Ok(s) => {
            let mut c: Config = serde_json::from_str(&s).unwrap_or_default();
            c.overlay.style %= crate::icons::STYLE_COUNT;
            c.overlay.opacity = c.overlay.opacity.clamp(0.2, 1.0);
            c.overlay.click = c.overlay.click.min(3);
            c.overlay.show_when = c.overlay.show_when.min(2);
            c
        }
        Err(_) => Config::default(),
    }
}

pub fn save(c: &Config) {
    let d = dir();
    let _ = std::fs::create_dir_all(&d);
    let tmp = d.join("config.json.tmp");
    if let Ok(s) = serde_json::to_string_pretty(c) {
        if std::fs::write(&tmp, s).is_ok() {
            let _ = std::fs::rename(&tmp, d.join("config.json"));
        }
    }
}
