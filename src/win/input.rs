// Keyboard hotkeys go through RegisterHotKey: it works even when an elevated
// window has focus and can't be dropped by Windows for being slow.
// Mouse buttons need a low level hook, it lives on its own thread.
// Keys are recorded straight from our window (see main_subclass in mod.rs).

use super::sys::*;
use crate::keys::{self as k, Binding, ACTIONS};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, Ordering};
use std::sync::Mutex;
use windows::Win32::Foundation::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const WM_REFRESH: u32 = WM_APP + 50;

static TID: AtomicU32 = AtomicU32::new(0);
static RECORDING: AtomicBool = AtomicBool::new(false);
static SWALLOW_UP: AtomicU16 = AtomicU16::new(0);
static MOUSE_BINDS: Mutex<Vec<(usize, Binding)>> = Mutex::new(Vec::new());
static HELD: Mutex<Vec<(u16, usize)>> = Mutex::new(Vec::new());

thread_local! {
    static MS_HOOK: Cell<Option<HHOOK>> = const { Cell::new(None) };
}

fn async_mods() -> u8 {
    let down = |vk: VIRTUAL_KEY| unsafe { GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000 != 0 };
    let mut m = 0;
    if down(VK_CONTROL) {
        m |= k::MOD_CTRL;
    }
    if down(VK_MENU) {
        m |= k::MOD_ALT;
    }
    if down(VK_SHIFT) {
        m |= k::MOD_SHIFT;
    }
    if down(VK_LWIN) || down(VK_RWIN) {
        m |= k::MOD_WIN;
    }
    m
}

pub fn key_down(vk: u16) -> bool {
    unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

fn refresh_hooks() {
    let want = RECORDING.load(Ordering::SeqCst) || !MOUSE_BINDS.lock().unwrap().is_empty();
    unsafe {
        match MS_HOOK.get() {
            None if want => MS_HOOK.set(SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), Some(instance()), 0).ok()),
            Some(h) if !want => {
                let _ = UnhookWindowsHookEx(h);
                MS_HOOK.set(None);
            }
            _ => {}
        }
    }
}

/// Modifier state as seen by the message being handled on this thread.
pub fn held_mods() -> u8 {
    let down = |vk: VIRTUAL_KEY| unsafe { GetKeyState(vk.0 as i32) as u16 & 0x8000 != 0 };
    let mut m = 0;
    if down(VK_CONTROL) {
        m |= k::MOD_CTRL;
    }
    if down(VK_MENU) {
        m |= k::MOD_ALT;
    }
    if down(VK_SHIFT) {
        m |= k::MOD_SHIFT;
    }
    if down(VK_LWIN) || down(VK_RWIN) {
        m |= k::MOD_WIN;
    }
    m
}

pub fn is_recording() -> bool {
    RECORDING.load(Ordering::SeqCst)
}

fn poke() {
    let tid = TID.load(Ordering::SeqCst);
    if tid != 0 {
        unsafe {
            let _ = PostThreadMessageW(tid, WM_REFRESH, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn start() {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("hooks".into())
        .spawn(move || unsafe {
            let mut msg = MSG::default();
            let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
            TID.store(thread_id(), Ordering::SeqCst);
            let _ = tx.send(());
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                if msg.message == WM_REFRESH {
                    refresh_hooks();
                }
            }
        })
        .unwrap();
    let _ = rx.recv();
}

/// Registers keyboard hotkeys on the host window and hands mouse ones to the hook.
/// Returns which bindings failed (taken by another app).
pub fn apply(host: HWND, keys: &[Option<Binding>; ACTIONS]) -> [bool; ACTIONS] {
    let mut bad = [false; ACTIONS];
    let mut mouse = Vec::new();
    for (i, k) in keys.iter().enumerate() {
        unsafe {
            let _ = UnregisterHotKey(Some(host), i as i32 + 1);
        }
        let Some(b) = k else { continue };
        if b.is_mouse() {
            if mouse.iter().any(|(_, m): &(usize, Binding)| m == b) {
                bad[i] = true;
            } else {
                mouse.push((i, *b));
            }
            continue;
        }
        let mut m = MOD_NOREPEAT;
        if b.mods & k::MOD_CTRL != 0 {
            m |= MOD_CONTROL;
        }
        if b.mods & k::MOD_ALT != 0 {
            m |= MOD_ALT;
        }
        if b.mods & k::MOD_SHIFT != 0 {
            m |= MOD_SHIFT;
        }
        if b.mods & k::MOD_WIN != 0 {
            m |= MOD_WIN;
        }
        bad[i] = unsafe { RegisterHotKey(Some(host), i as i32 + 1, m, b.vk as u32).is_err() };
    }
    *MOUSE_BINDS.lock().unwrap() = mouse;
    poke();
    bad
}

pub fn unregister_all(host: HWND) {
    for i in 0..ACTIONS {
        unsafe {
            let _ = UnregisterHotKey(Some(host), i as i32 + 1);
        }
    }
}

pub fn start_recording() {
    RECORDING.store(true, Ordering::SeqCst);
    poke();
}

pub fn stop_recording() {
    RECORDING.store(false, Ordering::SeqCst);
    poke();
}

fn finish(msg: u32, w: usize, l: isize) {
    RECORDING.store(false, Ordering::SeqCst);
    post(msg, w, l);
    poke();
}

unsafe extern "system" fn mouse_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code != HC_ACTION as i32 {
        return CallNextHookEx(None, code, wp, lp);
    }
    let ms = &*(lp.0 as *const MSLLHOOKSTRUCT);
    let xbtn = ((ms.mouseData >> 16) & 0xFFFF) as u16;
    let xvk = if xbtn == XBUTTON1 { k::VK_XBUTTON1 } else { k::VK_XBUTTON2 };
    let (vk, down) = match wp.0 as u32 {
        WM_MBUTTONDOWN => (k::VK_MBUTTON, true),
        WM_MBUTTONUP => (k::VK_MBUTTON, false),
        WM_XBUTTONDOWN => (xvk, true),
        WM_XBUTTONUP => (xvk, false),
        _ => return CallNextHookEx(None, code, wp, lp),
    };
    if !down && SWALLOW_UP.load(Ordering::SeqCst) == vk {
        SWALLOW_UP.store(0, Ordering::SeqCst);
        return LRESULT(1);
    }
    if RECORDING.load(Ordering::SeqCst) {
        if down {
            SWALLOW_UP.store(vk, Ordering::SeqCst);
            finish(WM_RECORDED, vk as usize, async_mods() as isize);
        }
        return LRESULT(1);
    }
    if down {
        let mods = async_mods();
        let hit = MOUSE_BINDS.lock().unwrap().iter().find(|(_, b)| b.vk == vk && b.mods == mods).map(|(a, _)| *a);
        if let Some(a) = hit {
            HELD.lock().unwrap().push((vk, a));
            post(WM_BIND_DOWN, a, 0);
            return LRESULT(1);
        }
    } else {
        let mut held = HELD.lock().unwrap();
        if let Some(pos) = held.iter().position(|(v, _)| *v == vk) {
            let (_, a) = held.remove(pos);
            post(WM_BIND_UP, a, 0);
            return LRESULT(1);
        }
    }
    CallNextHookEx(None, code, wp, lp)
}
