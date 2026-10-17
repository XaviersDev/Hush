# Hush

Переключатель микрофона для Windows. Трей, оверлей, горячие клавиши, рация.

Языки: русский, English, 简体中文, 日本語, 한국어, Deutsch, Español, українська. Тексты лежат в `i18n/strings.txt`.

Rust + Slint, без браузера внутри.

Автор: AlliSighs

## Сборка

На Windows:

```
cargo build --release
```

С Linux:

```
rustup target add x86_64-pc-windows-gnu
sudo apt install mingw-w64
cargo build --release --target x86_64-pc-windows-gnu
```

Готовый файл: `target/.../release/Hush.exe`.

Установщик (нужен NSIS):

```
cd installer
makensis hush.nsi
```

Получится `dist/HushSetup.exe`. Ставит в `%LOCALAPPDATA%\Programs\Hush` без прав админа, при повторном запуске обновляет.

Настройки лежат в `%APPDATA%\Hush\config.json`.

`vendor/i-slint-backend-winit` это Slint с одной правкой: некоторые драйверы отдают частоту монитора 0 Гц, и оригинал на этом падает.
