use std::fmt::Write as _;
use std::path::PathBuf;

const LANGS: [&str; 8] = ["ru", "en", "zh", "ja", "ko", "de", "es", "uk"];

/// Reads i18n/strings.txt and writes a Slint global (tr.slint) plus a Rust table (strings.rs),
/// both in the same key order, so the UI and the code can't drift apart.
fn gen_strings(out: &PathBuf) {
    let src = std::fs::read_to_string("i18n/strings.txt").unwrap();
    let mut keys: Vec<(String, Vec<Option<String>>)> = Vec::new();
    for (n, line) in src.lines().enumerate() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(k) = line.strip_prefix('@') {
            keys.push((k.trim().to_string(), vec![None; LANGS.len()]));
            continue;
        }
        let (code, text) = line.split_once(' ').unwrap_or_else(|| panic!("strings.txt:{}: bad line", n + 1));
        let li = LANGS.iter().position(|l| *l == code).unwrap_or_else(|| panic!("strings.txt:{}: unknown lang {code}", n + 1));
        let entry = &mut keys.last_mut().unwrap_or_else(|| panic!("strings.txt:{}: text before key", n + 1)).1;
        entry[li] = Some(text.replace("\\n", "\n"));
    }
    for (k, v) in &keys {
        for (i, t) in v.iter().enumerate() {
            assert!(t.is_some(), "strings.txt: {k} has no {} text", LANGS[i]);
        }
    }

    let mut slint = String::from("export global Tr {\n    in property <[string]> s;\n    in property <string> font: \"Inter\";\n");
    for (i, (k, _)) in keys.iter().enumerate() {
        let _ = writeln!(slint, "    out property <string> {}: s[{i}];", k.replace('_', "-"));
    }
    slint.push_str("}\n");
    std::fs::write(out.join("tr.slint"), slint).unwrap();

    let mut rs = String::new();
    let _ = writeln!(rs, "pub const KEYS: usize = {};", keys.len());
    let _ = writeln!(rs, "pub mod k {{");
    for (i, (k, _)) in keys.iter().enumerate() {
        let _ = writeln!(rs, "    pub const {}: usize = {i};", k.to_uppercase());
    }
    let _ = writeln!(rs, "}}");
    let _ = writeln!(rs, "pub static TABLE: [[&str; KEYS]; {}] = [", LANGS.len());
    for li in 0..LANGS.len() {
        let _ = writeln!(rs, "    [");
        for (_, v) in &keys {
            let _ = writeln!(rs, "        {:?},", v[li].as_ref().unwrap());
        }
        let _ = writeln!(rs, "    ],");
    }
    let _ = writeln!(rs, "];");
    std::fs::write(out.join("strings.rs"), rs).unwrap();
}

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    println!("cargo:rerun-if-changed=i18n/strings.txt");
    gen_strings(&out);
    slint_build::compile_with_config(
        "ui/app.slint",
        slint_build::CompilerConfiguration::new().with_include_paths(vec![out.clone()]),
    )
    .unwrap();
    println!("cargo:rerun-if-changed=assets");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/app.ico");
        res.set_manifest_file("assets/app.manifest");
        res.set("ProductName", "Hush");
        res.set("FileDescription", "Hush");
        res.set("CompanyName", "AlliSighs");
        res.set("LegalCopyright", "© 2026 AlliSighs");
        res.compile().unwrap();
    }
}
