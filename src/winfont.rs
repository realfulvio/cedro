//! Il carattere dell'edizione (Figtree nella standard, Titillium Web in quella del Comune di Bondeno,
//! entrambi con licenza OFL) incorporato nel programma: si carica all'avvio per il solo processo
//! di Cedro, senza installarlo. Se il caricamento fallisce si ripiega su Segoe UI.

use std::sync::atomic::{AtomicBool, Ordering};
use windows::{core::*, Win32::Graphics::Gdi::AddFontMemResourceEx};

#[cfg(feature = "bondeno")]
static REGULAR: &[u8] = include_bytes!("../assets/fonts/TitilliumWeb-Regular.ttf");
#[cfg(feature = "bondeno")]
static BOLD: &[u8] = include_bytes!("../assets/fonts/TitilliumWeb-Bold.ttf");
#[cfg(feature = "bondeno")]
const FACE: PCWSTR = w!("Titillium Web");
#[cfg(not(feature = "bondeno"))]
static REGULAR: &[u8] = include_bytes!("../assets/fonts/Figtree-Regular.ttf");
#[cfg(not(feature = "bondeno"))]
static BOLD: &[u8] = include_bytes!("../assets/fonts/Figtree-Bold.ttf");
#[cfg(not(feature = "bondeno"))]
const FACE: PCWSTR = w!("Figtree");
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
        FACE
    } else {
        w!("Segoe UI")
    }
}
