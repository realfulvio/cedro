//! I caratteri delle finestre (Figtree, e Titillium Web con il tema Bondeno, entrambi con licenza OFL) incorporati nel programma: si carica all'avvio per il solo processo
//! di Cedro, senza installarlo. Se il caricamento fallisce si ripiega su Segoe UI.

use std::sync::atomic::{AtomicBool, Ordering};
use windows::{core::*, Win32::Graphics::Gdi::AddFontMemResourceEx};

static REGULAR: &[u8] = include_bytes!("../assets/fonts/Figtree-Regular.ttf");
static BOLD: &[u8] = include_bytes!("../assets/fonts/Figtree-Bold.ttf");
static TITILLIUM_REGULAR: &[u8] = include_bytes!("../assets/fonts/TitilliumWeb-Regular.ttf");
static TITILLIUM_BOLD: &[u8] = include_bytes!("../assets/fonts/TitilliumWeb-Bold.ttf");
static LOADED: AtomicBool = AtomicBool::new(false);

pub fn load() {
    let mut ok = true;
    for data in [REGULAR, BOLD, TITILLIUM_REGULAR, TITILLIUM_BOLD] {
        let mut count = 0u32;
        let handle = unsafe { AddFontMemResourceEx(data.as_ptr().cast(), data.len() as u32, None, &mut count) };
        ok &= !handle.is_invalid() && count > 0;
    }
    LOADED.store(ok, Ordering::Relaxed);
}

pub fn face() -> PCWSTR {
    if LOADED.load(Ordering::Relaxed) {
        if cedro::edizione::bondeno() { w!("Titillium Web") } else { w!("Figtree") }
    } else {
        w!("Segoe UI")
    }
}
