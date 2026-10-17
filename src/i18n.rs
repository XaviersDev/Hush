use crate::{AppWindow, Tr};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/strings.rs"));

pub struct Lang {
    pub code: &'static str,
    pub name: &'static str,
    /// CJK text gets a system font that has the glyphs and the right shapes
    pub font: &'static str,
}

pub const LANGS: [Lang; 8] = [
    Lang { code: "ru", name: "Русский", font: "" },
    Lang { code: "en", name: "English", font: "" },
    Lang { code: "zh", name: "简体中文", font: "Microsoft YaHei UI" },
    Lang { code: "ja", name: "日本語", font: "Yu Gothic UI" },
    Lang { code: "ko", name: "한국어", font: "Malgun Gothic" },
    Lang { code: "de", name: "Deutsch", font: "" },
    Lang { code: "es", name: "Español", font: "" },
    Lang { code: "uk", name: "Українська", font: "" },
];

static CUR: AtomicUsize = AtomicUsize::new(1);

pub fn set(i: usize) {
    CUR.store(i.min(LANGS.len() - 1), Ordering::SeqCst);
}

pub fn current() -> usize {
    CUR.load(Ordering::SeqCst)
}

pub fn index_of(code: &str) -> Option<usize> {
    LANGS.iter().position(|l| l.code == code)
}

static FONT_CHECK: OnceLock<fn(&str) -> bool> = OnceLock::new();

/// Lets the platform say which fonts are installed. Asking for a missing family
/// can crash the renderer on systems with no fallback fonts at all.
pub fn set_font_check(f: fn(&str) -> bool) {
    let _ = FONT_CHECK.set(f);
}

/// Font for a language, or "" when it isn't installed (then the default is used).
pub fn font_of(i: usize) -> &'static str {
    let f = LANGS[i].font;
    if f.is_empty() || FONT_CHECK.get().is_none_or(|ok| ok(f)) {
        f
    } else {
        ""
    }
}

pub fn tr(key: usize) -> &'static str {
    TABLE[current()][key]
}

pub fn apply(ui: &AppWindow) {
    let font = font_of(current());
    let t = ui.global::<Tr>();
    let all: Vec<SharedString> = TABLE[current()].iter().map(|s| SharedString::from(*s)).collect();
    t.set_s(ModelRc::new(VecModel::from(all)));
    t.set_font(if font.is_empty() { "Inter".into() } else { font.into() });
}
