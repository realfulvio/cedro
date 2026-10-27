//! Prove della traduzione in inglese: stanno in un file a parte perché la lingua è una sola per tutto il processo.
use cedro::i18n::{lit, set, tr, Lang};

#[test]
fn user_text_is_never_translated() {
    set(Lang::En);
    let shown = tr(&format!("{} e altri 3", lit("Foto e altri ricordi.jpg"))).into_owned();
    assert_eq!(shown, "Foto e altri ricordi.jpg and 3 more");
    assert_eq!(tr(&lit("Foto e altri ricordi.jpg")), "Foto e altri ricordi.jpg");
}
