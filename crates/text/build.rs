//! Optional craft-fonts build input (see storytold/craft-fonts `docs/integration.md` and
//! craftrules `standards/fonts.md`).
//!
//! With `CRAFT_FONTS_DIR=<absolute path of a craft-fonts checkout>` (a relative path resolves from
//! `crates/text`) this embeds every font listed in its
//! `fonts/manifest.txt` as [`CRAFT_FONTS`](crate::fonts::CRAFT_FONTS); unset, `CRAFT_FONTS` is
//! empty and nothing else changes. A `CRAFT_FONTS_DIR` that is not a checkout is a warning, or an
//! error with `CRAFT_FONTS_REQUIRED=1` (release builds).
//!
//! Native builds embed every font. The web (wasm32) build embeds the UI font (BIZ UDPGothic
//! Regular) and Regular Hangul faces, since wasm cannot discover an installed Korean font.
//! Keep the optional input small enough for the web size budget (25 MiB per file).
//! Only the local directory is read: no network.

use std::fmt::Write as _;
use std::path::PathBuf;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=CRAFT_FONTS_DIR");
    println!("cargo::rerun-if-env-changed=CRAFT_FONTS_REQUIRED");
    let mut src = String::from("pub static CRAFT_FONTS: &[CraftFont] = &[\n");
    if let Some(dir) = std::env::var_os("CRAFT_FONTS_DIR").map(PathBuf::from) {
        match craft_fonts(&dir) {
            Ok(entries) => src.push_str(&entries),
            Err(e) if std::env::var_os("CRAFT_FONTS_REQUIRED").is_some() => {
                println!("cargo::error=CRAFT_FONTS_DIR={}: {e}", dir.display());
            }
            Err(e) => println!("cargo::warning=building without craft-fonts: CRAFT_FONTS_DIR={}: {e}", dir.display()),
        }
    }
    src.push_str("];\n");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default()).join("craft_fonts.rs");
    if let Err(e) = std::fs::write(&out, src) {
        println!("cargo::error=writing {}: {e}", out.display());
    }
}

/// One `CraftFont { .. }` initialiser per manifest line.
fn craft_fonts(dir: &std::path::Path) -> Result<String, String> {
    let manifest = dir.join("fonts/manifest.txt");
    println!("cargo::rerun-if-changed={}", manifest.display());
    let text = std::fs::read_to_string(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let wasm = std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32");
    let mut out = String::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let f: Vec<&str> = line.split(" | ").map(str::trim).collect();
        let [family, style, file, scripts, ..] = f.as_slice() else {
            return Err(format!("malformed manifest line: {line}"));
        };
        if wasm && !web_font(family, style, scripts) {
            continue;
        }
        let path = dir.join(file).canonicalize().map_err(|e| format!("{file}: {e}"))?;
        println!("cargo::rerun-if-changed={}", path.display());
        let scripts: Vec<String> = scripts.split(',').map(|s| format!("{:?}", s.trim())).collect();
        let _ = writeln!(
            out,
            "    CraftFont {{ family: {family:?}, style: {style:?}, scripts: &[{}], bytes: include_bytes!({:?}) }},",
            scripts.join(", "),
            path.display().to_string(),
        );
    }
    Ok(out)
}

fn web_font(family: &str, style: &str, scripts: &str) -> bool {
    style == "Regular" && (family == "BIZ UDPGothic" || scripts.split(',').map(str::trim).any(|s| matches!(s, "Hang" | "Kore")))
}

#[cfg(test)]
mod tests {
    use super::web_font;

    #[test]
    fn web_keeps_japanese_ui_and_korean_regular_faces() {
        assert!(web_font("BIZ UDPGothic", "Regular", "Jpan"));
        assert!(web_font("Noto Sans CJK KR", "Regular", "Hang, Latn"));
        assert!(web_font("Noto Sans KR", "Regular", "Kore"));
    }

    #[test]
    fn web_does_not_embed_every_cjk_weight_or_family() {
        assert!(!web_font("Noto Sans CJK KR", "Bold", "Hang"));
        assert!(!web_font("BIZ UDPGothic", "Bold", "Jpan"));
        assert!(!web_font("Noto Sans CJK SC", "Regular", "Hans"));
        assert!(!web_font("Unrelated", "Regular", "Hangul"));
    }
}
