//! Tabelle di laboratorio. CSV e TSV diventano Excel con i numeri veri (anche quelli con la virgola),
//! mentre i nomi dei geni (MARCH1, SEPT2, 2310009E13) e i codici con gli zeri davanti restano testo,
//! come sono scritti: Excel, aprendo il CSV da solo, li trasforma in date o in numeri. Le piastre
//! (6…1536 pozzetti) che i lettori scrivono a griglia escono anche una riga per pozzetto.
//! Al contrario, Excel (.xlsx) e Calc (.ods) si leggono in righe per CSV, Markdown, HTML e JSON.

use std::io::{Cursor, Read};

/// Una cella letta da un foglio di calcolo.
#[derive(Clone, Debug, PartialEq)]
pub enum Cella {
    Vuota,
    Testo(String),
    Numero(f64),
    /// Una percentuale: il valore (0,125 per 12,5%) e le cifre decimali da mostrare.
    Percento(f64, usize),
}

impl Cella {
    /// La cella scritta in un CSV: i numeri con la virgola o con il punto.
    pub fn testo(&self, virgola: bool) -> String {
        let num = |v: f64| {
            // 15 cifre significative come Excel: 0,1+0,2 resta 0,3 e non 0,30000000000000004.
            let s = if v.is_finite() { format!("{}", format!("{v:.14e}").parse::<f64>().unwrap_or(v)) } else { String::new() };
            if virgola {
                s.replace('.', ",")
            } else {
                s
            }
        };
        match self {
            Cella::Vuota => String::new(),
            // Un testo che inizia con = o @ Excel, riaprendo il CSV, lo eseguirebbe come formula.
            Cella::Testo(t) if t.starts_with(['=', '@']) => format!("'{t}"),
            Cella::Testo(t) => t.clone(),
            Cella::Numero(v) => num(*v),
            Cella::Percento(v, dec) => {
                let s = format!("{:.*}%", *dec, v * 100.0);
                if virgola {
                    s.replace('.', ",")
                } else {
                    s
                }
            }
        }
    }
}

/// Un foglio: nome e righe.
pub struct Foglio {
    pub nome: String,
    pub righe: Vec<Vec<Cella>>,
}

impl Foglio {
    pub fn testo(&self, virgola: bool) -> Vec<Vec<String>> {
        self.righe.iter().map(|r| r.iter().map(|c| c.testo(virgola)).collect()).collect()
    }
}

// ------------------------------------------------------------------ CSV in ingresso

/// Il separatore più probabile: quello che compare nel maggior numero delle prime righe (i lettori
/// di piastre mettono in cima righe di informazioni senza separatori). Il .tsv è a tabulazioni.
pub fn separatore(text: &str, ext: &str) -> char {
    if ext == "tsv" || ext == "tab" {
        return '\t';
    }
    // Le righe vere: un a capo dentro le virgolette non chiude la riga (note su più righe).
    let mut record: Vec<&str> = Vec::new();
    let (mut inizio, mut dentro) = (0usize, false);
    for (i, b) in text.bytes().enumerate() {
        match b {
            b'"' => dentro = !dentro,
            b'\n' if !dentro => {
                record.push(text[inizio..i].trim_end_matches('\r'));
                inizio = i + 1;
                if record.len() >= 60 {
                    break;
                }
            }
            _ => {}
        }
    }
    if record.len() < 60 && inizio < text.len() {
        record.push(&text[inizio..]);
    }
    let mut best = (';', 0usize, 0usize);
    for sep in ['\t', ';', ','] {
        let (mut righe, mut tot) = (0, 0);
        for line in record.iter().copied() {
            let mut quoted = false;
            let chars: Vec<char> = line.chars().collect();
            let punto_e_virgola = line.contains(';');
            let cifra = |i: Option<usize>| i.and_then(|i| chars.get(i)).is_some_and(char::is_ascii_digit);
            // In una riga con il punto e virgola, la virgola fra due cifre è il decimale italiano (0,512;0,733).
            let n = (0..chars.len())
                .filter(|&i| {
                    let c = chars[i];
                    if c == '"' {
                        quoted = !quoted;
                    }
                    !quoted && c == sep && !(sep == ',' && punto_e_virgola && cifra(i.checked_sub(1)) && cifra(Some(i + 1)))
                })
                .count();
            if n > 0 {
                righe += 1;
                tot += n;
            }
        }
        if (righe, tot) > (best.1, best.2) {
            best = (sep, righe, tot);
        }
    }
    best.0
}

/// Vero se i decimali del file sono con la virgola (CSV dell'Excel italiano, `;` come separatore).
pub fn decimali_con_virgola(rows: &[Vec<String>]) -> bool {
    decimali(rows).0
}

/// I decimali del file: con la virgola, e se si è sicuri (ci sono numeri con i decimali scritti in
/// un modo solo). Quando si è sicuri «1,234» con i decimali a punto è milleduecentotrentaquattro.
pub fn decimali(rows: &[Vec<String>]) -> (bool, bool) {
    let (mut virgola, mut punto) = (0usize, 0usize);
    for cell in rows.iter().flatten().take(200_000) {
        let t = cell.trim().trim_end_matches('%');
        let t = t.strip_prefix(['-', '+']).unwrap_or(t);
        let t = t.split(['e', 'E']).next().unwrap_or("");
        // «1,234» o «1.234» possono essere migliaia (CSV americano con le virgolette): non contano.
        let semplice = |sep: char| match t.split_once(sep) {
            Some((a, b)) => {
                let migliaia = b.len() == 3 && (1..=3).contains(&a.len()) && !a.starts_with('0');
                a.bytes().all(|c| c.is_ascii_digit()) && !b.is_empty() && b.bytes().all(|c| c.is_ascii_digit()) && !migliaia
            }
            None => false,
        };
        if semplice(',') {
            virgola += 1;
        } else if semplice('.') {
            punto += 1;
        }
    }
    (virgola > punto, virgola.max(punto) > 0 && virgola.min(punto) == 0)
}

/// Il numero scritto nella cella, se è proprio un numero. Restano testo: i codici con gli zeri
/// davanti (007), quelli troppo lunghi per Excel (codici a barre), i nomi dei geni che sembrano
/// notazione scientifica (2310009E13: la «E» conta solo con la virgola o con il segno, 1,5E3 o 1E-05),
/// i numeri ambigui con un solo gruppo di migliaia (1.234 con i decimali a virgola) e le date.
pub fn numero(s: &str, virgola: bool) -> Option<Cella> {
    numero_in(s, virgola, false)
}

/// Come [`numero`]; con `migliaia` un solo gruppo di migliaia (1,234 con i decimali a punto) vale
/// come migliaia: i decimali del file sono sicuri.
pub fn numero_in(s: &str, virgola: bool, migliaia: bool) -> Option<Cella> {
    let t = s.trim();
    if t.is_empty() || t.len() > 40 {
        return None;
    }
    let (t, pct) = match t.strip_suffix('%') {
        Some(x) => (x.trim_end(), true),
        None => (t, false),
    };
    let (neg, body) = if let Some(b) = t.strip_prefix('-').or_else(|| t.strip_prefix('\u{2212}')) {
        (true, b)
    } else {
        (false, t.strip_prefix('+').unwrap_or(t))
    };
    let (mant, exp) = match body.find(['e', 'E']) {
        Some(i) => (&body[..i], Some(&body[i + 1..])),
        None => (body, None),
    };
    let (dec, gruppo) = if virgola { (',', '.') } else { ('.', ',') };
    let (int_part, frac) = match mant.split_once(dec) {
        Some((a, b)) => (a, Some(b)),
        None => (mant, None),
    };
    if int_part.is_empty() && frac.is_none_or(str::is_empty) {
        return None;
    }
    if let Some(f) = frac {
        if f.is_empty() || !f.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
    }
    let intero: String = if int_part.contains(gruppo) {
        let gruppi: Vec<&str> = int_part.split(gruppo).collect();
        let ok = (1..=3).contains(&gruppi[0].len()) && gruppi.iter().all(|g| g.bytes().all(|c| c.is_ascii_digit())) && gruppi[1..].iter().all(|g| g.len() == 3);
        // «1.234» da solo può essere milleduecento o uno virgola due: resta testo.
        if !ok || (gruppi.len() == 2 && frac.is_none() && !migliaia) || exp.is_some() {
            return None;
        }
        gruppi.concat()
    } else {
        if !int_part.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        int_part.to_string()
    };
    if intero.len() > 1 && intero.starts_with('0') {
        return None;
    }
    // Oltre 15 cifre Excel perde precisione (anche con i decimali): resta testo.
    if intero.len() > 15 {
        return None;
    }
    // «+39333…»: un telefono o un codice, non un numero da sommare.
    if t.starts_with('+') && frac.is_none() && exp.is_none() && intero.len() >= 8 {
        return None;
    }
    let mut norm = String::with_capacity(t.len() + 2);
    if neg {
        norm.push('-');
    }
    norm.push_str(if intero.is_empty() { "0" } else { &intero });
    if let Some(f) = frac {
        norm.push('.');
        norm.push_str(f);
    }
    if let Some(e) = exp {
        let (segno, cifre) = match e.as_bytes().first() {
            Some(b'-' | b'+') => (true, &e[1..]),
            _ => (false, e),
        };
        if cifre.is_empty() || cifre.len() > 3 || !cifre.bytes().all(|c| c.is_ascii_digit()) || (frac.is_none() && !segno) {
            return None;
        }
        norm.push('e');
        norm.push_str(e);
    }
    let v: f64 = norm.parse().ok()?;
    if !v.is_finite() {
        return None;
    }
    Some(if pct { Cella::Percento(v / 100.0, frac.map_or(0, str::len)) } else { Cella::Numero(v) })
}

/// Le righe del CSV come celle: numeri dove sono numeri, il resto testo.
pub fn celle(rows: &[Vec<String>], virgola: bool) -> Vec<Vec<Cella>> {
    celle_in(rows, virgola, false)
}

fn celle_in(rows: &[Vec<String>], virgola: bool, migliaia: bool) -> Vec<Vec<Cella>> {
    rows.iter()
        .map(|r| {
            r.iter()
                .map(|c| {
                    if c.trim().is_empty() {
                        Cella::Vuota
                    } else {
                        numero_in(c, virgola, migliaia).unwrap_or_else(|| Cella::Testo(c.clone()))
                    }
                })
                .collect()
        })
        .collect()
}

// ------------------------------------------------------------------ Piastre

/// Una piastra trovata nella tabella: il nome (dalla cella d'angolo o dalla riga sopra) e i valori.
pub struct Piastra {
    pub nome: String,
    pub righe: usize,
    pub colonne: usize,
    /// valori[riga][colonna]
    pub valori: Vec<Vec<Cella>>,
}

/// I formati di piastra: righe × colonne.
const FORMATI: [(usize, usize); 8] = [(2, 3), (3, 4), (4, 6), (6, 8), (8, 12), (16, 24), (32, 48), (1, 1)];

fn lettera(i: usize) -> String {
    // A…Z, poi AA…AF (piastre da 1536).
    if i < 26 {
        ((b'A' + i as u8) as char).to_string()
    } else {
        format!("A{}", (b'A' + (i - 26) as u8) as char)
    }
}

/// Il nome del pozzetto: A01, H12, P24.
pub fn pozzetto(riga: usize, col: usize, colonne: usize) -> String {
    if colonne >= 10 {
        format!("{}{:02}", lettera(riga), col + 1)
    } else {
        format!("{}{}", lettera(riga), col + 1)
    }
}

/// Le piastre nella tabella: una riga di intestazione 1, 2, … N seguita da righe A, B, … con i valori.
pub fn piastre(rows: &[Vec<Cella>], testo: &[Vec<String>]) -> Vec<Piastra> {
    let mut out = Vec::new();
    let mut r = 0;
    while r < testo.len() {
        let row = &testo[r];
        for c0 in 1..row.len() {
            if row[c0].trim() != "1" {
                continue;
            }
            let n = (0..).take_while(|k| row.get(c0 + k).is_some_and(|v| v.trim() == (k + 1).to_string())).count();
            let m = (0..).take_while(|k| testo.get(r + 1 + k).and_then(|x| x.get(c0 - 1)).is_some_and(|v| v.trim().eq_ignore_ascii_case(&lettera(*k)))).count();
            let Some(&(fr, fc)) = FORMATI.iter().find(|(fr, fc)| *fc == n && *fr == m && *fr > 1) else { continue };
            let valori = (0..fr).map(|i| (0..fc).map(|j| rows[r + 1 + i].get(c0 + j).cloned().unwrap_or(Cella::Vuota)).collect()).collect();
            let angolo = row[c0 - 1].trim();
            // Il titolo sopra la griglia vale solo se è da solo nella sua riga («Read 1:450», «Piastra 2»).
            let sopra = match r.checked_sub(1).map(|k| testo[k].iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect::<Vec<_>>()) {
                Some(v) if v.len() == 1 => v[0],
                _ => "",
            };
            let nome = if !angolo.is_empty() && angolo != "<>" {
                angolo.to_string()
            } else if !sopra.is_empty() && sopra.chars().count() <= 40 {
                sopra.to_string()
            } else {
                String::new()
            };
            out.push(Piastra { nome, righe: fr, colonne: fc, valori });
            r += fr;
            break;
        }
        r += 1;
    }
    // Nomi vuoti o ripetuti: Piastra 1, Piastra 2…
    let mut visti: Vec<String> = Vec::new();
    for (i, p) in out.iter_mut().enumerate() {
        if p.nome.is_empty() || visti.contains(&p.nome) {
            p.nome = format!("Piastra {}", i + 1);
        }
        visti.push(p.nome.clone());
    }
    out
}

/// Una riga per pozzetto: Pozzetto, Riga, Colonna e un valore per piastra (le piastre dello stesso
/// formato, per esempio le letture di una cinetica, stanno affiancate).
pub fn per_pozzetto(piastre: &[Piastra]) -> Vec<Vec<Cella>> {
    let Some(prima) = piastre.first() else { return Vec::new() };
    let stesse: Vec<&Piastra> = piastre.iter().filter(|p| (p.righe, p.colonne) == (prima.righe, prima.colonne)).collect();
    let mut out = Vec::with_capacity(prima.righe * prima.colonne + 1);
    let mut head = vec![Cella::Testo("Pozzetto".into()), Cella::Testo("Riga".into()), Cella::Testo("Colonna".into())];
    head.extend(stesse.iter().map(|p| Cella::Testo(if stesse.len() == 1 && p.nome.starts_with("Piastra ") { "Valore".into() } else { p.nome.clone() })));
    out.push(head);
    for i in 0..prima.righe {
        for j in 0..prima.colonne {
            let mut row = vec![Cella::Testo(pozzetto(i, j, prima.colonne)), Cella::Testo(lettera(i)), Cella::Numero((j + 1) as f64)];
            row.extend(stesse.iter().map(|p| p.valori[i][j].clone()));
            out.push(row);
        }
    }
    out
}

// ------------------------------------------------------------------ Excel in uscita

/// Quante righe ci stanno in un foglio di Excel.
const MAX_RIGHE: usize = 1_048_576;
const MAX_COLONNE: usize = 16_384;

/// L'Excel della tabella. Ritorna il file e la frase per l'utente.
pub fn in_excel(rows: &[Vec<String>], intestazione: bool) -> Result<(Vec<u8>, String), String> {
    use rust_xlsxwriter::{Color, Format, FormatBorder, Workbook};
    let (virgola, sicuro) = decimali(rows);
    let celle = celle_in(rows, virgola, sicuro);
    let piastre = piastre(&celle, rows);
    let mut book = Workbook::new();
    let titoli = Format::new().set_bold().set_background_color(Color::RGB(0xE3F1E6)).set_border_bottom(FormatBorder::Thin);
    let err = |e: rust_xlsxwriter::XlsxError| e.to_string();

    let larghezza = celle.iter().map(Vec::len).max().unwrap_or(0);
    if larghezza > MAX_COLONNE {
        return Err(format!("troppe colonne per Excel: {larghezza} (al massimo {MAX_COLONNE})"));
    }
    let blocchi: Vec<&[Vec<Cella>]> = if celle.is_empty() { vec![&celle[..]] } else { celle.chunks(MAX_RIGHE - 1).collect() };
    let mut testo_protetto = Conta::default();
    for (k, blocco) in blocchi.iter().enumerate() {
        let ws = book.add_worksheet();
        ws.set_name(if k == 0 { "Dati".to_string() } else { format!("Dati {}", k + 1) }).map_err(err)?;
        let (base, titolo) = if k > 0 && intestazione { (1u32, Some(&celle[0])) } else { (0, None) };
        if let Some(t) = titolo {
            scrivi_riga(ws, 0, t, None, Some(&titoli), virgola, &mut testo_protetto).map_err(err)?;
        }
        for (i, row) in blocco.iter().enumerate() {
            let fmt = (intestazione && k == 0 && i == 0).then_some(&titoli);
            let grezza = rows.get(k * (MAX_RIGHE - 1) + i).map(Vec::as_slice);
            scrivi_riga(ws, base + i as u32, row, grezza, fmt, virgola, &mut testo_protetto).map_err(err)?;
        }
        larghezze(ws, titolo.into_iter().chain(blocco.iter())).map_err(err)?;
        if intestazione && !blocco.is_empty() && piastre.is_empty() {
            ws.set_freeze_panes(1, 0).map_err(err)?;
            let ultima = (base as usize + blocco.len()).saturating_sub(1) as u32;
            if ultima > 0 && larghezza > 0 {
                ws.autofilter(0, 0, ultima, (larghezza - 1) as u16).map_err(err)?;
            }
        }
    }
    let mut frase = format!("Excel con {} righe.", celle.len());
    if virgola {
        frase.push_str(" Numeri con la virgola letti come numeri.");
    }
    if !piastre.is_empty() {
        let pozzetti = per_pozzetto(&piastre);
        let ws = book.add_worksheet();
        ws.set_name("Pozzetti").map_err(err)?;
        for (i, row) in pozzetti.iter().enumerate() {
            scrivi_riga(ws, i as u32, row, None, (i == 0).then_some(&titoli), virgola, &mut testo_protetto).map_err(err)?;
        }
        larghezze(ws, pozzetti.iter()).map_err(err)?;
        ws.set_freeze_panes(1, 0).map_err(err)?;
        ws.autofilter(0, 0, (pozzetti.len() - 1) as u32, (pozzetti[0].len() - 1) as u16).map_err(err)?;
        let p = &piastre[0];
        let n = piastre.iter().filter(|q| (q.righe, q.colonne) == (p.righe, p.colonne)).count();
        if n == 1 {
            frase.push_str(&format!(" Piastra da {} pozzetti anche una riga per pozzetto (foglio Pozzetti).", p.righe * p.colonne));
        } else {
            frase.push_str(&format!(" {n} piastre da {} pozzetti, anche una riga per pozzetto (foglio Pozzetti).", p.righe * p.colonne));
        }
    }
    if testo_protetto.protetti > 0 {
        frase.push_str(" Nomi di geni e codici lasciati come testo.");
    }
    if testo_protetto.tagliate > 0 {
        frase.push_str(&format!(" {} celle troppo lunghe per Excel tagliate a {MAX_CELLA} caratteri.", testo_protetto.tagliate));
    }
    Ok((book.save_to_buffer().map_err(err)?, frase))
}

/// Vero per un testo che Excel, aprendo il CSV, avrebbe cambiato: geni come date, codici come numeri.
fn sembra_numero_o_data(t: &str) -> bool {
    let t = t.trim();
    let mesi = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "SEPT", "OCT", "NOV", "DEC", "GEN", "MAG", "GIU", "LUG", "AGO", "SET", "OTT", "DIC"];
    let up = t.to_ascii_uppercase();
    let gene_data = mesi.iter().any(|m| up.strip_prefix(m).is_some_and(|r| !r.is_empty() && r.len() <= 3 && r.trim_start_matches('-').bytes().all(|c| c.is_ascii_digit())));
    let zeri = t.len() > 1 && t.starts_with('0') && t.bytes().all(|c| c.is_ascii_digit());
    let lungo = t.len() > 15 && t.bytes().all(|c| c.is_ascii_digit());
    let esp = t.contains(['E', 'e']) && t.split(['E', 'e']).all(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit()));
    gene_data || zeri || lungo || esp
}

/// Il massimo di caratteri in una cella di Excel.
const MAX_CELLA: usize = 32_767;

/// Quello che la frase finale racconta: testi protetti e celle tagliate.
#[derive(Default)]
struct Conta {
    protetti: usize,
    tagliate: usize,
}

/// Le cifre dopo la virgola scritte nel CSV quando finiscono con uno zero («1,50»): Excel le
/// mostrerebbe come 1,5, perdendo le cifre significative.
fn decimali_scritti(grezzo: &str, virgola: bool) -> Option<usize> {
    let t = grezzo.trim();
    if t.contains(['e', 'E']) {
        return None;
    }
    let (_, frac) = t.split_once(if virgola { ',' } else { '.' })?;
    (frac.ends_with('0') && frac.bytes().all(|c| c.is_ascii_digit())).then_some(frac.len().min(15))
}

#[allow(clippy::too_many_arguments)]
fn scrivi_riga(
    ws: &mut rust_xlsxwriter::Worksheet,
    r: u32,
    row: &[Cella],
    grezza: Option<&[String]>,
    fmt: Option<&rust_xlsxwriter::Format>,
    virgola: bool,
    conta: &mut Conta,
) -> Result<(), rust_xlsxwriter::XlsxError> {
    use rust_xlsxwriter::Format;
    for (i, cella) in row.iter().enumerate() {
        let c = i as u16;
        let tagliato = |t: &str, conta: &mut Conta| -> String {
            if t.len() > MAX_CELLA && t.chars().count() > MAX_CELLA {
                conta.tagliate += 1;
                t.chars().take(MAX_CELLA).collect()
            } else {
                t.to_string()
            }
        };
        match (cella, fmt) {
            (Cella::Vuota, _) => {}
            (Cella::Testo(t), Some(f)) => {
                ws.write_string_with_format(r, c, tagliato(t, conta), f)?;
            }
            (Cella::Testo(t), None) => {
                if sembra_numero_o_data(t) {
                    conta.protetti += 1;
                }
                ws.write_string(r, c, tagliato(t, conta))?;
            }
            (Cella::Numero(v), Some(f)) => {
                ws.write_number_with_format(r, c, *v, f)?;
            }
            (Cella::Numero(v), None) => match grezza.and_then(|g| g.get(i)).and_then(|g| decimali_scritti(g, virgola)) {
                Some(dec) => {
                    ws.write_number_with_format(r, c, *v, &Format::new().set_num_format(format!("0.{}", "0".repeat(dec))))?;
                }
                None => {
                    ws.write_number(r, c, *v)?;
                }
            },
            (Cella::Percento(v, dec), _) => {
                let code = if *dec == 0 { "0%".to_string() } else { format!("0.{}%", "0".repeat(*dec)) };
                ws.write_number_with_format(r, c, *v, &Format::new().set_num_format(code))?;
            }
        }
    }
    Ok(())
}

fn larghezze<'a>(ws: &mut rust_xlsxwriter::Worksheet, rows: impl Iterator<Item = &'a Vec<Cella>>) -> Result<(), rust_xlsxwriter::XlsxError> {
    let mut w: Vec<usize> = Vec::new();
    for row in rows.take(5000) {
        for (c, cella) in row.iter().enumerate() {
            let n = match cella {
                Cella::Vuota => 0,
                Cella::Testo(t) => t.lines().map(|l| l.chars().count()).max().unwrap_or(0),
                other => other.testo(false).len(),
            };
            if w.len() <= c {
                w.resize(c + 1, 0);
            }
            w[c] = w[c].max(n);
        }
    }
    for (c, n) in w.into_iter().enumerate() {
        ws.set_column_width(c as u16, (n.clamp(6, 60) + 2) as f64)?;
    }
    Ok(())
}

// ------------------------------------------------------------------ Excel e Calc in ingresso

/// I fogli di un .xlsx/.xlsm o di un .ods.
pub fn leggi(ext: &str, bytes: &[u8]) -> Result<Vec<Foglio>, String> {
    if ext == "xls" {
        let mut fogli = crate::ufficio::xls::tabella(bytes)?;
        for f in &mut fogli {
            pulisci_bordi(&mut f.righe);
        }
        return Ok(fogli);
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| "il file non è un foglio di calcolo leggibile (forse un .xls vecchio: salvalo in .xlsx)".to_string())?;
    let mut fogli = if ext == "ods" { leggi_ods(&mut zip)? } else { leggi_xlsx(&mut zip)? };
    for f in &mut fogli {
        pulisci_bordi(&mut f.righe);
    }
    Ok(fogli)
}

/// Toglie le celle vuote in fondo alle righe e le righe vuote in fondo.
fn pulisci_bordi(righe: &mut Vec<Vec<Cella>>) {
    for r in righe.iter_mut() {
        while r.last() == Some(&Cella::Vuota) {
            r.pop();
        }
    }
    while righe.last().is_some_and(Vec::is_empty) {
        righe.pop();
    }
}

/// Il limite di un file dentro lo zip: un foglio da centinaia di MB si rifiuta invece di finire la memoria.
const MAX_XML: u64 = 512 << 20;

fn voce(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, nome: &str) -> Option<String> {
    let f = zip.by_name(nome).ok()?;
    let mut s = String::new();
    f.take(MAX_XML).read_to_string(&mut s).ok()?;
    Some(s)
}

fn leggi_xlsx(zip: &mut zip::ZipArchive<Cursor<&[u8]>>) -> Result<Vec<Foglio>, String> {
    let wb = voce(zip, "xl/workbook.xml").ok_or("cartella di lavoro non trovata nel file")?;
    let rels = voce(zip, "xl/_rels/workbook.xml.rels").unwrap_or_default();
    let shared = voce(zip, "xl/sharedStrings.xml").map(|s| stringhe_condivise(&s)).transpose()?.unwrap_or_default();
    let stili = voce(zip, "xl/styles.xml").map(|s| formati_data(&s)).unwrap_or_default();
    let wb_doc = roxmltree::Document::parse(&wb).map_err(|e| format!("cartella di lavoro non letta: {e}"))?;
    let date1904 = wb_doc.descendants().any(|n| n.has_tag_name("workbookPr") && matches!(n.attribute("date1904"), Some("1" | "true")));
    let rel_doc = roxmltree::Document::parse(&rels).ok();
    let mut out = Vec::new();
    for s in wb_doc.descendants().filter(|n| n.has_tag_name("sheet")) {
        let nome = s.attribute("name").unwrap_or("Foglio").to_string();
        let id = s.attributes().find(|a| a.name() == "id").map(|a| a.value()).unwrap_or("");
        let target = rel_doc
            .as_ref()
            .and_then(|d| d.descendants().find(|n| n.has_tag_name("Relationship") && n.attribute("Id") == Some(id)))
            .and_then(|n| n.attribute("Target"))
            .unwrap_or("");
        if target.is_empty() || target.contains("chartsheets/") {
            continue;
        }
        let path = match target.strip_prefix('/') {
            Some(abs) => abs.to_string(),
            None => format!("xl/{target}"),
        };
        let Some(xml) = voce(zip, &path) else { continue };
        let righe = foglio_xlsx(&xml, &shared, &stili, date1904)?;
        out.push(Foglio { nome, righe });
    }
    if out.is_empty() {
        return Err("nessun foglio nel file".into());
    }
    Ok(out)
}

fn testo_di(n: roxmltree::Node) -> String {
    // Il testo dei «t» (anche nelle parti con formati diversi), senza la pronuncia giapponese (rPh).
    let mut s = String::new();
    for d in n.descendants() {
        if d.has_tag_name("t") && !d.ancestors().any(|a| a.has_tag_name("rPh")) {
            s.push_str(d.text().unwrap_or(""));
        }
    }
    s
}

fn stringhe_condivise(xml: &str) -> Result<Vec<String>, String> {
    let doc = roxmltree::Document::parse_with_options(xml, roxmltree::ParsingOptions { nodes_limit: u32::MAX, ..Default::default() }).map_err(|e| format!("testi del foglio non letti: {e}"))?;
    Ok(doc.root_element().children().filter(|n| n.has_tag_name("si")).map(testo_di).collect())
}

/// Per ogni stile di cella: 0 numero, 1 data, 2 ora, 3 data e ora.
fn formati_data(xml: &str) -> Vec<u8> {
    let Ok(doc) = roxmltree::Document::parse(xml) else { return Vec::new() };
    let mut propri: std::collections::HashMap<u32, u8> = std::collections::HashMap::new();
    for f in doc.descendants().filter(|n| n.has_tag_name("numFmt")) {
        if let (Some(id), Some(code)) = (f.attribute("numFmtId").and_then(|v| v.parse().ok()), f.attribute("formatCode")) {
            propri.insert(id, tipo_formato(code));
        }
    }
    let Some(xfs) = doc.descendants().find(|n| n.has_tag_name("cellXfs")) else { return Vec::new() };
    xfs.children()
        .filter(|n| n.has_tag_name("xf"))
        .map(|xf| {
            let id: u32 = xf.attribute("numFmtId").and_then(|v| v.parse().ok()).unwrap_or(0);
            match id {
                14..=17 => 1,
                18..=21 | 45 | 47 => 2,
                46 => 5,
                22 => 3,
                _ => propri.get(&id).copied().unwrap_or(0),
            }
        })
        .collect()
}

fn tipo_formato(code: &str) -> u8 {
    let mut s = String::new();
    let mut quoted = false;
    let mut bracket = false;
    let mut chars = code.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => quoted = !quoted,
            '[' if !quoted => {
                bracket = true;
                // [h], [mm], [ss]: una durata che può superare le 24 ore (tempi di incubazione).
                let dentro: String = chars.clone().take_while(|&c| c != ']').collect::<String>().to_ascii_lowercase();
                if !dentro.is_empty() && (dentro.bytes().all(|b| b == b'h') || dentro.bytes().all(|b| b == b'm') || dentro.bytes().all(|b| b == b's')) {
                    // 4: ore e minuti, 5: anche i secondi.
                    let resto = chars.as_str().split(';').next().unwrap_or("").to_ascii_lowercase();
                    return if dentro.starts_with('s') || resto.contains('s') { 5 } else { 4 };
                }
            }
            ']' if !quoted => bracket = false,
            '\\' | '_' | '*' => {
                chars.next();
            }
            _ if quoted || bracket => {}
            _ => s.push(c.to_ascii_lowercase()),
        }
    }
    // Solo la prima sezione (quella dei numeri positivi).
    let s = s.split(';').next().unwrap_or("");
    let data = s.contains('y') || s.contains('d');
    let ora = s.contains('h') || s.contains('s');
    match (data, ora) {
        (true, true) => 3,
        (true, false) => 1,
        (false, true) => 2,
        _ => 0,
    }
}

/// Il giorno civile da un numero di giorni dal 1970-01-01.
fn giorno(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Il numero di serie di Excel come data ISO (2026-11-11, 2026-11-11 14:30, 14:30:05).
pub fn data_excel(v: f64, tipo: u8, date1904: bool) -> String {
    let mut giorni = v.floor() as i64;
    let mut secondi = ((v - v.floor()) * 86_400.0).round() as i64;
    if secondi >= 86_400 {
        giorni += 1;
        secondi -= 86_400;
    }
    // Il sistema del 1900 conta un 29 febbraio 1900 che non esiste: prima del 1° marzo si aggiusta.
    let base = if date1904 { 24_107 } else if giorni < 61 { 25_568 } else { 25_569 };
    let (y, m, d) = giorno(giorni - base);
    let (h, mi, s) = (secondi / 3600, secondi / 60 % 60, secondi % 60);
    let ora = if s == 0 { format!("{h:02}:{mi:02}") } else { format!("{h:02}:{mi:02}:{s:02}") };
    if tipo == 4 || tipo == 5 {
        // Durata: le ore continuano oltre le 24 (36:00:00), niente data.
        let tot = (v * 86_400.0).round() as i64;
        let (h, mi, s) = (tot / 3600, tot / 60 % 60, tot % 60);
        return if tipo == 4 && s == 0 { format!("{h}:{mi:02}") } else { format!("{h}:{mi:02}:{s:02}") };
    }
    match tipo {
        2 => ora,
        3 if secondi != 0 => format!("{y:04}-{m:02}-{d:02} {ora}"),
        _ => format!("{y:04}-{m:02}-{d:02}"),
    }
}

/// La colonna di un riferimento come «AB12» (0 per A).
fn colonna(r: &str) -> Option<usize> {
    let mut c = 0usize;
    let mut any = false;
    for b in r.bytes() {
        if b.is_ascii_alphabetic() {
            c = c * 26 + (b.to_ascii_uppercase() - b'A' + 1) as usize;
            any = true;
        } else {
            break;
        }
    }
    any.then(|| c - 1)
}

fn foglio_xlsx(xml: &str, shared: &[String], stili: &[u8], date1904: bool) -> Result<Vec<Vec<Cella>>, String> {
    let doc = roxmltree::Document::parse_with_options(xml, roxmltree::ParsingOptions { nodes_limit: u32::MAX, ..Default::default() }).map_err(|e| format!("foglio non letto: {e}"))?;
    let Some(data) = doc.descendants().find(|n| n.has_tag_name("sheetData")) else { return Ok(Vec::new()) };
    let mut righe: Vec<Vec<Cella>> = Vec::new();
    for row in data.children().filter(|n| n.has_tag_name("row")) {
        let r = row.attribute("r").and_then(|v| v.parse::<usize>().ok()).map_or(righe.len(), |v| v.saturating_sub(1));
        if r >= MAX_RIGHE {
            break;
        }
        if righe.len() <= r {
            righe.resize(r + 1, Vec::new());
        }
        let mut celle: Vec<Cella> = Vec::new();
        for c in row.children().filter(|n| n.has_tag_name("c")) {
            let col = c.attribute("r").and_then(colonna).unwrap_or(celle.len());
            if col >= MAX_COLONNE {
                continue;
            }
            let v = c.children().find(|n| n.has_tag_name("v")).and_then(|n| n.text()).unwrap_or("");
            // Una formula senza il risultato salvato (file scritti da script): si tiene la formula.
            let formula = c.children().find(|n| n.has_tag_name("f")).and_then(|n| n.text()).filter(|f| !f.trim().is_empty());
            let cella = match c.attribute("t").unwrap_or("n") {
                _ if v.is_empty() && c.attribute("t") != Some("inlineStr") && formula.is_some() => Cella::Testo(format!("={}", formula.unwrap_or_default())),
                "s" => v.trim().parse::<usize>().ok().and_then(|i| shared.get(i)).map_or(Cella::Vuota, |s| Cella::Testo(s.clone())),
                "inlineStr" => c.children().find(|n| n.has_tag_name("is")).map_or(Cella::Vuota, |n| Cella::Testo(testo_di(n))),
                "b" => Cella::Testo(if v.trim() == "1" { "TRUE" } else { "FALSE" }.into()),
                "str" | "e" | "d" => Cella::Testo(v.to_string()),
                _ if v.trim().is_empty() => Cella::Vuota,
                _ => match v.trim().parse::<f64>() {
                    Ok(x) => {
                        let tipo = c.attribute("s").and_then(|s| s.parse::<usize>().ok()).and_then(|s| stili.get(s)).copied().unwrap_or(0);
                        if tipo != 0 && (0.0..2_958_466.0).contains(&x) {
                            Cella::Testo(data_excel(x, tipo, date1904))
                        } else {
                            Cella::Numero(x)
                        }
                    }
                    Err(_) => Cella::Testo(v.to_string()),
                },
            };
            if celle.len() <= col {
                celle.resize(col + 1, Cella::Vuota);
            }
            celle[col] = cella;
        }
        righe[r] = celle;
    }
    Ok(righe)
}

fn leggi_ods(zip: &mut zip::ZipArchive<Cursor<&[u8]>>) -> Result<Vec<Foglio>, String> {
    let xml = voce(zip, "content.xml").ok_or("contenuto del foglio non trovato")?;
    let doc = roxmltree::Document::parse_with_options(&xml, roxmltree::ParsingOptions { nodes_limit: u32::MAX, ..Default::default() }).map_err(|e| format!("foglio non letto: {e}"))?;
    let num = |n: roxmltree::Node, a: &str| n.attributes().find(|x| x.name() == a).map(|x| x.value().to_string());
    let mut out = Vec::new();
    for t in doc.descendants().filter(|n| n.tag_name().name() == "table" && n.parent().is_some_and(|p| p.tag_name().name() == "spreadsheet")) {
        let nome = num(t, "name").unwrap_or_else(|| "Foglio".into());
        let mut righe: Vec<Vec<Cella>> = Vec::new();
        let mut vuote_in_sospeso = 0usize;
        for row in t.descendants().filter(|n| n.tag_name().name() == "table-row") {
            let ripeti = num(row, "number-rows-repeated").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).max(1);
            let mut celle: Vec<Cella> = Vec::new();
            let mut vuote = 0usize;
            for c in row.children().filter(|n| matches!(n.tag_name().name(), "table-cell" | "covered-table-cell")) {
                let rc = num(c, "number-columns-repeated").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).max(1);
                let cella = cella_ods(c);
                if cella == Cella::Vuota {
                    vuote += rc;
                    continue;
                }
                if celle.len() + vuote + rc > MAX_COLONNE {
                    break;
                }
                celle.extend(std::iter::repeat_n(Cella::Vuota, vuote));
                vuote = 0;
                celle.extend(std::iter::repeat_n(cella, rc));
            }
            if celle.is_empty() {
                vuote_in_sospeso += ripeti;
                continue;
            }
            if righe.len() + vuote_in_sospeso + ripeti > MAX_RIGHE {
                break;
            }
            righe.extend(std::iter::repeat_n(Vec::new(), vuote_in_sospeso));
            vuote_in_sospeso = 0;
            righe.extend(std::iter::repeat_n(celle, ripeti));
        }
        out.push(Foglio { nome, righe });
    }
    if out.is_empty() {
        return Err("nessun foglio nel file".into());
    }
    Ok(out)
}

fn cella_ods(c: roxmltree::Node) -> Cella {
    let attr = |a: &str| c.attributes().find(|x| x.name() == a).map(|x| x.value());
    let testo = || {
        let mut righe = Vec::new();
        for p in c.children().filter(|n| n.tag_name().name() == "p") {
            let mut s = String::new();
            for d in p.descendants() {
                if d.is_text() {
                    s.push_str(d.text().unwrap_or(""));
                } else if d.tag_name().name() == "s" {
                    let n = d.attributes().find(|x| x.name() == "c").and_then(|x| x.value().parse::<usize>().ok()).unwrap_or(1);
                    s.push_str(&" ".repeat(n.min(1000)));
                } else if d.tag_name().name() == "tab" {
                    s.push('\t');
                } else if d.tag_name().name() == "line-break" {
                    s.push('\n');
                }
            }
            righe.push(s);
        }
        righe.join("\n")
    };
    match attr("value-type") {
        Some("float" | "currency") => attr("value").and_then(|v| v.parse().ok()).map_or_else(|| Cella::Testo(testo()), Cella::Numero),
        Some("percentage") => {
            let shown = testo();
            let dec = shown.split([',', '.']).nth(1).map_or(0, |d| d.trim_end_matches('%').trim().len());
            attr("value").and_then(|v| v.parse().ok()).map_or_else(|| Cella::Testo(shown), |v| Cella::Percento(v, dec))
        }
        Some("date") => Cella::Testo(attr("date-value").map(|d| d.replacen('T', " ", 1)).map(|d| d.trim_end_matches(" 00:00:00").to_string()).unwrap_or_else(testo)),
        Some("time") => Cella::Testo(attr("time-value").map(durata_iso).unwrap_or_else(testo)),
        Some("boolean") => Cella::Testo(if attr("boolean-value") == Some("true") { "TRUE" } else { "FALSE" }.into()),
        _ => {
            let t = testo();
            if t.is_empty() {
                Cella::Vuota
            } else {
                Cella::Testo(t)
            }
        }
    }
}

/// PT14H30M05S → 14:30:05.
fn durata_iso(d: &str) -> String {
    let mut h = 0u64;
    let mut m = 0u64;
    let mut s = 0f64;
    let mut num = String::new();
    for c in d.trim_start_matches("PT").chars() {
        match c {
            'H' => h = num.parse().unwrap_or(0),
            'M' => m = num.parse().unwrap_or(0),
            'S' => s = num.parse().unwrap_or(0.0),
            _ => {
                num.push(c);
                continue;
            }
        }
        num.clear();
    }
    let s = s.round() as u64;
    if s == 0 {
        format!("{h:02}:{m:02}")
    } else {
        format!("{h:02}:{m:02}:{s:02}")
    }
}

#[cfg(test)]
mod prove {
    use super::*;

    #[test]
    fn i_numeri_veri_diventano_numeri_e_i_geni_restano_testo() {
        let n = |s| numero(s, true);
        assert_eq!(n("1,5"), Some(Cella::Numero(1.5)));
        assert_eq!(n("-0,25"), Some(Cella::Numero(-0.25)));
        assert_eq!(n("1.234,5"), Some(Cella::Numero(1234.5)));
        assert_eq!(n("1,2E-05"), Some(Cella::Numero(1.2e-5)));
        assert_eq!(n("1E-05"), Some(Cella::Numero(1e-5)));
        assert_eq!(n("12,5%"), Some(Cella::Percento(0.125, 1)));
        assert_eq!(n("42"), Some(Cella::Numero(42.0)));
        for testo in ["MARCH1", "SEPT2", "1-Mar", "2310009E13", "007", "1.234", "12345678901234567890", "2026-11-11", "NA", "A01", "1,2,3"] {
            assert_eq!(n(testo), None, "{testo}");
        }
        assert_eq!(numero("1.5", false), Some(Cella::Numero(1.5)));
        assert_eq!(numero("1,5", false), None);
    }

    #[test]
    fn una_piastra_da_96_diventa_una_riga_per_pozzetto() {
        let mut csv = String::from("Software;Gen5 3.11\nLettura;450 nm\n\n<>;1;2;3;4;5;6;7;8;9;10;11;12\n");
        for (i, r) in "ABCDEFGH".chars().enumerate() {
            csv.push(r);
            for j in 0..12 {
                csv.push_str(&format!(";{},{}", i, j));
            }
            csv.push('\n');
        }
        let sep = separatore(&csv, "csv");
        assert_eq!(sep, ';');
        let rows = crate::textconv::parse_csv(&csv, sep);
        assert!(decimali_con_virgola(&rows));
        let celle = celle(&rows, true);
        let p = piastre(&celle, &rows);
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].righe, p[0].colonne), (8, 12));
        let lunga = per_pozzetto(&p);
        assert_eq!(lunga.len(), 97);
        assert_eq!(lunga[1][0], Cella::Testo("A01".into()));
        assert_eq!(lunga[96][0], Cella::Testo("H12".into()));
        assert_eq!(lunga[96][3], Cella::Numero(7.11));
        assert_eq!(lunga[0][3], Cella::Testo("Valore".into()));
        let (xlsx, frase) = in_excel(&rows, true).unwrap();
        assert!(frase.contains("96 pozzetti"), "{frase}");
        let fogli = leggi("xlsx", &xlsx).unwrap();
        assert_eq!(fogli.len(), 2);
        assert_eq!(fogli[1].nome, "Pozzetti");
        assert_eq!(fogli[1].righe[96][3], Cella::Numero(7.11));
        assert_eq!(fogli[0].righe[3][0], Cella::Testo("A".into()));
        assert_eq!(fogli[0].righe[3][1], Cella::Numero(0.0));
    }

    #[test]
    fn excel_e_ritorno_tengono_geni_codici_e_decimali() {
        let rows: Vec<Vec<String>> = vec![
            vec!["Gene".into(), "Campione".into(), "Ct".into(), "Vitalità".into()],
            vec!["MARCH1".into(), "007".into(), "23,51".into(), "12,5%".into()],
            vec!["SEPT2".into(), "2310009E13".into(), "1,2E-05".into(), "80%".into()],
        ];
        let (xlsx, frase) = in_excel(&rows, true).unwrap();
        assert!(frase.contains("virgola") && frase.contains("geni"), "{frase}");
        let f = &leggi("xlsx", &xlsx).unwrap()[0];
        assert_eq!(f.righe[1][0], Cella::Testo("MARCH1".into()));
        assert_eq!(f.righe[1][1], Cella::Testo("007".into()));
        assert_eq!(f.righe[1][2], Cella::Numero(23.51));
        assert_eq!(f.righe[2][1], Cella::Testo("2310009E13".into()));
        assert_eq!(f.righe[2][2], Cella::Numero(1.2e-5));
        assert_eq!(f.testo(true)[1][2], "23,51");
        assert_eq!(f.testo(false)[1][2], "23.51");
    }

    #[test]
    fn date_di_excel() {
        assert_eq!(data_excel(46_337.0, 1, false), "2026-11-11");
        assert_eq!(data_excel(46_337.604_166_666_7, 3, false), "2026-11-11 14:30");
        assert_eq!(data_excel(0.5, 2, false), "12:00");
        assert_eq!(data_excel(1.0, 1, false), "1900-01-01");
        assert_eq!(tipo_formato("dd/mm/yyyy"), 1);
        assert_eq!(tipo_formato("0.00\" days\""), 0);
        assert_eq!(tipo_formato("[h]:mm:ss"), 5);
        assert_eq!(tipo_formato("[h]:mm"), 4);
        assert_eq!(data_excel(1.5, 5, false), "36:00:00");
        assert_eq!(durata_iso("PT14H30M05S"), "14:30:05");
    }

    #[test]
    fn un_ods_si_legge_con_le_celle_ripetute() {
        let content = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"><office:body><office:spreadsheet>
<table:table table:name="Misure">
<table:table-row><table:table-cell office:value-type="string"><text:p>Campione</text:p></table:table-cell><table:table-cell office:value-type="string"><text:p>OD</text:p></table:table-cell></table:table-row>
<table:table-row><table:table-cell office:value-type="string"><text:p>S1</text:p></table:table-cell><table:table-cell office:value-type="float" office:value="0.512"><text:p>0,512</text:p></table:table-cell><table:table-cell table:number-columns-repeated="1000"/></table:table-row>
<table:table-row table:number-rows-repeated="3"><table:table-cell office:value-type="float" office:value="1"><text:p>1</text:p></table:table-cell></table:table-row>
<table:table-row table:number-rows-repeated="1048570"><table:table-cell table:number-columns-repeated="1024"/></table:table-row>
</table:table></office:spreadsheet></office:body></office:document-content>"#;
        let mut buf = Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default();
            z.start_file("content.xml", o).unwrap();
            std::io::Write::write_all(&mut z, content.as_bytes()).unwrap();
            z.finish().unwrap();
        }
        let f = leggi("ods", buf.get_ref()).unwrap();
        assert_eq!(f[0].nome, "Misure");
        assert_eq!(f[0].righe.len(), 5);
        assert_eq!(f[0].righe[1], vec![Cella::Testo("S1".into()), Cella::Numero(0.512)]);
        assert_eq!(f[0].righe[4], vec![Cella::Numero(1.0)]);
    }
}
