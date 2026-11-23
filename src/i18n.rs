//! Interface language: English (default) or German.
//!
//! Texts are written as `tr!("Deutsch", "English")`; formatted texts as
//! `trf!("Bild {}", "Frame {}", n)`. The choice is saved per user in
//! `%APPDATA%\Animora3D\settings.txt` (or the platform equivalent).

use std::sync::atomic::{AtomicBool, Ordering};

static GERMAN: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    English,
    German,
}

impl Lang {
    pub fn label(self) -> &'static str {
        match self {
            Lang::English => "English",
            Lang::German => "Deutsch",
        }
    }
}

pub fn lang() -> Lang {
    if GERMAN.load(Ordering::Relaxed) { Lang::German } else { Lang::English }
}

pub fn is_en() -> bool {
    lang() == Lang::English
}

pub fn set_lang(l: Lang) {
    GERMAN.store(l == Lang::German, Ordering::Relaxed);
}

fn settings_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))?;
    Some(base.join("Animora3D").join("settings.txt"))
}

/// Reads the saved language (English when nothing is saved).
pub fn load_saved() {
    // ANIMORA_LANG=de / en overrides the saved choice (handy for testing)
    if let Ok(v) = std::env::var("ANIMORA_LANG") {
        set_lang(if v.trim().eq_ignore_ascii_case("de") { Lang::German } else { Lang::English });
        return;
    }
    let l = settings_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| if t.lines().any(|l| l.trim() == "lang=de") { Lang::German } else { Lang::English })
        .unwrap_or(Lang::English);
    set_lang(l);
}

/// Switches the language and remembers it for the next start.
pub fn save(l: Lang) {
    set_lang(l);
    if let Some(p) = settings_path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, if l == Lang::German { "lang=de\n" } else { "lang=en\n" });
    }
}

/// Picks the German or English text.
#[macro_export]
macro_rules! tr {
    ($de:expr, $en:expr) => {
        if $crate::i18n::is_en() { $en } else { $de }
    };
}

/// Formats the German or English text with the same arguments.
#[macro_export]
macro_rules! trf {
    ($de:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::i18n::is_en() { format!($en $(, $arg)*) } else { format!($de $(, $arg)*) }
    };
}
