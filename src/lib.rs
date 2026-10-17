pub mod config;
pub mod i18n;
pub mod icons;
pub mod keys;

#[cfg(windows)]
pub mod win;

slint::include_modules!();
