//! Titillium Web (licenza OFL) incorporato nel programma: come su Windows si carica all'avvio per il solo
//! processo di Cedro, senza installarlo. Si registra in fontconfig (che Pango e Cairo usano) da file
//! scritti nella cartella della cache; se qualcosa manca si ripiega sul carattere del desktop.

use std::ffi::{c_char, c_int, c_void, CString};
use std::path::PathBuf;

static REGULAR: &[u8] = include_bytes!("../assets/fonts/TitilliumWeb-Regular.ttf");
static BOLD: &[u8] = include_bytes!("../assets/fonts/TitilliumWeb-Bold.ttf");

pub const FACE: &str = "Titillium Web";

fn cache_dir() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CACHE_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(std::env::var_os("HOME")?).join(".cache"),
    };
    Some(base.join("cedro/fonts"))
}

/// Registra i due file. Va chiamata prima di aprire qualunque finestra; vale `true` se il carattere è disponibile.
pub fn load() -> bool {
    let Some(dir) = cache_dir() else { return false };
    if std::fs::create_dir_all(&dir).is_err() {
        return false;
    }
    let lib = unsafe { libc::dlopen(c"libfontconfig.so.1".as_ptr(), libc::RTLD_NOW) };
    if lib.is_null() {
        return false;
    }
    let add = unsafe { libc::dlsym(lib, c"FcConfigAppFontAddFile".as_ptr()) };
    if add.is_null() {
        return false;
    }
    let add: unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int = unsafe { std::mem::transmute(add) };
    let mut ok = true;
    for (name, data) in [("TitilliumWeb-Regular.ttf", REGULAR), ("TitilliumWeb-Bold.ttf", BOLD)] {
        let path = dir.join(name);
        if std::fs::metadata(&path).ok().map(|m| m.len() as usize) != Some(data.len()) && std::fs::write(&path, data).is_err() {
            ok = false;
            continue;
        }
        let Ok(c_path) = CString::new(path.to_string_lossy().as_bytes()) else { ok = false; continue };
        ok &= unsafe { add(std::ptr::null_mut(), c_path.as_ptr()) } != 0;
    }
    ok
}

/// Rende Titillium Web il carattere di tutte le finestre GTK di Cedro, finestre di dialogo e menu compresi.
pub fn apply() {
    if let Some(settings) = gtk4::Settings::default() {
        settings.set_gtk_font_name(Some(&format!("{FACE} 11")));
    }
}

/// Le finestre di Cedro sono chiare come su Windows, anche con il tema scuro di KDE o GNOME:
/// Adwaita chiaro sotto il CSS di Cedro, così elenchi, menu a tendina e pulsanti non diventano neri.
pub fn light_theme() {
    if let Some(settings) = gtk4::Settings::default() {
        settings.set_gtk_theme_name(Some("Adwaita"));
        settings.set_gtk_application_prefer_dark_theme(false);
    }
}
