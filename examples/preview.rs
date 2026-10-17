// Offscreen render of the UI to PNG files, for checking the layout without Windows.
use hush::*;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::Rgb8Pixel;
use slint::platform::{Platform, PointerEventButton, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, Image, LogicalPosition, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

thread_local!(static NOW: Cell<u64> = const { Cell::new(0) });

struct P(Rc<MinimalSoftwareWindow>);
impl Platform for P {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
    fn duration_since_start(&self) -> Duration {
        Duration::from_millis(NOW.with(|n| n.get()))
    }
}

fn advance(ms: u64) {
    for _ in 0..(ms / 50).max(1) {
        NOW.with(|n| n.set(n.get() + 50));
        slint::platform::update_timers_and_animations();
    }
}

fn img(p: tiny_skia::Pixmap) -> Image {
    let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(p.data(), p.width(), p.height());
    Image::from_rgba8_premultiplied(buf)
}

fn shot(win: &MinimalSoftwareWindow, dir: &str, name: &str) {
    advance(1200);
    let s = win.size();
    let (w, h) = (s.width as usize, s.height as usize);
    let mut buf = vec![Rgb8Pixel::default(); w * h];
    win.request_redraw();
    win.draw_if_needed(|r| {
        r.render(&mut buf, w);
    });
    let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
    for p in &buf {
        ppm.extend_from_slice(&[p.r, p.g, p.b]);
    }
    let path = format!("{dir}/{name}.ppm");
    std::fs::write(&path, ppm).unwrap();
    std::process::Command::new("convert").args([&path, &format!("{dir}/{name}.png")]).status().unwrap();
    std::fs::remove_file(path).unwrap();
}

fn hover(win: &MinimalSoftwareWindow, x: f32, y: f32) {
    win.dispatch_event(WindowEvent::PointerMoved { position: LogicalPosition::new(x, y) });
    advance(700);
}

#[allow(dead_code)]
fn click(win: &MinimalSoftwareWindow, x: f32, y: f32) {
    let position = LogicalPosition::new(x, y);
    win.dispatch_event(WindowEvent::PointerPressed { position, button: PointerEventButton::Left });
    win.dispatch_event(WindowEvent::PointerReleased { position, button: PointerEventButton::Left });
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let scale: f32 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(1.5);
    let win = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(P(win.clone()))).unwrap();
    let ui = AppWindow::new().unwrap();
    win.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: scale });
    win.set_size(slint::PhysicalSize::new((380.0 * scale) as u32, (700.0 * scale) as u32));
    ui.show().unwrap();

    let s = ui.global::<AppState>();
    s.set_version("1.0".into());
    s.set_target_sub("Микрофон (Realtek(R) Audio), Arctis Nova 7".into());
    s.set_toggle_key("Ctrl + Alt + M".into());
    s.set_ov_enabled(true);
    s.set_ov_style(3);
    s.set_ov_click(1);
    s.set_ov_opacity(0.85);
    s.set_autostart(true);
    s.set_sound(true);
    let on: Vec<Image> = (0..icons::STYLE_COUNT).map(|i| img(icons::render(i, false, (36.0 * scale) as u32))).collect();
    let off: Vec<Image> = (0..icons::STYLE_COUNT).map(|i| img(icons::render(i, true, (36.0 * scale) as u32))).collect();
    s.set_styles_on(ModelRc::new(VecModel::from(on)));
    s.set_styles_off(ModelRc::new(VecModel::from(off)));
    let lang = std::env::args().nth(3).and_then(|c| i18n::index_of(&c)).unwrap_or(0);
    i18n::set(lang);
    i18n::apply(&ui);
    s.set_lang_name(i18n::LANGS[lang].name.into());
    let langs: Vec<DeviceItem> = i18n::LANGS
        .iter()
        .enumerate()
        .map(|(i, l)| DeviceItem { name: l.name.into(), sub: "".into(), selected: i == lang, font: l.font.into() })
        .collect();
    s.set_languages(ModelRc::new(VecModel::from(langs)));
    s.set_style_names(ModelRc::new(VecModel::from((0..icons::STYLE_COUNT).map(|i| SharedString::from(icons::style_name(i))).collect::<Vec<_>>())));
    s.set_target_title(i18n::tr(i18n::k::ALL_MICS).into());
    let keys: Vec<KeyItem> = (0..keys::ACTIONS)
        .map(|i| KeyItem {
            title: keys::action_title(i).into(),
            hint: keys::action_hint(i).into(),
            value: match i {
                0 => "Ctrl + Alt + M".into(),
                1 => "Мышь 5".into(),
                _ => "".into(),
            },
            recording: i == 2,
            bad: false,
        })
        .collect();
    s.set_keys(ModelRc::new(VecModel::from(keys)));
    let devs = vec![
        DeviceItem { name: "Все микрофоны".into(), sub: "2 подключено".into(), selected: true, font: "".into() },
        DeviceItem { name: "По умолчанию".into(), sub: "Микрофон (Realtek(R) Audio)".into(), selected: false, font: "".into() },
        DeviceItem { name: "Микрофон".into(), sub: "Realtek(R) Audio".into(), selected: false, font: "".into() },
        DeviceItem { name: "Arctis Nova 7 Chat".into(), sub: "SteelSeries Arctis Nova 7".into(), selected: false, font: "".into() },
    ];
    s.set_devices(ModelRc::new(VecModel::from(devs)));

    shot(&win, &dir, "1_home_live");
    s.set_muted(true);
    shot(&win, &dir, "2_home_muted");
    s.set_picker_open(true);
    shot(&win, &dir, "3_picker");
    s.set_picker_open(false);
    s.set_muted(false);
    s.set_page(1);
    shot(&win, &dir, "4_overlay");
    hover(&win, 60.0, 430.0);
    shot(&win, &dir, "4b_overlay_tip");
    s.set_page(2);
    hover(&win, 5.0, 5.0);
    shot(&win, &dir, "5_keys");
    s.set_page(3);
    hover(&win, 60.0, 241.0);
    shot(&win, &dir, "6_settings_tip");
    hover(&win, 5.0, 5.0);
    s.set_lang_open(true);
    shot(&win, &dir, "7_langs");
}
