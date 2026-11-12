//! Le voci di laboratorio sui vetrini (.svs): copia anonima, tessere per l'AI, conta mitotica e
//! consulto. Il lavoro vero sta in `svs_lab`; qui i nomi dei file e i messaggi.

use std::path::Path;

use super::{err, reserve, stem, CedroResult, Options, Output};

/// Le voci che valgono solo per i vetrini.
pub const SOLO_VETRINI: &[&str] = &["tiles", "mitosi", "consult"];

/// `None` se `action` non è una voce dei vetrini (per «strip» solo se il file è un vetrino).
pub fn perform(path: &Path, action: &str, dir: &Path, opts: &Options) -> Option<CedroResult<Output>> {
    let vetrino = crate::svs::is_slide(path);
    if !SOLO_VETRINI.contains(&action) && !(action == "strip" && vetrino) {
        return None;
    }
    if !vetrino {
        return Some(Err(err("serve un vetrino digitale (.svs)")));
    }
    let report = |p: f32| crate::progress::report(p);
    Some((|| match action {
        "strip" => {
            // Il nome del file spesso è quello del paziente o del proprietario: la copia prende il codice del caso.
            let codice = crate::svs_lab::codice_caso(path);
            let out = reserve(dir, &format!("{codice}-anonimo"), "svs")?;
            let tolti = crate::svs_lab::copia_anonima(path, &out).map_err(err)?;
            let message = if tolti.is_empty() { format!("Vetrino copiato come {codice}: non c'era niente da togliere") } else { format!("Vetrino anonimo {codice}: tolti {}", tolti.join(", ")) };
            Ok(Output { path: out, message })
        }
        "tiles" => {
            // Anche le tessere e la cartella hanno il codice del caso, non il nome del file.
            let base = crate::svs_lab::codice_caso(path);
            let mut cartella = dir.join(format!("{base}-tessere"));
            let mut n = 2;
            while cartella.exists() {
                cartella = dir.join(format!("{base}-tessere ({n})"));
                n += 1;
            }
            let f = crate::svs_lab::tessere(path, &cartella, &base, &opts.tessere, &report).map_err(err)?;
            let um = f.um_per_px.map(|u| format!(" a {} µm per pixel", format!("{u:.2}").replace('.', ","))).unwrap_or_default();
            let mut message = format!("{} tessere da {} pixel{um}, con l'elenco in tessere.csv", f.scritte, opts.tessere.lato);
            if f.scartate > 0 {
                message.push_str(&format!(" ({} di vetro vuoto lasciate fuori)", f.scartate));
            }
            Ok(Output { path: cartella, message })
        }
        "mitosi" => {
            let out = reserve(dir, &format!("{}-conta-mitotica", stem(path)), "jpg")?;
            let dove = out.with_file_name(format!("{}-dove.jpg", out.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()));
            report(0.1);
            let c = crate::svs_lab::conta_mitotica(path, &out, &dove).map_err(err)?;
            report(1.0);
            let um = format!("{:.2}", c.um_per_px).replace('.', ",");
            Ok(Output {
                path: out,
                message: format!("Zona di 2,37 mm² in 10 campi numerati ({um} µm per pixel), suggerita dove ci sono più nuclei (non dove ci sono più mitosi: Cedro non le conta); la panoramica con la zona è accanto. Verifica la zona al microscopio e conta le mitosi nei 10 campi: è un aiuto, non una diagnosi."),
            })
        }
        "consult" => {
            let codice = crate::svs_lab::codice_caso(path);
            let out = reserve(dir, &format!("{codice}-consulto"), "zip")?;
            let (_, tolti) = crate::svs_lab::consulto(path, &out, &codice, &report).map_err(err)?;
            Ok(Output {
                path: out,
                message: format!("Consulto {codice}: vetrino anonimo (tolti {}), panoramica e scheda da compilare, con le impronte SHA-256", tolti.join(", ")),
            })
        }
        _ => Err(err("voce sconosciuta")),
    })())
}

#[cfg(test)]
mod prove {
    use super::super::{perform_with, Options};
    use std::path::Path;

    #[test]
    fn le_voci_dei_vetrini_dal_motore() {
        let d = std::env::temp_dir().join(format!("cedro-svslab-motore-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let src = d.join("Rossi Fido.svs");
        std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/svs/cmu1-pezzo.svs"), &src).unwrap();
        let out = perform_with(&src, "strip", 80, None, &Options::default()).unwrap();
        let nome = out.path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(nome.starts_with("caso-") && nome.ends_with("-anonimo.svs") && !nome.contains("Rossi") && out.message.starts_with("Vetrino anonimo caso-"), "{nome} {}", out.message);
        let o = Options { tessere: crate::svs_lab::Tessere { lato: 64, ingrandimento: 0, ..Default::default() }, ..Options::default() };
        let out = perform_with(&src, "tiles", 80, None, &o).unwrap();
        assert!(out.path.is_dir() && out.path.join("tessere.csv").exists(), "{}", out.message);
        // Né la cartella né le tessere hanno il nome del file.
        assert!(!out.path.to_string_lossy().contains("Rossi"), "{}", out.path.display());
        let csv = std::fs::read_to_string(out.path.join("tessere.csv")).unwrap();
        assert!(!csv.contains("Rossi") && csv.lines().nth(1).is_some_and(|l| l.starts_with("caso-")), "{csv}");
        let out = perform_with(&src, "consult", 80, None, &Options::default()).unwrap();
        let nome = out.path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(nome.starts_with("caso-") && nome.ends_with("-consulto.zip") && !nome.contains("Rossi"), "{nome}");
        let v = crate::pacchetto::verifica(&out.path, &|_| {}).unwrap();
        assert!(v.tutto_bene() && v.integri == 3, "{v:?}");
        // Un'immagine qualsiasi non è un vetrino.
        let png = d.join("foto.png");
        image::RgbImage::new(4, 4).save(&png).unwrap();
        assert!(perform_with(&png, "mitosi", 80, None, &Options::default()).err().is_some_and(|e| e.0.contains("vetrino")));
        let _ = std::fs::remove_dir_all(&d);
    }
}
