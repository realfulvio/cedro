//! Titillium Web (licenza OFL) incorporato nel programma: si carica all'avvio per il solo
//! processo di Cedro, senza installarlo. Se il caricamento fallisce si ripiega su Segoe UI.

use std::sync::atomic::{AtomicBool, Ordering};
use windows::{core::*, Win32::Graphics::Gdi::AddFontMemResourceEx};

static REGULAR: &[u8] = include_bytes!("../assets/fonts/TitilliumWeb-Regular.ttf");
static BOLD: &[u8] = include_bytes!("../assets/fonts/TitilliumWeb-Bold.ttf");
static LOADED: AtomicBool = AtomicBool::new(false);

pub fn load() {
    let mut ok = true;
    for data in [REGULAR, BOLD] {
        let mut count = 0u32;
        let handle = unsafe { AddFontMemResourceEx(data.as_ptr().cast(), data.len() as u32, None, &mut count) };
        ok &= !handle.is_invalid() && count > 0;
    }
    LOADED.store(ok, Ordering::Relaxed);
}

pub fn face() -> PCWSTR {
    if LOADED.load(Ordering::Relaxed) {
        w!("Titillium Web")
    } else {
        w!("Segoe UI")
    }
}
