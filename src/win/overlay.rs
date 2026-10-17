use super::sys::*;
use super::{dispatch, Cmd};
use crate::{config, icons};
use std::cell::Cell;
use tiny_skia::{LineCap, Paint, PathBuilder, Stroke, Transform};
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const CLASS: PCWSTR = w!("Hush.Overlay");
const MIN: i32 = 18;

const MAX: i32 = 360;

#[derive(Clone, Copy, Debug)]
pub enum Ev {
    Move,
    LDown,
    LUp,
    RUp,
    MUp,
    Wheel(i16),
    Leave,
    CaptureLost,
}

pub enum Outcome {
    Nothing,
    Toggle,
    Menu,
    Moved,
}

thread_local! {
    static LOCKED: Cell<bool> = const { Cell::new(false) };
    static CLICK: Cell<u8> = const { Cell::new(0) };
    static SIZE: Cell<i32> = const { Cell::new(40) };
}

#[derive(Clone, Copy)]
struct Drag {
    resize: bool,
    sx: i32,
    sy: i32,
    x0: i32,
    y0: i32,
    s0: i32,
    moved: bool,
}

pub struct Overlay {
    pub hwnd: HWND,
    pub x: i32,
    pub y: i32,
    pub size: i32,
    style: usize,
    muted: bool,
    opacity: f32,
    locked: bool,
    click: u8,
    capture_hidden: bool,
    hover: bool,
    shown: bool,
    drag: Option<Drag>,
}

fn grip(s: i32) -> i32 {
    ((s as f32 * 0.32) as i32).max(10)
}

fn in_grip(px: i32, py: i32, s: i32) -> bool {
    let g = grip(s);
    px >= s - g && py >= s - g
}

fn cursor() -> POINT {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    p
}

unsafe extern "system" fn wndproc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let ev = match msg {
        WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
        WM_SETCURSOR => {
            let mut p = cursor();
            let _ = ScreenToClient(h, &mut p);
            let s = SIZE.get();
            let id = if !LOCKED.get() {
                if in_grip(p.x, p.y, s) {
                    IDC_SIZENWSE
                } else {
                    IDC_SIZEALL
                }
            } else if CLICK.get() != 0 {
                IDC_HAND
            } else {
                IDC_ARROW
            };
            if let Ok(c) = LoadCursorW(None, id) {
                SetCursor(Some(c));
            }
            return LRESULT(1);
        }
        WM_MOUSEMOVE => Ev::Move,
        WM_LBUTTONDOWN => Ev::LDown,
        WM_LBUTTONUP => Ev::LUp,
        WM_RBUTTONUP => Ev::RUp,
        WM_MBUTTONUP => Ev::MUp,
        WM_MOUSEWHEEL => Ev::Wheel(((wp.0 >> 16) & 0xFFFF) as u16 as i16),
        0x02A3 => Ev::Leave,
        WM_CAPTURECHANGED => Ev::CaptureLost,
        WM_RBUTTONDOWN | WM_MBUTTONDOWN => return LRESULT(0),
        _ => return DefWindowProcW(h, msg, wp, lp),
    };
    dispatch(Cmd::Overlay(ev));
    LRESULT(0)
}

pub fn scale() -> f32 {
    unsafe { GetDpiForSystem() as f32 / 96.0 }
}

fn work_area() -> RECT {
    let mut r = RECT::default();
    unsafe {
        let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut r as *mut _ as _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
    }
    r
}

impl Overlay {
    pub fn new(cfg: &config::Overlay, muted: bool) -> Self {
        unsafe {
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(wndproc),
                hInstance: instance(),
                lpszClassName: CLASS,
                ..Default::default()
            };
            RegisterClassExW(&wc);
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                CLASS,
                w!("Hush"),
                WS_POPUP,
                0,
                0,
                40,
                40,
                None,
                None,
                Some(instance()),
                None,
            )
            .unwrap_or_default();
            let mut o = Overlay {
                hwnd,
                x: cfg.x,
                y: cfg.y,
                size: cfg.size,
                style: cfg.style,
                muted,
                opacity: cfg.opacity,
                locked: cfg.locked,
                click: cfg.click,
                capture_hidden: false,
                hover: false,
                shown: false,
                drag: None,
            };
            if !cfg.placed || !o.on_screen() {
                o.reset();
            }
            o
        }
    }

    fn on_screen(&self) -> bool {
        if self.size < MIN || self.size > MAX {
            return false;
        }
        let c = POINT { x: self.x + self.size / 2, y: self.y + self.size / 2 };
        unsafe { !MonitorFromPoint(c, MONITOR_DEFAULTTONULL).is_invalid() }
    }

    pub fn reset(&mut self) {
        let k = scale();
        let wa = work_area();
        self.size = (44.0 * k).round() as i32;
        let m = (28.0 * k) as i32;
        self.x = wa.right - self.size - m;
        self.y = wa.bottom - self.size - m;
        if self.shown {
            self.render();
        }
    }

    pub fn fix_position(&mut self) -> bool {
        if !self.on_screen() {
            self.reset();
            return true;
        }
        false
    }

    pub fn configure(&mut self, cfg: &config::Overlay, muted: bool, visible: bool) {
        self.style = cfg.style;
        self.opacity = cfg.opacity;
        self.locked = cfg.locked;
        self.click = cfg.click;
        self.muted = muted;
        LOCKED.set(self.locked);
        CLICK.set(self.click);
        SIZE.set(self.size);
        unsafe {
            let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            let through = self.locked && self.click == 0;
            let want = if through { ex | WS_EX_TRANSPARENT.0 as isize } else { ex & !(WS_EX_TRANSPARENT.0 as isize) };
            if want != ex {
                SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, want);
            }
            if cfg.hide_capture != self.capture_hidden {
                let aff = if cfg.hide_capture { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE };
                if SetWindowDisplayAffinity(self.hwnd, aff).is_ok() {
                    self.capture_hidden = cfg.hide_capture;
                }
            }
        }
        if self.locked {
            self.drag = None;
            self.hover = false;
        }
        self.render();
        self.set_visible(visible);
    }

    pub fn set_muted(&mut self, m: bool) {
        if self.muted != m {
            self.muted = m;
            self.render();
        }
    }

    pub fn set_visible(&mut self, v: bool) {
        if v == self.shown {
            return;
        }
        self.shown = v;
        unsafe {
            if v {
                self.render();
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                self.keep_on_top();
            } else {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
        }
    }

    pub fn keep_on_top(&self) {
        if self.shown && self.drag.is_none() {
            unsafe {
                let _ = SetWindowPos(
                    self.hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
                );
            }
        }
    }

    fn render(&self) {
        let s = self.size.clamp(MIN, MAX) as u32;
        let mut pm = icons::render(self.style, self.muted, s);
        if self.opacity < 0.999 {
            for b in pm.data_mut() {
                *b = (*b as f32 * self.opacity + 0.5) as u8;
            }
        }
        if self.hover && !self.locked {
            let k = s as f32;
            let mut p = Paint::default();
            p.anti_alias = true;
            p.set_color_rgba8(255, 255, 255, 70);
            let frame = icons::rounded_rect(1.0, 1.0, k - 2.0, k - 2.0, k * 0.22);
            pm.stroke_path(&frame, &p, &Stroke { width: 1.5, ..Default::default() }, Transform::identity(), None);
            let g = grip(s as i32) as f32 * 0.7;
            let e = k - 3.5;
            let mut pb = PathBuilder::new();
            pb.move_to(e, e - g);
            pb.line_to(e - g, e);
            pb.move_to(e, e - g * 0.5);
            pb.line_to(e - g * 0.5, e);
            if let Some(path) = pb.finish() {
                let mut dark = Paint::default();
                dark.anti_alias = true;
                dark.set_color_rgba8(0, 0, 0, 120);
                let st = |w| Stroke { width: w, line_cap: LineCap::Round, ..Default::default() };
                pm.stroke_path(&path, &dark, &st(3.6), Transform::identity(), None);
                p.set_color_rgba8(255, 255, 255, 220);
                pm.stroke_path(&path, &p, &st(1.6), Transform::identity(), None);
            }
        }
        if !(self.locked && self.click == 0) {
            // keep the whole square clickable, even between strokes
            for px in pm.data_mut().chunks_exact_mut(4) {
                if px[3] == 0 {
                    px[3] = 1;
                }
            }
        }
        icons::to_bgra(pm.data_mut());
        unsafe {
            let screen = GetDC(None);
            let mem = CreateCompatibleDC(Some(screen));
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: s as i32,
                    biHeight: -(s as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            if let Ok(bmp) = CreateDIBSection(Some(mem), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
                std::ptr::copy_nonoverlapping(pm.data().as_ptr(), bits as *mut u8, pm.data().len());
                let old = SelectObject(mem, bmp.into());
                let blend = BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                };
                let pos = POINT { x: self.x, y: self.y };
                let size = SIZE { cx: s as i32, cy: s as i32 };
                let src = POINT::default();
                let _ = UpdateLayeredWindow(
                    self.hwnd,
                    Some(screen),
                    Some(&pos),
                    Some(&size),
                    Some(mem),
                    Some(&src),
                    COLORREF(0),
                    Some(&blend),
                    ULW_ALPHA,
                );
                SelectObject(mem, old);
                let _ = DeleteObject(bmp.into());
            }
            let _ = DeleteDC(mem);
            ReleaseDC(None, screen);
        }
    }

    fn set_geometry(&mut self, x: i32, y: i32, s: i32) {
        let s = s.clamp(MIN, MAX);
        let resized = s != self.size;
        self.x = x;
        self.y = y;
        self.size = s;
        SIZE.set(s);
        if resized {
            self.render();
        } else {
            unsafe {
                let _ = SetWindowPos(self.hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
            }
        }
    }

    pub fn handle(&mut self, ev: Ev) -> Outcome {
        match ev {
            Ev::Move => {
                if !self.hover && !self.locked {
                    self.hover = true;
                    let mut t = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: self.hwnd,
                        dwHoverTime: 0,
                    };
                    unsafe {
                        let _ = TrackMouseEvent(&mut t);
                    }
                    self.render();
                }
                if let Some(mut d) = self.drag {
                    let p = cursor();
                    let (dx, dy) = (p.x - d.sx, p.y - d.sy);
                    if !d.moved && dx.abs() + dy.abs() > 3 {
                        d.moved = true;
                    }
                    if d.moved {
                        if d.resize {
                            self.set_geometry(d.x0, d.y0, d.s0 + dx.max(dy));
                        } else {
                            self.set_geometry(d.x0 + dx, d.y0 + dy, self.size);
                        }
                    }
                    self.drag = Some(d);
                }
                Outcome::Nothing
            }
            Ev::LDown => {
                if !self.locked {
                    let p = cursor();
                    let resize = in_grip(p.x - self.x, p.y - self.y, self.size);
                    self.drag = Some(Drag { resize, sx: p.x, sy: p.y, x0: self.x, y0: self.y, s0: self.size, moved: false });
                    unsafe {
                        SetCapture(self.hwnd);
                    }
                }
                Outcome::Nothing
            }
            Ev::LUp => {
                if let Some(d) = self.drag.take() {
                    unsafe {
                        let _ = ReleaseCapture();
                    }
                    if d.moved {
                        return Outcome::Moved;
                    }
                    if !d.resize && self.click == 1 {
                        return Outcome::Toggle;
                    }
                    Outcome::Nothing
                } else if self.click == 1 {
                    Outcome::Toggle
                } else {
                    Outcome::Nothing
                }
            }
            Ev::RUp => {
                if self.click == 2 {
                    Outcome::Toggle
                } else {
                    Outcome::Menu
                }
            }
            Ev::MUp => {
                if self.click == 3 {
                    Outcome::Toggle
                } else {
                    Outcome::Nothing
                }
            }
            Ev::Wheel(delta) => {
                if self.locked || delta == 0 {
                    return Outcome::Nothing;
                }
                let f = if delta > 0 { 1.12 } else { 1.0 / 1.12 };
                let s = ((self.size as f32 * f).round() as i32).clamp(MIN, MAX);
                let (cx, cy) = (self.x + self.size / 2, self.y + self.size / 2);
                self.set_geometry(cx - s / 2, cy - s / 2, s);
                Outcome::Moved
            }
            Ev::Leave => {
                if self.hover {
                    self.hover = false;
                    if self.drag.is_none() {
                        self.render();
                    }
                }
                Outcome::Nothing
            }
            Ev::CaptureLost => {
                if let Some(d) = self.drag.take() {
                    if d.moved {
                        return Outcome::Moved;
                    }
                }
                Outcome::Nothing
            }
        }
    }
}
