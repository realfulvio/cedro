//! Avanzamento dei lavori lunghi: chi esegue il lavoro registra una funzione per il proprio thread
//! (`con`), il motore la chiama con la frazione fatta (`report`). Fuori da `con` non succede niente.

use std::cell::RefCell;

thread_local! {
    static SINK: RefCell<Option<Box<dyn Fn(f32)>>> = const { RefCell::new(None) };
}

/// Esegue `f` facendo arrivare l'avanzamento a `sink` (0.0..=1.0).
pub fn con<T>(sink: impl Fn(f32) + 'static, f: impl FnOnce() -> T) -> T {
    SINK.with(|s| *s.borrow_mut() = Some(Box::new(sink)));
    let r = f();
    SINK.with(|s| *s.borrow_mut() = None);
    r
}

/// Dice quanta parte del lavoro è fatta.
pub fn report(frazione: f32) {
    SINK.with(|s| {
        if let Some(k) = s.borrow().as_ref() {
            k(frazione.clamp(0.0, 1.0));
        }
    });
}
