mod audio;
mod input;
mod overlay;
pub mod sys;
mod tray;

use crate::config::{self, Config, Target};
use crate::keys::{self, Binding, ACTIONS, A_LOCK, A_MENU, A_OVERLAY, A_PTT, A_TOGGLE, A_WINDOW};
use crate::i18n::{self, k, tr};
use crate::{icons, AppState, AppWindow, DeviceItem, KeyItem};
use slint::winit_030::{winit, WinitWindowAccessor};
use slint::{ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use sys::*;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::*;

const T_TOPMOST: usize = 1;
const T_PTT_POLL: usize = 2;
const T_PTT_RELEASE: usize = 3;
const T_DEVICES: usize = 4;
const T_RECORD: usize = 5;
const T_SAVE: usize = 6;

const M_TOGGLE: usize = 10;
const M_OVERLAY: usize = 11;
const M_LOCK: usize = 12;
const M_OPEN: usize = 13;
const M_QUIT: usize = 14;
const M_HIDE_OVERLAY: usize = 15;

static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

pub enum Cmd {
    Init(bool),
    Toggle,
    Audio(bool),
    Devices,
    Hotkey(usize),
    BindDown(usize),
    BindUp(usize),
    Timer(usize),
    Tray(u32),
    Overlay(overlay::Ev),
    Show,
    WindowClosed,
    Quit,
    Exit(bool),
    PickDevice(usize),
    PickLang(usize),
    SetBool(String, bool),
    SetInt(String, i32),
    SetFloat(String, f32),
    RecordKey(usize),
    ClearKey(usize),
    Recorded(u16, u8),
    RecordCancel,
    RecordClear,
    ResetOverlay,
    TaskbarSettings,
    TaskbarCreated,
    DisplayChanged,
    SaveNow,
}

enum Menu {
    Tray,
    Overlay,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    static QUEUE: RefCell<VecDeque<Cmd>> = const { RefCell::new(VecDeque::new()) };
    static BUSY: Cell<bool> = const { Cell::new(false) };
}

fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.borrow_mut().as_mut().map(f))
}

/// Everything that touches app state goes through here. Calls made while a
/// command is running (window messages sent during a modal menu, nested
/// callbacks) are queued and run right after, so state is never borrowed twice.
pub fn dispatch(c: Cmd) {
    QUEUE.with(|q| q.borrow_mut().push_back(c));
    if BUSY.get() {
        return;
    }
    BUSY.set(true);
    while let Some(c) = QUEUE.with(|q| q.borrow_mut().pop_front()) {
        if let Some(Some(menu)) = with_app(|a| a.handle(c)) {
            let Some((host, items)) = with_app(|a| (a.host, a.menu_items(&menu))) else { continue };
            let id = tray::popup(host, &items);
            if id != 0 {
                with_app(|a| a.menu_command(id));
            }
        }
    }
    BUSY.set(false);
}

unsafe extern "system" fn host_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let cmd = match msg {
        WM_HOTKEY => Cmd::Hotkey(wp.0.wrapping_sub(1)),
        WM_TIMER => Cmd::Timer(wp.0),
        WM_TRAY => Cmd::Tray((lp.0 & 0xFFFF) as u32),
        WM_AUDIO => Cmd::Audio(wp.0 == 1),
        WM_DEVICES => Cmd::Devices,
        WM_SHOW => Cmd::Show,
        WM_BIND_DOWN => Cmd::BindDown(wp.0),
        WM_BIND_UP => Cmd::BindUp(wp.0),
        WM_RECORDED => Cmd::Recorded(wp.0 as u16, lp.0 as u8),
        WM_REC_CANCEL => Cmd::RecordCancel,
        WM_REC_CLEAR => Cmd::RecordClear,
        WM_EXIT => Cmd::Exit(wp.0 == 1),
        WM_DISPLAYCHANGE => Cmd::DisplayChanged,
        WM_SETTINGCHANGE if wp.0 == SPI_SETWORKAREA.0 as usize => Cmd::DisplayChanged,
        WM_QUERYENDSESSION => {
            dispatch(Cmd::SaveNow);
            return LRESULT(1);
        }
        m if m != 0 && m == TASKBAR_CREATED.load(Ordering::SeqCst) => Cmd::TaskbarCreated,
        _ => return DefWindowProcW(h, msg, wp, lp),
    };
    dispatch(cmd);
    LRESULT(0)
}

/// Key recording happens in our own window, no keyboard hook needed.
unsafe extern "system" fn main_subclass(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
    if input::is_recording() {
        match msg {
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let vk = wp.0 as u16;
                if !keys::is_modifier(vk) && vk != 0xE5 && vk != 0xFF {
                    let cmd = match vk {
                        0x1B => Cmd::RecordCancel,
                        0x08 | 0x2E => Cmd::RecordClear,
                        _ => Cmd::Recorded(vk, input::held_mods()),
                    };
                    input::stop_recording();
                    dispatch(cmd);
                }
                return LRESULT(0);
            }
            WM_KEYUP | WM_SYSKEYUP | WM_CHAR | WM_SYSCHAR | WM_DEADCHAR | WM_SYSDEADCHAR => return LRESULT(0),
            _ => {}
        }
    }
    DefSubclassProc(h, msg, wp, lp)
}

fn create_host() -> HWND {
    unsafe {
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(host_proc),
            hInstance: instance(),
            lpszClassName: HOST_CLASS,
            ..Default::default()
        };
        RegisterClassExW(&wc);
        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::SeqCst);
        CreateWindowExW(WS_EX_TOOLWINDOW, HOST_CLASS, w!("Hush"), WS_POPUP, 0, 0, 0, 0, None, None, Some(instance()), None)
            .unwrap_or_default()
    }
}

fn set_timer(h: HWND, id: usize, ms: u32) {
    unsafe {
        SetTimer(Some(h), id, ms, None);
    }
}

fn kill_timer(h: HWND, id: usize) {
    unsafe {
        let _ = KillTimer(Some(h), id);
    }
}

fn to_image(p: tiny_skia::Pixmap) -> Image {
    let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(p.data(), p.width(), p.height());
    Image::from_rgba8_premultiplied(buf)
}

fn dev_name(d: &audio::Device) -> String {
    if d.name.is_empty() {
        tr(k::MICROPHONE).to_string()
    } else {
        d.name.clone()
    }
}

fn full_name(d: &audio::Device) -> String {
    let name = dev_name(d);
    if d.adapter.is_empty() || d.adapter == name {
        name
    } else {
        format!("{} ({})", name, d.adapter)
    }
}

struct App {
    ui: AppWindow,
    cfg: Config,
    host: HWND,
    audio: Option<audio::Audio>,
    overlay: overlay::Overlay,
    tray: tray::Tray,
    bad: [bool; ACTIONS],
    recording: Option<usize>,
    ptt: Option<u16>,
    want: Option<bool>,
    muted: bool,
    has_mic: bool,
    window_shown: bool,
    window_styled: bool,
    targets: Vec<Target>,
    ticks: u32,
}

impl App {
    fn handle(&mut self, c: Cmd) -> Option<Menu> {
        match c {
            Cmd::Init(show) => self.init(show),
            Cmd::Toggle => self.toggle(),
            Cmd::Audio(external) => self.on_audio(external),
            Cmd::Devices => set_timer(self.host, T_DEVICES, 300),
            Cmd::Hotkey(a) => return self.action(a, 0),
            Cmd::BindDown(a) => return self.action(a, u16::MAX),
            Cmd::BindUp(a) => {
                if a == A_PTT {
                    self.ptt_end();
                }
            }
            Cmd::Timer(id) => self.timer(id),
            Cmd::Tray(ev) => match ev {
                WM_LBUTTONUP => {
                    if self.cfg.tray_click == 0 {
                        self.toggle();
                    } else {
                        self.toggle_window();
                    }
                }
                0x0401 => self.show_window(),
                WM_CONTEXTMENU => return Some(Menu::Tray),
                _ => {}
            },
            Cmd::Overlay(ev) => match self.overlay.handle(ev) {
                overlay::Outcome::Toggle => self.toggle(),
                overlay::Outcome::Menu => return Some(Menu::Overlay),
                overlay::Outcome::Moved => self.overlay_moved(),
                overlay::Outcome::Nothing => {}
            },
            Cmd::Show => self.show_window(),
            Cmd::WindowClosed => self.window_closed(),
            Cmd::Quit => self.quit(),
            Cmd::Exit(unmute) => {
                if unmute && self.has_mic {
                    if let Some(a) = &self.audio {
                        a.set_muted(false);
                    }
                }
                self.quit();
            }
            Cmd::PickDevice(i) => {
                if let Some(t) = self.targets.get(i).cloned() {
                    self.cfg.target = t;
                    self.refresh_devices(false);
                    self.save_later();
                }
            }
            Cmd::PickLang(i) => {
                if let Some(l) = i18n::LANGS.get(i) {
                    self.cfg.lang = l.code.to_string();
                    self.apply_lang();
                    self.save_later();
                }
            }
            Cmd::SetBool(k, v) => self.set_bool(&k, v),
            Cmd::SetInt(k, v) => self.set_int(&k, v),
            Cmd::SetFloat(k, v) => {
                if k == "ov-opacity" {
                    self.cfg.overlay.opacity = v.clamp(0.2, 1.0);
                    self.apply_overlay();
                    self.save_later();
                }
            }
            Cmd::RecordKey(i) => {
                if self.recording == Some(i) {
                    self.stop_recording();
                } else if i < ACTIONS {
                    // free our own hotkeys so the press reaches the window
                    input::unregister_all(self.host);
                    self.recording = Some(i);
                    input::start_recording();
                    set_timer(self.host, T_RECORD, 10_000);
                    self.publish_keys();
                }
            }
            Cmd::ClearKey(i) => {
                if i < ACTIONS {
                    self.cfg.keys[i] = None;
                    self.stop_recording();
                    self.apply_keys();
                }
            }
            Cmd::Recorded(vk, mods) => {
                if let Some(i) = self.recording {
                    let b = Binding { mods, vk };
                    for (j, k) in self.cfg.keys.iter_mut().enumerate() {
                        if j != i && *k == Some(b) {
                            *k = None;
                        }
                    }
                    self.cfg.keys[i] = Some(b);
                    self.stop_recording();
                    self.apply_keys();
                }
            }
            Cmd::RecordCancel => self.stop_recording(),
            Cmd::RecordClear => {
                if let Some(i) = self.recording {
                    self.cfg.keys[i] = None;
                    self.stop_recording();
                    self.apply_keys();
                }
            }
            Cmd::ResetOverlay => {
                self.overlay.reset();
                self.overlay_moved();
                self.apply_overlay();
            }
            Cmd::TaskbarSettings => open_url("ms-settings:taskbar"),
            Cmd::TaskbarCreated => {
                self.tray.refresh_dpi();
                self.tray.update(self.muted, self.has_mic);
            }
            Cmd::DisplayChanged => {
                if self.overlay.fix_position() {
                    self.overlay_moved();
                }
            }
            Cmd::SaveNow => config::save(&self.cfg),
        }
        None
    }

    fn init(&mut self, show: bool) {
        if self.cfg.autostart {
            set_autostart(true);
        }
        let k = overlay::scale();
        let px = (36.0 * k).round() as u32;
        let s = self.ui.global::<AppState>();
        let on: Vec<Image> = (0..icons::STYLE_COUNT).map(|i| to_image(icons::render(i, false, px))).collect();
        let off: Vec<Image> = (0..icons::STYLE_COUNT).map(|i| to_image(icons::render(i, true, px))).collect();
        s.set_styles_on(ModelRc::new(VecModel::from(on)));
        s.set_styles_off(ModelRc::new(VecModel::from(off)));
        s.set_version(env!("CARGO_PKG_VERSION").into());

        let lang = i18n::index_of(&self.cfg.lang).unwrap_or_else(system_lang);
        i18n::set(lang);
        i18n::apply(&self.ui);
        self.publish_langs();

        self.bad = input::apply(self.host, &self.cfg.keys);
        self.refresh_devices(true);
        if self.has_mic {
            self.want = Some(self.muted);
        }
        self.publish_settings();
        self.publish_keys();
        self.tray.add();
        self.tray.update(self.muted, self.has_mic);
        self.apply_overlay();
        set_timer(self.host, T_TOPMOST, 2000);
        if show {
            self.show_window();
        }
    }

    // state

    fn read_state(&mut self) {
        let st = self.audio.as_ref().and_then(|a| a.muted());
        let (has, muted) = match st {
            Some(m) => (true, m),
            None => (false, true),
        };
        let changed = has != self.has_mic || muted != self.muted;
        self.has_mic = has;
        self.muted = muted;
        if changed {
            let s = self.ui.global::<AppState>();
            s.set_muted(muted);
            s.set_has_mic(has);
            self.tray.update(muted, has);
            self.overlay.set_muted(muted);
            self.overlay.set_visible(self.overlay_visible());
        }
    }

    fn set_muted(&mut self, m: bool) {
        if !self.has_mic {
            return;
        }
        let Some(a) = &self.audio else { return };
        let was = self.muted;
        a.set_muted(m);
        self.want = Some(m);
        self.read_state();
        if self.cfg.sound && was != self.muted {
            click_sound(self.muted);
        }
    }

    fn toggle(&mut self) {
        self.set_muted(!self.muted);
    }

    fn on_audio(&mut self, external: bool) {
        self.read_state();
        if !external || !self.has_mic {
            return;
        }
        match self.want {
            Some(w) if self.cfg.hold_state && w != self.muted => {
                if let Some(a) = &self.audio {
                    a.set_muted(w);
                }
                self.read_state();
            }
            _ => self.want = Some(self.muted),
        }
    }

    fn refresh_devices(&mut self, first: bool) {
        if self.audio.is_none() {
            self.audio = audio::Audio::new().ok();
        }
        let Some(a) = &mut self.audio else {
            self.read_state();
            return;
        };
        a.refresh(&self.cfg.target);
        if !first {
            if let (Some(w), Some(cur)) = (self.want, a.muted()) {
                if w != cur {
                    a.set_muted(w);
                }
            }
        }
        self.publish_devices();
        self.read_state();
        // force a UI sync on the first pass
        if first {
            let s = self.ui.global::<AppState>();
            s.set_muted(self.muted);
            s.set_has_mic(self.has_mic);
        }
    }

    fn publish_devices(&mut self) {
        let Some(a) = &self.audio else { return };
        let mut items = Vec::new();
        let mut targets = Vec::new();
        let n = a.devices.len();
        let all_sub = match n {
            0 => tr(k::NOTHING_CONNECTED).to_string(),
            1 => full_name(&a.devices[0]),
            _ => tr(k::CONNECTED_N).replace("{}", &n.to_string()),
        };
        items.push(DeviceItem { name: tr(k::ALL_MICS).into(), sub: all_sub.into(), selected: self.cfg.target == Target::All, font: "".into() });
        targets.push(Target::All);
        let def = a.default_id.as_deref().and_then(|id| a.name_of(id));
        let def_name = def.map(full_name).unwrap_or_else(|| tr(k::NONE).into());
        items.push(DeviceItem {
            name: tr(k::DEFAULT_MIC).into(),
            sub: def_name.clone().into(),
            selected: self.cfg.target == Target::Default,
            font: "".into(),
        });
        targets.push(Target::Default);
        for d in &a.devices {
            let t = Target::Device(d.id.clone());
            let name = dev_name(d);
            let sub = if d.adapter == name { String::new() } else { d.adapter.clone() };
            items.push(DeviceItem { name: name.into(), sub: sub.into(), selected: self.cfg.target == t, font: "".into() });
            targets.push(t);
        }
        let (title, sub) = match &self.cfg.target {
            Target::All => {
                let names: Vec<String> = a.devices.iter().map(full_name).collect();
                (tr(k::ALL_MICS).to_string(), if names.is_empty() { tr(k::NOTHING_CONNECTED).into() } else { names.join(", ") })
            }
            Target::Default => (tr(k::DEFAULT_MIC).to_string(), def_name),
            Target::Device(id) => match a.name_of(id) {
                Some(d) => (dev_name(d), d.adapter.clone()),
                None => {
                    items.push(DeviceItem {
                        name: tr(k::SELECTED_MIC).into(),
                        sub: tr(k::DISCONNECTED).into(),
                        selected: true,
                        font: "".into(),
                    });
                    targets.push(self.cfg.target.clone());
                    (tr(k::MICROPHONE).to_string(), tr(k::DISCONNECTED).to_string())
                }
            },
        };
        let s = self.ui.global::<AppState>();
        s.set_devices(ModelRc::new(VecModel::from(items)));
        s.set_target_title(title.into());
        s.set_target_sub(sub.into());
        self.targets = targets;
    }

    fn publish_langs(&self) {
        let cur = i18n::current();
        let items: Vec<DeviceItem> = i18n::LANGS
            .iter()
            .enumerate()
            .map(|(i, l)| DeviceItem { name: l.name.into(), sub: "".into(), selected: i == cur, font: i18n::font_of(i).into() })
            .collect();
        let s = self.ui.global::<AppState>();
        s.set_languages(ModelRc::new(VecModel::from(items)));
        s.set_lang_name(i18n::LANGS[cur].name.into());
        let names: Vec<SharedString> = (0..icons::STYLE_COUNT).map(|i| SharedString::from(icons::style_name(i))).collect();
        s.set_style_names(ModelRc::new(VecModel::from(names)));
    }

    fn apply_lang(&mut self) {
        i18n::set(i18n::index_of(&self.cfg.lang).unwrap_or_else(system_lang));
        i18n::apply(&self.ui);
        self.publish_langs();
        self.publish_devices();
        self.publish_keys();
        self.tray.update(self.muted, self.has_mic);
    }

    // hotkeys

    fn action(&mut self, a: usize, from_mouse: u16) -> Option<Menu> {
        match a {
            A_TOGGLE => self.toggle(),
            A_PTT => {
                let vk = if from_mouse == u16::MAX { 0 } else { self.cfg.keys[A_PTT].map(|b| b.vk).unwrap_or(0) };
                self.ptt_start(vk);
            }
            A_OVERLAY => {
                self.cfg.overlay.enabled = !self.cfg.overlay.enabled;
                self.apply_overlay();
                self.publish_settings();
                self.save_later();
            }
            A_LOCK => {
                self.cfg.overlay.locked = !self.cfg.overlay.locked;
                self.apply_overlay();
                self.publish_settings();
                self.save_later();
            }
            A_WINDOW => self.toggle_window(),
            A_MENU => return Some(Menu::Tray),
            _ => {}
        }
        None
    }

    fn ptt_start(&mut self, vk: u16) {
        kill_timer(self.host, T_PTT_RELEASE);
        if self.ptt.is_none() {
            self.ptt = Some(vk);
            self.set_muted(false);
            if vk != 0 {
                set_timer(self.host, T_PTT_POLL, 20);
            }
        }
    }

    fn ptt_end(&mut self) {
        if self.ptt.take().is_some() {
            kill_timer(self.host, T_PTT_POLL);
            set_timer(self.host, T_PTT_RELEASE, 150);
        }
    }

    fn timer(&mut self, id: usize) {
        match id {
            T_TOPMOST => {
                self.overlay.keep_on_top();
                self.ticks = self.ticks.wrapping_add(1);
                if self.ticks % 5 == 0 && self.audio.as_ref().is_some_and(|a| !a.healthy()) {
                    // audio service restarted or endpoints went stale
                    self.audio = None;
                    self.refresh_devices(false);
                }
            }
            T_PTT_POLL => {
                if let Some(vk) = self.ptt {
                    if vk == 0 || !input::key_down(vk) {
                        self.ptt_end();
                    }
                } else {
                    kill_timer(self.host, T_PTT_POLL);
                }
            }
            T_PTT_RELEASE => {
                kill_timer(self.host, T_PTT_RELEASE);
                if self.ptt.is_none() {
                    self.set_muted(true);
                }
            }
            T_DEVICES => {
                kill_timer(self.host, T_DEVICES);
                self.refresh_devices(false);
            }
            T_RECORD => self.stop_recording(),
            T_SAVE => {
                kill_timer(self.host, T_SAVE);
                config::save(&self.cfg);
            }
            _ => {}
        }
    }

    fn stop_recording(&mut self) {
        kill_timer(self.host, T_RECORD);
        input::stop_recording();
        if self.recording.take().is_some() {
            self.bad = input::apply(self.host, &self.cfg.keys);
            self.publish_keys();
        }
    }

    fn apply_keys(&mut self) {
        self.bad = input::apply(self.host, &self.cfg.keys);
        self.publish_keys();
        self.save_later();
    }

    fn publish_keys(&self) {
        let items: Vec<KeyItem> = (0..ACTIONS)
            .map(|i| KeyItem {
                title: keys::action_title(i).into(),
                hint: if self.bad[i] {
                    tr(k::KEY_TAKEN).into()
                } else {
                    keys::action_hint(i).into()
                },
                value: self.cfg.keys[i].map(|b| b.label()).unwrap_or_default().into(),
                recording: self.recording == Some(i),
                bad: self.bad[i],
            })
            .collect();
        let s = self.ui.global::<AppState>();
        s.set_keys(ModelRc::new(VecModel::from(items)));
        s.set_toggle_key(self.cfg.keys[A_TOGGLE].map(|b| b.label()).unwrap_or_default().into());
    }

    // settings

    fn publish_settings(&self) {
        let s = self.ui.global::<AppState>();
        let o = &self.cfg.overlay;
        s.set_ov_enabled(o.enabled);
        s.set_ov_style(o.style as i32);
        s.set_ov_click(o.click as i32);
        s.set_ov_show(o.show_when as i32);
        s.set_ov_locked(o.locked);
        s.set_ov_opacity(o.opacity);
        s.set_ov_hide_capture(o.hide_capture);
        s.set_autostart(self.cfg.autostart);
        s.set_sound(self.cfg.sound);
        s.set_tray_click(self.cfg.tray_click as i32);
        s.set_tray_style(self.cfg.tray_style as i32);
        s.set_hold_state(self.cfg.hold_state);
    }

    fn set_bool(&mut self, k: &str, v: bool) {
        match k {
            "ov-enabled" => self.cfg.overlay.enabled = v,
            "ov-locked" => self.cfg.overlay.locked = v,
            "ov-hide-capture" => self.cfg.overlay.hide_capture = v,
            "autostart" => {
                self.cfg.autostart = v;
                set_autostart(v);
            }
            "sound" => {
                self.cfg.sound = v;
                if v {
                    click_sound(self.muted);
                }
            }
            "hold-state" => {
                self.cfg.hold_state = v;
                if v && self.has_mic {
                    self.want = Some(self.muted);
                }
            }
            _ => return,
        }
        if k.starts_with("ov-") {
            self.apply_overlay();
        }
        self.save_later();
    }

    fn set_int(&mut self, k: &str, v: i32) {
        match k {
            "ov-style" => self.cfg.overlay.style = (v.max(0) as usize) % icons::STYLE_COUNT,
            "ov-click" => self.cfg.overlay.click = v.clamp(0, 3) as u8,
            "ov-show" => self.cfg.overlay.show_when = v.clamp(0, 2) as u8,
            "tray-click" => self.cfg.tray_click = v.clamp(0, 1) as u8,
            "tray-style" => {
                self.cfg.tray_style = v.clamp(0, 1) as u8;
                self.tray.set_style(self.cfg.tray_style);
                self.tray.update(self.muted, self.has_mic);
            }
            _ => return,
        }
        if k.starts_with("ov-") {
            self.apply_overlay();
        }
        self.save_later();
    }

    fn save_later(&self) {
        set_timer(self.host, T_SAVE, 400);
    }

    // overlay

    fn overlay_visible(&self) -> bool {
        let o = &self.cfg.overlay;
        o.enabled
            && match o.show_when {
                1 => self.muted,
                2 => !self.muted,
                _ => true,
            }
    }

    fn apply_overlay(&mut self) {
        let vis = self.overlay_visible();
        self.overlay.configure(&self.cfg.overlay, self.muted, vis);
    }

    fn overlay_moved(&mut self) {
        let o = &mut self.cfg.overlay;
        o.x = self.overlay.x;
        o.y = self.overlay.y;
        o.size = self.overlay.size;
        o.placed = true;
        self.save_later();
    }

    // window

    fn show_window(&mut self) {
        let k = overlay::scale();
        if !self.window_styled {
            let mut wa = RECT::default();
            unsafe {
                let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut wa as *mut _ as _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
            }
            let (w, h) = ((380.0 * k) as i32, (700.0 * k) as i32);
            let x = (wa.left + wa.right) / 2 - w / 2;
            let y = ((wa.top + wa.bottom) / 2 - h / 2).max(wa.top);
            self.ui.window().set_position(slint::PhysicalPosition::new(x, y));
        }
        let _ = self.ui.show();
        self.window_shown = true;
        self.window_styled = true;
        self.ui.window().with_winit_window(|w: &winit::window::Window| {
            prepare_window(w);
            w.set_minimized(false);
            w.focus_window();
        });
    }

    fn hide_window(&mut self) {
        let _ = self.ui.hide();
        self.window_closed();
    }

    fn window_closed(&mut self) {
        self.window_shown = false;
        self.stop_recording();
        if !self.cfg.tray_hint_shown {
            self.cfg.tray_hint_shown = true;
            self.tray.balloon(tr(k::BALLOON_TITLE), tr(k::BALLOON_TEXT));
            self.save_later();
        }
    }

    fn toggle_window(&mut self) {
        let focused = self.ui.window().with_winit_window(|w: &winit::window::Window| w.has_focus()).unwrap_or(false);
        if self.window_shown && focused {
            self.hide_window();
        } else {
            self.show_window();
        }
    }

    fn quit(&mut self) {
        config::save(&self.cfg);
        let _ = slint::quit_event_loop();
    }

    // menus

    fn menu_items(&self, m: &Menu) -> Vec<tray::Item> {
        use tray::{item, Item, SEP};
        match m {
            Menu::Tray => vec![
                Item {
                    disabled: !self.has_mic,
                    ..item(M_TOGGLE, if self.muted { tr(k::MENU_UNMUTE) } else { tr(k::MENU_MUTE) })
                },
                item(SEP, ""),
                Item { checked: self.cfg.overlay.enabled, ..item(M_OVERLAY, tr(k::OVERLAY)) },
                Item { checked: self.cfg.overlay.locked, disabled: !self.cfg.overlay.enabled, ..item(M_LOCK, tr(k::ACT_LOCK)) },
                item(SEP, ""),
                Item { bold: true, ..item(M_OPEN, tr(k::MENU_OPEN)) },
                item(M_QUIT, tr(k::QUIT)),
            ],
            Menu::Overlay => vec![
                Item { checked: self.cfg.overlay.locked, ..item(M_LOCK, tr(k::LOCK)) },
                item(M_HIDE_OVERLAY, tr(k::MENU_HIDE_OVERLAY)),
                item(SEP, ""),
                item(M_OPEN, tr(k::MENU_OPEN)),
            ],
        }
    }

    fn menu_command(&mut self, id: usize) {
        match id {
            M_TOGGLE => self.toggle(),
            M_OVERLAY | M_HIDE_OVERLAY => {
                self.cfg.overlay.enabled = id == M_OVERLAY && !self.cfg.overlay.enabled;
                self.apply_overlay();
                self.publish_settings();
                self.save_later();
            }
            M_LOCK => {
                self.cfg.overlay.locked = !self.cfg.overlay.locked;
                self.apply_overlay();
                self.publish_settings();
                self.save_later();
            }
            M_OPEN => {
                let s = self.ui.global::<AppState>();
                if !self.window_shown {
                    s.set_page(0);
                }
                self.show_window();
            }
            M_QUIT => self.quit(),
            _ => {}
        }
    }
}

thread_local! {
    static PREPARED: Cell<isize> = const { Cell::new(0) };
}

/// Dark caption, fixed size and key capture. The native window only exists once
/// the event loop runs, so this is called from the first window event too.
fn prepare_window(w: &winit::window::Window) {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(RawWindowHandle::Win32(h)) = w.window_handle().map(|h| h.as_raw()) else { return };
    let raw = h.hwnd.get();
    if PREPARED.get() == raw {
        return;
    }
    PREPARED.set(raw);
    let hwnd = HWND(raw as *mut _);
    w.set_resizable(false);
    w.set_enabled_buttons(winit::window::WindowButtons::CLOSE | winit::window::WindowButtons::MINIMIZE);
    w.set_theme(Some(winit::window::Theme::Dark));
    style_window(hwnd);
    unsafe {
        let _ = SetWindowSubclass(hwnd, Some(main_subclass), 1, 0);
    }
}

/// Windows UI language, falls back to English.
fn system_lang() -> usize {
    let id = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() } & 0x3FF;
    let code = match id {
        0x19 => "ru",
        0x04 => "zh",
        0x11 => "ja",
        0x12 => "ko",
        0x07 => "de",
        0x0A => "es",
        0x22 => "uk",
        _ => "en",
    };
    i18n::index_of(code).unwrap_or(1)
}

fn wire(ui: &AppWindow) {
    ui.window().on_winit_window_event(|win, _| {
        win.with_winit_window(prepare_window);
        slint::winit_030::EventResult::Propagate
    });
    let s = ui.global::<AppState>();
    s.on_toggle_mic(|| dispatch(Cmd::Toggle));
    s.on_pick_device(|i| dispatch(Cmd::PickDevice(i.max(0) as usize)));
    s.on_pick_lang(|i| dispatch(Cmd::PickLang(i.max(0) as usize)));
    s.on_set_bool(|k, v| dispatch(Cmd::SetBool(k.to_string(), v)));
    s.on_set_int(|k, v| dispatch(Cmd::SetInt(k.to_string(), v)));
    s.on_set_float(|k, v| dispatch(Cmd::SetFloat(k.to_string(), v)));
    s.on_record_key(|i| dispatch(Cmd::RecordKey(i.max(0) as usize)));
    s.on_clear_key(|i| dispatch(Cmd::ClearKey(i.max(0) as usize)));
    s.on_reset_overlay(|| dispatch(Cmd::ResetOverlay));
    s.on_open_taskbar_settings(|| dispatch(Cmd::TaskbarSettings));
    s.on_quit(|| dispatch(Cmd::Quit));
    ui.window().on_close_requested(|| {
        dispatch(Cmd::WindowClosed);
        slint::CloseRequestResponse::HideWindow
    });
}

pub fn run() {
    let tray_start = std::env::args().any(|a| a == "--tray");
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    if !single_instance() {
        if !tray_start {
            wake_existing();
        }
        return;
    }
    dark_menus();
    restart_on_crash();
    i18n::set_font_check(font_exists);

    let cfg = config::load();
    let ui = match AppWindow::new() {
        Ok(ui) => ui,
        Err(_) => return,
    };
    wire(&ui);

    let host = create_host();
    set_host(host);
    input::start();
    let overlay = overlay::Overlay::new(&cfg.overlay, false);
    let tray = tray::Tray::new(host, cfg.tray_style, false);

    let app = App {
        ui: ui.clone_strong(),
        cfg,
        host,
        audio: None,
        overlay,
        tray,
        bad: [false; ACTIONS],
        recording: None,
        ptt: None,
        want: None,
        muted: false,
        has_mic: true,
        window_shown: false,
        window_styled: false,
        targets: Vec::new(),
        ticks: 0,
    };
    APP.with(|a| *a.borrow_mut() = Some(app));
    dispatch(Cmd::Init(!tray_start));

    let _ = slint::run_event_loop_until_quit();

    let app = APP.with(|a| a.borrow_mut().take());
    if let Some(mut app) = app {
        config::save(&app.cfg);
        input::unregister_all(app.host);
        app.tray.remove();
        app.audio = None;
    }
    drop(ui);
}
