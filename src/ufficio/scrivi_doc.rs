//! Il documento (letto da Word, Writer, RTF o DOC) riscritto in un altro formato: testo semplice,
//! Markdown, pagina HTML, Word (.docx) e Writer (.odt). Gli ultimi due tengono pagina, margini,
//! caratteri, paragrafi, tabelle, immagini, note, intestazioni e piè di pagina; le etichette degli
//! elenchi restano come sono scritte nel file di partenza.

use std::collections::HashMap;
use std::fmt::Write as _;

use super::documento::*;
use super::pagina::{Dash, Rgb, Target};

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            // I caratteri di controllo non sono ammessi in XML.
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' => {}
            c => o.push(c),
        }
    }
    o
}

fn hex(c: Rgb) -> String {
    format!("{:02X}{:02X}{:02X}", c.0, c.1, c.2)
}

fn tw(pt: f32) -> i32 {
    (pt * 20.0).round() as i32
}

fn emu(pt: f32) -> i64 {
    (pt * 12700.0).round() as i64
}

/// L'immagine com'è nel file da scrivere: JPEG così com'è, il resto in PNG.
fn file_immagine(img: &crate::pdfdoc::DocImage) -> (Vec<u8>, &'static str) {
    if img.jpeg {
        return (img.data.clone(), "jpeg");
    }
    let mut out = Vec::new();
    if let Some(rgb) = image::RgbImage::from_raw(img.px.0, img.px.1, img.data.clone()) {
        let _ = rgb.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png);
    }
    (out, "png")
}

fn mime(ext: &str) -> &'static str {
    if ext == "jpeg" {
        "image/jpeg"
    } else {
        "image/png"
    }
}

/// La larghezza a occhio di una cornice larga quanto il suo testo (la misura vera la fa il programma
/// che apre il file; qui serve un valore di partenza che non tagli il testo).
fn larghezza_testo(blocchi: &[Blocco]) -> f32 {
    let mut w: f32 = 0.0;
    for b in blocchi {
        if let Blocco::Par(p) = b {
            let mut riga: f32 = p.rientro_sx.max(0.0) + p.rientro_dx.max(0.0);
            for r in &p.runs {
                let n = match &r.pezzo {
                    Pezzo::Testo(t) => t.chars().count() as f32,
                    Pezzo::NumPagina | Pezzo::NumPagine => 3.0,
                    Pezzo::Tab => 4.0,
                    Pezzo::Immagine { w, .. } => w / r.font.size.max(1.0) / 0.6,
                    _ => 0.0,
                };
                riga += n * r.font.size * 0.6;
            }
            w = w.max(riga);
        }
    }
    w + 2.0
}

/// Il testo di una nota senza il suo numero davanti (i lettori lo scrivono come testo).
fn testo_nota(numero: &str, blocchi: &[Blocco]) -> String {
    let t = testo_blocchi(blocchi, "\n");
    let t = t.trim_start();
    t.strip_prefix(numero).unwrap_or(t).trim().to_string()
}

fn testo_run(doc: &Documento, r: &Run, out: &mut String) {
    match &r.pezzo {
        Pezzo::Testo(t) => out.push_str(t),
        Pezzo::Tab => out.push('\t'),
        Pezzo::ACapo => out.push('\n'),
        Pezzo::Nota(i) => {
            let _ = write!(out, "[{}]", doc.note.get(*i).map_or("", |n| n.0.as_str()));
        }
        Pezzo::NotaFine(i) => {
            let _ = write!(out, "[{}]", doc.note_fine.get(*i).map_or("", |n| n.0.as_str()));
        }
        _ => {}
    }
}

fn testo_blocchi(blocchi: &[Blocco], sep: &str) -> String {
    let doc = Documento::default();
    let mut s = String::new();
    for b in blocchi {
        match b {
            Blocco::Par(p) => {
                if !s.is_empty() {
                    s.push_str(sep);
                }
                for r in &p.runs {
                    testo_run(&doc, r, &mut s);
                }
            }
            Blocco::Tab(t) => {
                for r in &t.righe {
                    let celle: Vec<String> = r.celle.iter().filter(|c| c.vmerge != Some(false)).map(|c| testo_blocchi(&c.blocchi, " ")).collect();
                    if !s.is_empty() {
                        s.push_str(sep);
                    }
                    s.push_str(&celle.join("\t"));
                }
            }
            Blocco::Mobile(m) => {
                if let MobileContenuto::Testo(b, _, _) | MobileContenuto::Cornice(b) = &m.contenuto {
                    let t = testo_blocchi(b, sep);
                    if !t.is_empty() {
                        if !s.is_empty() {
                            s.push_str(sep);
                        }
                        s.push_str(&t);
                    }
                }
            }
        }
    }
    s
}

// ------------------------------------------------------------------------ testo semplice

/// Testo semplice: un paragrafo per riga, le celle separate da tabulazioni, le note in fondo.
pub fn testo(doc: &Documento) -> String {
    let mut out = String::new();
    for sez in &doc.sezioni {
        testo_sezione(doc, &sez.blocchi, &mut out);
    }
    for (titolo, note) in [("Note", &doc.note), ("Note di chiusura", &doc.note_fine)] {
        if note.is_empty() {
            continue;
        }
        let _ = write!(out, "\n{titolo}\n");
        for (n, b) in note.iter() {
            let _ = writeln!(out, "[{n}] {}", testo_nota(n, b));
        }
    }
    let t = out.trim_end().to_string();
    t + "\n"
}

fn testo_sezione(doc: &Documento, blocchi: &[Blocco], out: &mut String) {
    for b in blocchi {
        match b {
            Blocco::Par(p) => {
                if let Some((e, _)) = &p.etichetta {
                    out.push_str(e);
                    out.push(if p.etichetta_tab { '\t' } else { ' ' });
                }
                for r in &p.runs {
                    testo_run(doc, r, out);
                }
                out.push('\n');
            }
            Blocco::Tab(t) => {
                for r in &t.righe {
                    let celle: Vec<String> = r.celle.iter().filter(|c| c.vmerge != Some(false)).map(|c| testo_blocchi(&c.blocchi, " ").replace(['\t', '\n'], " ")).collect();
                    out.push_str(&celle.join("\t"));
                    out.push('\n');
                }
            }
            Blocco::Mobile(m) => {
                if let MobileContenuto::Testo(b, _, _) | MobileContenuto::Cornice(b) = &m.contenuto {
                    testo_sezione(doc, b, out);
                }
            }
        }
    }
}

// ------------------------------------------------------------------------ Markdown

fn md_esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '*' | '_' | '`' | '[' | ']' | '<' | '>' | '|') {
            o.push('\\');
        }
        o.push(c);
    }
    o
}

fn link_url(t: &Target) -> String {
    match t {
        Target::Uri(u) => u.clone(),
        Target::Anchor(a) => format!("#{a}"),
    }
}

/// I pezzi di un paragrafo in Markdown: grassetto, corsivo, barrato, collegamenti, note.
fn md_runs(doc: &Documento, runs: &[Run], tabella: bool) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < runs.len() {
        let r = &runs[i];
        let chiave = |r: &Run| (r.font.bold, r.font.italic, r.font.strike, r.link.clone());
        // I pezzi di testo vicini con lo stesso aspetto si scrivono insieme.
        let mut testo = String::new();
        let mut j = i;
        while j < runs.len() && chiave(&runs[j]) == chiave(r) {
            match &runs[j].pezzo {
                Pezzo::Testo(t) => testo.push_str(&md_esc(t)),
                Pezzo::Tab => testo.push(' '),
                Pezzo::ACapo => testo.push_str(if tabella { "<br>" } else { "  \n" }),
                Pezzo::Nota(k) => {
                    let _ = write!(testo, "[^{}]", doc.note.get(*k).map_or(String::new(), |n| n.0.clone()));
                }
                Pezzo::NotaFine(k) => {
                    let _ = write!(testo, "[^f{}]", k + 1);
                }
                _ => {}
            }
            j += 1;
        }
        i = j;
        if testo.trim().is_empty() {
            out.push_str(&testo);
            continue;
        }
        let mut segno = String::new();
        if r.font.bold {
            segno.push_str("**");
        }
        if r.font.italic {
            segno.push('*');
        }
        if r.font.strike {
            segno.push_str("~~");
        }
        // I segni vanno attaccati al testo: gli spazi restano fuori.
        let inizio = testo.len() - testo.trim_start().len();
        let fine = testo.trim_end().len();
        let (a, corpo, z) = (&testo[..inizio], &testo[inizio..fine], &testo[fine..]);
        let chiuso: String = segno.chars().rev().collect();
        let corpo = format!("{segno}{corpo}{chiuso}");
        let corpo = match &r.link {
            Some(t) => format!("[{corpo}]({})", link_url(t)),
            None => corpo,
        };
        let _ = write!(out, "{a}{corpo}{z}");
    }
    out
}

/// Markdown: titoli, elenchi, tabelle, grassetti e corsivi, collegamenti, note.
pub fn markdown(doc: &Documento) -> String {
    let mut out = String::new();
    for sez in &doc.sezioni {
        md_blocchi(doc, &sez.blocchi, &mut out);
    }
    if !doc.note.is_empty() || !doc.note_fine.is_empty() {
        out.push('\n');
        for (n, b) in &doc.note {
            let _ = writeln!(out, "[^{n}]: {}", md_esc(&testo_nota(n, b)).replace('\n', " "));
        }
        for (k, (n, b)) in doc.note_fine.iter().enumerate() {
            let _ = writeln!(out, "[^f{}]: {}", k + 1, md_esc(&testo_nota(n, b)).replace('\n', " "));
        }
    }
    let t = out.trim_end().to_string();
    t + "\n"
}

fn md_blocchi(doc: &Documento, blocchi: &[Blocco], out: &mut String) {
    let mut in_elenco = false;
    for b in blocchi {
        match b {
            Blocco::Par(p) => {
                let testo = md_runs(doc, &p.runs, false);
                let elenco = p.etichetta.is_some();
                if !elenco && testo.trim().is_empty() {
                    continue;
                }
                if !(elenco && in_elenco) && !out.is_empty() {
                    out.push('\n');
                }
                in_elenco = elenco;
                if let Some(n) = p.titolo.filter(|_| !elenco) {
                    let _ = writeln!(out, "{} {}", "#".repeat(n.clamp(1, 6) as usize), testo.trim());
                    continue;
                }
                if let Some((e, _)) = &p.etichetta {
                    // Il livello dell'elenco dal rientro; numeri veri per le etichette che finiscono con . o ).
                    let livello = ((p.rientro_sx - 18.0) / 36.0).round().max(0.0) as usize;
                    let e = e.trim();
                    let numerata = e.chars().next().is_some_and(|c| c.is_ascii_alphanumeric()) && (e.ends_with('.') || e.ends_with(')'));
                    let segno = if numerata {
                        let n: String = e.chars().filter(|c| c.is_ascii_digit()).collect();
                        if n.is_empty() || n.len() != e.len() - 1 {
                            format!("- {} ", md_esc(e))
                        } else {
                            format!("{n}. ")
                        }
                    } else {
                        "- ".to_string()
                    };
                    let _ = writeln!(out, "{}{segno}{}", "   ".repeat(livello), testo.trim());
                    continue;
                }
                let _ = writeln!(out, "{}", testo.trim());
            }
            Blocco::Tab(t) => {
                in_elenco = false;
                if !out.is_empty() {
                    out.push('\n');
                }
                let righe: Vec<Vec<String>> = t
                    .righe
                    .iter()
                    .map(|r| {
                        let mut celle = Vec::new();
                        for c in &r.celle {
                            let testo = if c.vmerge == Some(false) {
                                String::new()
                            } else {
                                let parti: Vec<String> = c.blocchi.iter().filter_map(|b| if let Blocco::Par(p) = b { Some(md_runs(doc, &p.runs, true).trim().to_string()) } else { None }).collect();
                                parti.join("<br>")
                            };
                            celle.push(testo);
                            for _ in 1..c.span.max(1) {
                                celle.push(String::new());
                            }
                        }
                        celle
                    })
                    .collect();
                let n = righe.iter().map(|r| r.len()).max().unwrap_or(0).max(1);
                for (k, r) in righe.iter().enumerate() {
                    let mut r = r.clone();
                    r.resize(n, String::new());
                    let _ = writeln!(out, "| {} |", r.join(" | "));
                    if k == 0 {
                        let _ = writeln!(out, "|{}", " --- |".repeat(n));
                    }
                }
            }
            Blocco::Mobile(m) => {
                if let MobileContenuto::Testo(b, _, _) | MobileContenuto::Cornice(b) = &m.contenuto {
                    md_blocchi(doc, b, out);
                }
            }
        }
    }
}

// ------------------------------------------------------------------------ HTML

struct Html<'a> {
    doc: &'a Documento,
    img: Vec<Option<String>>,
    out: String,
}

fn css_linea(l: &Option<Linea>) -> String {
    match l {
        Some(l) => format!("{:.2}pt {} #{}", l.width.max(0.5), if l.double { "double" } else if l.dash != Dash::Solid { "dashed" } else { "solid" }, hex(l.color)),
        None => "none".into(),
    }
}

impl Html<'_> {
    fn src(&mut self, i: usize) -> Option<String> {
        if self.img.len() < self.doc.immagini.len() {
            self.img.resize(self.doc.immagini.len(), None);
        }
        if self.img.get(i)?.is_none() {
            use base64::Engine;
            let (dati, ext) = file_immagine(self.doc.immagini.get(i)?);
            self.img[i] = Some(format!("data:{};base64,{}", mime(ext), base64::engine::general_purpose::STANDARD.encode(dati)));
        }
        self.img[i].clone()
    }

    fn font_css(f: &Font) -> String {
        let mut s = format!("font-family:'{}';font-size:{}pt", f.family.replace('\'', ""), f.size);
        if f.bold {
            s.push_str(";font-weight:bold");
        }
        if f.italic {
            s.push_str(";font-style:italic");
        }
        let mut dec = Vec::new();
        if f.underline || f.double_underline {
            dec.push("underline");
        }
        if f.strike {
            dec.push("line-through");
        }
        if !dec.is_empty() {
            let _ = write!(s, ";text-decoration:{}", dec.join(" "));
            if f.double_underline {
                s.push_str(";text-decoration-style:double");
            }
        }
        if f.color != (0, 0, 0) {
            let _ = write!(s, ";color:#{}", hex(f.color));
        }
        if let Some(h) = f.highlight {
            let _ = write!(s, ";background:#{}", hex(h));
        }
        match f.vert {
            1 => s.push_str(";vertical-align:super;font-size:smaller"),
            -1 => s.push_str(";vertical-align:sub;font-size:smaller"),
            _ => {}
        }
        if f.caps {
            s.push_str(";text-transform:uppercase");
        }
        if f.small_caps {
            s.push_str(";font-variant:small-caps");
        }
        if f.spacing.abs() > 0.01 {
            let _ = write!(s, ";letter-spacing:{}pt", f.spacing);
        }
        s
    }

    fn runs(&mut self, p: &Paragrafo) {
        if let Some((e, f)) = &p.etichetta {
            let _ = write!(self.out, "<span style=\"{}\">{}</span>{}", Self::font_css(f), esc(e), if p.etichetta_tab { "&emsp;" } else { " " });
        }
        let mut i = 0;
        while i < p.runs.len() {
            let r = &p.runs[i];
            // I pezzi vicini con lo stesso carattere e lo stesso collegamento in un solo <span>.
            let mut j = i;
            let mut corpo = String::new();
            while j < p.runs.len() && p.runs[j].font == r.font && p.runs[j].link == r.link {
                match &p.runs[j].pezzo {
                    Pezzo::Testo(t) => {
                        // Gli spazi in fila restano tutti.
                        let mut prima = ' ';
                        for c in esc(t).chars() {
                            if c == ' ' && prima == ' ' {
                                corpo.push('\u{a0}');
                            } else {
                                corpo.push(c);
                            }
                            prima = c;
                        }
                    }
                    Pezzo::Tab => corpo.push_str("&emsp;"),
                    Pezzo::ACapo => corpo.push_str("<br>"),
                    Pezzo::Immagine { img, w, h } => {
                        if let Some(src) = self.src(*img) {
                            let _ = write!(corpo, "<img src=\"{src}\" alt=\"\" style=\"width:{w:.1}pt;height:{h:.1}pt;vertical-align:baseline\">");
                        }
                    }
                    Pezzo::Nota(k) => {
                        let n = self.doc.note.get(*k).map_or(String::new(), |n| esc(&n.0));
                        let _ = write!(corpo, "<sup><a href=\"#nota{}\" id=\"rif{}\">{n}</a></sup>", k + 1, k + 1);
                    }
                    Pezzo::NotaFine(k) => {
                        let n = self.doc.note_fine.get(*k).map_or(String::new(), |n| esc(&n.0));
                        let _ = write!(corpo, "<sup><a href=\"#notaf{}\" id=\"riff{}\">{n}</a></sup>", k + 1, k + 1);
                    }
                    _ => {}
                }
                j += 1;
            }
            i = j;
            if corpo.is_empty() {
                continue;
            }
            let span = format!("<span style=\"{}\">{corpo}</span>", Self::font_css(&r.font));
            match &r.link {
                Some(t) => {
                    let _ = write!(self.out, "<a href=\"{}\">{span}</a>", esc(&link_url(t)));
                }
                None => self.out.push_str(&span),
            }
        }
    }

    fn par(&mut self, p: &Paragrafo) {
        let tag = match p.titolo {
            Some(n) if (1..=6).contains(&n) => format!("h{n}"),
            _ => "p".into(),
        };
        let mut st = format!("margin:{}pt {}pt {}pt {}pt", p.prima, p.rientro_dx, p.dopo, p.rientro_sx);
        if p.prima_riga.abs() > 0.01 {
            let _ = write!(st, ";text-indent:{}pt", p.prima_riga);
        }
        match p.allinea {
            Allinea::Centro => st.push_str(";text-align:center"),
            Allinea::Destra => st.push_str(";text-align:right"),
            Allinea::Giustifica => st.push_str(";text-align:justify"),
            Allinea::Sinistra => {}
        }
        match p.interlinea {
            Interlinea::Multipla(m) => {
                let _ = write!(st, ";line-height:{:.2}", m * 1.15);
            }
            Interlinea::Esatta(v) => {
                let _ = write!(st, ";line-height:{v}pt");
            }
            Interlinea::Minima(v) => {
                let _ = write!(st, ";line-height:max({v}pt,1.15em)");
            }
        }
        if let Some(c) = p.sfondo {
            let _ = write!(st, ";background:#{}", hex(c));
        }
        for (k, lato) in ["top", "bottom", "left", "right"].iter().enumerate() {
            if p.bordi[k].is_some() {
                let _ = write!(st, ";border-{lato}:{}", css_linea(&p.bordi[k]));
            }
        }
        if p.salto_prima {
            st.push_str(";break-before:page");
        }
        if p.runs.is_empty() && p.etichetta.is_none() {
            let _ = write!(st, ";min-height:{}pt", p.font_fine.size * 1.15);
        }
        let _ = write!(self.out, "<{tag} style=\"{st}\">");
        for s in &p.segnalibri {
            let _ = write!(self.out, "<a id=\"{}\"></a>", esc(s));
        }
        self.runs(p);
        let _ = writeln!(self.out, "</{tag}>");
    }

    fn tabella(&mut self, t: &Tabella) {
        let margine = match t.allinea {
            1 => "margin-left:auto;margin-right:auto".to_string(),
            2 => "margin-left:auto;margin-right:0".to_string(),
            _ => format!("margin-left:{}pt", t.rientro.max(0.0)),
        };
        let _ = writeln!(self.out, "<table style=\"border-collapse:collapse;{margine}\">");
        self.out.push_str("<colgroup>");
        for g in &t.griglia {
            let _ = write!(self.out, "<col style=\"width:{g:.1}pt\">");
        }
        self.out.push_str("</colgroup>\n");
        let colonne = posizioni(t);
        for (ri, r) in t.righe.iter().enumerate() {
            let alt = if r.altezza.abs() > 0.5 { format!(" style=\"height:{:.1}pt\"", r.altezza.abs()) } else { String::new() };
            let _ = write!(self.out, "<tr{alt}>");
            for (ci, c) in r.celle.iter().enumerate() {
                if c.vmerge == Some(false) {
                    continue;
                }
                let col = colonne[ri][ci];
                let righe = if c.vmerge == Some(true) { righe_unite(t, &colonne, ri, col) } else { 1 };
                let mut st = format!("padding:{}pt {}pt {}pt {}pt;vertical-align:{}", c.margini[2], c.margini[1], c.margini[3], c.margini[0], ["top", "middle", "bottom"][c.valign.min(2) as usize]);
                for (k, lato) in ["top", "bottom", "left", "right"].iter().enumerate() {
                    let _ = write!(st, ";border-{lato}:{}", css_linea(&c.bordi[k]));
                }
                if let Some(s) = c.sfondo {
                    let _ = write!(st, ";background:#{}", hex(s));
                }
                let span = if c.span > 1 { format!(" colspan=\"{}\"", c.span) } else { String::new() };
                let rs = if righe > 1 { format!(" rowspan=\"{righe}\"") } else { String::new() };
                let _ = write!(self.out, "<td{span}{rs} style=\"{st}\">");
                self.blocchi(&c.blocchi);
                self.out.push_str("</td>");
            }
            self.out.push_str("</tr>\n");
        }
        self.out.push_str("</table>\n");
    }

    fn blocchi(&mut self, blocchi: &[Blocco]) {
        for b in blocchi {
            match b {
                Blocco::Par(p) => self.par(p),
                Blocco::Tab(t) => self.tabella(t),
                Blocco::Mobile(m) => match &m.contenuto {
                    MobileContenuto::Immagine(i) => {
                        if let Some(src) = self.src(*i) {
                            let float = match m.x {
                                Riferimento::Allineato(_, 2) => "float:right;margin:0 0 6pt 9pt",
                                Riferimento::Allineato(_, 1) => "display:block;margin:6pt auto",
                                _ if m.avvolge == Avvolge::Lati => "float:left;margin:0 9pt 6pt 0",
                                _ => "display:block;margin:6pt 0",
                            };
                            let _ = writeln!(self.out, "<img src=\"{src}\" alt=\"\" style=\"width:{:.1}pt;height:{:.1}pt;{float}\">", m.w, m.h);
                        }
                    }
                    MobileContenuto::Testo(b, fill, line) => {
                        let mut st = format!("width:{:.1}pt;padding:3.6pt 7.2pt;margin:6pt 0", m.w.max(20.0));
                        if let Some(c) = fill {
                            let _ = write!(st, ";background:#{}", hex(*c));
                        }
                        if line.is_some() {
                            let _ = write!(st, ";border:{}", css_linea(line));
                        }
                        let _ = write!(self.out, "<div style=\"{st}\">");
                        self.blocchi(b);
                        self.out.push_str("</div>\n");
                    }
                    MobileContenuto::Cornice(b) => self.blocchi(b),
                },
            }
        }
    }
}

/// La colonna della griglia dove comincia ogni cella.
fn posizioni(t: &Tabella) -> Vec<Vec<usize>> {
    t.righe
        .iter()
        .map(|r| {
            let mut col = 0;
            r.celle
                .iter()
                .map(|c| {
                    let a = col;
                    col += c.span.max(1);
                    a
                })
                .collect()
        })
        .collect()
}

/// Quante righe occupa una cella che comincia un'unione verticale.
fn righe_unite(t: &Tabella, colonne: &[Vec<usize>], ri: usize, col: usize) -> usize {
    let mut n = 1;
    for (k, r) in t.righe.iter().enumerate().skip(ri + 1) {
        let segue = r.celle.iter().enumerate().any(|(ci, c)| colonne[k][ci] == col && c.vmerge == Some(false));
        if !segue {
            break;
        }
        n += 1;
    }
    n
}

/// Una pagina HTML sola, con le immagini dentro: si apre e si stampa da qualunque browser.
pub fn html(doc: &Documento, titolo: &str) -> String {
    let sez = doc.sezioni.first();
    let larga = sez.map_or(468.0, |s| s.w - s.margini[0] - s.margini[1]).max(200.0);
    let mut h = Html { doc, img: Vec::new(), out: String::new() };
    for (k, s) in doc.sezioni.iter().enumerate() {
        if k > 0 && s.pagina_nuova {
            h.out.push_str("<div style=\"break-before:page\"></div>\n");
        }
        h.blocchi(&s.blocchi);
    }
    for (classe, note, pre) in [("note", &doc.note, "nota"), ("note-fine", &doc.note_fine, "notaf")] {
        if note.is_empty() {
            continue;
        }
        let _ = writeln!(h.out, "<section class=\"{classe}\"><hr>");
        for (k, (n, b)) in note.iter().enumerate() {
            let _ = write!(h.out, "<div id=\"{pre}{}\"><a href=\"#{}{}\">{}</a> ", k + 1, if pre == "nota" { "rif" } else { "riff" }, k + 1, esc(n));
            let testo = testo_nota(n, b);
            let _ = writeln!(h.out, "{}</div>", esc(&testo));
        }
        h.out.push_str("</section>\n");
    }
    format!(
        "<!DOCTYPE html>\n<html lang=\"it\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{}</title>\n<style>\nbody{{max-width:{larga:.0}pt;margin:2em auto;padding:0 1em;color:#000;background:#fff}}\np,h1,h2,h3,h4,h5,h6{{font-size:inherit;font-weight:inherit}}\ntable{{border-collapse:collapse}}\ntd>p:first-child{{margin-top:0}}\nsection.note,section.note-fine{{font-size:9pt;margin-top:2em}}\n@media print{{body{{margin:0;max-width:none}}}}\n</style>\n</head>\n<body>\n{}</body>\n</html>\n",
        esc(titolo),
        h.out
    )
}

// ------------------------------------------------------------------------ Word (.docx)

const W_NS: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\" xmlns:wps=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\"";

struct Word<'a> {
    doc: &'a Documento,
    /// Relazioni del pezzo che si scrive (documento, intestazione, piè): id → (tipo, destinazione, esterna).
    rels: Vec<(String, String, bool)>,
    /// Immagine del documento → nome del file in word/media.
    media: HashMap<usize, String>,
    file_media: Vec<(String, Vec<u8>)>,
    id_disegno: u32,
    id_segno: u32,
}

const REL_IMG: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const REL_LINK: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";

fn w_linea(tag: &str, l: &Option<Linea>) -> String {
    match l {
        Some(l) => {
            let val = if l.double {
                "double"
            } else {
                match l.dash {
                    Dash::Solid => "single",
                    Dash::Dotted => "dotted",
                    _ => "dashed",
                }
            };
            format!("<w:{tag} w:val=\"{val}\" w:sz=\"{}\" w:space=\"{}\" w:color=\"{}\"/>", ((l.width * 8.0).round() as i32).clamp(2, 96), l.space.round() as i32, hex(l.color))
        }
        None => format!("<w:{tag} w:val=\"nil\"/>"),
    }
}

fn w_rpr(f: &Font) -> String {
    let fam = esc(&f.family);
    let mut s = format!("<w:rPr><w:rFonts w:ascii=\"{fam}\" w:hAnsi=\"{fam}\" w:cs=\"{fam}\" w:eastAsia=\"{fam}\"/>");
    s.push_str(if f.bold { "<w:b/><w:bCs/>" } else { "<w:b w:val=\"0\"/>" });
    s.push_str(if f.italic { "<w:i/><w:iCs/>" } else { "<w:i w:val=\"0\"/>" });
    if f.caps {
        s.push_str("<w:caps/>");
    }
    if f.small_caps {
        s.push_str("<w:smallCaps/>");
    }
    if f.strike {
        s.push_str("<w:strike/>");
    }
    let _ = write!(s, "<w:color w:val=\"{}\"/>", hex(f.color));
    if f.spacing.abs() > 0.01 {
        let _ = write!(s, "<w:spacing w:val=\"{}\"/>", tw(f.spacing));
    }
    let hp = (f.size * 2.0).round() as i32;
    let _ = write!(s, "<w:sz w:val=\"{hp}\"/><w:szCs w:val=\"{hp}\"/>");
    if let Some(h) = f.highlight {
        let _ = write!(s, "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{}\"/>", hex(h));
    }
    if f.double_underline {
        s.push_str("<w:u w:val=\"double\"/>");
    } else if f.underline {
        s.push_str("<w:u w:val=\"single\"/>");
    }
    match f.vert {
        1 => s.push_str("<w:vertAlign w:val=\"superscript\"/>"),
        -1 => s.push_str("<w:vertAlign w:val=\"subscript\"/>"),
        _ => {}
    }
    s.push_str("</w:rPr>");
    s
}

fn w_testo(t: &str) -> String {
    format!("<w:t xml:space=\"preserve\">{}</w:t>", esc(t))
}

impl Word<'_> {
    fn rel(&mut self, tipo: &str, target: &str, esterna: bool) -> String {
        if let Some(k) = self.rels.iter().position(|r| r.0 == tipo && r.1 == target) {
            return format!("rId{}", k + 100);
        }
        self.rels.push((tipo.to_string(), target.to_string(), esterna));
        format!("rId{}", self.rels.len() + 99)
    }

    fn immagine_rel(&mut self, i: usize) -> Option<String> {
        if !self.media.contains_key(&i) {
            let img = self.doc.immagini.get(i)?;
            let (dati, ext) = file_immagine(img);
            if dati.is_empty() {
                return None;
            }
            let nome = format!("image{}.{ext}", self.file_media.len() + 1);
            self.file_media.push((nome.clone(), dati));
            self.media.insert(i, nome);
        }
        let nome = self.media[&i].clone();
        Some(self.rel(REL_IMG, &format!("media/{nome}"), false))
    }

    fn grafica(&mut self, rid: &str, w: f32, h: f32) -> String {
        self.id_disegno += 1;
        let id = self.id_disegno;
        format!(
            "<a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic><pic:nvPicPr><pic:cNvPr id=\"{id}\" name=\"Immagine {id}\"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed=\"{rid}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic>",
            emu(w),
            emu(h)
        )
    }

    fn inline(&mut self, i: usize, w: f32, h: f32) -> String {
        let Some(rid) = self.immagine_rel(i) else { return String::new() };
        let g = self.grafica(&rid, w, h);
        let id = self.id_disegno;
        format!("<w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"{}\" cy=\"{}\"/><wp:docPr id=\"{id}\" name=\"Immagine {id}\"/><wp:cNvGraphicFramePr/>{g}</wp:inline></w:drawing>", emu(w), emu(h))
    }

    fn ancora(&mut self, m: &Mobile) -> String {
        let pos = |r: &Riferimento, oriz: bool| -> String {
            let (base, dentro) = match r {
                Riferimento::Spostato(b, v) => (*b, format!("<wp:posOffset>{}</wp:posOffset>", emu(*v))),
                Riferimento::Allineato(b, a) => {
                    let v = match (oriz, a) {
                        (true, 0) => "left",
                        (true, 1) => "center",
                        (true, _) => "right",
                        (false, 0) => "top",
                        (false, 1) => "center",
                        (false, _) => "bottom",
                    };
                    (*b, format!("<wp:align>{v}</wp:align>"))
                }
            };
            let rel = match (base, oriz) {
                (Base::Pagina, _) => "page",
                (Base::Margine, _) => "margin",
                (Base::Colonna, true) | (Base::Paragrafo, true) => "column",
                (_, false) => "paragraph",
            };
            let tag = if oriz { "positionH" } else { "positionV" };
            format!("<wp:{tag} relativeFrom=\"{rel}\">{dentro}</wp:{tag}>")
        };
        let avvolge = match m.avvolge {
            Avvolge::Lati => "<wp:wrapSquare wrapText=\"bothSides\"/>",
            Avvolge::SopraSotto => "<wp:wrapTopAndBottom/>",
            Avvolge::Nessuno => "<wp:wrapNone/>",
        };
        let (w, h) = (m.w.max(1.0), m.h.abs().max(1.0));
        let grafica = match &m.contenuto {
            MobileContenuto::Immagine(i) => {
                let Some(rid) = self.immagine_rel(*i) else { return String::new() };
                self.grafica(&rid, w, h)
            }
            MobileContenuto::Testo(b, fill, line) => {
                let corpo = self.blocchi_cella(b);
                self.id_disegno += 1;
                let riemp = fill.map_or("<a:noFill/>".to_string(), |c| format!("<a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>", hex(c)));
                let bordo = line.map_or("<a:ln><a:noFill/></a:ln>".to_string(), |l| format!("<a:ln w=\"{}\"><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill></a:ln>", emu(l.width), hex(l.color)));
                format!(
                    "<a:graphic><a:graphicData uri=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\"><wps:wsp><wps:cNvSpPr txBox=\"1\"/><wps:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom>{riemp}{bordo}</wps:spPr><wps:txbx><w:txbxContent>{corpo}</w:txbxContent></wps:txbx><wps:bodyPr rot=\"0\" vert=\"horz\" wrap=\"square\" lIns=\"91440\" tIns=\"45720\" rIns=\"91440\" bIns=\"45720\" anchor=\"t\"><a:noAutofit/></wps:bodyPr></wps:wsp></a:graphicData></a:graphic>",
                    emu(w),
                    emu(h)
                )
            }
            MobileContenuto::Cornice(b) => {
                // Una cornice di paragrafi: casella senza bordo e senza margini interni.
                let corpo = self.blocchi_cella(b);
                self.id_disegno += 1;
                format!(
                    "<a:graphic><a:graphicData uri=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\"><wps:wsp><wps:cNvSpPr txBox=\"1\"/><wps:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:noFill/><a:ln><a:noFill/></a:ln></wps:spPr><wps:txbx><w:txbxContent>{corpo}</w:txbxContent></wps:txbx><wps:bodyPr rot=\"0\" vert=\"horz\" wrap=\"square\" lIns=\"0\" tIns=\"0\" rIns=\"0\" bIns=\"0\" anchor=\"t\"><a:spAutoFit/></wps:bodyPr></wps:wsp></a:graphicData></a:graphic>",
                    emu(if m.w > 0.0 { m.w } else { larghezza_testo(b) }),
                    emu(if m.h.abs() > 0.0 { m.h.abs() } else { 20.0 })
                )
            }
        };
        let id = self.id_disegno;
        format!(
            "<w:r><w:drawing><wp:anchor distT=\"0\" distB=\"0\" distL=\"114300\" distR=\"114300\" simplePos=\"0\" relativeHeight=\"{}\" behindDoc=\"{}\" locked=\"0\" layoutInCell=\"1\" allowOverlap=\"1\"><wp:simplePos x=\"0\" y=\"0\"/>{}{}<wp:extent cx=\"{}\" cy=\"{}\"/><wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/>{avvolge}<wp:docPr id=\"{id}\" name=\"Oggetto {id}\"/><wp:cNvGraphicFramePr/>{grafica}</wp:anchor></w:drawing></w:r>",
            251_659_264 + id,
            if m.dietro { 1 } else { 0 },
            pos(&m.x, true),
            pos(&m.y, false),
            emu(w),
            emu(h)
        )
    }

    fn ppr(p: &Paragrafo, extra: &str) -> String {
        let mut s = String::from("<w:pPr>");
        if let Some(n) = p.titolo.filter(|n| (1..=9).contains(n)) {
            let _ = write!(s, "<w:pStyle w:val=\"Titolo{n}\"/>");
        }
        if p.insieme_al_seguente {
            s.push_str("<w:keepNext/>");
        }
        if p.righe_insieme {
            s.push_str("<w:keepLines/>");
        }
        if p.salto_prima {
            s.push_str("<w:pageBreakBefore/>");
        }
        s.push_str(if p.vedove { "<w:widowControl/>" } else { "<w:widowControl w:val=\"0\"/>" });
        if p.bordi.iter().any(|b| b.is_some()) {
            s.push_str("<w:pBdr>");
            for (k, tag) in ["top", "bottom", "left", "right", "between"].iter().enumerate() {
                if p.bordi[k].is_some() {
                    s.push_str(&w_linea(tag, &p.bordi[k]));
                }
            }
            s.push_str("</w:pBdr>");
        }
        if let Some(c) = p.sfondo {
            let _ = write!(s, "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{}\"/>", hex(c));
        }
        if !p.tabs.is_empty() {
            s.push_str("<w:tabs>");
            for t in &p.tabs {
                let val = match t.tipo {
                    TipoTab::Sinistra => "left",
                    TipoTab::Centro => "center",
                    TipoTab::Destra => "right",
                    TipoTab::Decimale => "decimal",
                };
                let leader = match t.leader {
                    Some('.') => " w:leader=\"dot\"",
                    Some('-') => " w:leader=\"hyphen\"",
                    Some('_') => " w:leader=\"underscore\"",
                    _ => "",
                };
                let _ = write!(s, "<w:tab w:val=\"{val}\" w:pos=\"{}\"{leader}/>", tw(t.pos));
            }
            s.push_str("</w:tabs>");
        }
        let (line, rule) = match p.interlinea {
            Interlinea::Multipla(m) => ((m * 240.0).round() as i32, "auto"),
            Interlinea::Esatta(v) => (tw(v), "exact"),
            Interlinea::Minima(v) => (tw(v), "atLeast"),
        };
        let _ = write!(s, "<w:spacing w:before=\"{}\" w:after=\"{}\" w:line=\"{line}\" w:lineRule=\"{rule}\"", tw(p.prima), tw(p.dopo));
        if p.auto_prima {
            s.push_str(" w:beforeAutospacing=\"1\"");
        }
        if p.auto_dopo {
            s.push_str(" w:afterAutospacing=\"1\"");
        }
        s.push_str("/>");
        let _ = write!(s, "<w:ind w:left=\"{}\" w:right=\"{}\"", tw(p.rientro_sx), tw(p.rientro_dx));
        if p.prima_riga < 0.0 {
            let _ = write!(s, " w:hanging=\"{}\"", tw(-p.prima_riga));
        } else {
            let _ = write!(s, " w:firstLine=\"{}\"", tw(p.prima_riga));
        }
        s.push_str("/>");
        if p.contestuale {
            s.push_str("<w:contextualSpacing/>");
        }
        let jc = match p.allinea {
            Allinea::Sinistra => "left",
            Allinea::Centro => "center",
            Allinea::Destra => "right",
            Allinea::Giustifica => "both",
        };
        let _ = write!(s, "<w:jc w:val=\"{jc}\"/>");
        if let Some(n) = p.titolo.filter(|n| (1..=9).contains(n)) {
            let _ = write!(s, "<w:outlineLvl w:val=\"{}\"/>", n - 1);
        }
        s.push_str(&w_rpr(&p.font_fine));
        s.push_str(extra);
        s.push_str("</w:pPr>");
        s
    }

    fn par(&mut self, p: &Paragrafo, ancorati: &[&Mobile], extra: &str) -> String {
        let mut s = format!("<w:p>{}", Self::ppr(p, extra));
        for m in ancorati {
            s.push_str(&self.ancora(m));
        }
        for b in &p.segnalibri {
            self.id_segno += 1;
            let _ = write!(s, "<w:bookmarkStart w:id=\"{}\" w:name=\"{}\"/><w:bookmarkEnd w:id=\"{}\"/>", self.id_segno, esc(b), self.id_segno);
        }
        if let Some((e, f)) = &p.etichetta {
            let _ = write!(s, "<w:r>{}{}{}</w:r>", w_rpr(f), w_testo(e), if p.etichetta_tab { "<w:tab/>" } else { "<w:t xml:space=\"preserve\"> </w:t>" });
        }
        let mut i = 0;
        while i < p.runs.len() {
            let link = p.runs[i].link.clone();
            let mut j = i;
            let mut corpo = String::new();
            while j < p.runs.len() && p.runs[j].link == link {
                corpo.push_str(&self.run(&p.runs[j]));
                j += 1;
            }
            i = j;
            match &link {
                Some(Target::Uri(u)) => {
                    let rid = self.rel(REL_LINK, u, true);
                    let _ = write!(s, "<w:hyperlink r:id=\"{rid}\">{corpo}</w:hyperlink>");
                }
                Some(Target::Anchor(a)) => {
                    let _ = write!(s, "<w:hyperlink w:anchor=\"{}\">{corpo}</w:hyperlink>", esc(a));
                }
                None => s.push_str(&corpo),
            }
        }
        s.push_str("</w:p>");
        s
    }

    fn run(&mut self, r: &Run) -> String {
        let rpr = w_rpr(&r.font);
        let dentro = match &r.pezzo {
            Pezzo::Testo(t) => w_testo(t),
            Pezzo::Tab => "<w:tab/>".into(),
            Pezzo::ACapo => "<w:br/>".into(),
            Pezzo::SaltoPagina => "<w:br w:type=\"page\"/>".into(),
            Pezzo::SaltoColonna => "<w:br w:type=\"column\"/>".into(),
            Pezzo::Immagine { img, w, h } => self.inline(*img, *w, *h),
            Pezzo::Nota(k) => {
                let mut f = r.font.clone();
                f.vert = 1;
                return format!("<w:r>{}<w:footnoteReference w:id=\"{}\"/></w:r>", w_rpr(&f), k + 1);
            }
            Pezzo::NotaFine(k) => {
                let mut f = r.font.clone();
                f.vert = 1;
                return format!("<w:r>{}<w:endnoteReference w:id=\"{}\"/></w:r>", w_rpr(&f), k + 1);
            }
            Pezzo::NumPagina | Pezzo::NumPagine => {
                let campo = if matches!(r.pezzo, Pezzo::NumPagina) { "PAGE" } else { "NUMPAGES" };
                return format!("<w:r>{rpr}<w:fldChar w:fldCharType=\"begin\"/></w:r><w:r>{rpr}<w:instrText xml:space=\"preserve\"> {campo} </w:instrText></w:r><w:r>{rpr}<w:fldChar w:fldCharType=\"separate\"/></w:r><w:r>{rpr}<w:t>1</w:t></w:r><w:r>{rpr}<w:fldChar w:fldCharType=\"end\"/></w:r>");
            }
        };
        if dentro.is_empty() {
            return String::new();
        }
        format!("<w:r>{rpr}{dentro}</w:r>")
    }

    fn tabella(&mut self, t: &Tabella) -> String {
        let jc = ["left", "center", "right"][t.allinea.min(2) as usize];
        let tot: f32 = t.griglia.iter().sum();
        let mut s = format!("<w:tbl><w:tblPr><w:tblW w:w=\"{}\" w:type=\"dxa\"/><w:jc w:val=\"{jc}\"/>", tw(tot));
        if t.allinea == 0 {
            let primo = t.righe.first().and_then(|r| r.celle.first()).map_or(5.4, |c| c.margini[0]);
            let _ = write!(s, "<w:tblInd w:w=\"{}\" w:type=\"dxa\"/>", tw(t.rientro - primo));
        }
        s.push_str("<w:tblLayout w:type=\"fixed\"/><w:tblCellMar><w:left w:w=\"0\" w:type=\"dxa\"/><w:right w:w=\"0\" w:type=\"dxa\"/></w:tblCellMar></w:tblPr><w:tblGrid>");
        for g in &t.griglia {
            let _ = write!(s, "<w:gridCol w:w=\"{}\"/>", tw(*g));
        }
        s.push_str("</w:tblGrid>");
        let colonne = posizioni(t);
        for (ri, r) in t.righe.iter().enumerate() {
            s.push_str("<w:tr><w:trPr>");
            if r.non_dividere {
                s.push_str("<w:cantSplit/>");
            }
            if r.altezza.abs() > 0.5 {
                let _ = write!(s, "<w:trHeight w:val=\"{}\" w:hRule=\"{}\"/>", tw(r.altezza.abs()), if r.altezza < 0.0 { "exact" } else { "atLeast" });
            }
            if r.intestazione {
                s.push_str("<w:tblHeader/>");
            }
            s.push_str("</w:trPr>");
            for (ci, c) in r.celle.iter().enumerate() {
                let col = colonne[ri][ci];
                let span = c.span.max(1);
                let larga: f32 = t.griglia.iter().skip(col).take(span).sum();
                let _ = write!(s, "<w:tc><w:tcPr><w:tcW w:w=\"{}\" w:type=\"dxa\"/>", tw(larga));
                if span > 1 {
                    let _ = write!(s, "<w:gridSpan w:val=\"{span}\"/>");
                }
                match c.vmerge {
                    Some(true) => s.push_str("<w:vMerge w:val=\"restart\"/>"),
                    Some(false) => s.push_str("<w:vMerge/>"),
                    None => {}
                }
                s.push_str("<w:tcBorders>");
                for (k, tag) in ["top", "bottom", "left", "right"].iter().enumerate() {
                    s.push_str(&w_linea(tag, &c.bordi[k]));
                }
                s.push_str("</w:tcBorders>");
                if let Some(f) = c.sfondo {
                    let _ = write!(s, "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{}\"/>", hex(f));
                }
                let [ml, mr, mt, mb] = c.margini;
                let _ = write!(s, "<w:tcMar><w:top w:w=\"{}\" w:type=\"dxa\"/><w:left w:w=\"{}\" w:type=\"dxa\"/><w:bottom w:w=\"{}\" w:type=\"dxa\"/><w:right w:w=\"{}\" w:type=\"dxa\"/></w:tcMar>", tw(mt), tw(ml), tw(mb), tw(mr));
                let _ = write!(s, "<w:vAlign w:val=\"{}\"/></w:tcPr>", ["top", "center", "bottom"][c.valign.min(2) as usize]);
                s.push_str(&self.blocchi_cella(&c.blocchi));
                s.push_str("</w:tc>");
            }
            s.push_str("</w:tr>");
        }
        s.push_str("</w:tbl>");
        s
    }

    /// Il contenuto di una cella, di una casella o di una nota: deve finire con un paragrafo.
    fn blocchi_cella(&mut self, blocchi: &[Blocco]) -> String {
        let mut s = self.blocchi(blocchi, None);
        if !matches!(blocchi.iter().rev().find(|b| !matches!(b, Blocco::Mobile(_))), Some(Blocco::Par(_))) {
            s.push_str("<w:p/>");
        }
        s
    }

    /// I blocchi; gli oggetti mobili vanno nel paragrafo che segue. `sez`: la sezione da chiudere
    /// nell'ultimo paragrafo.
    fn blocchi(&mut self, blocchi: &[Blocco], sez: Option<&str>) -> String {
        let mut s = String::new();
        let mut attesa: Vec<&Mobile> = Vec::new();
        let ultimo_par = blocchi.iter().rposition(|b| matches!(b, Blocco::Par(_)));
        for (k, b) in blocchi.iter().enumerate() {
            match b {
                Blocco::Mobile(m) => attesa.push(m),
                Blocco::Par(p) => {
                    let extra = if Some(k) == ultimo_par && k + 1 == blocchi.len() { sez.unwrap_or("") } else { "" };
                    let a = std::mem::take(&mut attesa);
                    s.push_str(&self.par(p, &a, extra));
                }
                Blocco::Tab(t) => {
                    if !attesa.is_empty() {
                        let a = std::mem::take(&mut attesa);
                        s.push_str(&self.par(&Paragrafo::default(), &a, ""));
                    }
                    s.push_str(&self.tabella(t));
                }
            }
        }
        if !attesa.is_empty() {
            s.push_str(&self.par(&Paragrafo::default(), &attesa, ""));
        }
        // La sezione si chiude in un paragrafo: se l'ultimo blocco non lo è, se ne aggiunge uno vuoto.
        if let Some(sp) = sez {
            if !matches!(blocchi.last(), Some(Blocco::Par(_))) {
                let _ = write!(s, "<w:p><w:pPr>{sp}</w:pPr></w:p>");
            }
        }
        s
    }

    fn rels_xml(&self) -> String {
        let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
        for (k, (tipo, target, esterna)) in self.rels.iter().enumerate() {
            let _ = write!(s, "<Relationship Id=\"rId{}\" Type=\"{tipo}\" Target=\"{}\"{}/>", k + 100, esc(target), if *esterna { " TargetMode=\"External\"" } else { "" });
        }
        s.push_str("</Relationships>");
        s
    }
}

/// Word (.docx): pagina, margini, sezioni, caratteri, paragrafi, tabelle, immagini e caselle di testo,
/// note, intestazioni e piè di pagina.
pub fn docx(doc: &Documento) -> Vec<u8> {
    let mut w = Word { doc, rels: Vec::new(), media: HashMap::new(), file_media: Vec::new(), id_disegno: 0, id_segno: 0 };
    let mut parti: Vec<(String, String, String)> = Vec::new(); // (file, tipo di contenuto, xml)
    let mut parti_rels: Vec<(String, String)> = Vec::new();
    let mut doc_rels: Vec<(String, String, bool)> = Vec::new();
    // Intestazioni e piè: un file per ognuno, con le sue relazioni.
    let mut n_testate = 0;
    let mut riferimenti: Vec<String> = Vec::new();
    for sez in &doc.sezioni {
        let mut r = String::new();
        for (tag, int) in [("header", &sez.testa), ("footer", &sez.piede)] {
            for (tipo, b) in [("default", &int.normale), ("first", &int.prima), ("even", &int.pari)] {
                let Some(b) = b else { continue };
                n_testate += 1;
                let nome = format!("{tag}{n_testate}.xml");
                w.rels.clear();
                let corpo = w.blocchi_cella(b);
                let radice = if tag == "header" { "hdr" } else { "ftr" };
                parti.push((format!("word/{nome}"), format!("application/vnd.openxmlformats-officedocument.wordprocessingml.{tag}+xml"), format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:{radice} {W_NS}>{corpo}</w:{radice}>")));
                parti_rels.push((format!("word/_rels/{nome}.rels"), w.rels_xml()));
                doc_rels.push((format!("http://schemas.openxmlformats.org/officeDocument/2006/relationships/{tag}"), nome, false));
                let _ = write!(r, "<w:{tag}Reference w:type=\"{tipo}\" r:id=\"rId{}\"/>", doc_rels.len());
            }
        }
        riferimenti.push(r);
    }
    // Le note.
    let mut note_xml = [String::new(), String::new()];
    let mut note_rels = [String::new(), String::new()];
    for (k, (tag, note)) in [("footnote", &doc.note), ("endnote", &doc.note_fine)].into_iter().enumerate() {
        if note.is_empty() {
            continue;
        }
        w.rels.clear();
        let mut s = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:{tag}s {W_NS}><w:{tag} w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></w:{tag}><w:{tag} w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:{tag}>");
        for (i, (_, b)) in note.iter().enumerate() {
            let _ = write!(s, "<w:{tag} w:id=\"{}\">{}</w:{tag}>", i + 1, w.blocchi_cella(b));
        }
        let _ = write!(s, "</w:{tag}s>");
        note_xml[k] = s;
        note_rels[k] = w.rels_xml();
    }
    // Il corpo, sezione per sezione.
    w.rels.clear();
    let mut corpo = String::new();
    let n = doc.sezioni.len();
    for (k, sez) in doc.sezioni.iter().enumerate() {
        let tipo = if k == 0 || sez.pagina_nuova {
            match sez.parita {
                1 => "oddPage",
                2 => "evenPage",
                _ => "nextPage",
            }
        } else {
            "continuous"
        };
        let mut sp = format!("<w:sectPr>{}", riferimenti[k]);
        let _ = write!(sp, "<w:type w:val=\"{tipo}\"/><w:pgSz w:w=\"{}\" w:h=\"{}\"{}/>", tw(sez.w), tw(sez.h), if sez.w > sez.h { " w:orient=\"landscape\"" } else { "" });
        let [ml, mr, mt, mb] = sez.margini;
        let _ = write!(sp, "<w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\" w:header=\"{}\" w:footer=\"{}\" w:gutter=\"0\"/>", tw(mt), tw(mr), tw(mb), tw(ml), tw(sez.dist_testa), tw(sez.dist_piede));
        if let Some(p) = sez.primo_numero {
            let _ = write!(sp, "<w:pgNumType w:start=\"{p}\"/>");
        }
        if sez.colonne > 1 {
            let _ = write!(sp, "<w:cols w:num=\"{}\" w:space=\"{}\"/>", sez.colonne, tw(sez.spazio_colonne));
        }
        if sez.prima_diversa {
            sp.push_str("<w:titlePg/>");
        }
        sp.push_str("</w:sectPr>");
        if k + 1 == n {
            corpo.push_str(&w.blocchi(&sez.blocchi, None));
            corpo.push_str(&sp);
        } else {
            corpo.push_str(&w.blocchi(&sez.blocchi, Some(&sp)));
        }
    }
    if doc.sezioni.is_empty() {
        corpo.push_str("<w:p/><w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:pgMar w:top=\"1417\" w:right=\"1134\" w:bottom=\"1134\" w:left=\"1134\" w:header=\"708\" w:footer=\"708\" w:gutter=\"0\"/></w:sectPr>");
    }
    let documento = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:document {W_NS}><w:body>{corpo}</w:body></w:document>");
    // Relazioni del documento: stili, impostazioni, intestazioni/piè, note, poi immagini e collegamenti.
    let mut rels = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
    for (k, (tipo, target, _)) in doc_rels.iter().enumerate() {
        let _ = write!(rels, "<Relationship Id=\"rId{}\" Type=\"{tipo}\" Target=\"{target}\"/>", k + 1);
    }
    let base = doc_rels.len();
    let _ = write!(rels, "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>", base + 1);
    let _ = write!(rels, "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings\" Target=\"settings.xml\"/>", base + 2);
    if !note_xml[0].is_empty() {
        let _ = write!(rels, "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes\" Target=\"footnotes.xml\"/>", base + 3);
    }
    if !note_xml[1].is_empty() {
        let _ = write!(rels, "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/endnotes\" Target=\"endnotes.xml\"/>", base + 4);
    }
    for (k, (tipo, target, esterna)) in w.rels.iter().enumerate() {
        let _ = write!(rels, "<Relationship Id=\"rId{}\" Type=\"{tipo}\" Target=\"{}\"{}/>", k + 100, esc(target), if *esterna { " TargetMode=\"External\"" } else { "" });
    }
    rels.push_str("</Relationships>");
    let mut stili = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:docDefaults><w:rPrDefault><w:rPr><w:lang w:val=\"it-IT\"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr/></w:pPrDefault></w:docDefaults><w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/></w:style>");
    for n in 1..=9 {
        let _ = write!(stili, "<w:style w:type=\"paragraph\" w:styleId=\"Titolo{n}\"><w:name w:val=\"heading {n}\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/><w:pPr><w:outlineLvl w:val=\"{}\"/></w:pPr></w:style>", n - 1);
    }
    stili.push_str("</w:styles>");
    let impostazioni = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:settings xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">{}<w:defaultTabStop w:val=\"{}\"/><w:compat><w:compatSetting w:name=\"compatibilityMode\" w:uri=\"http://schemas.microsoft.com/office/word\" w:val=\"15\"/></w:compat></w:settings>",
        if doc.pari_dispari { "<w:evenAndOddHeaders/>" } else { "" },
        tw(if doc.tab_predefinita > 0.0 { doc.tab_predefinita } else { 36.0 })
    );
    let mut tipi = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"png\" ContentType=\"image/png\"/><Default Extension=\"jpeg\" ContentType=\"image/jpeg\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/><Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/><Override PartName=\"/word/settings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml\"/>");
    for (file, tipo, _) in &parti {
        let _ = write!(tipi, "<Override PartName=\"/{file}\" ContentType=\"{tipo}\"/>");
    }
    if !note_xml[0].is_empty() {
        tipi.push_str("<Override PartName=\"/word/footnotes.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml\"/>");
    }
    if !note_xml[1].is_empty() {
        tipi.push_str("<Override PartName=\"/word/endnotes.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml\"/>");
    }
    tipi.push_str("<Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/></Types>");
    let radice = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/></Relationships>";
    let core = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>{}</dc:title></cp:coreProperties>", esc(&doc.titolo));
    let mut file: Vec<(String, Vec<u8>)> = vec![
        ("[Content_Types].xml".into(), tipi.into_bytes()),
        ("_rels/.rels".into(), radice.as_bytes().to_vec()),
        ("docProps/core.xml".into(), core.into_bytes()),
        ("word/document.xml".into(), documento.into_bytes()),
        ("word/_rels/document.xml.rels".into(), rels.into_bytes()),
        ("word/styles.xml".into(), stili.into_bytes()),
        ("word/settings.xml".into(), impostazioni.into_bytes()),
    ];
    for (f, _, x) in parti {
        file.push((f, x.into_bytes()));
    }
    for (f, x) in parti_rels {
        file.push((f, x.into_bytes()));
    }
    for (k, nome) in ["footnotes", "endnotes"].iter().enumerate() {
        if !note_xml[k].is_empty() {
            file.push((format!("word/{nome}.xml"), note_xml[k].clone().into_bytes()));
            file.push((format!("word/_rels/{nome}.xml.rels"), note_rels[k].clone().into_bytes()));
        }
    }
    for (nome, dati) in std::mem::take(&mut w.file_media) {
        file.push((format!("word/media/{nome}"), dati));
    }
    let voci: Vec<(&str, &[u8])> = file.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect();
    crate::zipstore::zip_archive(&voci)
}

// ------------------------------------------------------------------------ Writer (.odt)

struct Odt<'a> {
    doc: &'a Documento,
    /// Stili automatici: proprietà → nome, nell'ordine in cui nascono.
    stili: HashMap<String, String>,
    elenco_stili: Vec<String>,
    figure: HashMap<usize, String>,
    file_figure: Vec<(String, Vec<u8>, &'static str)>,
    n_cornici: u32,
    /// Il nome della pagina maestra per il primo paragrafo di una sezione.
    maestra: Option<String>,
    /// Il paragrafo che si scrive comincia su una pagina o una colonna nuova (salto dentro il paragrafo).
    rottura: Option<&'static str>,
}

fn pt(v: f32) -> String {
    format!("{:.2}pt", v)
}

fn o_linea(l: &Option<Linea>) -> String {
    match l {
        Some(l) => format!("{} {} #{}", pt(l.width.max(0.25)), if l.double { "double" } else if l.dash != Dash::Solid { "dashed" } else { "solid" }, hex(l.color)),
        None => "none".into(),
    }
}

impl Odt<'_> {
    fn stile(&mut self, famiglia: &str, prefisso: &str, corpo: String) -> String {
        let chiave = format!("{famiglia}|{corpo}");
        if let Some(n) = self.stili.get(&chiave) {
            return n.clone();
        }
        let n = format!("{prefisso}{}", self.stili.len() + 1);
        self.stili.insert(chiave, n.clone());
        // Le cornici e le immagini di Writer discendono dai loro stili (senza, sarebbero forme da disegno).
        let padre = match (famiglia, prefisso) {
            ("graphic", "fr") => " style:parent-style-name=\"Frame\"",
            ("graphic", _) => " style:parent-style-name=\"Graphics\"",
            _ => "",
        };
        self.elenco_stili.push(format!("<style:style style:name=\"{n}\" style:family=\"{famiglia}\"{padre}>{corpo}</style:style>"));
        n
    }

    fn testo_prop(f: &Font) -> String {
        let fam = esc(&f.family);
        let mut s = format!("<style:text-properties fo:font-family=\"'{fam}'\" style:font-family-asian=\"'{fam}'\" style:font-family-complex=\"'{fam}'\" fo:font-size=\"{}\" style:font-size-asian=\"{}\" style:font-size-complex=\"{}\"", pt(f.size), pt(f.size), pt(f.size));
        let peso = if f.bold { "bold" } else { "normal" };
        let stile = if f.italic { "italic" } else { "normal" };
        let _ = write!(s, " fo:font-weight=\"{peso}\" style:font-weight-asian=\"{peso}\" style:font-weight-complex=\"{peso}\" fo:font-style=\"{stile}\" style:font-style-asian=\"{stile}\" style:font-style-complex=\"{stile}\"");
        let _ = write!(s, " fo:color=\"#{}\"", hex(f.color));
        if f.double_underline {
            s.push_str(" style:text-underline-style=\"solid\" style:text-underline-type=\"double\" style:text-underline-width=\"auto\" style:text-underline-color=\"font-color\"");
        } else if f.underline {
            s.push_str(" style:text-underline-style=\"solid\" style:text-underline-width=\"auto\" style:text-underline-color=\"font-color\"");
        }
        if f.strike {
            s.push_str(" style:text-line-through-style=\"solid\"");
        }
        if let Some(h) = f.highlight {
            let _ = write!(s, " fo:background-color=\"#{}\"", hex(h));
        }
        match f.vert {
            1 => s.push_str(" style:text-position=\"super 58%\""),
            -1 => s.push_str(" style:text-position=\"sub 58%\""),
            _ => {}
        }
        if f.caps {
            s.push_str(" fo:text-transform=\"uppercase\"");
        }
        if f.small_caps {
            s.push_str(" fo:font-variant=\"small-caps\"");
        }
        if f.spacing.abs() > 0.01 {
            let _ = write!(s, " fo:letter-spacing=\"{}\"", pt(f.spacing));
        }
        s.push_str("/>");
        s
    }

    fn stile_testo(&mut self, f: &Font) -> String {
        self.stile("text", "T", Self::testo_prop(f))
    }

    fn stile_par(&mut self, p: &Paragrafo, maestra: Option<&str>) -> String {
        let mut s = String::from("<style:paragraph-properties");
        let al = match p.allinea {
            Allinea::Sinistra => "start",
            Allinea::Centro => "center",
            Allinea::Destra => "end",
            Allinea::Giustifica => "justify",
        };
        let _ = write!(s, " fo:text-align=\"{al}\" fo:margin-left=\"{}\" fo:margin-right=\"{}\" fo:text-indent=\"{}\" fo:margin-top=\"{}\" fo:margin-bottom=\"{}\"", pt(p.rientro_sx), pt(p.rientro_dx), pt(p.prima_riga), pt(p.prima), pt(p.dopo));
        match p.interlinea {
            Interlinea::Multipla(m) => {
                let _ = write!(s, " fo:line-height=\"{:.0}%\"", m * 100.0);
            }
            Interlinea::Esatta(v) => {
                let _ = write!(s, " fo:line-height=\"{}\"", pt(v));
            }
            Interlinea::Minima(v) => {
                let _ = write!(s, " style:line-height-at-least=\"{}\"", pt(v));
            }
        }
        if p.insieme_al_seguente {
            s.push_str(" fo:keep-with-next=\"always\"");
        }
        if p.righe_insieme {
            s.push_str(" fo:keep-together=\"always\"");
        }
        if let Some(r) = self.rottura.take() {
            let _ = write!(s, " fo:break-before=\"{r}\"");
        } else if p.salto_prima && maestra.is_none() {
            s.push_str(" fo:break-before=\"page\"");
        }
        let (v, o) = if p.vedove { (2, 2) } else { (0, 0) };
        let _ = write!(s, " fo:widows=\"{v}\" fo:orphans=\"{o}\"");
        if let Some(c) = p.sfondo {
            let _ = write!(s, " fo:background-color=\"#{}\"", hex(c));
        }
        for (k, lato) in ["top", "bottom", "left", "right"].iter().enumerate() {
            if p.bordi[k].is_some() {
                let _ = write!(s, " fo:border-{lato}=\"{}\"", o_linea(&p.bordi[k]));
            }
        }
        if p.contestuale {
            s.push_str(" style:contextual-spacing=\"true\"");
        }
        if p.tabs.is_empty() {
            s.push_str("/>");
        } else {
            s.push_str("><style:tab-stops>");
            for t in &p.tabs {
                let tipo = match t.tipo {
                    TipoTab::Sinistra => "left",
                    TipoTab::Centro => "center",
                    TipoTab::Destra => "right",
                    TipoTab::Decimale => "char",
                };
                // Le tabulazioni di Writer partono dal rientro sinistro.
                let _ = write!(s, "<style:tab-stop style:position=\"{}\" style:type=\"{tipo}\"", pt(t.pos - p.rientro_sx));
                if tipo == "char" {
                    s.push_str(" style:char=\",\"");
                }
                if let Some(l) = t.leader {
                    let _ = write!(s, " style:leader-text=\"{l}\" style:leader-style=\"{}\"", if l == '.' { "dotted" } else { "solid" });
                }
                s.push_str("/>");
            }
            s.push_str("</style:tab-stops></style:paragraph-properties>");
        }
        // Il carattere del paragrafo: quello del testo (in Writer conta per l'altezza della riga); il
        // segno di fine paragrafo solo per i paragrafi vuoti.
        let f = p.runs.iter().find(|r| matches!(r.pezzo, Pezzo::Testo(_))).map_or(&p.font_fine, |r| &r.font);
        s.push_str(&Self::testo_prop(f));
        let corpo = s;
        match maestra {
            Some(m) => {
                // Lo stile con la pagina maestra si scrive con l'attributo nell'intestazione dello stile.
                let chiave = format!("paragraph|{m}|{corpo}");
                if let Some(n) = self.stili.get(&chiave) {
                    return n.clone();
                }
                let n = format!("P{}", self.stili.len() + 1);
                self.stili.insert(chiave, n.clone());
                self.elenco_stili.push(format!("<style:style style:name=\"{n}\" style:family=\"paragraph\" style:parent-style-name=\"Standard\" style:master-page-name=\"{m}\">{corpo}</style:style>"));
                n
            }
            None => self.stile("paragraph", "P", corpo),
        }
    }

    fn testo_xml(t: &str) -> String {
        // In ODF gli spazi in fila e quello in testa si scrivono con <text:s/>.
        let mut o = String::new();
        let mut spazi = 0;
        let flush = |o: &mut String, n: &mut usize| {
            if *n == 0 {
                return;
            }
            if o.is_empty() || o.ends_with('>') && !o.ends_with("</text:span>") {
                if *n == 1 {
                    o.push_str("<text:s/>");
                } else {
                    let _ = write!(o, "<text:s text:c=\"{n}\"/>");
                }
            } else {
                o.push(' ');
                if *n > 1 {
                    let _ = write!(o, "<text:s text:c=\"{}\"/>", *n - 1);
                }
            }
            *n = 0;
        };
        for c in t.chars() {
            if c == ' ' {
                spazi += 1;
                continue;
            }
            flush(&mut o, &mut spazi);
            match c {
                '\t' => o.push_str("<text:tab/>"),
                '\n' => o.push_str("<text:line-break/>"),
                c => o.push_str(&esc(&c.to_string())),
            }
        }
        flush(&mut o, &mut spazi);
        o
    }

    fn figura(&mut self, i: usize) -> Option<String> {
        if !self.figure.contains_key(&i) {
            let img = self.doc.immagini.get(i)?;
            let (dati, ext) = file_immagine(img);
            if dati.is_empty() {
                return None;
            }
            let nome = format!("Pictures/immagine{}.{}", self.file_figure.len() + 1, if ext == "jpeg" { "jpg" } else { "png" });
            self.file_figure.push((nome.clone(), dati, mime(ext)));
            self.figure.insert(i, nome);
        }
        self.figure.get(&i).cloned()
    }

    fn run(&mut self, r: &Run) -> String {
        let st = self.stile_testo(&r.font);
        let dentro = match &r.pezzo {
            Pezzo::Testo(t) => Self::testo_xml(t),
            Pezzo::Tab => "<text:tab/>".into(),
            Pezzo::ACapo | Pezzo::SaltoColonna | Pezzo::SaltoPagina => "<text:line-break/>".into(),
            Pezzo::Immagine { img, w, h } => {
                let Some(href) = self.figura(*img) else { return String::new() };
                self.n_cornici += 1;
                let gr = self.stile("graphic", "gr", "<style:graphic-properties style:vertical-pos=\"top\" style:vertical-rel=\"baseline\" fo:border=\"none\" fo:padding=\"0pt\" fo:margin-left=\"0pt\" fo:margin-right=\"0pt\" fo:margin-top=\"0pt\" fo:margin-bottom=\"0pt\"/>".into());
                return format!("<draw:frame draw:style-name=\"{gr}\" draw:name=\"Immagine{}\" text:anchor-type=\"as-char\" svg:width=\"{}\" svg:height=\"{}\" draw:z-index=\"0\"><draw:image xlink:href=\"{href}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/></draw:frame>", self.n_cornici, pt(*w), pt(*h));
            }
            Pezzo::Nota(k) | Pezzo::NotaFine(k) => {
                let fine = matches!(r.pezzo, Pezzo::NotaFine(_));
                let lista = if fine { &self.doc.note_fine } else { &self.doc.note };
                let Some((n, b)) = lista.get(*k) else { return String::new() };
                let (n, b) = (n.clone(), b.clone());
                // Il numero lo mette Writer: dal testo della nota si toglie quello scritto dal lettore.
                let b = senza_numero(&b, &n);
                let corpo = self.blocchi(&b);
                return format!("<text:note text:id=\"{}{}\" text:note-class=\"{}\"><text:note-citation>{}</text:note-citation><text:note-body>{corpo}</text:note-body></text:note>", if fine { "en" } else { "ftn" }, k + 1, if fine { "endnote" } else { "footnote" }, esc(&n));
            }
            Pezzo::NumPagina => "<text:page-number text:select-page=\"current\">1</text:page-number>".into(),
            Pezzo::NumPagine => "<text:page-count>1</text:page-count>".into(),
        };
        format!("<text:span text:style-name=\"{st}\">{dentro}</text:span>")
    }

    fn par(&mut self, p: &Paragrafo, ancorati: &[&Mobile]) -> String {
        // In ODF un salto di pagina o di colonna non sta dentro un paragrafo: lo si divide in due e il
        // secondo comincia sulla pagina (o colonna) nuova.
        if let Some(k) = p.runs.iter().position(|r| matches!(r.pezzo, Pezzo::SaltoPagina | Pezzo::SaltoColonna)) {
            let mut primo = p.clone();
            let mut resto = p.clone();
            primo.runs.truncate(k);
            primo.dopo = 0.0;
            primo.insieme_al_seguente = false;
            resto.runs.drain(..=k);
            resto.etichetta = None;
            resto.segnalibri.clear();
            resto.prima = 0.0;
            resto.salto_prima = false;
            // Il testo dopo il salto continua il paragrafo: non è una prima riga.
            resto.prima_riga = 0.0;
            let colonna = matches!(p.runs[k].pezzo, Pezzo::SaltoColonna);
            let mut s = String::new();
            // Un paragrafo vuoto prima del salto non si scrive (il salto era in testa).
            if !primo.runs.is_empty() || primo.etichetta.is_some() || !ancorati.is_empty() || self.maestra.is_some() {
                s.push_str(&self.par(&primo, ancorati));
                self.rottura = Some(if colonna { "column" } else { "page" });
            } else {
                self.rottura = Some(if colonna { "column" } else { "page" });
                resto.etichetta = p.etichetta.clone();
                resto.segnalibri = p.segnalibri.clone();
                resto.prima_riga = p.prima_riga;
                resto.prima = p.prima;
            }
            s.push_str(&self.par(&resto, if s.is_empty() { ancorati } else { &[] }));
            return s;
        }
        let maestra = self.maestra.take();
        let st = self.stile_par(p, maestra.as_deref());
        let (tag, attr) = match p.titolo.filter(|n| (1..=10).contains(n)) {
            Some(n) => ("text:h", format!(" text:outline-level=\"{n}\"")),
            None => ("text:p", String::new()),
        };
        let mut s = format!("<{tag} text:style-name=\"{st}\"{attr}>");
        for m in ancorati {
            s.push_str(&self.mobile(m));
        }
        for b in &p.segnalibri {
            let _ = write!(s, "<text:bookmark text:name=\"{}\"/>", esc(b));
        }
        if let Some((e, f)) = &p.etichetta {
            let ts = self.stile_testo(f);
            let _ = write!(s, "<text:span text:style-name=\"{ts}\">{}{}</text:span>", Self::testo_xml(e), if p.etichetta_tab { "<text:tab/>" } else { " " });
        }
        let mut i = 0;
        while i < p.runs.len() {
            let link = p.runs[i].link.clone();
            let mut j = i;
            let mut corpo = String::new();
            while j < p.runs.len() && p.runs[j].link == link {
                corpo.push_str(&self.run(&p.runs[j]));
                j += 1;
            }
            i = j;
            match &link {
                Some(t) => {
                    let _ = write!(s, "<text:a xlink:type=\"simple\" xlink:href=\"{}\">{corpo}</text:a>", esc(&link_url(t)));
                }
                None => s.push_str(&corpo),
            }
        }
        let _ = write!(s, "</{tag}>");
        s
    }

    fn mobile(&mut self, m: &Mobile) -> String {
        let (base_x, x, pos_x) = match m.x {
            Riferimento::Spostato(b, v) => (b, Some(v), "from-left"),
            Riferimento::Allineato(b, a) => (b, None, ["left", "center", "right"][a.min(2) as usize]),
        };
        let (base_y, y, pos_y) = match m.y {
            Riferimento::Spostato(b, v) => (b, Some(v), "from-top"),
            Riferimento::Allineato(b, a) => (b, None, ["top", "middle", "bottom"][a.min(2) as usize]),
        };
        let rel_x = match base_x {
            Base::Pagina => "page",
            Base::Margine => "page-content",
            _ => "paragraph",
        };
        let rel_y = match base_y {
            Base::Pagina => "page",
            Base::Margine => "page-content",
            _ => "paragraph",
        };
        let ancora = if matches!(base_y, Base::Pagina | Base::Margine) && matches!(base_x, Base::Pagina | Base::Margine) { "paragraph" } else { "paragraph" };
        let avvolge = match m.avvolge {
            Avvolge::Lati => "parallel",
            Avvolge::SopraSotto => "none",
            Avvolge::Nessuno => "run-through",
        };
        let (riemp, bordo, pad) = match &m.contenuto {
            MobileContenuto::Testo(_, f, l) => (f.map_or("none".to_string(), |c| format!("#{}", hex(c))), o_linea(l), "3.6pt 7.2pt"),
            _ => ("none".into(), "none".into(), "0pt"),
        };
        let prefisso = if matches!(m.contenuto, MobileContenuto::Immagine(_)) { "gr" } else { "fr" };
        let gr = self.stile(
            "graphic",
            prefisso,
            format!(
                "<style:graphic-properties fo:margin-left=\"0pt\" fo:margin-right=\"0pt\" fo:margin-top=\"0pt\" fo:margin-bottom=\"0pt\" style:wrap=\"{avvolge}\" style:run-through=\"{}\" style:horizontal-pos=\"{pos_x}\" style:horizontal-rel=\"{rel_x}\" style:vertical-pos=\"{pos_y}\" style:vertical-rel=\"{rel_y}\" fo:background-color=\"{}\" draw:fill=\"{}\" fo:border=\"{bordo}\" fo:padding=\"{pad}\"/>",
                if m.dietro { "background" } else { "foreground" },
                if riemp == "none" { "transparent".to_string() } else { riemp.clone() },
                if riemp == "none" { "none" } else { "solid" }
            ),
        );
        self.n_cornici += 1;
        let n = self.n_cornici;
        let mut pos = String::new();
        if let Some(x) = x {
            let _ = write!(pos, " svg:x=\"{}\"", pt(x));
        }
        if let Some(y) = y {
            let _ = write!(pos, " svg:y=\"{}\"", pt(y));
        }
        let (w, h) = (m.w.max(1.0), m.h.abs().max(1.0));
        match &m.contenuto {
            MobileContenuto::Immagine(i) => {
                let Some(href) = self.figura(*i) else { return String::new() };
                format!("<draw:frame draw:style-name=\"{gr}\" draw:name=\"Immagine{n}\" text:anchor-type=\"{ancora}\"{pos} svg:width=\"{}\" svg:height=\"{}\" draw:z-index=\"{n}\"><draw:image xlink:href=\"{href}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/></draw:frame>", pt(w), pt(h))
            }
            MobileContenuto::Testo(b, _, _) | MobileContenuto::Cornice(b) => {
                let corpo = self.blocchi(b);
                // Altezza 0: almeno una riga per paragrafo, poi cresce col testo.
                let righe: f32 = b.iter().filter_map(|x| if let Blocco::Par(p) = x { Some(p.runs.first().map_or(p.font_fine.size, |r| r.font.size) * 1.2) } else { None }).sum();
                let alt = format!(" fo:min-height=\"{}\"", pt(if m.h > 0.0 { h } else { righe.max(1.0) }));
                // Larghezza 0: la cornice è larga quanto il suo testo (cresce da sola).
                let (largo, minimo) = if m.w > 0.0 { (format!(" svg:width=\"{}\"", pt(w)), String::new()) } else { (String::new(), " fo:min-width=\"0.5pt\"".to_string()) };
                format!("<draw:frame draw:style-name=\"{gr}\" draw:name=\"Cornice{n}\" text:anchor-type=\"{ancora}\"{pos}{largo} draw:z-index=\"{n}\"><draw:text-box{alt}{minimo}>{corpo}</draw:text-box></draw:frame>")
            }
        }
    }

    fn tabella(&mut self, t: &Tabella) -> String {
        let tot: f32 = t.griglia.iter().sum();
        let al = ["left", "center", "right"][t.allinea.min(2) as usize];
        let margine = if t.allinea == 0 {
            let primo = t.righe.first().and_then(|r| r.celle.first()).map_or(5.4, |c| c.margini[0]);
            format!(" fo:margin-left=\"{}\"", pt(t.rientro - primo))
        } else {
            String::new()
        };
        let ts = self.stile("table", "Tab", format!("<style:table-properties style:width=\"{}\" table:align=\"{al}\"{margine}/>", pt(tot)));
        let mut s = format!("<table:table table:style-name=\"{ts}\">");
        for g in &t.griglia {
            let cs = self.stile("table-column", "Col", format!("<style:table-column-properties style:column-width=\"{}\"/>", pt(*g)));
            let _ = write!(s, "<table:table-column table:style-name=\"{cs}\"/>");
        }
        let colonne = posizioni(t);
        let testa = t.righe.iter().take_while(|r| r.intestazione).count();
        for (ri, r) in t.righe.iter().enumerate() {
            if ri == 0 && testa > 0 {
                s.push_str("<table:table-header-rows>");
            }
            let mut rp = String::from("<style:table-row-properties");
            if r.altezza > 0.5 {
                let _ = write!(rp, " style:min-row-height=\"{}\"", pt(r.altezza));
            } else if r.altezza < -0.5 {
                let _ = write!(rp, " style:row-height=\"{}\"", pt(-r.altezza));
            }
            let _ = write!(rp, " fo:keep-together=\"{}\"/>", if r.non_dividere { "always" } else { "auto" });
            let rs = self.stile("table-row", "Riga", rp);
            let _ = write!(s, "<table:table-row table:style-name=\"{rs}\">");
            for (ci, c) in r.celle.iter().enumerate() {
                let span = c.span.max(1);
                if c.vmerge == Some(false) {
                    for _ in 0..span {
                        s.push_str("<table:covered-table-cell/>");
                    }
                    continue;
                }
                let mut cp = String::from("<style:table-cell-properties");
                for (k, lato) in ["top", "bottom", "left", "right"].iter().enumerate() {
                    let _ = write!(cp, " fo:border-{lato}=\"{}\"", o_linea(&c.bordi[k]));
                }
                let [ml, mr, mt, mb] = c.margini;
                let _ = write!(cp, " fo:padding-left=\"{}\" fo:padding-right=\"{}\" fo:padding-top=\"{}\" fo:padding-bottom=\"{}\" style:vertical-align=\"{}\"", pt(ml), pt(mr), pt(mt), pt(mb), ["top", "middle", "bottom"][c.valign.min(2) as usize]);
                if let Some(f) = c.sfondo {
                    let _ = write!(cp, " fo:background-color=\"#{}\"", hex(f));
                }
                cp.push_str("/>");
                let cs = self.stile("table-cell", "Cella", cp);
                let col = colonne[ri][ci];
                let righe = if c.vmerge == Some(true) { righe_unite(t, &colonne, ri, col) } else { 1 };
                let mut attr = String::new();
                if span > 1 {
                    let _ = write!(attr, " table:number-columns-spanned=\"{span}\"");
                }
                if righe > 1 {
                    let _ = write!(attr, " table:number-rows-spanned=\"{righe}\"");
                }
                let mut corpo = self.blocchi(&c.blocchi);
                if corpo.is_empty() {
                    corpo.push_str("<text:p/>");
                }
                let _ = write!(s, "<table:table-cell table:style-name=\"{cs}\" office:value-type=\"string\"{attr}>{corpo}</table:table-cell>");
                for _ in 1..span {
                    s.push_str("<table:covered-table-cell/>");
                }
            }
            s.push_str("</table:table-row>");
            if ri + 1 == testa {
                s.push_str("</table:table-header-rows>");
            }
        }
        s.push_str("</table:table>");
        s
    }

    fn blocchi(&mut self, blocchi: &[Blocco]) -> String {
        let mut s = String::new();
        let mut attesa: Vec<&Mobile> = Vec::new();
        for b in blocchi {
            match b {
                Blocco::Mobile(m) => attesa.push(m),
                Blocco::Par(p) => {
                    let a = std::mem::take(&mut attesa);
                    s.push_str(&self.par(p, &a));
                }
                Blocco::Tab(t) => {
                    if !attesa.is_empty() || self.maestra.is_some() {
                        let a = std::mem::take(&mut attesa);
                        s.push_str(&self.par(&Paragrafo::default(), &a));
                    }
                    s.push_str(&self.tabella(t));
                }
            }
        }
        if !attesa.is_empty() {
            s.push_str(&self.par(&Paragrafo::default(), &attesa));
        }
        s
    }
}

/// Le note senza il numero che il lettore ha scritto in testa al primo paragrafo.
fn senza_numero(blocchi: &[Blocco], numero: &str) -> Vec<Blocco> {
    let mut b = blocchi.to_vec();
    if let Some(Blocco::Par(p)) = b.first_mut() {
        if let Some(pos) = p.runs.iter().position(|r| matches!(&r.pezzo, Pezzo::Testo(t) if !t.trim().is_empty())) {
            if let Pezzo::Testo(t) = &p.runs[pos].pezzo {
                if t.trim() == numero {
                    p.runs.drain(..=pos);
                    if let Some(Run { pezzo: Pezzo::Testo(t), .. }) = p.runs.first_mut() {
                        *t = t.trim_start().to_string();
                    }
                } else if let Some(resto) = t.trim_start().strip_prefix(numero) {
                    p.runs[pos].pezzo = Pezzo::Testo(resto.trim_start().to_string());
                }
            }
        }
    }
    b
}

/// Writer (.odt): pagina e margini (una pagina maestra per sezione), caratteri, paragrafi, tabelle,
/// immagini e caselle, note, intestazioni e piè di pagina.
pub fn odt(doc: &Documento) -> Vec<u8> {
    let mut o = Odt { doc, stili: HashMap::new(), elenco_stili: Vec::new(), figure: HashMap::new(), file_figure: Vec::new(), n_cornici: 0, maestra: None, rottura: None };
    let mut layout = String::new();
    let mut maestre = String::new();
    let mut corpo = String::new();
    for (k, sez) in doc.sezioni.iter().enumerate() {
        let nome = format!("Pagina{}", k + 1);
        let [ml, mr, mt, mb] = sez.margini;
        let ha_testa = sez.testa.normale.is_some() || sez.testa.prima.is_some();
        let ha_piede = sez.piede.normale.is_some() || sez.piede.prima.is_some();
        // In Writer l'intestazione sta dentro il margine: il margine della pagina è la sua distanza dal bordo.
        // Come in Word il testo comincia al margine, o sotto l'intestazione se è più alta: in Writer
        // l'altezza minima dell'intestazione comprende lo spazio fino al testo.
        let (top, testa) = if ha_testa { (sez.dist_testa, format!("<style:header-style><style:header-footer-properties fo:min-height=\"{}\" fo:margin-bottom=\"0pt\" style:dynamic-spacing=\"true\"/></style:header-style>", pt((mt - sez.dist_testa).max(0.0)))) } else { (mt, "<style:header-style/>".to_string()) };
        let (bottom, piede) = if ha_piede { (sez.dist_piede, format!("<style:footer-style><style:header-footer-properties fo:min-height=\"{}\" fo:margin-top=\"0pt\" style:dynamic-spacing=\"true\"/></style:footer-style>", pt((mb - sez.dist_piede).max(0.0)))) } else { (mb, "<style:footer-style/>".to_string()) };
        let colonne = if sez.colonne > 1 { format!("<style:columns fo:column-count=\"{}\" fo:column-gap=\"{}\"/>", sez.colonne, pt(sez.spazio_colonne)) } else { String::new() };
        let _ = write!(
            layout,
            "<style:page-layout style:name=\"L{}\"><style:page-layout-properties fo:page-width=\"{}\" fo:page-height=\"{}\" style:print-orientation=\"{}\" fo:margin-left=\"{}\" fo:margin-right=\"{}\" fo:margin-top=\"{}\" fo:margin-bottom=\"{}\"{}>{colonne}</style:page-layout-properties>{testa}{piede}</style:page-layout>",
            k + 1,
            pt(sez.w),
            pt(sez.h),
            if sez.w > sez.h { "landscape" } else { "portrait" },
            pt(ml),
            pt(mr),
            pt(top),
            pt(bottom),
            sez.primo_numero.map_or(String::new(), |n| format!(" style:first-page-number=\"{n}\""))
        );
        let mut m = format!("<style:master-page style:name=\"{nome}\" style:page-layout-name=\"L{}\">", k + 1);
        for (tag, int) in [("header", &sez.testa), ("footer", &sez.piede)] {
            if let Some(b) = &int.normale {
                let x = o.blocchi(b);
                let _ = write!(m, "<style:{tag}>{x}</style:{tag}>");
            }
            if let Some(b) = int.pari.as_ref().filter(|_| doc.pari_dispari) {
                let x = o.blocchi(b);
                let _ = write!(m, "<style:{tag}-left>{x}</style:{tag}-left>");
            }
            // Prima pagina diversa: la sua intestazione anche se è vuota.
            if sez.prima_diversa && (int.normale.is_some() || int.prima.is_some()) {
                let x = int.prima.as_ref().map(|b| o.blocchi(b)).unwrap_or_default();
                let x = if x.is_empty() { "<text:p/>".to_string() } else { x };
                let _ = write!(m, "<style:{tag}-first>{x}</style:{tag}-first>");
            }
        }
        m.push_str("</style:master-page>");
        maestre.push_str(&m);
        if k == 0 || sez.pagina_nuova {
            o.maestra = Some(nome);
        }
        corpo.push_str(&o.blocchi(&sez.blocchi));
        if o.maestra.is_some() {
            corpo.push_str(&o.par(&Paragrafo::default(), &[]));
        }
    }
    if doc.sezioni.is_empty() {
        corpo.push_str("<text:p/>");
    }
    const NS: &str = "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:meta=\"urn:oasis:names:tc:opendocument:xmlns:meta:1.0\" office:version=\"1.3\"";
    // Gli stili automatici del corpo stanno in content.xml; quelli di intestazioni e piè (nati
    // prima) servono anche in styles.xml: si scrivono in tutti e due.
    let auto = o.elenco_stili.concat();
    let content = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {NS}><office:automatic-styles>{auto}</office:automatic-styles><office:body><office:text>{corpo}</office:text></office:body></office:document-content>");
    let tab = if doc.tab_predefinita > 0.0 { doc.tab_predefinita } else { 36.0 };
    let styles = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {NS}><office:styles><style:default-style style:family=\"paragraph\"><style:paragraph-properties style:tab-stop-distance=\"{}\"/><style:text-properties fo:language=\"it\" fo:country=\"IT\"/></style:default-style><style:style style:name=\"Standard\" style:family=\"paragraph\" style:class=\"text\"/><style:style style:name=\"Frame\" style:family=\"graphic\"><style:graphic-properties fo:border=\"none\" draw:stroke=\"none\" draw:fill=\"none\"/></style:style><style:style style:name=\"Graphics\" style:family=\"graphic\"><style:graphic-properties fo:border=\"none\" draw:stroke=\"none\"/></style:style></office:styles><office:automatic-styles>{layout}{auto}</office:automatic-styles><office:master-styles>{maestre}</office:master-styles></office:document-styles>",
        pt(tab)
    );
    let meta = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-meta {NS}><office:meta><dc:title>{}</dc:title><meta:generator>Cedro</meta:generator></office:meta></office:document-meta>", esc(&doc.titolo));
    let mut manifest = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\" manifest:version=\"1.3\"><manifest:file-entry manifest:full-path=\"/\" manifest:media-type=\"application/vnd.oasis.opendocument.text\"/><manifest:file-entry manifest:full-path=\"content.xml\" manifest:media-type=\"text/xml\"/><manifest:file-entry manifest:full-path=\"styles.xml\" manifest:media-type=\"text/xml\"/><manifest:file-entry manifest:full-path=\"meta.xml\" manifest:media-type=\"text/xml\"/>");
    for (nome, _, tipo) in &o.file_figure {
        let _ = write!(manifest, "<manifest:file-entry manifest:full-path=\"{nome}\" manifest:media-type=\"{tipo}\"/>");
    }
    manifest.push_str("</manifest:manifest>");
    let mut file: Vec<(String, Vec<u8>)> = vec![
        ("mimetype".into(), b"application/vnd.oasis.opendocument.text".to_vec()),
        ("META-INF/manifest.xml".into(), manifest.into_bytes()),
        ("content.xml".into(), content.into_bytes()),
        ("styles.xml".into(), styles.into_bytes()),
        ("meta.xml".into(), meta.into_bytes()),
    ];
    for (nome, dati, _) in std::mem::take(&mut o.file_figure) {
        file.push((nome, dati));
    }
    let voci: Vec<(&str, &[u8])> = file.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect();
    crate::zipstore::zip_archive(&voci)
}

#[cfg(test)]
mod test {
    use super::*;

    fn esempio() -> Documento {
        let f = Font { family: "Liberation Serif".into(), size: 12.0, ..Default::default() };
        let mut grassetto = f.clone();
        grassetto.bold = true;
        let par = |runs: Vec<Run>| Paragrafo { runs, font_fine: f.clone(), ..Default::default() };
        let t = |s: &str, f: &Font| Run { pezzo: Pezzo::Testo(s.into()), font: f.clone(), link: None };
        let mut titolo = par(vec![t("Delibera", &grassetto)]);
        titolo.titolo = Some(1);
        let mut voce = par(vec![t("Primo punto", &f)]);
        voce.etichetta = Some(("1.".into(), f.clone()));
        voce.etichetta_tab = true;
        voce.rientro_sx = 36.0;
        voce.prima_riga = -18.0;
        let cella = |s: &str| Cella { blocchi: vec![Blocco::Par(par(vec![t(s, &f)]))], span: 1, vmerge: None, sfondo: None, bordi: [None; 4], valign: 0, margini: [5.4, 5.4, 0.0, 0.0], larghezza: None };
        let tab = Tabella { griglia: vec![100.0, 100.0], righe: vec![Riga { celle: vec![cella("A1"), cella("B1")], altezza: 0.0, intestazione: false, non_dividere: false }], allinea: 0, rientro: 0.0, relativa: None };
        let nota = par(vec![t("Testo ", &f), t("con *nota*", &f), Run { pezzo: Pezzo::Nota(0), font: f.clone(), link: None }]);
        let sez = Sezione { blocchi: vec![Blocco::Par(titolo), Blocco::Par(voce), Blocco::Tab(tab), Blocco::Par(nota)], ..Default::default() };
        Documento { sezioni: vec![sez], note: vec![("1".into(), vec![Blocco::Par(par(vec![t("1", &f), t(" La nota.", &f)]))])], titolo: "Prova".into(), ..Default::default() }
    }

    #[test]
    fn testo_e_markdown() {
        let d = esempio();
        let t = testo(&d);
        assert!(t.contains("Delibera\n1.\tPrimo punto\nA1\tB1\nTesto con *nota*[1]"), "{t}");
        assert!(t.contains("[1] La nota."), "{t}");
        let m = markdown(&d);
        assert!(m.contains("# **Delibera**"), "{m}");
        assert!(m.contains("1. Primo punto"), "{m}");
        assert!(m.contains("| A1 | B1 |\n| --- | --- |"), "{m}");
        assert!(m.contains("con \\*nota\\*[^1]"), "{m}");
        assert!(m.contains("[^1]: La nota."), "{m}");
    }

    #[test]
    fn html_docx_e_odt() {
        let d = esempio();
        let h = html(&d, "Prova");
        assert!(h.contains("<h1") && h.contains("Delibera") && h.contains("<table") && h.contains("id=\"nota1\""), "{h}");
        let zip = |b: Vec<u8>| zip::ZipArchive::new(std::io::Cursor::new(b)).unwrap();
        let leggi = |z: &mut zip::ZipArchive<std::io::Cursor<Vec<u8>>>, n: &str| {
            let mut s = String::new();
            std::io::Read::read_to_string(&mut z.by_name(n).unwrap(), &mut s).unwrap();
            s
        };
        let mut w = zip(docx(&d));
        let x = leggi(&mut w, "word/document.xml");
        assert!(roxmltree::Document::parse(&x).is_ok());
        assert!(x.contains("Titolo1") && x.contains("<w:tbl>") && x.contains("footnoteReference"), "{x}");
        assert!(roxmltree::Document::parse(&leggi(&mut w, "word/footnotes.xml")).is_ok());
        // Riletto da Cedro: lo stesso testo.
        let w2 = super::super::docx::leggi_docx(&docx(&d)).unwrap();
        assert_eq!(testo(&w2).lines().next(), Some("Delibera"));
        let mut o = zip(odt(&d));
        assert_eq!(o.by_index(0).unwrap().name().unwrap(), "mimetype");
        let c = leggi(&mut o, "content.xml");
        assert!(roxmltree::Document::parse(&c).is_ok(), "{c}");
        assert!(roxmltree::Document::parse(&leggi(&mut o, "styles.xml")).is_ok());
        let o2 = super::super::odt::leggi_odt(&odt(&d)).unwrap();
        assert!(testo(&o2).contains("A1\tB1"), "{}", testo(&o2));
    }

    /// Prova a mano: `CEDRO_USCITE=a.docx:b.doc cargo test --lib prova_uscite -- --ignored` scrive accanto
    /// a ogni file le uscite .cedro.docx, .cedro.odt, .cedro.html, .cedro.md, .cedro.txt.
    #[test]
    #[ignore]
    fn prova_uscite() {
        let Ok(list) = std::env::var("CEDRO_USCITE") else { return };
        for f in list.split(':').filter(|s| !s.is_empty()) {
            let p = std::path::Path::new(f);
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            let bytes = std::fs::read(p).unwrap();
            let d = match super::super::leggi_documento(&ext, &bytes) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("ERRORE {f}: {e}");
                    continue;
                }
            };
            std::fs::write(format!("{f}.cedro.docx"), docx(&d)).unwrap();
            std::fs::write(format!("{f}.cedro.odt"), odt(&d)).unwrap();
            std::fs::write(format!("{f}.cedro.html"), html(&d, "prova")).unwrap();
            std::fs::write(format!("{f}.cedro.md"), markdown(&d)).unwrap();
            std::fs::write(format!("{f}.cedro.txt"), testo(&d)).unwrap();
        }
    }
}
