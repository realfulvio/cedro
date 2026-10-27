//! La nuova interfaccia della versione standard: cinque temi (disposizioni) in due modalità,
//! disegnati in Rust puro e identici su Windows e Linux. Qui c'è tutto quello che si vede e come
//! risponde ai clic; le finestre di sistema (`ui_win`, `linux_ui2`) copiano l'immagine sullo
//! schermo e passano mouse, tastiera e file trascinati.

pub mod assets;
pub mod gfx;
pub mod svgpath;
pub mod theme;
pub mod media;
pub mod model;
pub mod core;
pub mod screen;
pub mod views;

/// Le tavole sono 1536×1024: nelle finestre vere si disegna un po' più piccolo, così stanno in uno schermo normale.
pub const ZOOM: f32 = 0.85;
