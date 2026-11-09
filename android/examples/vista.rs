//! Disegna le schermate di Cedro per Android in PNG, sul computer: `cargo run -p cedro-android --example vista -- cartella-uscita`.
use std::path::PathBuf;
use std::time::Instant;

use cedro::ui::gfx::{Fonts, GlyphCache, Painter};
use cedro::ui::theme::ThemeId;
use cedro_android::ui::{App, Archivio, Info, Lavoro, Recente, Scheda, Stato};

fn recenti(app: &mut App) {
    let img = PathBuf::from("docs/hero.png");
    let t = cedro::ui::media::thumb_of(&img, 140);
    for (n, k) in [("verbale.pdf", cedro::Kind::Pdf), ("foto 1.jpg", cedro::Kind::Image), ("foto 2.jpg", cedro::Kind::Image), ("nota.m4a", cedro::Kind::Audio), ("piano.txt", cedro::Kind::Text)] {
        app.recenti.push(Recente { path: img.clone(), nome: n.into(), kind: k, thumb: if k == cedro::Kind::Image { t.clone() } else { None } });
    }
}

fn lavori(app: &mut App) {
    let l = |id, o: &str, e: &str, conv, st| Lavoro { id, origine: o.into(), etichetta: e.into(), conv, inizio: Instant::now(), quando: cedro_android::ui::adesso() - 120 * id as u64, stato: st, prog: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(if id == 1 { 430 } else { 1000 })), sorgente: None, controlli: None };
    let fatto = |n: &str, m: &str| Stato::Fatto { path: "x".into(), nome: n.into(), msg: m.into() };
    let mut app_lavori = vec![
        l(1, "Riunione.m4a", "Word", true, Stato::InCorso),
        l(2, "condiviso.png", "JPG", true, fatto("condiviso.jpg", "282 KB → 94 KB · 66% in meno")),
        l(3, "Verbale.pdf", "Testo", true, fatto("Verbale.txt", "12 pagine")),
        l(4, "foto.png", "Comprimi", false, Stato::Errore("Non ho potuto leggere il file.".into())),
        l(5, "scansione.png", "PNG", true, fatto("scansione.png", "1,2 MB")),
    ];
    // Il primo è una trascrizione, a metà da un minuto: si vede il riquadro con il tempo che resta.
    app_lavori[0].sorgente = Some(PathBuf::from("Riunione.m4a"));
    app_lavori[0].etichetta = "Testo".into();
    app.partenze.insert(1, (Instant::now() - std::time::Duration::from_secs(60), 30));
    app.lavori = app_lavori;
}

fn info() -> Info {
    let r = |k: &str, v: &str| (k.to_string(), v.to_string());
    Info {
        versione: "0.25.0 (2500)".into(),
        generali: vec![r("Telefono", "Xiaomi Mi 9T"), r("Android", "11 (API 30)"), r("Processore", "Snapdragon 730"), r("Core", "8 (2 grandi + 6 piccoli)"), r("Memoria", "6,0 GB (3,1 GB liberi)"), r("Scheda video", "Adreno 618")],
        istruzioni: vec![("NEON".into(), true), ("dotprod".into(), true), ("i8mm".into(), false)],
        accelerazioni: vec![("Trascrizione".into(), "processore, dotprod".into(), true), ("Video MP4".into(), "codificatore del telefono".into(), true)],
    }
}

fn scena(nome: &str, w: f32, h: f32, out: &PathBuf, f: impl FnOnce(&mut App)) {
    let mut app = App::new(ThemeId::WindowsEssenziale);
    app.anim.calmo = true;
    let img = PathBuf::from("docs/hero.png");
    let file = Archivio { path: img.clone(), nome: "Verbale_giunta.png".into(), dim: 2_400_000 };
    app.imposta_file(file, cedro::Kind::Image, false);
    app.miniatura = cedro::ui::media::thumb_of(&img, 600);
    recenti(&mut app);
    f(&mut app);
    let scala = 2.75;
    let mut cache = GlyphCache::new();
    let mut p = Painter::new((w * scala) as u32, (h * scala) as u32, scala, Fonts::current(), &mut cache);
    app.disegna(&mut p, w, h);
    p.save_png(&out.join(format!("{nome}.png"))).unwrap();
}

/// La finestra principale di un tema, con due file in coda e un lavoro in corso.
fn casa(nome: &str, tema: ThemeId, w: f32, h: f32, out: &PathBuf, f: impl FnOnce(&mut App)) {
    let mut app = App::new(tema);
    app.anim.calmo = true;
    app.vai_casa();
    let img = PathBuf::from("docs/hero.png");
    app.aggiungi_coda(vec![(img.clone(), "Verbale_giunta.png".into(), cedro::Kind::Image), (img.clone(), "Foto sopralluogo.png".into(), cedro::Kind::Image)]);
    let t = cedro::ui::media::thumb_of(&img, 140);
    let m = cedro::ui::media::thumb_of(&img, 600);
    app.info_voce(&img, m.or(t), Some((1600, 1200)));
    recenti(&mut app);
    f(&mut app);
    let scala = 2.75;
    let mut cache = GlyphCache::new();
    let mut p = Painter::new((w * scala) as u32, (h * scala) as u32, scala, Fonts::for_theme(tema), &mut cache);
    app.disegna(&mut p, w, h);
    p.save_png(&out.join(format!("{nome}.png"))).unwrap();
}

const DETTO: [(i64, u8, &str); 7] = [
    (4, 0, "Buonasera a tutti. Constatato il numero legale, dichiaro aperta la seduta del Consiglio comunale."),
    (11, 0, "Passiamo al primo punto all'ordine del giorno: approvazione dei verbali della seduta precedente."),
    (19, 1, "I verbali sono stati inviati a tutti i consiglieri il 5 ottobre. Non sono arrivate osservazioni."),
    (27, 0, "Bene. Chi è favorevole? Contrari? Astenuti? Approvato all'unanimità."),
    (36, 2, "Chiedo la parola sul secondo punto, la variazione di bilancio per le scuole di Scortichino."),
    (44, 2, "Vorrei capire da dove arrivano i 120 mila euro e se il cantiere parte prima dell'inverno."),
    (53, 3, "Arrivano dall'avanzo vincolato del 2025. Il cantiere apre il 3 novembre, la ditta è già incaricata."),
];

const RISPOSTA: &str = "## In breve\nSeduta del Consiglio comunale del 12 ottobre, 47 minuti. Approvati all'unanimità i verbali della seduta precedente. Discussa la variazione di bilancio da 120 mila euro per le scuole di Scortichino, poi messa in votazione.\n\n## Chi ha detto cosa\n- Sindaco: Apre la seduta e guida le due votazioni.\n- Segretaria comunale: I verbali sono stati inviati il 5 ottobre, senza osservazioni.\n- Interlocutore 3: Chiede da dove arrivano i 120 mila euro e quando parte il cantiere.\n- Interlocutore 4: Fondi dall'avanzo vincolato 2025; cantiere dal 3 novembre, ditta già incaricata.\n\n## Decisioni\n- Verbali della seduta precedente approvati all'unanimità.\n- Variazione di bilancio per le scuole di Scortichino messa in votazione.\n\n## Da fare\n- Aprire il cantiere delle scuole di Scortichino entro il 3 novembre (Interlocutore 4)\n";

/// «Rivedi» di una riunione con quattro voci: scheda, foglio e riassunto a scelta.
fn rivedi(nome: &str, tema: ThemeId, out: &PathBuf, scheda: u8, foglio: Option<cedro_android::ui::FoglioRiv>, fatto: bool, in_corso: bool) {
    use cedro::trascrizione::Segment;
    let segs: Vec<Segment> = DETTO.iter().map(|(t, v, x)| Segment { t0: t * 100, t1: t * 100 + 680, text: x.to_string(), speaker: Some(*v) }).collect();
    let nomi: Vec<String> = vec!["Sindaco".into(), "Segretaria comunale".into(), String::new(), String::new()];
    let riass = fatto.then(|| cedro::riassunto::Stored { text: RISPOSTA.into(), names: nomi.clone(), ms: 512_000, gpu: None, done: vec![] });
    let r = cedro::ui::review::Review::new(std::path::Path::new("Consiglio-comunale.m4a"), 2832.0, "it".into(), segs, nomi, riass);
    let mut app = App::new(tema);
    app.anim.calmo = true;
    app.apri_rivedi_con(r, "Consiglio-comunale".into(), scheda, foglio, in_corso.then(|| (7, 380, "Legge la riunione: parte 2 di 3".to_string())));
    let (w, h) = (392.0, 800.0);
    let scala = 2.75;
    let mut cache = GlyphCache::new();
    let mut p = Painter::new((w * scala) as u32, (h * scala) as u32, scala, Fonts::for_theme(tema), &mut cache);
    app.disegna(&mut p, w, h);
    p.save_png(&out.join(format!("{nome}.png"))).unwrap();
}

fn main() {
    // LANG_FOTO=en: le schermate in inglese, e a fine corsa l'elenco dei testi senza traduzione.
    let inglese = std::env::var("LANG_FOTO").is_ok_and(|v| v == "en");
    if inglese {
        cedro::i18n::set(cedro::i18n::Lang::En);
        cedro::i18n::record_misses(true);
    }
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "/tmp/vista".into()));
    std::fs::create_dir_all(&out).unwrap();
    let (pw, ph, lw, lh) = (392.0, 800.0, 800.0, 360.0);
    firma(&out, pw, ph);
    scena("foglio", pw, ph, &out, |a| a.foglio = true);
    scena("ruota", pw, ph, &out, |_| {});
    scena("ruota-strumenti", pw, ph, &out, |a| {
        a.modo = 1;
        a.scelta = Some(0);
    });
    scena("ruota-vuota", pw, ph, &out, |a| {
        a.file = None;
        a.miniatura = None;
    });
    scena("scegli", pw, ph, &out, |a| a.scelta_file = true);
    scena("lavori", pw, ph, &out, |a| {
        lavori(a);
        a.scheda = Scheda::Lavori;
    });
    scena("info", pw, ph, &out, |a| {
        a.info = info();
        a.scheda = Scheda::Info;
    });
    scena("aspetto", pw, ph, &out, |a| a.scheda = Scheda::Aspetto);
    scena("scuro-ruota", pw, ph, &out, |a| a.tema = ThemeId::BancoDiLavoro);
    scena("orizz-ruota", lw, lh, &out, |_| {});
    scena("orizz-foglio", lw, lh, &out, |a| a.foglio = true);
    scena("orizz-lavori", lw, lh, &out, |a| {
        lavori(a);
        a.scheda = Scheda::Lavori;
    });
    for (i, t) in cedro::ui::theme::THEMES.into_iter().enumerate() {
        casa(&format!("casa-{i}"), t, pw, ph, &out, |_| {});
        casa(&format!("casa-{i}-lavori"), t, pw, ph, &out, |a| {
            lavori(a);
            a.sopra = Some(Scheda::Lavori);
        });
    }
    let t = ThemeId::WindowsEssenziale;
    rivedi("rivedi-frasi", t, &out, 0, None, false, false);
    rivedi("rivedi-foglio", t, &out, 0, Some(cedro_android::ui::FoglioRiv::Voce(4)), false, false);
    rivedi("rivedi-chi", t, &out, 1, None, false, false);
    rivedi("rivedi-riassunto-vuoto", t, &out, 2, None, false, false);
    rivedi("rivedi-riassunto-corso", t, &out, 2, None, false, true);
    rivedi("rivedi-riassunto", t, &out, 2, None, true, false);
    rivedi("rivedi-riassunto-scuro", ThemeId::BancoDiLavoro, &out, 2, None, true, false);
    rivedi("rivedi-chi-scuro", ThemeId::BancoDiLavoro, &out, 1, None, false, false);
    for m in cedro::i18n::misses() {
        println!("SENZA TRADUZIONE: {m:?}");
    }
}

/// La firma digitale: la sezione (vuota e con gli ultimi file), il foglio dei formati con le iconcine
/// e il visualizzatore sui file di prova del computer (tests/data/firma).
fn firma(out: &std::path::Path, w: f32, h: f32) {
    use cedro::firma::{self, trust, At, Opts};
    use std::sync::Arc;
    let dati = PathBuf::from("tests/data/firma");
    let xml = std::fs::read_to_string(dati.join("tl.xml")).unwrap();
    trust::set_store(trust::Store { entries: trust::parse_tl(&xml, "IT").unwrap(), updated: Some(firma::now()) });
    let foto = |nome: &str, tema: ThemeId, f: &mut dyn FnMut(&mut App)| {
        let mut app = App::new(tema);
        app.anim.calmo = true;
        app.vai_casa();
        f(&mut app);
        let scala = 2.75;
        let mut cache = GlyphCache::new();
        let mut p = Painter::new((w * scala) as u32, (h * scala) as u32, scala, Fonts::for_theme(tema), &mut cache);
        app.disegna(&mut p, w, h);
        p.save_png(&out.join(format!("{nome}.png"))).unwrap();
    };
    let n = cedro_android::ui::attivita_n();
    for (k, t) in cedro::ui::theme::THEMES.into_iter().enumerate() {
        foto(&format!("firma-casa-{k}"), t, &mut |a| {
            a.firma.recent.clear();
            a.vista_attivita(n - 1);
        });
    }
    foto("firma-casa-recenti", ThemeId::WindowsEssenziale, &mut |a| {
        a.firma.recent = vec![
            cedro::ui::firma::Recent { path: "contratto_fornitura.pdf.p7m".into(), level: firma::Level::Ok, headline: "Firma valida".into(), when: firma::now() - 3600 },
            cedro::ui::firma::Recent { path: "determina_42.pdf".into(), level: firma::Level::Warn, headline: "2 firme valide, con un avviso".into(), when: firma::now() - 90_000 },
            cedro::ui::firma::Recent { path: "offerta.docx.p7m".into(), level: firma::Level::Bad, headline: "Certificato revocato".into(), when: firma::now() - 6 * 86400 },
        ];
        a.vista_attivita(n - 1);
    });
    foto("formati-immagini", ThemeId::WindowsEssenziale, &mut |a| {
        let img = PathBuf::from("docs/hero.png");
        a.aggiungi_coda(vec![(img, "foto.png".into(), cedro::Kind::Image)]);
        a.vista_formati();
    });
    foto("formati-scuro", ThemeId::BancoDiLavoro, &mut |a| {
        let img = PathBuf::from("docs/hero.png");
        a.aggiungi_coda(vec![(img, "foto.png".into(), cedro::Kind::Image)]);
        a.vista_formati();
    });
    for (nome, file, aperte, tema) in [
        ("vw-p7m", "mario.txt.p7m", vec![], ThemeId::WindowsEssenziale),
        ("vw-due-firme", "determina_due_firme.pdf", vec![1usize], ThemeId::WindowsEssenziale),
        ("vw-tsd", "mario.txt.p7m.tsd", vec![], ThemeId::BancoDiLavoro),
        ("vw-cambiato", "cambiato.txt.p7m", vec![], ThemeId::WindowsEssenziale),
        ("vw-p7s", "documento.txt.p7s", vec![], ThemeId::WindowsEssenziale),
    ] {
        let path = dati.join(file);
        let rep = firma::verify_path(&path, Opts { at: At::Now, online: false, detached: None, now: None });
        foto(nome, tema, &mut |a| {
            a.apri_firmato(path.clone());
            let doc = if rep.doc.is_empty() { None } else { cedro::ui::firma::write_temp(&rep.doc_name, &rep.doc) };
            if let Some(v) = a.firma.viewer.as_mut() {
                v.report = Some(Arc::new(rep.clone()));
                v.checking = false;
                v.doc_path = doc.clone();
                v.open.extend(aperte.iter().copied());
                if rep.signers.len() == 1 {
                    v.open.insert(0);
                }
            }
            a.firma_arrivi();
            // Le pagine, come le prepara glue.
            if let Some(d) = a.fpag.chiedi.take() {
                let kind = cedro::kind_of(&d);
                if kind == cedro::Kind::Pdf {
                    let n = cedro::pdf_info(&d, None).map(|(n, _)| n as usize).unwrap_or(0);
                    let mut imgs = Vec::new();
                    cedro::ui::media::pdf_page_thumbs(&d, &(0..n).collect::<Vec<_>>(), 1200.0, &mut |_, i| {
                        imgs.extend(i);
                        true
                    });
                    for i in imgs {
                        a.firma_pagina(&d, Some(i));
                    }
                } else {
                    let n = cedro::ui::preview::page_count(&d).unwrap_or(1);
                    for k in 0..n {
                        if let Some(i) = cedro::ui::preview::render_page(&d, k, 1200.0) {
                            a.firma_pagina(&d, Some(i));
                        }
                    }
                }
                a.firma_pagina(&d, None);
            }
        });
    }
}
