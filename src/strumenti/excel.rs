//! L'Excel pulito della lettura: «Mappa» (la piastra a colori con i valori veri), «Pozzetti» (una
//! riga per misura), «Spettri» (se lo strumento li scrive) e «Strumento» (che cosa ha scritto il file,
//! e la sua impronta SHA-256). I valori
//! sono quelli dello strumento; il «meno il bianco» è un calcolo di Cedro e sta in una colonna a
//! parte che lo dice.

use rust_xlsxwriter::{Chart, ChartType, Color, ConditionalFormat3ColorScale, Format, FormatAlign, FormatBorder, Workbook, Worksheet, XlsxError};

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
    if a_colonne(l) {
        campioni(&mut book, l, &titoli).map_err(err)?;
    } else {
        pozzetti(&mut book, l, o, &titoli).map_err(err)?;
    }
    if l.misure().any(|m| !m.spettro.is_empty()) {
        spettri(&mut book, l, &titoli).map_err(err)?;
    }
    strumento(&mut book, l, &titoli).map_err(err)?;
    book.save_to_buffer().map_err(err)
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
                    ws.write_number_with_format(riga, col, v - bianco.unwrap_or(0.0), &f)?;
                }
                None => {
                    let t = l.testo(m).unwrap_or_default();
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
        ws.write_formula_with_format(legenda, 2, format!("=MIN({area})").as_str(), &Format::new().set_background_color(Color::RGB(basso)).set_font_color(Color::White))?;
        ws.write_string_with_format(legenda, 3, "Max", &nota)?;
        ws.write_formula_with_format(legenda, 4, format!("=MAX({area})").as_str(), &Format::new().set_background_color(Color::RGB(alto)).set_font_color(Color::White))?;
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
    vedi(&[Posto::Campione, Posto::Impostazione, Posto::Elaborazione, Posto::CampioneAltro, Posto::ImpostazioneAltro, Posto::ElaboratoAltro], l);
    if let Some(i) = cols.iter().position(|(k, _)| *k == l.principale) {
        let c = cols.remove(i);
        cols.insert(0, c);
    }
    cols
}

fn pozzetti(book: &mut Workbook, l: &Lettura, o: &Opzioni, titoli: &Format) -> Result<(), XlsxError> {
    let ws = book.add_worksheet();
    ws.set_name(if l.a_piastra() { "Pozzetti" } else { "Misure" })?;
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
    for (k, u) in &cols {
        intest.push(intestazione_valore(k, u));
    }
    if con_bianco {
        intest.push("meno il bianco (calcolato da Cedro)".into());
    }
    for (c, t) in intest.iter().enumerate() {
        ws.write_string_with_format(0, c as u16, t, titoli)?;
    }
    let mut riga = 1u32;
    for (g, gr) in l.gruppi.iter().enumerate() {
        for m in gr.misure.iter().filter(|m| !m.e_spettro()) {
            let mut c = 0u16;
            let testo = |ws: &mut Worksheet, t: &str, c: &mut u16| -> Result<(), XlsxError> {
                if !t.is_empty() {
                    ws.write_string(riga, *c, t)?;
                }
                *c += 1;
                Ok(())
            };
            if molti {
                testo(ws, &gr.titolo, &mut c)?;
            }
            if piastra {
                testo(ws, m.pozzetto.as_deref().unwrap_or(""), &mut c)?;
            }
            testo(ws, &m.campione, &mut c)?;
            if bersagli {
                testo(ws, m.bersaglio.as_deref().unwrap_or(""), &mut c)?;
            }
            let ruolo = ruolo_di(m, &o.ruoli);
            testo(ws, if ruolo == Ruolo::Campione && !m.ruolo_dal_file { "" } else { ruolo.nome() }, &mut c)?;
            debug_assert_eq!(c as usize, primo_valore);
            for (k, _) in &cols {
                match m.valore(k).map(|v| &v.dato) {
                    Some(Dato::Numero(v)) => {
                        ws.write_number(riga, c, *v)?;
                    }
                    Some(Dato::Bool(b)) => {
                        ws.write_string(riga, c, if *b { "sì" } else { "no" })?;
                    }
                    Some(Dato::Testo(t)) | Some(Dato::Assente(t)) if !t.is_empty() => {
                        ws.write_string(riga, c, t)?;
                    }
                    _ => {}
                }
                c += 1;
            }
            if con_bianco {
                if let (Some(b), Some(v)) = (bianchi[g], l.valore(m)) {
                    ws.write_number(riga, c, v - b)?;
                }
            }
            riga += 1;
        }
    }
    ws.set_freeze_panes(1, 0)?;
    if riga > 1 {
        ws.autofilter(0, 0, riga - 1, (intest.len() - 1) as u16)?;
    }
    for (c, t) in intest.iter().enumerate() {
        ws.set_column_width(c as u16, (t.chars().count() as f64 + 3.0).clamp(10.0, 40.0))?;
    }
    Ok(())
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

fn scrivi_dato(ws: &mut Worksheet, riga: u32, c: u16, dato: &Dato) -> Result<(), XlsxError> {
    match dato {
        Dato::Numero(v) => {
            ws.write_number(riga, c, *v)?;
        }
        Dato::Bool(b) => {
            ws.write_string(riga, c, if *b { "sì" } else { "no" })?;
        }
        Dato::Testo(t) | Dato::Assente(t) if !t.is_empty() => {
            ws.write_string(riga, c, t)?;
        }
        _ => {}
    }
    Ok(())
}

/// Il foglio «Campioni»: una riga per campione (e posizione e ora), le assorbanze una colonna per
/// lunghezza d'onda, la concentrazione e i rapporti che lo strumento ha calcolato.
fn campioni(book: &mut Workbook, l: &Lettura, titoli: &Format) -> Result<(), XlsxError> {
    let ws = book.add_worksheet();
    ws.set_name("Campioni")?;
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
    for (c, t) in intest.iter().enumerate() {
        ws.write_string_with_format(0, c as u16, t, titoli)?;
    }
    for (i, ((g, campione, luogo, quando), misure)) in righe.iter().enumerate() {
        let r = i as u32 + 1;
        let mut c = 0u16;
        if molti {
            ws.write_string(r, c, &l.gruppi[*g].titolo)?;
            c += 1;
        }
        ws.write_string(r, c, campione)?;
        c += 1;
        if con_luogo {
            ws.write_string(r, c, luogo)?;
            c += 1;
        }
        if con_ora {
            ws.write_string(r, c, ora_leggibile(quando))?;
            c += 1;
        }
        for (_, k, w) in &cols {
            let v = misure.iter().filter(|m| w.is_none() || onda(m) == *w).find_map(|m| m.valore(k));
            if let Some(v) = v {
                scrivi_dato(ws, r, c, &v.dato)?;
            }
            c += 1;
        }
        for nome in &nomi_calcolati {
            let v = l.calcolati.iter().find(|x| x.nome == *nome && x.sorgenti.first().and_then(|s| di_misura.get(s.as_str())) == Some(&i));
            if let Some(v) = v {
                ws.write_number(r, c, v.valore)?;
            }
            c += 1;
        }
    }
    debug_assert!(intest.len() >= prima);
    ws.set_freeze_panes(1, if molti { 2 } else { 1 })?;
    if !righe.is_empty() {
        ws.autofilter(0, 0, righe.len() as u32, (intest.len() - 1) as u16)?;
    }
    for (c, t) in intest.iter().enumerate() {
        ws.set_column_width(c as u16, (t.chars().count() as f64 + 3.0).clamp(10.0, 40.0))?;
    }
    Ok(())
}

/// Gli spettri: una colonna di lunghezze d'onda e una colonna per campione, con il grafico.
fn spettri(book: &mut Workbook, l: &Lettura, titoli: &Format) -> Result<(), XlsxError> {
    let ws = book.add_worksheet();
    ws.set_name("Spettri")?;
    let spettri: Vec<&super::Misura> = l.misure().filter(|m| !m.spettro.is_empty()).collect();
    // Le lunghezze d'onda di tutti gli spettri, in ordine (di solito sono le stesse).
    let mut onde: Vec<f64> = spettri.iter().flat_map(|m| m.spettro.iter().map(|(w, _)| *w)).collect();
    onde.sort_by(|a, b| a.total_cmp(b));
    onde.dedup();
    ws.write_string_with_format(0, 0, "nm", titoli)?;
    for (i, w) in onde.iter().enumerate() {
        ws.write_number(i as u32 + 1, 0, *w)?;
    }
    let mut nomi: Vec<String> = Vec::new();
    for (c, m) in spettri.iter().enumerate() {
        let base = if m.campione.is_empty() { m.id.clone() } else { m.campione.clone() };
        let n = nomi.iter().filter(|x| **x == base || x.starts_with(&format!("{base} ("))).count();
        let nome = if n == 0 { base } else { format!("{base} ({})", n + 1) };
        let col = c as u16 + 1;
        ws.write_string_with_format(0, col, &nome, titoli)?;
        for (w, a) in &m.spettro {
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
    // Il grafico degli spettri (fino a 24: oltre le linee non si leggono più).
    if spettri.len() <= 24 && !onde.is_empty() {
        let ultima = onde.len() as u32;
        let mut grafico = Chart::new(ChartType::ScatterSmooth);
        for c in 1..=spettri.len() as u16 {
            grafico.add_series().set_categories(("Spettri", 1, 0, ultima, 0)).set_values(("Spettri", 1, c, ultima, c)).set_name(("Spettri", 0, c));
        }
        grafico.x_axis().set_name("Lunghezza d'onda (nm)");
        grafico.y_axis().set_name("Assorbanza (AU)");
        grafico.title().set_name("Spettri di assorbanza");
        grafico.set_width(760).set_height(380);
        ws.insert_chart(1, spettri.len() as u16 + 2, &grafico)?;
    }
    Ok(())
}

fn strumento(book: &mut Workbook, l: &Lettura, titoli: &Format) -> Result<(), XlsxError> {
    let ws = book.add_worksheet();
    ws.set_name(&nome_foglio("Strumento"))?;
    let mut righe: Vec<(String, String)> = vec![
        ("File".into(), l.file.clone()),
        ("SHA-256 del file originale".into(), l.sha256.clone()),
        ("Strumento letto come".into(), format!("{} ({})", l.nome, l.tecnica.nome())),
    ];
    let s = &l.strumento;
    for (k, v) in [("Produttore", &s.produttore), ("Modello", &s.modello), ("Identificativo", &s.identificativo), ("Numero di serie", &s.seriale), ("Programma", &s.software), ("Versione del programma", &s.versione)] {
        if !v.is_empty() {
            righe.push((k.into(), v.clone()));
        }
    }
    righe.extend(l.info.iter().cloned());
    for a in &l.avvisi {
        righe.push(("Avviso".into(), a.clone()));
    }
    righe.push(("Convertito da".into(), format!("Cedro {}", env!("CARGO_PKG_VERSION"))));
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
