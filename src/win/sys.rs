use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::OnceLock;
use tiny_skia::Pixmap;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Media::Audio::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::System::Recovery::*;
use windows::Win32::System::Registry::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const HOST_CLASS: PCWSTR = w!("Hush.Host");

pub const WM_TRAY: u32 = WM_APP + 1;
pub const WM_AUDIO: u32 = WM_APP + 2;
pub const WM_DEVICES: u32 = WM_APP + 3;
pub const WM_SHOW: u32 = WM_APP + 4;
pub const WM_BIND_DOWN: u32 = WM_APP + 5;
pub const WM_BIND_UP: u32 = WM_APP + 6;
pub const WM_RECORDED: u32 = WM_APP + 7;
pub const WM_REC_CANCEL: u32 = WM_APP + 8;
pub const WM_REC_CLEAR: u32 = WM_APP + 9;
/// Sent by the installer. wParam 1 = turn the mic back on before quitting.
pub const WM_EXIT: u32 = WM_APP + 10;

static HOST: AtomicIsize = AtomicIsize::new(0);

pub fn set_host(h: HWND) {
    HOST.store(h.0 as isize, Ordering::SeqCst);
}

pub fn host() -> HWND {
    HWND(HOST.load(Ordering::SeqCst) as *mut _)
}

/// Safe to call from any thread.
pub fn post(msg: u32, w: usize, l: isize) {
    let h = host();
    if !h.is_invalid() {
        unsafe {
            let _ = PostMessageW(Some(h), msg, WPARAM(w), LPARAM(l));
        }
    }
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn fill_wide(dst: &mut [u16], s: &str) {
    let n = dst.len() - 1;
    let mut i = 0;
    for c in s.encode_utf16().take(n) {
        dst[i] = c;
        i += 1;
    }
    dst[i] = 0;
}

/// true when this is the only running copy
pub fn single_instance() -> bool {
    unsafe {
        let h = CreateMutexW(None, false, w!("Local\\Hush.Instance"));
        let exists = GetLastError() == ERROR_ALREADY_EXISTS;
        match h {
            Ok(h) if !exists => {
                let _ = h;
                true
            }
            _ => false,
        }
    }
}

pub fn wake_existing() {
    unsafe {
        if let Ok(h) = FindWindowW(HOST_CLASS, PCWSTR::null()) {
            let _ = AllowSetForegroundWindow(ASFW_ANY);
            let _ = PostMessageW(Some(h), WM_SHOW, WPARAM(0), LPARAM(0));
        }
    }
}

/// Dark native menus (Windows 10 1903+). Undocumented uxtheme ordinals, ignored if missing.
pub fn dark_menus() {
    unsafe {
        let Ok(ux) = LoadLibraryW(w!("uxtheme.dll")) else { return };
        if let Some(f) = GetProcAddress(ux, PCSTR(135usize as *const u8)) {
            let f: extern "system" fn(i32) -> i32 = std::mem::transmute(f);
            f(2);
        }
        if let Some(f) = GetProcAddress(ux, PCSTR(136usize as *const u8)) {
            let f: extern "system" fn() = std::mem::transmute(f);
            f();
        }
    }
}

/// Windows restarts us after a crash or hang, and after an update reboot.
pub fn restart_on_crash() {
    unsafe {
        let _ = RegisterApplicationRestart(w!("--tray"), REGISTER_APPLICATION_RESTART_FLAGS(0));
    }
}

pub fn style_window(hwnd: HWND) {
    unsafe {
        let on: BOOL = TRUE;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &on as *const _ as _, 4);
        let caption: u32 = 0x0010_0E0E;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_CAPTION_COLOR, &caption as *const _ as _, 4);
        let border: u32 = 0x0024_2220;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &border as *const _ as _, 4);
        let text: u32 = 0x00E1_E7EC;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_TEXT_COLOR, &text as *const _ as _, 4);
    }
}

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const APPROVED_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run");

pub fn set_autostart(on: bool) {
    unsafe {
        if on {
            let Ok(exe) = std::env::current_exe() else { return };
            let cmd = wide(&format!("\"{}\" --tray", exe.display()));
            let _ = RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                w!("Hush"),
                REG_SZ.0,
                Some(cmd.as_ptr() as _),
                (cmd.len() * 2) as u32,
            );
            // re-enable if it was switched off in Task Manager
            let enabled: [u8; 12] = [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
            let _ = RegSetKeyValueW(
                HKEY_CURRENT_USER,
                APPROVED_KEY,
                w!("Hush"),
                REG_BINARY.0,
                Some(enabled.as_ptr() as _),
                enabled.len() as u32,
            );
        } else {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, w!("Hush"));
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, APPROVED_KEY, w!("Hush"));
        }
    }
}

pub fn open_url(url: &str) {
    let u = wide(url);
    unsafe {
        ShellExecuteW(None, w!("open"), PCWSTR(u.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

fn tick(f0: f32, f1: f32) -> Vec<u8> {
    let rate = 44100u32;
    let n = (rate as f32 * 0.075) as usize;
    let mut pcm = Vec::with_capacity(n * 2);
    let mut phase = 0.0f32;
    for i in 0..n {
        let t = i as f32 / rate as f32;
        let k = i as f32 / n as f32;
        let f = f0 + (f1 - f0) * k;
        phase += 2.0 * std::f32::consts::PI * f / rate as f32;
        let attack = (t / 0.004).min(1.0);
        let env = attack * (-t * 55.0).exp();
        let s = (phase.sin() * 0.8 + (phase * 2.0).sin() * 0.2) * env * 0.22;
        pcm.extend_from_slice(&((s * 32767.0) as i16).to_le_bytes());
    }
    let mut wav = Vec::with_capacity(44 + pcm.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    wav.extend_from_slice(&pcm);
    wav
}

static SOUNDS: OnceLock<(Vec<u8>, Vec<u8>)> = OnceLock::new();

pub fn click_sound(muted: bool) {
    let (on, off) = SOUNDS.get_or_init(|| (tick(820.0, 1180.0), tick(700.0, 430.0)));
    let data = if muted { off } else { on };
    unsafe {
        let _ = PlaySoundW(PCWSTR(data.as_ptr() as *const u16), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
}

pub fn hicon(pm: &Pixmap) -> Option<HICON> {
    let (w, h) = (pm.width() as i32, pm.height() as i32);
    unsafe {
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let color = CreateDIBSection(None, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        let dst = std::slice::from_raw_parts_mut(bits as *mut u8, (w * h * 4) as usize);
        for (d, s) in dst.chunks_exact_mut(4).zip(pm.data().chunks_exact(4)) {
            let a = s[3] as u32;
            let un = |c: u8| if a == 0 { 0 } else { ((c as u32 * 255 + a / 2) / a).min(255) as u8 };
            d[0] = un(s[2]);
            d[1] = un(s[1]);
            d[2] = un(s[0]);
            d[3] = s[3];
        }
        let mask = CreateBitmap(w, h, 1, 1, None);
        let info = ICONINFO { fIcon: TRUE, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
        let icon = CreateIconIndirect(&info).ok();
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
        icon
    }
}

pub fn font_exists(name: &str) -> bool {
    unsafe extern "system" fn found(_: *const LOGFONTW, _: *const TEXTMETRICW, _: u32, l: LPARAM) -> i32 {
        *(l.0 as *mut bool) = true;
        0
    }
    let mut lf = LOGFONTW { lfCharSet: DEFAULT_CHARSET, ..Default::default() };
    fill_wide(&mut lf.lfFaceName, name);
    let mut hit = false;
    unsafe {
        let dc = GetDC(None);
        EnumFontFamiliesExW(dc, &lf, Some(found), LPARAM(&mut hit as *mut bool as isize), 0);
        ReleaseDC(None, dc);
    }
    hit
}

pub fn instance() -> HINSTANCE {
    unsafe { GetModuleHandleW(None).map(|m| m.into()).unwrap_or_default() }
}

pub fn thread_id() -> u32 {
    unsafe { GetCurrentThreadId() }
}
