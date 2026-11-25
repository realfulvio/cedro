//! L'Excel pulito della lettura: «Mappa» (la piastra a colori con i valori veri), «Pozzetti» (una
//! riga per misura), «Spettri» (se lo strumento li scrive) e «Strumento» (che cosa ha scritto il file,
//! e la sua impronta SHA-256). I valori
//! sono quelli dello strumento; il «meno il bianco» è un calcolo di Cedro e sta in una colonna a
//! parte che lo dice.

use rust_xlsxwriter::{Chart, ChartType, Color, ConditionalFormat3ColorScale, Format, FormatAlign, Formula, FormatBorder, Workbook, XlsxError};

use std::collections::HashMap;

use super::{ruolo_di, Dato, Lettura, Misura, Posto, Ruolo, Tecnica, Valore};
use crate::tabella::{MAPPA_ALTO, MAPPA_BASSO, MAPPA_MEZZO};

/// Le scelte di chi converte.
#[derive(Clone, Debug, Default)]
pub struct Opzioni {
    /// I ruoli segnati sulla piastra: pozzetto (A1) → ruolo. Vincono su quelli del file.
    pub ruoli: Vec<(String, Ruolo)>,
    /// La mappa mostra i valori meno la media dei bianchi.
    pub meno_bianco: bool,
    /// Scala per chi non distingue i colori: blu e arancio (Okabe-Ito) invece di blu e rosso.
    pub daltonici: bool,
}

/// Colori della scala: basso, mezzo, alto.
pub fn scala(daltonici: bool) -> (u32, u32, u32) {
    if daltonici {
        (0x0072B2, 0xF7F7F7, 0xD55E00)
    } else {
        (MAPPA_BASSO, MAPPA_MEZZO, MAPPA_ALTO)
    }
}

/// La media dei bianchi di un gruppo, se ce ne sono con un valore numerico.
pub fn media_bianchi(l: &Lettura, g: usize, ruoli: &[(String, Ruolo)]) -> Option<f64> {
    let v: Vec<f64> = l.gruppi.get(g)?.misure.iter().filter(|m| ruolo_di(m, ruoli) == Ruolo::Bianco).filter_map(|m| l.valore(m)).collect();
    (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
}

fn riga_nome(r: usize) -> String {
    super::pozzetto_nome(r, 0).trim_end_matches(|c: char| c.is_ascii_digit()).to_string()
}

fn colonna_nome(c: u16) -> String {
    let mut n = c as u32 + 1;
    let mut s = Vec::new();
    while n > 0 {
        let r = (n - 1) % 26;
        s.push((b'A' + r as u8) as char);
        n = (n - 1) / 26;
    }
    s.into_iter().rev().collect()
}

/// Il nome del foglio: Excel vuole al più 31 caratteri e niente `[]:*?/\`.
fn nome_foglio(t: &str) -> String {
    t.chars().filter(|c| !"[]:*?/\\".contains(*c)).take(31).collect()
}

fn intestazione_valore(chiave: &str, unita: &str) -> String {
    if unita.is_empty() || unita == "(unitless)" {
        chiave.to_string()
    } else {
        format!("{chiave} ({unita})")
    }
}

/// L'Excel della lettura.
pub fn scrivi(l: &Lettura, o: &Opzioni) -> Result<Vec<u8>, String> {
    let err = |e: XlsxError| e.to_string();
    let mut book = Workbook::new();
    let titoli = Format::new().set_bold().set_background_color(Color::RGB(0xE3F1E6)).set_border_bottom(FormatBorder::Thin);
    if l.a_piastra() {
        mappa(&mut book, l, o).map_err(err)?;
    }
    foglio(&mut book, &tabella(l, o), &titoli).map_err(err)?;
    let e_cinetica = |m: &&super::Misura| m.curva.as_ref().is_some_and(|c| c.cinetica);
    if l.misure().any(|m| !m.spettro.is_empty() && !e_cinetica(&m)) {
        curve(&mut book, l, &titoli, false).map_err(err)?;
    }
    if l.misure().any(|m| !m.spettro.is_empty() && e_cinetica(&m)) {
        curve(&mut book, l, &titoli, true).map_err(err)?;
    }
    if !l.calcolati.is_empty() && !a_colonne(l) {
        calcolati(&mut book, l, &titoli).map_err(err)?;
    }
    if l.misure().any(|m| !m.picchi.is_empty()) {
        picchi(&mut book, l, &titoli).map_err(err)?;
    }
    if l.misure().any(|m| !m.analiti.is_empty()) {
        analiti(&mut book, l, &titoli).map_err(err)?;
    }
    strumento(&mut book, l, &titoli).map_err(err)?;
    book.save_to_buffer().map_err(err)
}

/// Gli analiti dei multiplex: una riga per analita di ogni pozzetto, con il conteggio e le statistiche.
fn analiti(book: &mut Workbook, l: &Lettura, titoli: &Format) -> Result<(), XlsxError> {
    let ws = book.add_worksheet();
    ws.set_name("Analiti")?;
    let piastra = l.a_piastra();
    let molti = l.gruppi.len() > 1;
    let mut statistiche: Vec<(String, String)> = Vec::new();
    let mut calcolati: Vec<(String, String)> = Vec::new();
    let mut altre: Vec<String> = Vec::new();
    for m in l.misure() {
        for a in &m.analiti {
            for s in &a.statistiche {
                if !statistiche.iter().any(|(k, _)| *k == s.ruolo) {
                    statistiche.push((s.ruolo.clone(), s.unita.clone()));
                }
            }
            for c in &a.calcolati {
                if !calcolati.iter().any(|(k, _)| *k == c.nome) {
                    calcolati.push((c.nome.clone(), c.unita.clone()));
                }
            }
            for v in &a.altro {
                if !altre.contains(&v.chiave) {
                    altre.push(v.chiave.clone());
                }
            }
        }
    }
    let mut intest: Vec<String> = Vec::new();
    if molti {
        intest.push("Gruppo".into());
    }
    if piastra {
        intest.push("Pozzetto".into());
    }
    intest.extend(["Campione".to_string(), "Analita".to_string(), "Sfera".to_string(), "Sfere contate".to_string()]);
    intest.extend(statistiche.iter().map(|(k, u)| intestazione_valore(k, u)));
    intest.extend(calcolati.iter().map(|(k, u)| intestazione_valore(k, u)));
    intest.extend(altre.iter().cloned());
    for (c, x) in intest.iter().enumerate() {
        ws.write_string_with_format(0, c as u16, x, titoli)?;
    }
    let mut r = 1u32;
    for g in &l.gruppi {
        for m in &g.misure {
            for a in &m.analiti {
                let mut c = 0u16;
                if molti {
                    ws.write_string(r, c, &g.titolo)?;
                    c += 1;
                }
                if piastra {
                    ws.write_string(r, c, m.pozzetto.as_deref().unwrap_or(""))?;
                    c += 1;
                }
                ws.write_string(r, c, &m.campione)?;
                ws.write_string(r, c + 1, &a.nome)?;
                if let Some(s) = &a.sfera {
                    match s.parse::<f64>() {
                        Ok(n) => ws.write_number(r, c + 2, n)?,
                        Err(_) => ws.write_string(r, c + 2, s)?,
                    };
                }
                if let Some(n) = a.conteggio.filter(|n| n.is_finite()) {
                    ws.write_number(r, c + 3, n)?;
                }
                c += 4;
                for (i, (k, _)) in statistiche.iter().enumerate() {
                    if let Some(s) = a.statistiche.iter().find(|s| s.ruolo == *k).filter(|s| s.valore.is_finite()) {
                        ws.write_number(r, c + i as u16, s.valore)?;
                    }
                }
                c += statistiche.len() as u16;
                for (i, (k, _)) in calcolati.iter().enumerate() {
                    if let Some(v) = a.calcolati.iter().find(|v| v.nome == *k).filter(|v| v.valore.is_finite()) {
                        ws.write_number(r, c + i as u16, v.valore)?;
                    }
                }
                c += calcolati.len() as u16;
                for (i, k) in altre.iter().enumerate() {
                    if let Some(v) = a.altro.iter().find(|v| v.chiave == *k) {
                        match dato_cella(&v.dato) {
                            Some(Cella::Numero(x)) => ws.write_number(r, c + i as u16, x)?,
                            // Le statistiche in più dello strumento arrivano come testo («5.36E+001»): in Excel sono numeri.
                            Some(Cella::Testo(t)) => match super::fonte::numero(&t) {
                                Some(x) => ws.write_number(r, c + i as u16, x)?,
                                None => ws.write_string(r, c + i as u16, &t)?,
                            },
                            None => &mut *ws,
                        };
                    }
                }
                r += 1;
            }
        }
    }
    ws.set_freeze_panes(1, 0)?;
    if r > 1 {
        ws.autofilter(0, 0, r - 1, (intest.len() - 1) as u16)?;
    }
    for c in 0..intest.len() as u16 {
        ws.set_column_width(c, 18.0)?;
    }
    Ok(())
}

/// I picchi che lo strumento ha trovato (diffusione dinamica della luce): una riga per picco di ogni misura.
fn picchi(book: &mut Workbook, l: &Lettura, titoli: &Format) -> Result<(), XlsxError> {
    let ws = book.add_worksheet();
    ws.set_name("Picchi")?;
    let piastra = l.a_piastra();
    let mut colonne: Vec<(String, String)> = Vec::new();
    for m in l.misure() {
        for p in &m.picchi {
            for v in p.iter().filter(|v| v.chiave != "peak index") {
                if !colonne.iter().any(|(k, _)| *k == v.chiave) {
                    colonne.push((v.chiave.clone(), v.unita.clone()));
                }
            }
        }
    }
    let mut intest: Vec<String> = Vec::new();
    if piastra {
        intest.push("Pozzetto".into());
    }
    intest.extend(["Campione".to_string(), "Picco".to_string()]);
    intest.extend(colonne.iter().map(|(k, u)| intestazione_valore(k, u)));
    for (c, x) in intest.iter().enumerate() {
        ws.write_string_with_format(0, c as u16, x, titoli)?;
    }
    let mut r = 1u32;
    for m in l.misure() {
        for p in &m.picchi {
            let mut c = 0u16;
            if piastra {
                ws.write_string(r, c, m.pozzetto.as_deref().unwrap_or(""))?;
                c += 1;
            }
            ws.write_string(r, c, &m.campione)?;
            let indice = p.iter().find(|v| v.chiave == "peak index").map(|v| match &v.dato {
                Dato::Testo(t) => t.clone(),
                _ => String::new(),
            });
            match indice.as_deref().and_then(|t| t.parse::<f64>().ok()) {
                Some(n) => ws.write_number(r, c + 1, n)?,
                None => ws.write_string(r, c + 1, indice.as_deref().unwrap_or(""))?,
            };
            for (i, (k, _)) in colonne.iter().enumerate() {
                if let Some(x) = p.iter().find(|v| v.chiave == *k).and_then(Valore::numero) {
                    ws.write_number(r, c + 2 + i as u16, x)?;
                }
            }
            r += 1;
        }
    }
    ws.set_freeze_panes(1, 0)?;
    if r > 1 {
        ws.autofilter(0, 0, r - 1, (intest.len() - 1) as u16)?;
    }
    for c in 0..intest.len() as u16 {
        ws.set_column_width(c, 18.0)?;
    }
    Ok(())
}

fn mappa(book: &mut Workbook, l: &Lettura, o: &Opzioni) -> Result<(), XlsxError> {
    let ws = book.add_worksheet();
    ws.set_name("Mappa")?;
    let titolo = Format::new().set_bold().set_font_size(13);
    let testa = Format::new().set_bold().set_align(FormatAlign::Center).set_background_color(Color::RGB(0xE3F1E6)).set_border(FormatBorder::Thin);
    let cella = Format::new().set_align(FormatAlign::Center).set_align(FormatAlign::VerticalCenter).set_border(FormatBorder::Thin).set_border_color(Color::RGB(0xBFBFBF));
    let nota = Format::new().set_italic().set_font_color(Color::RGB(0x595959));
    let (basso, mezzo, alto) = scala(o.daltonici);
    let piastre: Vec<usize> = (0..l.gruppi.len()).filter(|&g| l.gruppi[g].misure.iter().any(|m| m.pozzetto.is_some())).collect();
    let colonne_max = piastre.iter().map(|&g| l.formato(g).1).max().unwrap_or(12);
    let larghezza = if colonne_max > 24 { 5.0 } else if colonne_max > 12 { 7.5 } else { 10.0 };
    ws.set_column_width(0, 4.5)?;
    for c in 1..=colonne_max as u16 {
        ws.set_column_width(c, larghezza)?;
    }
    let mut r0 = 0u32;
    let mut ci_sono_ruoli = false;
    for &g in &piastre {
        let gr = &l.gruppi[g];
        let (righe, colonne) = l.formato(g);
        let bianco = if o.meno_bianco { media_bianchi(l, g, &o.ruoli) } else { None };
        let mut t = if gr.titolo.is_empty() { l.nome.to_string() } else { gr.titolo.clone() };
        if let Some(b) = bianco {
            t.push_str(&format!(" · meno il bianco ({b:.4})"));
        }
        ws.write_string_with_format(r0, 0, &t, &titolo)?;
        ws.write_blank(r0 + 1, 0, &testa)?;
        for c in 0..colonne {
            ws.write_number_with_format(r0 + 1, c as u16 + 1, (c + 1) as f64, &testa)?;
        }
        for r in 0..righe {
            ws.write_string_with_format(r0 + 2 + r as u32, 0, riga_nome(r), &testa)?;
            if colonne_max <= 24 {
                ws.set_row_height(r0 + 2 + r as u32, 22)?;
            }
            for c in 0..colonne {
                ws.write_blank(r0 + 2 + r as u32, c as u16 + 1, &cella)?;
            }
        }
        let mut valori: Vec<f64> = Vec::new();
        for m in &gr.misure {
            let Some((r, c)) = m.pozzetto.as_deref().and_then(super::pozzetto_rc) else { continue };
            if r >= righe || c >= colonne {
                continue;
            }
            let (riga, col) = (r0 + 2 + r as u32, c as u16 + 1);
            let ruolo = ruolo_di(m, &o.ruoli);
            let lettera = ruolo.lettera();
            ci_sono_ruoli |= !lettera.is_empty();
            let mut f = cella.clone();
            if !lettera.is_empty() {
                f = f.set_num_format(format!("General\" {lettera}\"")).set_bold();
            }
            match l.valore(m) {
                Some(v) => {
                    valori.push(v - bianco.unwrap_or(0.0));
                    ws.write_number_with_format(riga, col, v - bianco.unwrap_or(0.0), &f)?;
                }
                None => {
                    let t = l.testo(m).unwrap_or_default();
                    // Un pozzetto con più misure (i canali di un'immagine): quella senza valore non cancella le altre.
                    if t.is_empty() && lettera.is_empty() {
                        continue;
                    }
                    ws.write_string_with_format(riga, col, if lettera.is_empty() { t } else { format!("{t} {lettera}") }, &f)?;
                }
            }
        }
        let (prima, ultima) = (r0 + 2, r0 + 1 + righe as u32);
        let sc = ConditionalFormat3ColorScale::new().set_minimum_color(Color::RGB(basso)).set_midpoint_color(Color::RGB(mezzo)).set_maximum_color(Color::RGB(alto));
        ws.add_conditional_format(prima, 1, ultima, colonne as u16, &sc)?;
        let area = format!("B{}:{}{}", prima + 1, colonna_nome(colonne as u16), ultima + 1);
        let legenda = ultima + 1;
        ws.write_string_with_format(legenda, 1, "Min", &nota)?;
        // Con il risultato già scritto: LibreOffice non ricalcola all'apertura e mostrerebbe 0.
        let (vmin, vmax) = valori.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(*v), b.max(*v)));
        let risultato = |x: f64| if x.is_finite() { format!("{x}") } else { String::new() };
        ws.write_formula_with_format(legenda, 2, Formula::new(format!("=MIN({area})")).set_result(risultato(vmin)), &Format::new().set_background_color(Color::RGB(basso)).set_font_color(Color::White))?;
        ws.write_string_with_format(legenda, 3, "Max", &nota)?;
        ws.write_formula_with_format(legenda, 4, Formula::new(format!("=MAX({area})")).set_result(risultato(vmax)), &Format::new().set_background_color(Color::RGB(alto)).set_font_color(Color::White))?;
        ws.write_string_with_format(legenda, 5, if o.daltonici { "dal blu (più basso) all'arancio (più alto)" } else { "dal blu (più basso) al rosso (più alto)" }, &nota)?;
        r0 += righe as u32 + 4;
    }
    if ci_sono_ruoli {
        ws.write_string_with_format(r0, 0, "B = bianco · S = standard · C = controllo · R = riferimento", &nota)?;
    }
    ws.set_landscape();
    ws.set_print_fit_to_pages(1, 0);
    ws.set_print_gridlines(false);
    ws.set_header("&L&\"Calibri,Bold\"Mappa delle piastre&R&D");
    Ok(())
}

/// Le colonne dei valori: la principale per prima, poi le altre nell'ordine in cui compaiono.
fn colonne_valori(l: &Lettura) -> Vec<(String, String)> {
    let mut cols: Vec<(String, String)> = Vec::new();
    let mut vedi = |posti: &[Posto], l: &Lettura| {
        for m in l.misure() {
            for v in &m.valori {
                if posti.contains(&v.posto) && !cols.iter().any(|(k, _)| *k == v.chiave) {
                    cols.push((v.chiave.clone(), v.unita.clone()));
                }
            }
        }
    };
    vedi(&[Posto::Misura, Posto::Elaborato], l);
    vedi(&[Posto::Campione, Posto::Impostazione, Posto::Elaborazione, Posto::CampioneAltro, Posto::ImpostazioneAltro, Posto::ElaboratoAltro, Posto::ElaborazioneAltro, Posto::Immagine, Posto::ImmagineAltro], l);
    // I multiplex hanno per pozzetto note che contano (stato del pozzetto, eventi contati): in Excel si vedono.
    if l.tecnica == Tecnica::Multiplex {
        vedi(&[Posto::Altro], l);
    }
    if let Some(i) = cols.iter().position(|(k, _)| *k == l.principale) {
        let c = cols.remove(i);
        cols.insert(0, c);
    }
    cols
}

/// Una cella della tabella dei valori: testo o numero (le vuote sono `None`).
#[derive(Clone, Debug, PartialEq)]
pub enum Cella {
    Testo(String),
    Numero(f64),
}

/// La tabella che sta nel foglio «Pozzetti» (o «Campioni»): titoli e righe, per Excel e per il CSV.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tab {
    pub nome: &'static str,
    pub intest: Vec<String>,
    pub righe: Vec<Vec<Option<Cella>>>,
    /// Quante colonne restano ferme quando si scorre.
    pub ferme: u16,
}

fn testo_cella(t: &str) -> Option<Cella> {
    (!t.is_empty()).then(|| Cella::Testo(t.to_string()))
}

fn dato_cella(dato: &Dato) -> Option<Cella> {
    match dato {
        Dato::Numero(v) => Some(Cella::Numero(*v)),
        Dato::Bool(b) => Some(Cella::Testo(if *b { "sì" } else { "no" }.into())),
        Dato::Testo(t) | Dato::Assente(t) => testo_cella(t),
    }
}

/// La tabella di tutte le misure: una riga per pozzetto (o per misura), con i ruoli segnati da chi converte.
pub fn tab_pozzetti(l: &Lettura, o: &Opzioni) -> Tab {
    let piastra = l.a_piastra();
    let molti = l.gruppi.len() > 1;
    let bersagli = l.misure().any(|m| m.bersaglio.is_some());
    let cols = colonne_valori(l);
    let bianchi: Vec<Option<f64>> = (0..l.gruppi.len()).map(|g| media_bianchi(l, g, &o.ruoli)).collect();
    let con_bianco = bianchi.iter().any(Option::is_some);
    let mut intest: Vec<String> = Vec::new();
    if molti {
        intest.push("Gruppo".into());
    }
    if piastra {
        intest.push("Pozzetto".into());
    }
    intest.push("Campione".into());
    if bersagli {
        intest.push("Bersaglio".into());
    }
    intest.push("Ruolo".into());
    let primo_valore = intest.len();
    // Le letture fatte di curve non hanno un valore solo per pozzetto: la prima colonna è l'ultimo punto o il massimo.
    let con_curva = l.principale == super::PRINCIPALE_CURVA && l.curva().is_some();
    if let (true, Some(c)) = (con_curva, l.curva()) {
        intest.push(format!("{} · {} ({}) (calcolato da Cedro)", c.nome_riassunto(), c.concetto, c.unita));
    }
    for (k, u) in &cols {
        intest.push(intestazione_valore(k, u));
    }
    // «Meno il bianco» sta subito dopo il valore principale: è quello che si guarda per primo.
    let pos_bianco = if cols.is_empty() && !con_curva { intest.len() } else { primo_valore + 1 };
    if con_bianco {
        intest.insert(pos_bianco, "meno il bianco (calcolato da Cedro)".into());
    }
    let mut righe: Vec<Vec<Option<Cella>>> = Vec::new();
    for (g, gr) in l.gruppi.iter().enumerate() {
        for m in gr.misure.iter().filter(|m| !m.e_spettro()) {
            let mut r: Vec<Option<Cella>> = Vec::new();
            if molti {
                r.push(testo_cella(&gr.titolo));
            }
            if piastra {
                r.push(testo_cella(m.pozzetto.as_deref().unwrap_or("")));
            }
            r.push(testo_cella(&m.campione));
            if bersagli {
                r.push(testo_cella(m.bersaglio.as_deref().unwrap_or("")));
            }
            let ruolo = ruolo_di(m, &o.ruoli);
            r.push(testo_cella(if ruolo == Ruolo::Campione && !m.ruolo_dal_file { "" } else { ruolo.nome() }));
            debug_assert_eq!(r.len(), primo_valore);
            if con_curva {
                r.push(l.valore(m).map(Cella::Numero));
            }
            for (k, _) in &cols {
                r.push(m.valore(k).and_then(|v| dato_cella(&v.dato)));
            }
            if con_bianco {
                r.insert(
                    pos_bianco,
                    match (bianchi[g], l.valore(m)) {
                        (Some(b), Some(v)) => Some(Cella::Numero(v - b)),
                        _ => None,
                    },
                );
            }
            righe.push(r);
        }
    }
    Tab { nome: if piastra { "Pozzetti" } else { "Misure" }, intest, righe, ferme: 0 }
}

/// Scrive una tabella in un foglio: titoli in grassetto, prima riga ferma, filtri e larghezze.
fn foglio(book: &mut Workbook, t: &Tab, titoli: &Format) -> Result<(), XlsxError> {
    let ws = book.add_worksheet();
    ws.set_name(t.nome)?;
    for (c, x) in t.intest.iter().enumerate() {
        ws.write_string_with_format(0, c as u16, x, titoli)?;
    }
    for (i, r) in t.righe.iter().enumerate() {
        for (c, cella) in r.iter().enumerate() {
            match cella {
                Some(Cella::Testo(x)) => {
                    ws.write_string(i as u32 + 1, c as u16, x)?;
                }
                Some(Cella::Numero(v)) => {
                    ws.write_number(i as u32 + 1, c as u16, *v)?;
                }
                None => {}
            }
        }
    }
    ws.set_freeze_panes(1, t.ferme)?;
    if !t.righe.is_empty() && !t.intest.is_empty() {
        ws.autofilter(0, 0, t.righe.len() as u32, (t.intest.len() - 1) as u16)?;
    }
    for (c, x) in t.intest.iter().enumerate() {
        ws.set_column_width(c as u16, (x.chars().count() as f64 + 3.0).clamp(10.0, 40.0))?;
    }
    Ok(())
}

/// La tabella dei valori come la scrive l'Excel: una riga per campione (spettrofotometri) o per pozzetto.
pub fn tabella(l: &Lettura, o: &Opzioni) -> Tab {
    if a_colonne(l) {
        tab_campioni(l)
    } else {
        tab_pozzetti(l, o)
    }
}

/// Il CSV della tabella dei valori. Con il punto e virgola i decimali hanno la virgola (l'Excel italiano
/// lo apre giusto); con la virgola o la tabulazione, il punto.
pub fn csv(l: &Lettura, o: &Opzioni, sep: char) -> String {
    let t = tabella(l, o);
    let num = |v: f64| {
        let s = format!("{v}");
        if sep == ';' {
            s.replace('.', ",")
        } else {
            s
        }
    };
    let mut righe: Vec<Vec<String>> = vec![t.intest.clone()];
    for r in &t.righe {
        righe.push(r.iter().map(|c| match c {
            Some(Cella::Testo(x)) => x.clone(),
            Some(Cella::Numero(v)) => num(*v),
            None => String::new(),
        }).collect());
    }
    crate::textconv::write_csv(&righe, sep)
}

/// Gli spettrofotometri senza piastra leggono ogni campione a più lunghezze d'onda (A260, A280):
/// nel foglio una riga per campione, con una colonna per lunghezza d'onda.
fn a_colonne(l: &Lettura) -> bool {
    !l.a_piastra() && l.tecnica == Tecnica::Spettrofotometria && l.misure().any(|m| !m.e_spettro() && m.valore("detector wavelength setting").is_some())
}

/// L'ora come la scrive lo strumento: «2020-05-26 15:36:00». Lo +00:00 che Allotrope vuole quando il
/// file non dice il fuso qui non si scrive, per non far credere che sia UTC.
fn ora_leggibile(iso: &str) -> String {
    let t = iso.replace('T', " ");
    match t.strip_suffix("+00:00") {
        Some(s) => s.to_string(),
        None => t,
    }
}

/// Il foglio «Campioni»: una riga per campione (e posizione e ora), le assorbanze una colonna per
/// lunghezza d'onda, la concentrazione e i rapporti che lo strumento ha calcolato.
pub fn tab_campioni(l: &Lettura) -> Tab {
    type Chiave = (usize, String, String, String);
    let molti = l.gruppi.len() > 1;
    let mut righe: Vec<(Chiave, Vec<&Misura>)> = Vec::new();
    let mut indice: HashMap<Chiave, usize> = HashMap::new();
    let mut di_misura: HashMap<&str, usize> = HashMap::new();
    for (g, gr) in l.gruppi.iter().enumerate() {
        for m in gr.misure.iter().filter(|m| !m.e_spettro()) {
            let k: Chiave = (g, m.campione.clone(), m.luogo.clone().or_else(|| m.pozzetto.clone()).unwrap_or_default(), m.quando.clone().unwrap_or_default());
            let i = *indice.entry(k.clone()).or_insert_with(|| {
                righe.push((k, Vec::new()));
                righe.len() - 1
            });
            righe[i].1.push(m);
            di_misura.insert(m.id.as_str(), i);
        }
    }
    let onda = |m: &Misura| m.valore("detector wavelength setting").and_then(Valore::numero);
    // Le colonne dei valori: l'assorbanza si divide per lunghezza d'onda, il resto è una colonna sola.
    let mut cols: Vec<(String, String, Option<f64>)> = Vec::new();
    for (k, u) in colonne_valori(l) {
        if k == "detector wavelength setting" {
            continue;
        }
        let e_misura = l.misure().find_map(|m| m.valore(&k)).is_some_and(|v| v.posto == Posto::Misura);
        let mut onde: Vec<f64> = Vec::new();
        if e_misura {
            for m in l.misure().filter(|m| m.valore(&k).is_some()) {
                if let Some(w) = onda(m) {
                    if !onde.contains(&w) {
                        onde.push(w);
                    }
                }
            }
        }
        if onde.len() > 1 {
            for w in onde {
                cols.push((intestazione_valore(&format!("{k} {w} nm"), &u), k.clone(), Some(w)));
            }
        } else {
            cols.push((intestazione_valore(&k, &u), k, None));
        }
    }
    // I calcoli dello strumento (A260/280…): una colonna per nome, sulla riga del campione da cui vengono.
    let mut nomi_calcolati: Vec<String> = Vec::new();
    for c in &l.calcolati {
        if !nomi_calcolati.contains(&c.nome) {
            nomi_calcolati.push(c.nome.clone());
        }
    }
    let con_luogo = righe.iter().any(|(k, _)| !k.2.is_empty());
    let con_ora = righe.iter().any(|(k, _)| !k.3.is_empty());
    let mut intest: Vec<String> = Vec::new();
    if molti {
        intest.push("Gruppo".into());
    }
    intest.push("Campione".into());
    if con_luogo {
        intest.push("Posizione".into());
    }
    if con_ora {
        intest.push("Data e ora".into());
    }
    let prima = intest.len();
    intest.extend(cols.iter().map(|c| c.0.clone()));
    intest.extend(nomi_calcolati.iter().cloned());
    let mut tab_righe: Vec<Vec<Option<Cella>>> = Vec::new();
    for (i, ((g, campione, luogo, quando), misure)) in righe.iter().enumerate() {
        let mut r: Vec<Option<Cella>> = Vec::new();
        if molti {
            r.push(testo_cella(&l.gruppi[*g].titolo));
        }
        r.push(testo_cella(campione));
        if con_luogo {
            r.push(testo_cella(luogo));
        }
        if con_ora {
            r.push(testo_cella(&ora_leggibile(quando)));
        }
        for (_, k, w) in &cols {
            let v = misure.iter().filter(|m| w.is_none() || onda(m) == *w).find_map(|m| m.valore(k));
            r.push(v.and_then(|v| dato_cella(&v.dato)));
        }
        for nome in &nomi_calcolati {
            let v = l.calcolati.iter().find(|x| x.nome == *nome && x.sorgenti.first().and_then(|s| di_misura.get(s.as_str())) == Some(&i));
            r.push(v.filter(|v| !v.valore.is_nan()).map(|v| Cella::Numero(v.valore)));
        }
        tab_righe.push(r);
    }
    debug_assert!(intest.len() >= prima);
    Tab { nome: "Campioni", intest, righe: tab_righe, ferme: if molti { 2 } else { 1 } }
}

/// Le curve dei pozzetti e degli spettri: una colonna con l'asse (nm o secondi) e una colonna per curva, con il grafico.
/// `cinetica`: le cinetiche (il tempo sull'asse); altrimenti gli spettri.
fn curve(book: &mut Workbook, l: &Lettura, titoli: &Format, cinetica: bool) -> Result<(), XlsxError> {
    let ws = book.add_worksheet();
    let nome_foglio = if cinetica { "Cinetica" } else { "Spettri" };
    ws.set_name(nome_foglio)?;
    let e_questa = |m: &super::Misura| !m.spettro.is_empty() && m.curva.as_ref().is_some_and(|c| c.cinetica) == cinetica;
    let con_gruppi = l.gruppi.iter().filter(|g| g.misure.iter().any(|m| e_questa(m))).count() > 1;
    let spettri: Vec<(&str, &super::Misura)> = l.gruppi.iter().flat_map(|g| g.misure.iter().filter(|m| e_questa(m)).map(move |m| (g.titolo.as_str(), m))).collect();
    // I punti di tutte le curve, in ordine (di solito sono gli stessi).
    let mut onde: Vec<f64> = spettri.iter().flat_map(|(_, m)| m.spettro.iter().map(|(w, _)| *w)).collect();
    onde.sort_by(|a, b| a.total_cmp(b));
    onde.dedup();
    ws.write_string_with_format(0, 0, if cinetica { "s" } else { "nm" }, titoli)?;
    for (i, w) in onde.iter().enumerate() {
        ws.write_number(i as u32 + 1, 0, *w)?;
    }
    let mut nomi: Vec<String> = Vec::new();
    for (c, (gruppo, m)) in spettri.iter().enumerate() {
        let mut base = if m.campione.is_empty() { m.id.clone() } else { m.campione.clone() };
        if con_gruppi && !gruppo.is_empty() {
            base = format!("{base} · {gruppo}");
        }
        let n = nomi.iter().filter(|x| **x == base || x.starts_with(&format!("{base} ("))).count();
        let nome = if n == 0 { base } else { format!("{base} ({})", n + 1) };
        let col = c as u16 + 1;
        ws.write_string_with_format(0, col, &nome, titoli)?;
        for (w, a) in &m.spettro {
            if !a.is_finite() {
                continue;
            }
            if let Ok(i) = onde.binary_search_by(|x| x.total_cmp(w)) {
                ws.write_number(i as u32 + 1, col, *a)?;
            }
        }
        nomi.push(nome);
    }
    ws.set_freeze_panes(1, 1)?;
    ws.set_column_width(0, 8)?;
    for c in 1..=spettri.len() as u16 {
        ws.set_column_width(c, 16)?;
    }
    // Il grafico (fino a 24 curve: oltre le linee non si leggono più).
    if spettri.len() <= 24 && !onde.is_empty() {
        let ultima = onde.len() as u32;
        let mut grafico = Chart::new(ChartType::ScatterSmooth);
        for c in 1..=spettri.len() as u16 {
            grafico.add_series().set_categories((nome_foglio, 1, 0, ultima, 0)).set_values((nome_foglio, 1, c, ultima, c)).set_name((nome_foglio, 0, c));
        }
        let curva = spettri.first().and_then(|(_, m)| m.curva.as_ref());
        let (concetto, unita) = curva.map(|c| (c.concetto.as_str(), c.unita.as_str())).unwrap_or(("absorbance", "AU"));
        let nome_y = match concetto {
            "fluorescence" => "Fluorescenza",
            "luminescence" => "Luminescenza",
            _ => "Assorbanza",
        };
        grafico.x_axis().set_name(if cinetica { "Tempo (s)" } else { "Lunghezza d'onda (nm)" });
        grafico.y_axis().set_name(&format!("{nome_y} ({})", if unita.is_empty() { "AU" } else { unita }));
        grafico.title().set_name(&format!("{} di {}", if cinetica { "Cinetiche" } else { "Spettri" }, nome_y.to_lowercase()));
        grafico.set_width(760).set_height(380);
        ws.insert_chart(1, spettri.len() as u16 + 2, &grafico)?;
    }
    Ok(())
}

/// I valori che lo strumento ha calcolato: una riga per valore, con il pozzetto e le misure da cui viene.

/// La prima misura da cui viene un calcolo, anche passando dai calcoli da cui dipende.
fn prima_misura<'a>(l: &Lettura, c: &super::Calcolato, di_misura: &HashMap<&str, (&'a str, &'a super::Misura)>, livello: usize) -> Option<(&'a str, &'a super::Misura)> {
    if let Some(x) = c.sorgenti.first().and_then(|s| di_misura.get(s.as_str())) {
        return Some(*x);
    }
    if livello > 12 {
        return None;
    }
    c.da_calcoli.iter().filter_map(|&a| l.calcolati.get(a)).find_map(|x| prima_misura(l, x, di_misura, livello + 1))
}

fn calcolati(book: &mut Workbook, l: &Lettura, titoli: &Format) -> Result<(), XlsxError> {
    let ws = book.add_worksheet();
    ws.set_name("Calcolati")?;
    let di_misura: HashMap<&str, (&str, &super::Misura)> = l.gruppi.iter().flat_map(|g| g.misure.iter().map(move |m| (m.id.as_str(), (g.titolo.as_str(), m)))).collect();
    let piastra = l.a_piastra();
    let mut intest: Vec<&str> = Vec::new();
    if piastra {
        intest.push("Pozzetto");
    }
    intest.extend(["Campione", "Nome", "Valore", "Misure da cui viene"]);
    // Come l'ha calcolato lo strumento (la formula), solo se almeno un calcolo lo dice.
    let con_formula = l.calcolati.iter().any(|c| !c.descrizione.is_empty());
    if con_formula {
        intest.push("Come è calcolato");
    }
    for (c, x) in intest.iter().enumerate() {
        ws.write_string_with_format(0, c as u16, *x, titoli)?;
    }
    for (i, c) in l.calcolati.iter().enumerate() {
        let r = i as u32 + 1;
        let prima = prima_misura(l, c, &di_misura, 0);
        let mut col = 0u16;
        if piastra {
            ws.write_string(r, col, prima.and_then(|(_, m)| m.pozzetto.as_deref()).unwrap_or(""))?;
            col += 1;
        }
        ws.write_string(r, col, prima.map(|(_, m)| m.campione.as_str()).unwrap_or(""))?;
        ws.write_string(r, col + 1, &c.nome)?;
        if c.valore.is_nan() {
            ws.write_string(r, col + 2, "N/A")?;
        } else {
            ws.write_number(r, col + 2, c.valore)?;
        }
        let mut origine: Vec<String> = c.sorgenti.iter().filter_map(|s| di_misura.get(s.as_str())).map(|(g, m)| if g.is_empty() { m.id.clone() } else { format!("{g}") }).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
        // I calcoli da cui viene, per nome.
        let altri: std::collections::BTreeSet<&str> = c.da_calcoli.iter().filter_map(|&a| l.calcolati.get(a)).map(|x| x.nome.as_str()).collect();
        origine.extend(altri.into_iter().map(str::to_string));
        ws.write_string(r, col + 3, &origine.join(", "))?;
        if con_formula {
            ws.write_string(r, col + 4, &c.descrizione)?;
        }
    }
    ws.set_freeze_panes(1, 0)?;
    if !l.calcolati.is_empty() {
        ws.autofilter(0, 0, l.calcolati.len() as u32, (intest.len() - 1) as u16)?;
    }
    for (c, x) in intest.iter().enumerate() {
        ws.set_column_width(c as u16, if *x == "Nome" || *x == "Misure da cui viene" || *x == "Come è calcolato" { 36.0 } else { 14.0 })?;
    }
    Ok(())
}

/// Le righe del foglio «Strumento»: che cosa e il suo valore.
pub fn righe_strumento(l: &Lettura) -> Vec<(String, String)> {
    let mut righe: Vec<(String, String)> = vec![
        ("File".into(), l.file.clone()),
        ("SHA-256 del file originale".into(), l.sha256.clone()),
        ("Strumento letto come".into(), format!("{} ({})", l.nome, l.tecnica.nome())),
    ];
    let s = &l.strumento;
    for (k, v) in [("Produttore", &s.produttore), ("Modello", &s.modello), ("Identificativo", &s.identificativo), ("Numero di serie", &s.seriale), ("Programma", &s.software), ("Versione del programma", &s.versione), ("Sistema dati", &s.istanza), ("Inventario", &s.inventario), ("Descrizione", &s.descrizione), ("Marchio", &s.marchio), ("Firmware", &s.firmware)] {
        if !v.is_empty() {
            righe.push((k.into(), v.clone()));
        }
    }
    righe.extend(l.info.iter().cloned());
    for c in &l.calibrazioni {
        let mut t = c.esito.clone().unwrap_or_default();
        if !c.quando.is_empty() {
            if !t.is_empty() {
                t.push_str(" · ");
            }
            t.push_str(&ora_leggibile(&c.quando));
        }
        if let Some(e) = &c.scadenza {
            t.push_str(&format!(" · scade il {}", ora_leggibile(e)));
        }
        righe.push((c.nome.clone(), t));
    }
    for a in &l.avvisi {
        righe.push(("Avviso".into(), a.clone()));
    }
    righe.push(("Convertito da".into(), format!("Cedro {}", env!("CARGO_PKG_VERSION"))));
    righe
}

fn strumento(book: &mut Workbook, l: &Lettura, titoli: &Format) -> Result<(), XlsxError> {
    let ws = book.add_worksheet();
    ws.set_name(&nome_foglio("Strumento"))?;
    let righe = righe_strumento(l);
    ws.write_string_with_format(0, 0, "Che cosa", titoli)?;
    ws.write_string_with_format(0, 1, "Valore", titoli)?;
    for (i, (k, v)) in righe.iter().enumerate() {
        ws.write_string(i as u32 + 1, 0, k)?;
        ws.write_string(i as u32 + 1, 1, v)?;
    }
    ws.set_column_width(0, 30)?;
    ws.set_column_width(1, 70)?;
    Ok(())
}
