//! L'Excel pulito della lettura: «Mappa» (la piastra a colori con i valori veri), «Pozzetti» (una
//! riga per misura) e «Strumento» (che cosa ha scritto il file, e la sua impronta SHA-256). I valori
//! sono quelli dello strumento; il «meno il bianco» è un calcolo di Cedro e sta in una colonna a
//! parte che lo dice.

use rust_xlsxwriter::{Color, ConditionalFormat3ColorScale, Format, FormatAlign, FormatBorder, Workbook, Worksheet, XlsxError};

use super::{ruolo_di, Dato, Lettura, Posto, Ruolo};
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
    pozzetti(&mut book, l, o, &titoli).map_err(err)?;
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
    vedi(&[Posto::Campione, Posto::Impostazione, Posto::Elaborazione, Posto::CampioneAltro, Posto::ImpostazioneAltro], l);
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
        for m in &gr.misure {
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
