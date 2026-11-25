//! I file delle traduzioni: nella lingua d'arrivo il segnaposto è sempre `{}` (il `{#}`, che vuole solo numeri,
//! vale solo nel testo italiano a sinistra: scritto a destra resta com'è e finisce sullo schermo).

use std::path::Path;

#[test]
fn nelle_traduzioni_il_segnaposto_a_destra_e_sempre_graffe_vuote() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("i18n");
    let mut sbagliate: Vec<String> = Vec::new();
    for lingua in ["en", "de", "es", "fr", "pt"] {
        let testo = std::fs::read_to_string(dir.join(format!("{lingua}.tsv"))).unwrap();
        for (n, riga) in testo.lines().enumerate() {
            if riga.starts_with('#') {
                continue;
            }
            if let Some((it, tr)) = riga.split_once('\t') {
                if tr.contains("{#}") {
                    sbagliate.push(format!("{lingua}.tsv riga {}: «{it}»", n + 1));
                }
            }
        }
    }
    assert!(sbagliate.is_empty(), "{} righe da correggere:\n{}", sbagliate.len(), sbagliate.join("\n"));
}
