//! Cedro per Android, prototipo: una NativeActivity senza Java né Kotlin. `ui` disegna e decide (e si può
//! provare anche sul computer, vedi `examples/vista.rs`); `glue` è la parte Android.

pub mod ui;

#[cfg(target_os = "android")]
mod glue;
