use crate::i18n::{k, tr};
use serde::{Deserialize, Serialize};

pub const MOD_CTRL: u8 = 1;
pub const MOD_ALT: u8 = 2;
pub const MOD_SHIFT: u8 = 4;
pub const MOD_WIN: u8 = 8;

pub const VK_MBUTTON: u16 = 0x04;
pub const VK_XBUTTON1: u16 = 0x05;
pub const VK_XBUTTON2: u16 = 0x06;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub mods: u8,
    pub vk: u16,
}

impl Binding {
    pub fn is_mouse(&self) -> bool {
        matches!(self.vk, VK_MBUTTON | VK_XBUTTON1 | VK_XBUTTON2)
    }

    pub fn label(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.mods & MOD_CTRL != 0 {
            parts.push("Ctrl".into());
        }
        if self.mods & MOD_ALT != 0 {
            parts.push("Alt".into());
        }
        if self.mods & MOD_SHIFT != 0 {
            parts.push("Shift".into());
        }
        if self.mods & MOD_WIN != 0 {
            parts.push("Win".into());
        }
        parts.push(vk_name(self.vk));
        parts.join(" + ")
    }
}

pub fn is_modifier(vk: u16) -> bool {
    matches!(vk, 0x10 | 0x11 | 0x12 | 0x5B | 0x5C | 0xA0..=0xA5)
}

pub fn modifier_bit(vk: u16) -> u8 {
    match vk {
        0x11 | 0xA2 | 0xA3 => MOD_CTRL,
        0x12 | 0xA4 | 0xA5 => MOD_ALT,
        0x10 | 0xA0 | 0xA1 => MOD_SHIFT,
        0x5B | 0x5C => MOD_WIN,
        _ => 0,
    }
}

pub fn vk_name(vk: u16) -> String {
    let s = match vk {
        0x04 => tr(k::MOUSE_WHEEL),
        0x05 => tr(k::MOUSE_4),
        0x06 => tr(k::MOUSE_5),
        0x08 => "Backspace",
        0x09 => "Tab",
        0x0C => "Clear",
        0x0D => "Enter",
        0x13 => "Pause",
        0x14 => "Caps Lock",
        0x1B => "Esc",
        0x20 => "Space",
        0x21 => "Page Up",
        0x22 => "Page Down",
        0x23 => "End",
        0x24 => "Home",
        0x25 => "←",
        0x26 => "↑",
        0x27 => "→",
        0x28 => "↓",
        0x2C => "Print Screen",
        0x2D => "Insert",
        0x2E => "Delete",
        0x5D => "Menu",
        0x6A => "Num *",
        0x6B => "Num +",
        0x6C => "Num Enter",
        0x6D => "Num -",
        0x6E => "Num .",
        0x6F => "Num /",
        0x90 => "Num Lock",
        0x91 => "Scroll Lock",
        0xAD => "Mute",
        0xAE => "Vol -",
        0xAF => "Vol +",
        0xB0 => "Next",
        0xB1 => "Prev",
        0xB2 => "Stop",
        0xB3 => "Play",
        0xBA => ";",
        0xBB => "=",
        0xBC => ",",
        0xBD => "-",
        0xBE => ".",
        0xBF => "/",
        0xC0 => "`",
        0xDB => "[",
        0xDC => "\\",
        0xDD => "]",
        0xDE => "'",
        0xE2 => "<",
        _ => "",
    };
    if !s.is_empty() {
        return s.into();
    }
    match vk {
        0x30..=0x39 | 0x41..=0x5A => char::from(vk as u8).to_string(),
        0x60..=0x69 => format!("Num {}", vk - 0x60),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        _ => format!("Key {vk:#04X}"),
    }
}

pub const ACTIONS: usize = 6;
pub const A_TOGGLE: usize = 0;
pub const A_PTT: usize = 1;
pub const A_OVERLAY: usize = 2;
pub const A_LOCK: usize = 3;
pub const A_WINDOW: usize = 4;
pub const A_MENU: usize = 5;

pub fn action_title(i: usize) -> &'static str {
    tr([k::ACT_TOGGLE, k::ACT_PTT, k::OVERLAY, k::ACT_LOCK, k::ACT_WINDOW, k::ACT_MENU][i])
}

pub fn action_hint(i: usize) -> &'static str {
    match i {
        A_PTT => tr(k::ACT_PTT_HINT),
        A_OVERLAY => tr(k::ACT_OVERLAY_HINT),
        A_MENU => tr(k::ACT_MENU_HINT),
        _ => "",
    }
}
