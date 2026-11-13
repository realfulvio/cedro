//! I fogli di calcolo impaginati come li stampa Excel: area di stampa (o le celle usate), pagine
//! prima in giù e poi a destra, titoli ripetuti, adattamento a N pagine, centratura, intestazione e piè
//! di pagina con i codici di Excel (&P, &N, &A…), griglia e intestazioni di riga e colonna se chieste.

use super::font::{FontId, Fonti};
use super::foglio::{Area, Cartella, Foglio, HAlign, Stile, VAlign, Valore};
use super::pagina::{Dash, Link, Mark, Op, Pagina, Rgb, Stampato, Target, BLACK};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Adatta {
    /// Come dice il file (scala o «adatta a» di Excel).
    ComeFile,
    /// Tutte le colonne in una pagina di larghezza.
    Larghezza,
    /// Ogni foglio in una pagina sola.
    Pagina,
    /// Al 100%, anche se il file chiede di rimpicciolire.
    Vera,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orienta {
    ComeFile,
    /// Orizzontale se il foglio è più largo della pagina verticale.
    Auto,
    Verticale,
    Orizzontale,
}

pub struct OpzioniFogli {
    pub solo_attivo: bool,
    pub adatta: Adatta,
    pub orienta: Orienta,
    /// La carta scelta (verticale, punti); `None` quella del file.
    pub carta: Option<(f32, f32)>,
    /// Griglia: `None` come il file.
    pub griglia: Option<bool>,
    /// Ripete la prima riga in cima a ogni pagina se il file non ha titoli suoi.
    pub ripeti_prima: bool,
    /// Il nome del foglio e il numero di pagina in fondo, se il file non ha un piè suo.
    pub piede_nome: bool,
    pub file_name: String,
}

impl Default for OpzioniFogli {
    fn default() -> Self {
        OpzioniFogli { solo_attivo: false, adatta: Adatta::ComeFile, orienta: Orienta::ComeFile, carta: None, griglia: None, ripeti_prima: false, piede_nome: false, file_name: String::new() }
    }
}

/// Spazio tra il testo e il bordo della cella (2 pixel).
const PAD: f32 = 1.5;
/// Excel e Calc stampano la griglia e le intestazioni di riga e colonna in nero, senza sfondo.
const GRID: Rgb = (0, 0, 0);

struct Sheet<'a> {
    f: &'a Foglio,
    book: &'a Cartella,
    rows: Vec<u32>,
    cols: Vec<u32>,
    heights: std::collections::HashMap<u32, f32>,
}

impl Sheet<'_> {
    fn h(&self, r: u32) -> f32 {
        if self.f.row_hidden(r) {
            return 0.0;
        }
        self.heights.get(&r).copied().or_else(|| self.f.righe.get(&r).and_then(|x| x.0)).unwrap_or(self.f.riga_default)
    }
    fn w(&self, c: u32) -> f32 {
        self.f.col_width(c)
    }
    fn style(&self, r: u32, c: u32) -> &Stile {
        let i = self.f.celle.get(&(r, c)).map_or_else(|| self.f.empty_style(r, c), |x| x.stile);
        self.book.stile(i)
    }
}

/// Le righe (o colonne) di ogni pagina: gruppi che stanno in `avail`, con i salti manuali.
fn groups(items: &[u32], size: impl Fn(u32) -> f32, avail: f32, breaks: &[u32], use_breaks: bool) -> Vec<Vec<u32>> {
    let mut out: Vec<Vec<u32>> = Vec::new();
    let mut cur: Vec<u32> = Vec::new();
    let mut used = 0.0f32;
    for &i in items {
        let s = size(i);
        let manual = use_breaks && breaks.contains(&i) && !cur.is_empty();
        if manual || (!cur.is_empty() && used + s > avail + 0.01) {
            out.push(std::mem::take(&mut cur));
            used = 0.0;
        }
        cur.push(i);
        used += s;
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    if out.is_empty() {
        out.push(Vec::new());
    }
    out
}

/// Il testo di una cella com'è mostrata e il colore chiesto dal formato.
fn shown(book: &Cartella, v: &Valore, mostra: Option<&String>, st: &Stile) -> (String, Option<Rgb>) {
    let l = super::numfmt::locale();
    if let Some(m) = mostra {
        return (m.clone(), None);
    }
    match v {
        Valore::Vuoto => (String::new(), None),
        Valore::Numero(x) => super::numfmt::format(*x, &st.numfmt, book.date1904, l),
        Valore::Testo(t) => (if st.numfmt.contains('@') || st.numfmt.contains(';') { super::numfmt::format_text(t, &st.numfmt) } else { t.clone() }, None),
        Valore::Bool(b) => ((if *b { if crate::i18n::english() { "TRUE" } else { "VERO" } } else if crate::i18n::english() { "FALSE" } else { "FALSO" }).to_string(), None),
        Valore::Errore(e) => (e.clone(), None),
    }
}

/// Spezza un testo nelle righe che stanno in `w` punti (a capo del file compresi).
fn wrap(fonts: &mut Fonti, font: FontId, size: f32, text: &str, w: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for para in text.split('\n') {
        let mut cur = String::new();
        for word in para.split(' ') {
            let cand = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
            if fonts.width(font, &cand, size) <= w || cur.is_empty() {
                cur = cand;
                // Una parola più larga della cella va a capo a metà.
                while fonts.width(font, &cur, size) > w && cur.chars().count() > 1 {
                    let mut head = String::new();
                    for ch in cur.chars() {
                        if fonts.width(font, &format!("{head}{ch}"), size) > w && !head.is_empty() {
                            break;
                        }
                        head.push(ch);
                    }
                    let rest = cur[head.len()..].to_string();
                    lines.push(head);
                    cur = rest;
                }
            } else {
                lines.push(std::mem::replace(&mut cur, word.to_string()));
            }
        }
        lines.push(cur);
    }
    lines
}

/// I pezzi di un'intestazione o di un piè di Excel: sinistra, centro, destra.
fn header_parts(code: &str, page: u32, pages: u32, sheet: &str, file: &str) -> [String; 3] {
    let mut parts = [String::new(), String::new(), String::new()];
    let mut cur = 1usize;
    let (date, time) = {
        let (d, _) = crate::pdfdoc::date_now();
        // D:AAAAMMGGhhmmssZ
        let d = d.trim_start_matches("D:");
        let g = |a: usize, b: usize| d.get(a..b).unwrap_or("");
        (format!("{}/{}/{}", g(6, 8), g(4, 6), g(0, 4)), format!("{}:{}", g(8, 10), g(10, 12)))
    };
    let mut chars = code.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '&' {
            parts[cur].push(c);
            continue;
        }
        match chars.next() {
            Some('L') => cur = 0,
            Some('C') => cur = 1,
            Some('R') => cur = 2,
            Some('P') => {
                // &P+1, &P-1
                let mut n = page as i64;
                if let Some(&s) = chars.peek() {
                    if s == '+' || s == '-' {
                        chars.next();
                        let d: String = std::iter::from_fn(|| chars.next_if(|c| c.is_ascii_digit())).collect();
                        let k: i64 = d.parse().unwrap_or(0);
                        n += if s == '+' { k } else { -k };
                    }
                }
                parts[cur].push_str(&n.to_string());
            }
            Some('N') => parts[cur].push_str(&pages.to_string()),
            Some('A') => parts[cur].push_str(sheet),
            Some('F') => parts[cur].push_str(file),
            Some('Z') => {}
            Some('D') => parts[cur].push_str(&date),
            Some('T') => parts[cur].push_str(&time),
            Some('&') => parts[cur].push('&'),
            Some('"') => {
                // &"Arial,Grassetto": il carattere, ignorato.
                for x in chars.by_ref() {
                    if x == '"' {
                        break;
                    }
                }
            }
            Some(d) if d.is_ascii_digit() => {
                while chars.next_if(|c| c.is_ascii_digit()).is_some() {}
            }
            Some('K') => {
                for _ in 0..6 {
                    chars.next();
                }
            }
            Some(_) | None => {}
        }
    }
    parts
}

struct Placed {
    /// Le pagine di un foglio: (righe, colonne) per pagina.
    pages: Vec<(Vec<u32>, Vec<u32>)>,
    scale: f32,
    paper: (f32, f32),
    /// Righe e colonne dei titoli.
    title_rows: Vec<u32>,
    title_cols: Vec<u32>,
    head_w: f32,
    head_h: f32,
}

fn plan(s: &Sheet, area: Area, o: &OpzioniFogli) -> Placed {
    let f = s.f;
    let st = &f.stampa;
    let base_paper = o.carta.or(st.carta).unwrap_or((595.0, 842.0));
    let title_rows: Vec<u32> = match st.titoli_righe.or_else(|| o.ripeti_prima.then_some((area.0, area.0))) {
        Some((a, b)) => (a..=b.min(area.2)).filter(|r| !f.row_hidden(*r)).collect(),
        None => Vec::new(),
    };
    let title_cols: Vec<u32> = match st.titoli_colonne {
        Some((a, b)) => (a..=b.min(area.3)).filter(|c| !f.col_hidden(*c)).collect(),
        None => Vec::new(),
    };
    let total_w: f32 = s.cols.iter().map(|c| s.w(*c)).sum();
    let total_h: f32 = s.rows.iter().map(|r| s.h(*r)).sum();
    let (head_w, head_h) = if st.intestazioni { (24.0 + 6.0 * (area.2 + 1).to_string().len() as f32, f.riga_default) } else { (0.0, 0.0) };
    let [ml, mr, mt, mb, ..] = st.margini;
    let printable = |landscape: bool| {
        let (pw, ph) = if landscape { (base_paper.1, base_paper.0) } else { base_paper };
        (pw - ml - mr, ph - mt - mb)
    };
    let landscape = match o.orienta {
        Orienta::Verticale => false,
        Orienta::Orizzontale => true,
        Orienta::Auto => total_w + head_w > printable(false).0 + 0.5,
        Orienta::ComeFile => st.orizzontale.unwrap_or(false),
    };
    let paper = if landscape { (base_paper.1, base_paper.0) } else { base_paper };
    let (avail_w, avail_h) = printable(landscape);
    let tw: f32 = title_cols.iter().map(|c| s.w(*c)).sum::<f32>() + head_w;
    let th: f32 = title_rows.iter().map(|r| s.h(*r)).sum::<f32>() + head_h;
    let fit = match o.adatta {
        Adatta::ComeFile => st.adatta,
        Adatta::Larghezza => Some((1, 0)),
        Adatta::Pagina => Some((1, 1)),
        Adatta::Vera => None,
    };
    let scale = match (o.adatta, fit) {
        (Adatta::Vera, _) => 1.0,
        (_, Some((wp, hp))) => {
            let mut s1 = 1.0f32;
            if wp > 0 && total_w > 0.0 {
                s1 = s1.min((avail_w * wp as f32) / (total_w + tw * wp as f32));
            }
            if hp > 0 && total_h > 0.0 {
                s1 = s1.min((avail_h * hp as f32) / (total_h + th * hp as f32));
            }
            // Excel scende a percentuali intere, non sotto il 10%.
            ((s1 * 100.0).floor() / 100.0).clamp(0.1, 1.0)
        }
        _ => st.scala as f32 / 100.0,
    };
    let use_breaks = fit.is_none();
    let col_groups = groups(&s.cols, |c| s.w(c), avail_w / scale - tw, &st.salti_colonne, use_breaks);
    let row_groups = groups(&s.rows, |r| s.h(r), avail_h / scale - th, &st.salti_righe, use_breaks);
    let mut pages = Vec::new();
    if st.giu_poi_destra {
        for cg in &col_groups {
            for rg in &row_groups {
                pages.push((rg.clone(), cg.clone()));
            }
        }
    } else {
        for rg in &row_groups {
            for cg in &col_groups {
                pages.push((rg.clone(), cg.clone()));
            }
        }
    }
    Placed { pages, scale, paper, title_rows, title_cols, head_w, head_h }
}

fn col_name(mut c: u32) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (c % 26) as u8) as char);
        if c < 26 {
            break;
        }
        c = c / 26 - 1;
    }
    s
}

/// L'ultima colonna da stampare contando il testo che esce a destra delle celle (come Calc).
fn overflow_edge(book: &Cartella, f: &Foglio, area: Area, fonts: &mut Fonti) -> u32 {
    let mut edge = area.3;
    for (&(r, c), cell) in f.celle.range((area.0, 0)..=(area.2, u32::MAX)) {
        if c < area.1 || c > area.3 || f.row_hidden(r) {
            continue;
        }
        let Valore::Testo(t) = &cell.valore else { continue };
        let st = book.stile(cell.stile);
        if st.wrap || !matches!(st.h, HAlign::General | HAlign::Left) || f.unioni.iter().any(|u| u.0 <= r && r <= u.2 && u.1 <= c && c <= u.3) {
            continue;
        }
        let font = fonts.get(&st.font.nome, st.font.bold, st.font.italic);
        let mut need = fonts.width(font, t.lines().next().unwrap_or(""), st.font.size) + 2.0 * PAD;
        let mut x = c;
        need -= f.col_width(x);
        // Il testo passa sopra le celle vuote a destra, finché serve.
        while need > 0.0 && x + 1 < super::foglio::MAX_COLS && !f.celle.get(&(r, x + 1)).is_some_and(|n| n.valore != Valore::Vuoto) {
            x += 1;
            need -= f.col_width(x);
        }
        edge = edge.max(x);
    }
    edge
}

/// Impagina i fogli della cartella. `fonts` misura e poi resta nel documento.
pub fn impagina(book: &Cartella, mut fonts: Fonti, o: &OpzioniFogli, title: &str) -> Result<Stampato, String> {
    let mut out: Vec<Pagina> = Vec::new();
    let mut images = Vec::new();
    let chosen: Vec<usize> = if o.solo_attivo {
        vec![book.attivo.min(book.fogli.len().saturating_sub(1))]
    } else {
        (0..book.fogli.len()).filter(|i| !book.fogli[*i].nascosto).collect()
    };
    // Prima si impaginano tutti i fogli, poi si scrivono: &N è il totale delle pagine.
    let mut plans = Vec::new();
    for &i in &chosen {
        let f = &book.fogli[i];
        let used = f.used(&book.stili);
        let Some(mut area) = f.stampa.area.or(used) else { continue };
        // Un'area a righe o colonne intere ($1:$1048576) si ferma all'ultima cella usata, come in Excel.
        if let (Some(_), Some(u)) = (f.stampa.area, used) {
            if area.2 >= super::foglio::MAX_ROWS - 1 {
                area.2 = u.2.max(area.0);
            }
            if area.3 >= super::foglio::MAX_COLS - 1 {
                area.3 = u.3.max(area.1);
            }
        } else if f.stampa.area.is_some() && used.is_none() {
            continue;
        }
        let mut area = (area.0, area.1, area.2.min(super::foglio::MAX_ROWS - 1), area.3.min(super::foglio::MAX_COLS - 1));
        if book.testo_fuori && f.stampa.area.is_none() {
            area.3 = overflow_edge(book, f, area, &mut fonts);
        }
        let rows: Vec<u32> = (area.0..=area.2).filter(|r| !f.row_hidden(*r)).collect();
        let cols: Vec<u32> = (area.1..=area.3).filter(|c| !f.col_hidden(*c)).collect();
        let mut sheet = Sheet { f, book, rows, cols, heights: std::collections::HashMap::new() };
        autofit_rows(&mut sheet, &mut fonts);
        let p = plan(&sheet, area, o);
        plans.push((sheet, area, p));
    }
    let total: u32 = plans.iter().map(|(_, _, p)| p.pages.len() as u32).sum();
    let mut number = 0u32;
    for (sheet, area, p) in &plans {
        let f = sheet.f;
        let first_of_sheet = out.len();
        if let Some(n) = f.stampa.primo_numero {
            number = n.saturating_sub(1);
        }
        for (rows, cols) in &p.pages {
            number += 1;
            let mut page = Pagina::new(p.paper.0, p.paper.1);
            if out.len() == first_of_sheet {
                page.marks.push(Mark { level: 1, title: f.nome.clone(), y: f.stampa.margini[2] });
            }
            draw_page(sheet, *area, p, rows, cols, &mut page, &mut fonts, &mut images, o);
            let [_, _, _, _, mh, mf] = f.stampa.margini;
            let footer = if f.stampa.piede.is_empty() && o.piede_nome {
                format!("&L&A&R{}", if crate::i18n::english() { "Page &P of &N" } else { "Pagina &P di &N" })
            } else {
                f.stampa.piede.clone()
            };
            for (code, top) in [(&f.stampa.testa, true), (&footer, false)] {
                if code.is_empty() {
                    continue;
                }
                let parts = header_parts(code, number, total, &f.nome, &o.file_name);
                let font = fonts.get("Calibri", false, false);
                let size = 10.0;
                let (asc, desc) = (fonts.face(font).ascent * size / 1000.0, fonts.face(font).descent * size / 1000.0);
                let [ml, mr, ..] = f.stampa.margini;
                for (k, text) in parts.iter().enumerate() {
                    for (n, line) in text.split('\n').enumerate() {
                        if line.trim().is_empty() {
                            continue;
                        }
                        let w = fonts.width(font, line, size);
                        let x = match k {
                            0 => ml,
                            1 => (p.paper.0 - w) / 2.0,
                            _ => p.paper.0 - mr - w,
                        };
                        let lines = text.split('\n').count() as f32;
                        let y = if top { mh + asc + n as f32 * size * 1.2 } else { p.paper.1 - mf - desc - (lines - 1.0 - n as f32) * size * 1.2 };
                        push_text(&mut page, &mut fonts, font, x, y, size, BLACK, line);
                    }
                }
            }
            out.push(page);
        }
    }
    if out.is_empty() {
        return Err("nel foglio non c'è niente da stampare".into());
    }
    Ok(Stampato { pages: out, images, fonts, title: title.to_string() })
}

/// Le righe senza altezza scritta con testo a capo: alte quanto serve, come fa Excel.
fn autofit_rows(s: &mut Sheet, fonts: &mut Fonti) {
    let f = s.f;
    let mut fit: std::collections::HashMap<u32, f32> = std::collections::HashMap::new();
    for ((r, c), cell) in f.celle.range((s.rows.first().copied().unwrap_or(0), 0)..) {
        if s.rows.last().is_some_and(|l| r > l) {
            break;
        }
        if f.righe.get(r).and_then(|x| x.0).is_some() {
            continue;
        }
        let st = s.book.stile(cell.stile);
        let (text, _) = shown(s.book, &cell.valore, cell.mostra.as_ref(), st);
        let font = fonts.get(&st.font.nome, st.font.bold, st.font.italic);
        let line = fonts.face(font).line(st.font.size);
        let lines = if st.wrap { wrap(fonts, font, st.font.size, &text, (f.col_width(*c) - 2.0 * PAD).max(1.0)).len() } else { text.split('\n').count().max(1) };
        let need = (lines as f32 * line + 1.5).max(f.riga_default);
        if need > f.riga_default + 0.5 || st.font.size > 11.5 {
            let e = fit.entry(*r).or_insert(f.riga_default);
            *e = e.max(need.min(409.0));
        }
    }
    s.heights = fit;
}

fn push_text(page: &mut Pagina, fonts: &mut Fonti, font: FontId, x: f32, y: f32, size: f32, color: Rgb, text: &str) -> f32 {
    let mut cx = x;
    for (f, part) in fonts.runs(font, text) {
        let w = fonts.face(f).width(&part, size);
        page.ops.push(Op::Text { x: cx, y, font: f, size, color, text: part });
        cx += w;
    }
    cx - x
}

#[allow(clippy::too_many_arguments)]
fn draw_page(s: &Sheet, area: Area, p: &Placed, rows: &[u32], cols: &[u32], page: &mut Pagina, fonts: &mut Fonti, images: &mut Vec<crate::pdfdoc::DocImage>, o: &OpzioniFogli) {
    let f = s.f;
    let st = &f.stampa;
    let k = p.scale;
    let [ml, mr, mt, mb, ..] = st.margini;
    // Le righe e le colonne della pagina, con i titoli davanti se non ci sono già.
    let mut prows: Vec<u32> = Vec::new();
    if rows.first().is_some_and(|r0| p.title_rows.iter().all(|t| t < r0)) {
        prows.extend(&p.title_rows);
    }
    prows.extend(rows);
    let mut pcols: Vec<u32> = Vec::new();
    if cols.first().is_some_and(|c0| p.title_cols.iter().all(|t| t < c0)) {
        pcols.extend(&p.title_cols);
    }
    pcols.extend(cols);
    let content_w = p.head_w + pcols.iter().map(|c| s.w(*c)).sum::<f32>();
    let content_h = p.head_h + prows.iter().map(|r| s.h(*r)).sum::<f32>();
    let mut x0 = ml;
    let mut y0 = mt;
    if st.centra_o {
        x0 = ml + ((p.paper.0 - ml - mr) - content_w * k).max(0.0) / 2.0;
    }
    if st.centra_v {
        y0 = mt + ((p.paper.1 - mt - mb) - content_h * k).max(0.0) / 2.0;
    }
    // Posizioni (in punti della pagina) di ogni riga e colonna.
    let mut xs: Vec<(u32, f32, f32)> = Vec::new();
    let mut x = x0 + p.head_w * k;
    for &c in &pcols {
        let w = s.w(c) * k;
        xs.push((c, x, w));
        x += w;
    }
    let mut ys: Vec<(u32, f32, f32)> = Vec::new();
    let mut y = y0 + p.head_h * k;
    for &r in &prows {
        let h = s.h(r) * k;
        ys.push((r, y, h));
        y += h;
    }
    let (left, top) = (x0 + p.head_w * k, y0 + p.head_h * k);
    let (right, bottom) = (x, y);
    let col_at = |c: u32| xs.iter().find(|e| e.0 == c).map(|e| (e.1, e.2));
    let row_at = |r: u32| ys.iter().find(|e| e.0 == r).map(|e| (e.1, e.2));
    let grid = o.griglia.unwrap_or(st.griglia);

    // Le celle unite che toccano la pagina: (area, rettangolo intero, anche fuori pagina).
    let merges: Vec<(Area, f32, f32, f32, f32)> = f
        .unioni
        .iter()
        .filter(|m| prows.iter().any(|r| (m.0..=m.2).contains(r)) && pcols.iter().any(|c| (m.1..=m.3).contains(c)))
        .filter_map(|m| {
            // Il rettangolo della cella unita sulla pagina: dalla prima riga e colonna visibili.
            let vr: Vec<&(u32, f32, f32)> = ys.iter().filter(|e| (m.0..=m.2).contains(&e.0)).collect();
            let vc: Vec<&(u32, f32, f32)> = xs.iter().filter(|e| (m.1..=m.3).contains(&e.0)).collect();
            let (r0, r1) = (vr.first()?, vr.last()?);
            let (c0, c1) = (vc.first()?, vc.last()?);
            // La parte della cella unita prima della pagina sposta il testo come in Excel.
            let before_w: f32 = (m.1..c0.0).filter(|c| !f.col_hidden(*c)).map(|c| s.w(c) * k).sum();
            let before_h: f32 = (m.0..r0.0).filter(|r| !f.row_hidden(*r)).map(|r| s.h(r) * k).sum();
            let after_w: f32 = (c1.0 + 1..=m.3).filter(|c| !f.col_hidden(*c)).map(|c| s.w(c) * k).sum();
            let after_h: f32 = (r1.0 + 1..=m.2).filter(|r| !f.row_hidden(*r)).map(|r| s.h(r) * k).sum();
            Some((*m, c0.1 - before_w, r0.1 - before_h, c1.1 + c1.2 - c0.1 + before_w + after_w, r1.1 + r1.2 - r0.1 + before_h + after_h))
        })
        .collect();
    let merge_of = |r: u32, c: u32| merges.iter().find(|m| (m.0 .0..=m.0 .2).contains(&r) && (m.0 .1..=m.0 .3).contains(&c));

    page.ops.push(Op::Clip { x: left - 2.0, y: top - 2.0, w: right - left + 4.0, h: bottom - top + 4.0 });
    // 1. sfondi
    for &(r, ry, rh) in &ys {
        for &(c, cx, cw) in &xs {
            if let Some(m) = merge_of(r, c) {
                if (r, c) == (m.0 .0.max(ys[0].0.min(m.0 .0)), m.0 .1) || (r == ys.iter().find(|e| (m.0 .0..=m.0 .2).contains(&e.0)).map_or(r, |e| e.0) && c == xs.iter().find(|e| (m.0 .1..=m.0 .3).contains(&e.0)).map_or(c, |e| e.0)) {
                    if let Some(fill) = s.book.stile(f.celle.get(&(m.0 .0, m.0 .1)).map_or(0, |x| x.stile)).fill {
                        page.ops.push(Op::Rect { x: m.1, y: m.2, w: m.3, h: m.4, color: fill });
                    }
                }
                continue;
            }
            if let Some(fill) = s.style(r, c).fill {
                page.ops.push(Op::Rect { x: cx, y: ry, w: cw, h: rh, color: fill });
            }
        }
    }
    // 2. griglia
    if grid {
        for &(r, ry, rh) in &ys {
            for &(c, cx, cw) in &xs {
                let inside_right = merge_of(r, c).is_some_and(|m| c < m.0 .3);
                let inside_bottom = merge_of(r, c).is_some_and(|m| r < m.0 .2);
                if !inside_right {
                    page.ops.push(Op::Line { x1: cx + cw, y1: ry, x2: cx + cw, y2: ry + rh, width: 0.25, color: GRID, dash: Dash::Solid });
                }
                if !inside_bottom {
                    page.ops.push(Op::Line { x1: cx, y1: ry + rh, x2: cx + cw, y2: ry + rh, width: 0.25, color: GRID, dash: Dash::Solid });
                }
            }
        }
        page.ops.push(Op::Line { x1: left, y1: top, x2: right, y2: top, width: 0.25, color: GRID, dash: Dash::Solid });
        page.ops.push(Op::Line { x1: left, y1: top, x2: left, y2: bottom, width: 0.25, color: GRID, dash: Dash::Solid });
    }
    // 3. testo
    for &(r, ry, rh) in &ys {
        for (ci, &(c, cx, cw)) in xs.iter().enumerate() {
            let (cell_r, cell_c, bx, by, bw, bh) = match merge_of(r, c) {
                Some(m) => {
                    let first_r = ys.iter().find(|e| (m.0 .0..=m.0 .2).contains(&e.0)).map_or(r, |e| e.0);
                    let first_c = xs.iter().find(|e| (m.0 .1..=m.0 .3).contains(&e.0)).map_or(c, |e| e.0);
                    if (r, c) != (first_r, first_c) {
                        continue;
                    }
                    (m.0 .0, m.0 .1, m.1, m.2, m.3, m.4)
                }
                None => (r, c, cx, ry, cw, rh),
            };
            let Some(cell) = f.celle.get(&(cell_r, cell_c)) else { continue };
            let st = s.book.stile(cell.stile);
            let (text, fmt_color) = shown(s.book, &cell.valore, cell.mostra.as_ref(), st);
            if text.is_empty() {
                continue;
            }
            let size = st.font.size * k;
            let font = fonts.get(&st.font.nome, st.font.bold, st.font.italic);
            let color = fmt_color.unwrap_or(st.font.color);
            let face_asc = fonts.face(font).ascent * size / 1000.0;
            let face_desc = fonts.face(font).descent * size / 1000.0;
            let line_h = fonts.face(font).line(size);
            let numeric = matches!(cell.valore, Valore::Numero(_)) && cell.mostra.is_none() || cell.mostra.is_some() && matches!(cell.valore, Valore::Numero(_));
            let halign = match st.h {
                HAlign::General if numeric => HAlign::Right,
                HAlign::General if matches!(cell.valore, Valore::Bool(_) | Valore::Errore(_)) => HAlign::Center,
                HAlign::General => HAlign::Left,
                h => h,
            };
            let indent = st.indent as f32 * 7.5 * k;
            let pad = PAD * k;
            let inner_w = (bw - 2.0 * pad - indent).max(1.0);
            let mut size = size;
            let mut lines: Vec<String> = if st.wrap || halign == HAlign::Justify {
                wrap(fonts, font, size, &text, inner_w)
            } else {
                text.split('\n').map(str::to_string).collect()
            };
            if st.shrink && !st.wrap {
                let w = lines.iter().map(|l| fonts.width(font, l, size)).fold(0.0, f32::max);
                if w > inner_w {
                    size *= inner_w / w;
                }
            }
            // Un numero che non sta nella colonna: «####», come Excel.
            if numeric && !st.wrap && merge_of(r, c).is_none() {
                let w = fonts.width(font, &lines[0], size);
                if w > inner_w + pad {
                    let hash_w = fonts.width(font, "#", size).max(0.1);
                    lines = vec!["#".repeat(((bw - 2.0 * pad) / hash_w).floor().max(1.0) as usize)];
                }
            }
            if halign == HAlign::Fill {
                let w = fonts.width(font, &lines[0], size).max(0.1);
                let n = (inner_w / w).floor().max(1.0) as usize;
                lines = vec![lines[0].repeat(n)];
            }
            let total_h = lines.len() as f32 * line_h;
            let first_base = match st.v {
                VAlign::Top => by + pad * 0.5 + face_asc,
                VAlign::Center => by + (bh - total_h) / 2.0 + face_asc,
                VAlign::Bottom => by + bh - total_h - face_desc * 0.15 + face_asc - 0.5 * k,
            };
            // Il testo che non va a capo esce nelle celle vuote accanto (solo testo, non numeri).
            let (mut clip_l, mut clip_r) = (bx, bx + bw);
            if !st.wrap && !numeric && merge_of(r, c).is_none() {
                let widest = lines.iter().map(|l| fonts.width(font, l, size)).fold(0.0, f32::max);
                if widest > inner_w {
                    let free = |cc: u32| f.celle.get(&(r, cc)).is_none_or(|x| matches!(&x.valore, Valore::Vuoto)) && merge_of(r, cc).is_none();
                    if matches!(halign, HAlign::Left | HAlign::Center | HAlign::CenterAcross) {
                        let mut j = ci + 1;
                        while j < xs.len() && clip_r - bx < widest + 2.0 * pad + indent && free(xs[j].0) {
                            clip_r = xs[j].1 + xs[j].2;
                            j += 1;
                        }
                        if j == xs.len() && clip_r - bx < widest + 2.0 * pad {
                            clip_r = right.max(clip_r);
                        }
                    }
                    if matches!(halign, HAlign::Right | HAlign::Center | HAlign::CenterAcross) {
                        let mut j = ci;
                        while j > 0 && bx + bw - clip_l < widest + 2.0 * pad && free(xs[j - 1].0) {
                            clip_l = xs[j - 1].1;
                            j -= 1;
                        }
                    }
                }
            }
            // «Centra nelle colonne»: il centro delle celle vuote che seguono, nello stesso stile.
            let (mut ax, mut aw) = (bx, bw);
            if halign == HAlign::CenterAcross {
                let mut j = ci + 1;
                while j < xs.len() && f.celle.get(&(r, xs[j].0)).is_none_or(|x| matches!(x.valore, Valore::Vuoto)) && s.style(r, xs[j].0).h == HAlign::CenterAcross {
                    aw += xs[j].2;
                    j += 1;
                }
                clip_r = clip_r.max(ax + aw);
                let _ = &mut ax;
            }
            page.ops.push(Op::Clip { x: clip_l, y: by, w: clip_r - clip_l, h: bh });
            for (n, line) in lines.iter().enumerate() {
                let w = fonts.width(font, line, size);
                let tx = match halign {
                    HAlign::Right => ax + aw - pad - indent - w,
                    HAlign::Center | HAlign::CenterAcross => ax + (aw - w) / 2.0,
                    _ => ax + pad + indent,
                };
                let base = first_base + n as f32 * line_h;
                let drawn = push_text(page, fonts, font, tx, base, size, color, line);
                if st.font.underline {
                    page.ops.push(Op::Line { x1: tx, y1: base + size * 0.12, x2: tx + drawn, y2: base + size * 0.12, width: size * 0.06, color, dash: Dash::Solid });
                }
                if st.font.strike {
                    page.ops.push(Op::Line { x1: tx, y1: base - size * 0.28, x2: tx + drawn, y2: base - size * 0.28, width: size * 0.06, color, dash: Dash::Solid });
                }
                if let Some(link) = &cell.link {
                    page.links.push(Link { x: tx, y: base - face_asc, w: drawn, h: line_h, target: Target::Uri(link.clone()) });
                }
            }
            page.ops.push(Op::Unclip);
        }
    }
    // 4. immagini
    for (ir, ic, dx, dy, iw, ih, img) in &f.immagini {
        let (Some((cx, _)), Some((ry, _))) = (col_at(*ic), row_at(*ir)) else { continue };
        let index = images.len();
        images.push(img.clone());
        page.ops.push(Op::Image { index, x: cx + dx * k, y: ry + dy * k, w: iw * k, h: ih * k });
    }
    // 5. bordi (sopra tutto, come in Excel)
    for &(r, ry, rh) in &ys {
        for &(c, cx, cw) in &xs {
            let st = s.style(r, c);
            let m = merge_of(r, c);
            let edge = |side: usize| -> bool {
                match m {
                    None => true,
                    Some(m) => match side {
                        0 => c == m.0 .1,
                        1 => c == m.0 .3,
                        2 => r == m.0 .0,
                        _ => r == m.0 .2,
                    },
                }
            };
            // Il bordo di una cella unita sta sulle celle del suo contorno; Excel lo prende dalla prima.
            let src = |side: usize| -> Option<super::foglio::Bordo> {
                let own = st.bordi[side];
                if own.is_some() || m.is_none() {
                    return own;
                }
                let m = m.expect("unita");
                s.book.stile(f.celle.get(&(m.0 .0, m.0 .1)).map_or(0, |x| x.stile)).bordi[side]
            };
            // Il bordo a destra di una cella vale anche come sinistro della cella dopo (e così sotto).
            let next_right = s.style(r, c + 1).bordi[0];
            let next_down = s.style(r + 1, c).bordi[2];
            let sides = [
                (0usize, src(0), cx, ry, cx, ry + rh),
                (1, src(1).or(if m.is_none() { next_right } else { None }), cx + cw, ry, cx + cw, ry + rh),
                (2, src(2), cx, ry, cx + cw, ry),
                (3, src(3).or(if m.is_none() { next_down } else { None }), cx, ry + rh, cx + cw, ry + rh),
            ];
            for (side, b, x1, y1, x2, y2) in sides {
                let Some(b) = b else { continue };
                if !edge(side) {
                    continue;
                }
                let w = (b.width * k).max(0.25);
                if b.double {
                    let (ox, oy) = if side < 2 { (0.75, 0.0) } else { (0.0, 0.75) };
                    page.ops.push(Op::Line { x1: x1 - ox, y1: y1 - oy, x2: x2 - ox, y2: y2 - oy, width: 0.4, color: b.color, dash: Dash::Solid });
                    page.ops.push(Op::Line { x1: x1 + ox, y1: y1 + oy, x2: x2 + ox, y2: y2 + oy, width: 0.4, color: b.color, dash: Dash::Solid });
                } else {
                    page.ops.push(Op::Line { x1, y1, x2, y2, width: w, color: b.color, dash: b.dash });
                }
            }
        }
    }
    page.ops.push(Op::Unclip);
    // 6. intestazioni di riga e colonna (A, B, C… e 1, 2, 3…)
    if st.intestazioni {
        let normal = &s.book.stile(0).font;
        let font = fonts.get(&normal.nome, false, false);
        let size = normal.size.min(10.0) * k;
        let hy = y0;
        let hh = p.head_h * k;
        for &(c, cx, cw) in &xs {
            let t = col_name(c);
            let w = fonts.width(font, &t, size);
            push_text(page, fonts, font, cx + (cw - w) / 2.0, hy + hh * 0.72, size, BLACK, &t);
            page.ops.push(Op::Line { x1: cx + cw, y1: hy, x2: cx + cw, y2: hy + hh, width: 0.25, color: GRID, dash: Dash::Solid });
        }
        for &(r, ry, rh) in &ys {
            let t = (r + 1).to_string();
            let w = fonts.width(font, &t, size);
            push_text(page, fonts, font, x0 + (p.head_w * k - w) / 2.0, ry + rh * 0.72, size, BLACK, &t);
            page.ops.push(Op::Line { x1: x0, y1: ry + rh, x2: x0 + p.head_w * k, y2: ry + rh, width: 0.25, color: GRID, dash: Dash::Solid });
        }
    }
    let _ = area;
}

#[cfg(test)]
mod tests {
    use super::super::foglio::{Bordo, Carattere, Cella, Stampa};
    use super::*;

    fn book() -> Cartella {
        let mut f = Foglio::new("Spese");
        let bold = 1u32;
        let money = 2u32;
        // Colonna B larga 15 caratteri (110 pixel), come in un foglio vero di importi.
        f.colonne.insert(1, (Some(82.5), false, 0));
        f.celle.insert((0, 0), Cella { valore: Valore::Testo("Voce".into()), stile: bold, mostra: None, link: None });
        f.celle.insert((0, 1), Cella { valore: Valore::Testo("Importo".into()), stile: bold, mostra: None, link: None });
        for r in 1..120u32 {
            f.celle.insert((r, 0), Cella { valore: Valore::Testo(format!("Riga numero {r} con un testo lungo che esce")), stile: 0, mostra: None, link: None });
            f.celle.insert((r, 1), Cella { valore: Valore::Numero(r as f64 * 1234.5), stile: money, mostra: None, link: None });
        }
        f.unioni.push((121, 0, 121, 3));
        f.celle.insert((121, 0), Cella { valore: Valore::Testo("Totale unito".into()), stile: bold, mostra: None, link: None });
        f.stampa = Stampa { titoli_righe: Some((0, 0)), piede: "&CPagina &P di &N".into(), ..Stampa::default() };
        let mut hidden = Foglio::new("Nascosto");
        hidden.nascosto = true;
        hidden.celle.insert((0, 0), Cella { valore: Valore::Testo("segreto".into()), stile: 0, mostra: None, link: None });
        let thin = Bordo { width: 0.5, color: BLACK, dash: Dash::Solid, double: false };
        let stili = vec![
            Stile::default(),
            Stile { font: Carattere { bold: true, ..Carattere::default() }, fill: Some((221, 235, 247)), bordi: [Some(thin); 4], ..Stile::default() },
            Stile { numfmt: "#,##0.00 \"€\"".into(), ..Stile::default() },
        ];
        Cartella { fogli: vec![f, hidden], stili, attivo: 0, date1904: false, testo_fuori: false }
    }

    fn texts(p: &Pagina) -> String {
        p.ops.iter().filter_map(|o| if let Op::Text { text, .. } = o { Some(text.as_str()) } else { None }).collect::<Vec<_>>().join("|")
    }

    #[test]
    fn pages_titles_formats_and_footer() {
        let doc = impagina(&book(), Fonti::builtin(), &OpzioniFogli::default(), "spese").unwrap();
        assert!(doc.pages.len() >= 3, "{}", doc.pages.len());
        let p2 = texts(&doc.pages[1]);
        // La riga dei titoli torna in cima alla seconda pagina; i numeri hanno il formato del file.
        assert!(p2.starts_with("Voce|Importo"), "{p2}");
        let all: String = doc.pages.iter().map(texts).collect();
        assert!(all.contains("1.234,50 €"), "{all}");
        assert!(!all.contains("segreto"));
        assert!(all.contains(&format!("Pagina 2 di {}", doc.pages.len())), "{all}");
        assert_eq!(doc.pages[0].marks[0].title, "Spese");
        let pdf = super::super::pdf::scrivi(&doc, &super::super::pdf::Opzioni { embed: true, bookmarks: true, pdfa: false, links: true }).unwrap();
        assert!(lopdf::Document::load_mem(&pdf).unwrap().extract_text(&[1]).unwrap().contains("Importo"));
    }

    #[test]
    fn fit_one_page_and_orientation() {
        let o = OpzioniFogli { adatta: Adatta::Pagina, orienta: Orienta::Orizzontale, ..OpzioniFogli::default() };
        let doc = impagina(&book(), Fonti::builtin(), &o, "spese").unwrap();
        assert_eq!(doc.pages.len(), 1);
        assert!(doc.pages[0].w > doc.pages[0].h);
        let only_active = OpzioniFogli { solo_attivo: true, adatta: Adatta::Vera, ..OpzioniFogli::default() };
        assert!(impagina(&book(), Fonti::builtin(), &only_active, "spese").unwrap().pages.len() >= 3);
    }

    #[test]
    fn header_codes() {
        let p = header_parts("&L&A&C&\"Arial,Bold\"&14Titolo && co.&RPag. &P/&N", 2, 5, "Foglio1", "conti.xlsx");
        assert_eq!(p, ["Foglio1".to_string(), "Titolo & co.".to_string(), "Pag. 2/5".to_string()]);
        assert_eq!(col_name(0), "A");
        assert_eq!(col_name(27), "AB");
    }
}
