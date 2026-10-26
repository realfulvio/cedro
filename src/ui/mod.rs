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
