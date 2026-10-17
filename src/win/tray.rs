use super::sys::*;
use crate::i18n::{k, tr};
use crate::icons;
use windows::Win32::Foundation::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub struct Tray {
    host: HWND,
    icons: [Option<HICON>; 2],
    style: u8,
    muted: bool,
    tip: String,
}

fn icon_size() -> u32 {
    unsafe { GetSystemMetrics(SM_CXSMICON).max(16) as u32 }
}

impl Tray {
    pub fn new(host: HWND, style: u8, muted: bool) -> Self {
        let mut t = Tray { host, icons: [None, None], style, muted, tip: String::new() };
        t.build_icons();
        t
    }

    fn build_icons(&mut self) {
        for i in self.icons.iter_mut().flatten() {
            unsafe {
                let _ = DestroyIcon(*i);
            }
        }
        let s = icon_size();
        let badge = self.style == 0;
        self.icons = [hicon(&icons::render_tray(badge, false, s)), hicon(&icons::render_tray(badge, true, s))];
    }

    fn data(&self) -> NOTIFYICONDATAW {
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.host,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
            uCallbackMessage: WM_TRAY,
            hIcon: self.icons[self.muted as usize].unwrap_or_default(),
            ..Default::default()
        };
        fill_wide(&mut nid.szTip, &self.tip);
        nid
    }

    pub fn add(&mut self) {
        unsafe {
            let mut nid = self.data();
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
            let _ = Shell_NotifyIconW(NIM_ADD, &nid);
            nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            let _ = Shell_NotifyIconW(NIM_SETVERSION, &nid);
        }
    }

    pub fn update(&mut self, muted: bool, has_mic: bool) {
        self.muted = muted || !has_mic;
        self.tip = if !has_mic {
            tr(k::TIP_NONE).into()
        } else if muted {
            tr(k::TIP_OFF).into()
        } else {
            tr(k::TIP_ON).into()
        };
        unsafe {
            let nid = self.data();
            let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
    }

    pub fn set_style(&mut self, style: u8) {
        if style != self.style {
            self.style = style;
            self.build_icons();
            unsafe {
                let nid = self.data();
                let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
            }
        }
    }

    pub fn refresh_dpi(&mut self) {
        self.build_icons();
        self.add();
    }

    pub fn balloon(&self, title: &str, text: &str) {
        unsafe {
            let mut nid = self.data();
            nid.uFlags = NIF_INFO;
            nid.dwInfoFlags = NIIF_NONE | NIIF_NOSOUND;
            fill_wide(&mut nid.szInfoTitle, title);
            fill_wide(&mut nid.szInfo, text);
            let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
    }

    pub fn remove(&mut self) {
        unsafe {
            let nid = self.data();
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
            for i in self.icons.iter_mut() {
                if let Some(h) = i.take() {
                    let _ = DestroyIcon(h);
                }
            }
        }
    }
}

pub struct Item {
    pub id: usize,
    pub text: &'static str,
    pub checked: bool,
    pub disabled: bool,
    pub bold: bool,
}

pub fn item(id: usize, text: &'static str) -> Item {
    Item { id, text, checked: false, disabled: false, bold: false }
}

pub const SEP: usize = 0;

/// Shows a popup menu at the cursor and returns the picked id (0 if none).
pub fn popup(host: HWND, items: &[Item]) -> usize {
    unsafe {
        let Ok(m) = CreatePopupMenu() else { return 0 };
        let mut strs = Vec::new();
        for it in items {
            if it.id == SEP {
                let _ = AppendMenuW(m, MF_SEPARATOR, 0, None);
                continue;
            }
            let w = wide(it.text);
            let mut f = MF_STRING;
            if it.checked {
                f |= MF_CHECKED;
            }
            if it.disabled {
                f |= MF_GRAYED;
            }
            let _ = AppendMenuW(m, f, it.id, windows::core::PCWSTR(w.as_ptr()));
            if it.bold {
                let _ = SetMenuDefaultItem(m, it.id as u32, 0);
            }
            strs.push(w);
        }
        let mut p = POINT::default();
        let _ = GetCursorPos(&mut p);
        let _ = SetForegroundWindow(host);
        let flags = TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY | TPM_BOTTOMALIGN | TPM_RIGHTALIGN;
        let r = TrackPopupMenuEx(m, flags.0, p.x, p.y, host, None);
        let _ = PostMessageW(Some(host), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(m);
        drop(strs);
        r.0 as usize
    }
}
