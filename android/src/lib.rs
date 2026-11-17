//! Cedro per Android, prototipo: una NativeActivity senza Java né Kotlin. `ui` disegna e decide (e si può
//! provare anche sul computer, vedi `examples/vista.rs`); `glue` è la parte Android.

pub mod ui;
pub mod labs;
pub mod worker;
pub mod web;
pub mod storage;
#[cfg(any(target_os = "android", test))]
mod picker;
pub mod publication;

#[cfg(target_os = "android")]
mod glue;
