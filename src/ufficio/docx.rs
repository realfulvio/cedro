//! I documenti di Word (.docx, .docm, .dotx) letti com'è il file: stili con le loro eredità, elenchi
//! numerati, tabelle con gli stili di tabella, immagini nel testo e ancorate, caselle di testo,
//! intestazioni e piè di pagina per sezione (prima pagina, pari e dispari), note, campi (pagina,
//! totale, collegamenti), segnalibri. Il risultato è il `Documento` che `impagina_doc` mette in pagina.

use std::collections::HashMap;
use std::io::Cursor;

use super::documento::*;
use super::foglio::{attr, hex_color, voce, voce_bytes};
use super::pagina::{Dash, Rgb, Target};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn is_w(n: roxmltree::Node, name: &str) -> bool {
    n.is_element() && n.tag_name().name() == name && n.tag_name().namespace() == Some(W)
}

fn wchild<'a>(n: roxmltree::Node<'a, 'a>, name: &str) -> Option<roxmltree::Node<'a, 'a>> {
    n.children().find(|c| is_w(*c, name))
}

fn wval<'a>(n: roxmltree::Node<'a, 'a>) -> Option<&'a str> {
    attr(n, "val")
}

/// Un interruttore di Word: presente senza valore = acceso; «0», «false», «off» = spento.
fn on(n: Option<roxmltree::Node>) -> Option<bool> {
    n.map(|n| !matches!(wval(n), Some("0" | "false" | "off" | "none")))
}

fn twips(v: Option<&str>) -> Option<f32> {
    v.and_then(|v| v.trim_end_matches("pt").parse::<f32>().ok()).map(|t| t / 20.0)
}

fn color(v: Option<&str>) -> Option<Rgb> {
    v.filter(|v| *v != "auto").and_then(hex_color)
}

fn highlight(name: &str) -> Option<Rgb> {
    Some(match name {
        "yellow" => (255, 255, 0),
        "green" => (0, 255, 0),
        "cyan" => (0, 255, 255),
        "magenta" => (255, 0, 255),
        "blue" => (0, 0, 255),
        "red" => (255, 0, 0),
        "darkBlue" => (0, 0, 128),
        "darkCyan" => (0, 128, 128),
        "darkGreen" => (0, 128, 0),
        "darkMagenta" => (128, 0, 128),
        "darkRed" => (128, 0, 0),
        "darkYellow" => (128, 128, 0),
        "darkGray" => (128, 128, 128),
        "lightGray" => (192, 192, 192),
        "black" => (0, 0, 0),
        "white" => (255, 255, 255),
        _ => return None,
    })
}

/// Lo sfondo di un elemento w:shd: il riempimento, o il colore del motivo se è pieno o a percentuale.
fn shading(n: roxmltree::Node) -> Option<Rgb> {
    let fill = color(attr(n, "fill"));
    let fore = color(attr(n, "color")).unwrap_or((0, 0, 0));
    match attr(n, "val") {
        Some("solid") => Some(fore),
        Some(v) if v.starts_with("pct") => {
            let p: f32 = v[3..].parse().unwrap_or(0.0) / 100.0;
            let back = fill.unwrap_or((255, 255, 255));
            let mix = |a: u8, b: u8| (a as f32 * (1.0 - p) + b as f32 * p).round() as u8;
            Some((mix(back.0, fore.0), mix(back.1, fore.1), mix(back.2, fore.2)))
        }
        _ => fill,
    }
}

fn border(n: roxmltree::Node) -> Option<Linea> {
    let val = attr(n, "val")?;
    if matches!(val, "nil" | "none") {
        return None;
    }
    let width = attr(n, "sz").and_then(|v| v.parse::<f32>().ok()).map_or(0.5, |s| (s / 8.0).max(0.25));
    let (dash, double) = match val {
        "double" | "triple" | "thinThickSmallGap" | "thickThinSmallGap" | "thinThickMediumGap" | "thickThinMediumGap" | "thinThickLargeGap" | "thickThinLargeGap" => (Dash::Solid, true),
        "dashed" | "dashSmallGap" | "dotDash" | "dotDotDash" | "dashDotStroked" => (Dash::Dashed, false),
        "dotted" => (Dash::Dotted, false),
        _ => (Dash::Solid, false),
    };
    Some(Linea { width, color: color(attr(n, "color")).unwrap_or((0, 0, 0)), dash, double, space: attr(n, "space").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) })
}

// ------------------------------------------------------------------ proprietà con eredità

#[derive(Clone, Debug, Default)]
struct RProps {
    family: Option<String>,
    theme_family: Option<bool>,
    size: Option<f32>,
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<u8>,
    strike: Option<bool>,
    color: Option<Rgb>,
    highlight: Option<Option<Rgb>>,
    vert: Option<i8>,
    caps: Option<bool>,
    small_caps: Option<bool>,
    spacing: Option<f32>,
    hidden: Option<bool>,
}

impl RProps {
    fn parse(n: roxmltree::Node) -> RProps {
        let mut r = RProps::default();
        if let Some(f) = wchild(n, "rFonts") {
            if let Some(v) = attr(f, "ascii").or_else(|| attr(f, "hAnsi")) {
                r.family = Some(v.to_string());
                r.theme_family = None;
            } else if let Some(t) = attr(f, "asciiTheme").or_else(|| attr(f, "hAnsiTheme")) {
                r.theme_family = Some(t.starts_with("major"));
            }
        }
        r.size = wchild(n, "sz").and_then(|s| wval(s)).and_then(|v| v.parse::<f32>().ok()).map(|v| v / 2.0);
        r.bold = on(wchild(n, "b"));
        r.italic = on(wchild(n, "i"));
        r.underline = wchild(n, "u").map(|u| match wval(u) {
            Some("none") => 0,
            Some("double") => 2,
            _ => 1,
        });
        r.strike = on(wchild(n, "strike")).or(on(wchild(n, "dstrike")));
        r.color = wchild(n, "color").and_then(|c| color(wval(c)));
        if let Some(h) = wchild(n, "highlight") {
            r.highlight = Some(wval(h).and_then(highlight));
        } else if let Some(s) = wchild(n, "shd") {
            r.highlight = Some(shading(s));
        }
        r.vert = wchild(n, "vertAlign").map(|v| match wval(v) {
            Some("superscript") => 1,
            Some("subscript") => -1,
            _ => 0,
        });
        r.caps = on(wchild(n, "caps"));
        r.small_caps = on(wchild(n, "smallCaps"));
        r.spacing = twips(wchild(n, "spacing").and_then(|s| wval(s)));
        r.hidden = on(wchild(n, "vanish")).or(on(wchild(n, "specVanish")));
        r
    }

    fn merge(&mut self, o: &RProps) {
        macro_rules! m {
            ($($f:ident),*) => { $( if o.$f.is_some() { self.$f = o.$f.clone(); } )* };
        }
        if o.family.is_some() {
            self.family = o.family.clone();
            self.theme_family = None;
        }
        if o.theme_family.is_some() {
            self.theme_family = o.theme_family;
            self.family = None;
        }
        m!(size, bold, italic, underline, strike, color, highlight, vert, caps, small_caps, spacing, hidden);
    }

    fn font(&self, theme: &(String, String)) -> Font {
        let family = match (&self.family, self.theme_family) {
            (Some(f), _) => f.clone(),
            (None, Some(true)) => theme.0.clone(),
            (None, Some(false)) => theme.1.clone(),
            _ => "Times New Roman".into(),
        };
        Font {
            family,
            size: self.size.unwrap_or(10.0),
            bold: self.bold.unwrap_or(false),
            italic: self.italic.unwrap_or(false),
            underline: self.underline.is_some_and(|u| u > 0),
            double_underline: self.underline == Some(2),
            strike: self.strike.unwrap_or(false),
            color: self.color.unwrap_or((0, 0, 0)),
            highlight: self.highlight.flatten(),
            vert: self.vert.unwrap_or(0),
            caps: self.caps.unwrap_or(false),
            small_caps: self.small_caps.unwrap_or(false),
            spacing: self.spacing.unwrap_or(0.0),
        }
    }
}

#[derive(Clone, Debug, Default)]
struct PProps {
    jc: Option<Allinea>,
    left: Option<f32>,
    right: Option<f32>,
    first: Option<f32>,
    before: Option<f32>,
    after: Option<f32>,
    before_auto: Option<bool>,
    after_auto: Option<bool>,
    line: Option<Interlinea>,
    keep_next: Option<bool>,
    keep_lines: Option<bool>,
    page_break: Option<bool>,
    widow: Option<bool>,
    num: Option<(String, usize)>,
    tabs: Vec<(f32, Option<TipoTab>, Option<char>)>,
    outline: Option<u8>,
    shading: Option<Option<Rgb>>,
    borders: [Option<Option<Linea>>; 5],
    contextual: Option<bool>,
}

impl PProps {
    fn parse(n: roxmltree::Node) -> PProps {
        let mut p = PProps::default();
        p.jc = wchild(n, "jc").and_then(|j| wval(j)).map(|v| match v {
            "center" => Allinea::Centro,
            "right" | "end" => Allinea::Destra,
            "both" | "distribute" | "lowKashida" | "mediumKashida" | "highKashida" | "thaiDistribute" => Allinea::Giustifica,
            _ => Allinea::Sinistra,
        });
        if let Some(ind) = wchild(n, "ind") {
            p.left = twips(attr(ind, "left").or_else(|| attr(ind, "start")));
            p.right = twips(attr(ind, "right").or_else(|| attr(ind, "end")));
            if let Some(h) = twips(attr(ind, "hanging")) {
                p.first = Some(-h);
            } else if let Some(f) = twips(attr(ind, "firstLine")) {
                p.first = Some(f);
            }
        }
        if let Some(sp) = wchild(n, "spacing") {
            p.before_auto = attr(sp, "beforeAutospacing").map(|v| v != "0" && v != "false");
            p.after_auto = attr(sp, "afterAutospacing").map(|v| v != "0" && v != "false");
            p.before = if p.before_auto == Some(true) { Some(14.0) } else { twips(attr(sp, "before")) };
            p.after = if p.after_auto == Some(true) { Some(14.0) } else { twips(attr(sp, "after")) };
            if let Some(l) = attr(sp, "line").and_then(|v| v.parse::<f32>().ok()) {
                p.line = Some(match attr(sp, "lineRule") {
                    Some("exact") => Interlinea::Esatta(l / 20.0),
                    Some("atLeast") => Interlinea::Minima(l / 20.0),
                    _ => Interlinea::Multipla(l / 240.0),
                });
            }
        }
        p.keep_next = on(wchild(n, "keepNext"));
        p.keep_lines = on(wchild(n, "keepLines"));
        p.page_break = on(wchild(n, "pageBreakBefore"));
        p.widow = on(wchild(n, "widowControl"));
        p.contextual = on(wchild(n, "contextualSpacing"));
        if let Some(np) = wchild(n, "numPr") {
            let id = wchild(np, "numId").and_then(|x| wval(x)).unwrap_or("0").to_string();
            let lvl = wchild(np, "ilvl").and_then(|x| wval(x)).and_then(|v| v.parse().ok()).unwrap_or(0);
            p.num = Some((id, lvl));
        }
        if let Some(tabs) = wchild(n, "tabs") {
            for t in tabs.children().filter(|c| is_w(*c, "tab")) {
                let Some(pos) = twips(attr(t, "pos")) else { continue };
                let kind = match attr(t, "val") {
                    Some("clear") => None,
                    Some("center") => Some(TipoTab::Centro),
                    Some("right" | "end") => Some(TipoTab::Destra),
                    Some("decimal") => Some(TipoTab::Decimale),
                    _ => Some(TipoTab::Sinistra),
                };
                let leader = match attr(t, "leader") {
                    Some("dot" | "middleDot") => Some('.'),
                    Some("hyphen") => Some('-'),
                    Some("underscore" | "heavy") => Some('_'),
                    _ => None,
                };
                p.tabs.push((pos, kind, leader));
            }
        }
        p.outline = wchild(n, "outlineLvl").and_then(|o| wval(o)).and_then(|v| v.parse::<u8>().ok()).filter(|v| *v < 9).map(|v| v + 1);
        p.shading = wchild(n, "shd").map(shading);
        if let Some(b) = wchild(n, "pBdr") {
            for (i, names) in [["top", "top"], ["bottom", "bottom"], ["left", "start"], ["right", "end"], ["between", "between"]].iter().enumerate() {
                if let Some(x) = wchild(b, names[0]).or_else(|| wchild(b, names[1])) {
                    p.borders[i] = Some(border(x));
                }
            }
        }
        p
    }

    fn merge(&mut self, o: &PProps) {
        macro_rules! m {
            ($($f:ident),*) => { $( if o.$f.is_some() { self.$f = o.$f.clone(); } )* };
        }
        m!(jc, left, right, first, before, after, before_auto, after_auto, line, keep_next, keep_lines, page_break, widow, num, outline, shading, contextual);
        for (i, b) in o.borders.iter().enumerate() {
            if b.is_some() {
                self.borders[i] = *b;
            }
        }
        for t in &o.tabs {
            self.tabs.retain(|x| (x.0 - t.0).abs() > 0.5);
            self.tabs.push(*t);
        }
    }
}

// ------------------------------------------------------------------ stili

#[derive(Clone, Debug, Default)]
struct TcProps {
    shading: Option<Option<Rgb>>,
    borders: [Option<Option<Linea>>; 6],
    valign: Option<u8>,
}

impl TcProps {
    fn parse(n: roxmltree::Node) -> TcProps {
        let mut t = TcProps { shading: wchild(n, "shd").map(shading), ..Default::default() };
        // top, bottom, left, right, insideH, insideV
        let b = wchild(n, "tcBorders").or_else(|| wchild(n, "tblBorders"));
        if let Some(b) = b {
            for (i, names) in [["top", "top"], ["bottom", "bottom"], ["left", "start"], ["right", "end"], ["insideH", "insideH"], ["insideV", "insideV"]].iter().enumerate() {
                if let Some(x) = wchild(b, names[0]).or_else(|| wchild(b, names[1])) {
                    t.borders[i] = Some(border(x));
                }
            }
        }
        t.valign = wchild(n, "vAlign").and_then(|v| wval(v)).map(|v| match v {
            "center" => 1,
            "bottom" => 2,
            _ => 0,
        });
        t
    }

    fn merge(&mut self, o: &TcProps) {
        if o.shading.is_some() {
            self.shading = o.shading;
        }
        if o.valign.is_some() {
            self.valign = o.valign;
        }
        for (i, b) in o.borders.iter().enumerate() {
            if b.is_some() {
                self.borders[i] = *b;
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
struct Style {
    based_on: Option<String>,
    next_numbered: bool,
    p: PProps,
    r: RProps,
    /// Tabelle: proprietà di tutta la tabella e delle parti (firstRow, band1Horz…).
    tbl: TcProps,
    cell_margins: Option<[Option<f32>; 4]>,
    parts: HashMap<String, (PProps, RProps, TcProps)>,
}

struct Styles {
    by_id: HashMap<String, Style>,
    default_par: Option<String>,
    default_table: Option<String>,
    doc_r: RProps,
    doc_p: PProps,
}

impl Styles {
    fn parse(xml: Option<&str>) -> Styles {
        let mut s = Styles { by_id: HashMap::new(), default_par: None, default_table: None, doc_r: RProps::default(), doc_p: PProps::default() };
        let Some(xml) = xml else { return s };
        let Ok(doc) = roxmltree::Document::parse(xml) else { return s };
        let root = doc.root_element();
        if let Some(dd) = wchild(root, "docDefaults") {
            if let Some(r) = wchild(dd, "rPrDefault").and_then(|x| wchild(x, "rPr")) {
                s.doc_r = RProps::parse(r);
            }
            if let Some(p) = wchild(dd, "pPrDefault").and_then(|x| wchild(x, "pPr")) {
                s.doc_p = PProps::parse(p);
            }
        }
        for st in root.children().filter(|c| is_w(*c, "style")) {
            let Some(id) = attr(st, "styleId") else { continue };
            let kind = attr(st, "type").unwrap_or("paragraph").to_string();
            if attr(st, "default").is_some_and(|v| v == "1" || v == "true") {
                match kind.as_str() {
                    "paragraph" => s.default_par = Some(id.to_string()),
                    "table" => s.default_table = Some(id.to_string()),
                    _ => {}
                }
            }
            let mut style = Style {
                based_on: wchild(st, "basedOn").and_then(|b| wval(b)).map(str::to_string),
                p: wchild(st, "pPr").map(PProps::parse).unwrap_or_default(),
                r: wchild(st, "rPr").map(RProps::parse).unwrap_or_default(),
                ..Default::default()
            };
            if let Some(tp) = wchild(st, "tblPr") {
                style.tbl = TcProps::parse(tp);
                style.cell_margins = wchild(tp, "tblCellMar").map(cell_margins);
            }
            if let Some(tc) = wchild(st, "tcPr") {
                let t = TcProps::parse(tc);
                style.tbl.merge(&t);
            }
            for part in st.children().filter(|c| is_w(*c, "tblStylePr")) {
                let Some(t) = attr(part, "type") else { continue };
                let p = wchild(part, "pPr").map(PProps::parse).unwrap_or_default();
                let r = wchild(part, "rPr").map(RProps::parse).unwrap_or_default();
                let mut tc = wchild(part, "tcPr").map(TcProps::parse).unwrap_or_default();
                if let Some(tp) = wchild(part, "tblPr") {
                    tc.merge(&TcProps::parse(tp));
                }
                style.parts.insert(t.to_string(), (p, r, tc));
            }
            style.next_numbered = style.p.num.is_some();
            s.by_id.insert(id.to_string(), style);
        }
        s
    }

    /// La catena degli stili, dal più lontano (basedOn) al più vicino.
    fn chain(&self, id: &str) -> Vec<&Style> {
        let mut out = Vec::new();
        let mut cur = Some(id.to_string());
        while let Some(c) = cur.take() {
            if out.len() > 20 {
                break;
            }
            let Some(s) = self.by_id.get(&c) else { break };
            out.push(s);
            cur = s.based_on.clone();
        }
        out.reverse();
        out
    }

    fn para(&self, id: &str) -> (PProps, RProps) {
        let mut p = PProps::default();
        let mut r = RProps::default();
        for s in self.chain(id) {
            p.merge(&s.p);
            r.merge(&s.r);
        }
        (p, r)
    }

    fn run(&self, id: &str) -> RProps {
        let mut r = RProps::default();
        for s in self.chain(id) {
            r.merge(&s.r);
        }
        r
    }
}

fn cell_margins(n: roxmltree::Node) -> [Option<f32>; 4] {
    let g = |a: &str, b: &str| wchild(n, a).or_else(|| wchild(n, b)).and_then(|x| twips(attr(x, "w")));
    [g("left", "start"), g("right", "end"), g("top", "top"), g("bottom", "bottom")]
}

// ------------------------------------------------------------------ elenchi

#[derive(Clone, Debug, Default)]
struct Level {
    start: u32,
    fmt: String,
    text: String,
    suff_tab: bool,
    jc: Allinea,
    p: PProps,
    r: RProps,
    legal: bool,
    restart: Option<usize>,
}

struct Numbering {
    abstracts: HashMap<String, Vec<Level>>,
    /// numId → (abstractNumId, livelli sostituiti, inizi sostituiti)
    nums: HashMap<String, (String, HashMap<usize, Level>, HashMap<usize, u32>)>,
    counters: HashMap<String, [u32; 9]>,
    started: std::collections::HashSet<String>,
}

impl Numbering {
    fn parse(xml: Option<&str>) -> Numbering {
        let mut n = Numbering { abstracts: HashMap::new(), nums: HashMap::new(), counters: HashMap::new(), started: Default::default() };
        let Some(xml) = xml else { return n };
        let Ok(doc) = roxmltree::Document::parse(xml) else { return n };
        let level = |l: roxmltree::Node| Level {
            start: wchild(l, "start").and_then(|x| wval(x)).and_then(|v| v.parse().ok()).unwrap_or(1),
            fmt: wchild(l, "numFmt").and_then(|x| wval(x)).unwrap_or("decimal").to_string(),
            text: wchild(l, "lvlText").and_then(|x| wval(x)).unwrap_or("").to_string(),
            suff_tab: !matches!(wchild(l, "suff").and_then(|x| wval(x)), Some("space" | "nothing")),
            jc: match wchild(l, "lvlJc").and_then(|x| wval(x)) {
                Some("right" | "end") => Allinea::Destra,
                Some("center") => Allinea::Centro,
                _ => Allinea::Sinistra,
            },
            p: wchild(l, "pPr").map(PProps::parse).unwrap_or_default(),
            r: wchild(l, "rPr").map(RProps::parse).unwrap_or_default(),
            legal: on(wchild(l, "isLgl")).unwrap_or(false),
            restart: wchild(l, "lvlRestart").and_then(|x| wval(x)).and_then(|v| v.parse().ok()),
        };
        for a in doc.root_element().children().filter(|c| is_w(*c, "abstractNum")) {
            let Some(id) = attr(a, "abstractNumId") else { continue };
            let mut levels = vec![Level { start: 1, fmt: "decimal".into(), text: String::new(), suff_tab: true, ..Default::default() }; 9];
            for l in a.children().filter(|c| is_w(*c, "lvl")) {
                let i: usize = attr(l, "ilvl").and_then(|v| v.parse().ok()).unwrap_or(0);
                if i < 9 {
                    levels[i] = level(l);
                }
            }
            n.abstracts.insert(id.to_string(), levels);
        }
        for num in doc.root_element().children().filter(|c| is_w(*c, "num")) {
            let Some(id) = attr(num, "numId") else { continue };
            let abs = wchild(num, "abstractNumId").and_then(|x| wval(x)).unwrap_or("").to_string();
            let mut lv = HashMap::new();
            let mut starts = HashMap::new();
            for o in num.children().filter(|c| is_w(*c, "lvlOverride")) {
                let i: usize = attr(o, "ilvl").and_then(|v| v.parse().ok()).unwrap_or(0);
                if let Some(s) = wchild(o, "startOverride").and_then(|x| wval(x)).and_then(|v| v.parse().ok()) {
                    starts.insert(i, s);
                }
                if let Some(l) = wchild(o, "lvl") {
                    lv.insert(i, level(l));
                }
            }
            n.nums.insert(id.to_string(), (abs, lv, starts));
        }
        n
    }

    fn level(&self, num: &str, ilvl: usize) -> Option<Level> {
        let (abs, over, _) = self.nums.get(num)?;
        over.get(&ilvl).cloned().or_else(|| self.abstracts.get(abs).and_then(|l| l.get(ilvl).cloned()))
    }

    /// L'etichetta del prossimo paragrafo dell'elenco (e fa avanzare i contatori).
    fn next(&mut self, num: &str, ilvl: usize) -> Option<(String, Level)> {
        let ilvl = ilvl.min(8);
        let (abs, _, starts) = self.nums.get(num)?.clone();
        let levels: Vec<Level> = (0..9).map(|i| self.level(num, i).unwrap_or_default()).collect();
        let key = abs.clone();
        let c = self.counters.entry(key).or_insert([0; 9]);
        // Un numId con «ricomincia da» riparte la prima volta che si usa.
        if self.started.insert(num.to_string()) {
            for (l, s) in &starts {
                if *l < 9 {
                    c[*l] = s.saturating_sub(1);
                    if *l == ilvl {
                        c[*l] = s.saturating_sub(1);
                    }
                }
            }
        }
        if c[ilvl] == 0 && !starts.contains_key(&ilvl) {
            c[ilvl] = levels[ilvl].start.saturating_sub(1);
        }
        c[ilvl] += 1;
        for l in ilvl + 1..9 {
            if levels[l].restart.is_none_or(|r| r == 0 || r > ilvl) {
                c[l] = 0;
            }
        }
        let lv = levels[ilvl].clone();
        if lv.fmt == "none" {
            return Some((String::new(), lv));
        }
        let mut text = String::new();
        let mut chars = lv.text.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '%' {
                if let Some(d) = chars.peek().and_then(|d| d.to_digit(10)) {
                    chars.next();
                    let i = (d as usize).saturating_sub(1).min(8);
                    let v = if c[i] == 0 { levels[i].start } else { c[i] };
                    let fmt = if lv.legal { "decimal" } else { levels[i].fmt.as_str() };
                    text.push_str(&number_text(v, fmt));
                    continue;
                }
            }
            text.push(ch);
        }
        if lv.fmt == "bullet" {
            text = text.chars().map(bullet_char).collect();
        }
        Some((text, lv))
    }
}

fn roman(mut n: u32) -> String {
    const T: [(u32, &str); 13] = [(1000, "m"), (900, "cm"), (500, "d"), (400, "cd"), (100, "c"), (90, "xc"), (50, "l"), (40, "xl"), (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")];
    let mut s = String::new();
    for (v, t) in T {
        while n >= v {
            s.push_str(t);
            n -= v;
        }
    }
    s
}

fn letters(n: u32) -> String {
    // a…z, poi aa…zz, come Word.
    if n == 0 {
        return String::new();
    }
    let c = (b'a' + ((n - 1) % 26) as u8) as char;
    c.to_string().repeat(((n - 1) / 26 + 1) as usize)
}

pub fn number_text(v: u32, fmt: &str) -> String {
    match fmt {
        "lowerLetter" => letters(v),
        "upperLetter" => letters(v).to_uppercase(),
        "lowerRoman" => roman(v),
        "upperRoman" => roman(v).to_uppercase(),
        "decimalZero" => format!("{v:02}"),
        "ordinal" => format!("{v}°"),
        "bullet" | "none" => String::new(),
        _ => v.to_string(),
    }
}

/// I punti degli elenchi nei caratteri Symbol e Wingdings come caratteri Unicode.
pub fn bullet_char(c: char) -> char {
    let code = c as u32;
    let low = if (0xF000..=0xF0FF).contains(&code) { code - 0xF000 } else { return c };
    match low {
        0xB7 | 0x9F | 0x6C => '•',
        0xA7 | 0x6E => '▪',
        0xA8 => '◻',
        0x71 => '❑',
        0x76 => '❖',
        0xD8 => '➢',
        0xE0 | 0xE8 => '➔',
        0xFC => '✓',
        0xFB => '✗',
        0x2D => '–',
        0x6F => '□',
        0x77 => '⬥',
        _ => '•',
    }
}

// ------------------------------------------------------------------ lettura

struct Field {
    instr: String,
    result: bool,
    /// Casella di spunta di un modulo (FORMCHECKBOX): spuntata o no.
    check: Option<bool>,
}

struct Part {
    rels: HashMap<String, String>,
}

struct Ctx<'z> {
    zip: zip::ZipArchive<Cursor<&'z [u8]>>,
    styles: Styles,
    numbering: Numbering,
    theme: (String, String),
    doc: Documento,
    /// footnotes.xml ed endnotes.xml, letti una volta.
    notes_xml: [Option<std::rc::Rc<String>>; 2],
    fields: Vec<Field>,
    /// Paragrafi dentro una tabella: le proprietà dello stile di tabella.
    table_style: Vec<(PProps, RProps)>,
    image_cache: HashMap<String, Option<usize>>,
    footnote_count: usize,
    /// Le proprietà della cornice dell'ultimo paragrafo (per unire quelli nella stessa cornice).
    last_frame: Option<String>,
}

fn rels_of(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, part: &str) -> Part {
    let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
    let mut rels = HashMap::new();
    if let Some(xml) = voce(zip, &format!("{dir}/_rels/{file}.rels")) {
        if let Ok(d) = roxmltree::Document::parse(&xml) {
            for r in d.descendants().filter(|n| n.tag_name().name() == "Relationship") {
                let (Some(id), Some(target)) = (attr(r, "Id"), attr(r, "Target")) else { continue };
                let external = attr(r, "TargetMode") == Some("External");
                let t = if external || target.contains("://") { format!("ext:{target}") } else { resolve(dir, target) };
                rels.insert(id.to_string(), t);
            }
        }
    }
    Part { rels }
}

fn resolve(dir: &str, target: &str) -> String {
    if let Some(abs) = target.strip_prefix('/') {
        return abs.to_string();
    }
    let mut parts: Vec<&str> = if dir.is_empty() { Vec::new() } else { dir.split('/').collect() };
    for p in target.split('/') {
        match p {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            x => parts.push(x),
        }
    }
    parts.join("/")
}

fn emu(v: Option<&str>) -> f32 {
    v.and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0) as f32 / 12_700.0
}

impl<'z> Ctx<'z> {
    fn image(&mut self, part: &Part, rid: &str, w: f32, h: f32) -> Option<usize> {
        let target = part.rels.get(rid)?.clone();
        if target.starts_with("ext:") {
            return None;
        }
        let key = format!("{target}@{w:.1}x{h:.1}");
        if let Some(v) = self.image_cache.get(&key) {
            return *v;
        }
        let bytes = voce_bytes(&mut self.zip, &target);
        let img = bytes.and_then(|b| crate::pdfdoc::DocImage::from_bytes(&b, Some((w, h)), 0));
        let idx = img.map(|i| {
            self.doc.immagini.push(i);
            self.doc.immagini.len() - 1
        });
        self.image_cache.insert(key, idx);
        idx
    }

    fn base_r(&self, pstyle: &str) -> RProps {
        let mut r = self.styles.doc_r.clone();
        if let Some((_, tr)) = self.table_style.last() {
            r.merge(tr);
        }
        r.merge(&self.styles.para(pstyle).1);
        r
    }

    fn blocks(&mut self, parent: roxmltree::Node, part: &Part, out: &mut Vec<Blocco>) {
        for c in parent.children().filter(|c| c.is_element()) {
            if is_w(c, "p") {
                self.paragraph(c, part, out);
            } else if is_w(c, "tbl") {
                if let Some(t) = self.table(c, part, out) {
                    out.push(Blocco::Tab(t));
                }
            } else if is_w(c, "sdt") {
                if let Some(content) = wchild(c, "sdtContent") {
                    self.blocks(content, part, out);
                }
            } else if is_w(c, "customXml") || is_w(c, "ins") || is_w(c, "smartTag") {
                self.blocks(c, part, out);
            } else if c.tag_name().name() == "AlternateContent" {
                if let Some(ch) = c.children().find(|x| x.tag_name().name() == "Choice").or_else(|| c.children().find(|x| x.tag_name().name() == "Fallback")) {
                    self.blocks(ch, part, out);
                }
            }
        }
    }

    /// Le proprietà del paragrafo risolte: documento, tabella, stile, elenco, dirette.
    fn para_props(&self, ppr: Option<roxmltree::Node>) -> (String, PProps, RProps) {
        let pstyle = ppr.and_then(|p| wchild(p, "pStyle")).and_then(|s| wval(s)).map(str::to_string).or_else(|| self.styles.default_par.clone()).unwrap_or_default();
        let mut p = self.styles.doc_p.clone();
        if let Some((tp, _)) = self.table_style.last() {
            p.merge(tp);
        }
        let (sp, _) = self.styles.para(&pstyle);
        p.merge(&sp);
        let direct = ppr.map(PProps::parse).unwrap_or_default();
        // Il rientro dell'elenco sta tra lo stile e le proprietà dirette.
        let num = direct.num.clone().or(p.num.clone());
        if let Some((id, lvl)) = &num {
            if let Some(level) = self.numbering.level(id, *lvl) {
                p.merge(&level.p);
            }
        }
        p.merge(&direct);
        let r = self.base_r(&pstyle);
        (pstyle, p, r)
    }

    fn paragraph(&mut self, n: roxmltree::Node, part: &Part, out: &mut Vec<Blocco>) {
        let ppr = wchild(n, "pPr");
        let (pstyle, p, base_r) = self.para_props(ppr);
        let mut par = Paragrafo {
            allinea: p.jc.unwrap_or_default(),
            rientro_sx: p.left.unwrap_or(0.0),
            rientro_dx: p.right.unwrap_or(0.0),
            prima_riga: p.first.unwrap_or(0.0),
            prima: p.before.unwrap_or(0.0),
            dopo: p.after.unwrap_or(0.0),
            interlinea: p.line.unwrap_or_default(),
            insieme_al_seguente: p.keep_next.unwrap_or(false),
            righe_insieme: p.keep_lines.unwrap_or(false),
            salto_prima: p.page_break.unwrap_or(false),
            vedove: p.widow.unwrap_or(false),
            tabs: p.tabs.iter().filter_map(|(pos, k, l)| k.map(|tipo| Tab { pos: *pos, tipo, leader: *l })).collect(),
            titolo: p.outline.or_else(|| heading_level(&pstyle)),
            sfondo: p.shading.flatten(),
            bordi: [p.borders[0].flatten(), p.borders[1].flatten(), p.borders[2].flatten(), p.borders[3].flatten(), p.borders[4].flatten()],
            contestuale: p.contextual.unwrap_or(false),
            auto_prima: p.before_auto.unwrap_or(false),
            auto_dopo: p.after_auto.unwrap_or(false),
            stile: pstyle.clone(),
            ..Default::default()
        };
        par.tabs.sort_by(|a, b| a.pos.total_cmp(&b.pos));
        // Il segno di paragrafo: dà l'altezza alla riga vuota.
        let mut end_r = base_r.clone();
        if let Some(r) = ppr.and_then(|x| wchild(x, "rPr")) {
            end_r.merge(&RProps::parse(r));
        }
        par.font_fine = end_r.font(&self.theme);
        if let Some((id, lvl)) = &p.num {
            if id != "0" {
                if let Some((text, level)) = self.numbering.next(id, *lvl) {
                    let mut lr = base_r.clone();
                    lr.merge(&level.r);
                    // L'etichetta non prende sottolineature ed evidenziazioni del testo.
                    let mut font = lr.font(&self.theme);
                    if text.chars().all(|c| "•▪◻❑❖➢➔✓✗–□⬥".contains(c)) {
                        font.family = base_r.font(&self.theme).family;
                        if text.contains('•') {
                            font.family = "Symbol".into();
                        }
                    }
                    font.underline = false;
                    if !text.is_empty() {
                        par.etichetta = Some((text, font));
                        par.etichetta_tab = level.suff_tab;
                        par.etichetta_allinea = level.jc;
                    }
                }
            }
        }
        let mut anchored = Vec::new();
        self.inline(n, part, &base_r, None, &mut par, &mut anchored);
        // Paragrafo con dentro una sezione: lo lascia a chi legge il corpo.
        for m in anchored {
            out.push(Blocco::Mobile(m));
        }
        // Un paragrafo vuoto con il segno di paragrafo nascosto non si vede (Word lo unisce al seguente).
        if end_r.hidden == Some(true) && par.runs.is_empty() && par.etichetta.is_none() {
            return;
        }
        // Un paragrafo in cornice (framePr): un oggetto mobile; quelli uguali di seguito stanno insieme.
        if let Some(fp) = ppr.and_then(|p| wchild(p, "framePr")).filter(|f| attr(*f, "dropCap").is_none_or(|d| d == "none")) {
            let key = fp.attributes().map(|a| format!("{}={}", a.name(), a.value())).collect::<Vec<_>>().join(";");
            if self.last_frame.as_deref() == Some(key.as_str()) {
                if let Some(Blocco::Mobile(Mobile { contenuto: MobileContenuto::Cornice(b), .. })) = out.last_mut() {
                    b.push(Blocco::Par(par));
                    return;
                }
            }
            let tw = |k: &str| twips(attr(fp, k));
            let h = tw("h").unwrap_or(0.0);
            let base_h = match attr(fp, "hAnchor") {
                Some("page") => Base::Pagina,
                Some("margin") => Base::Margine,
                _ => Base::Colonna,
            };
            let base_v = match attr(fp, "vAnchor") {
                Some("page") => Base::Pagina,
                Some("text") => Base::Paragrafo,
                _ => Base::Margine,
            };
            let x = match attr(fp, "xAlign") {
                Some("center") => Riferimento::Allineato(base_h, 1),
                Some("right" | "outside") => Riferimento::Allineato(base_h, 2),
                Some("left" | "inside") => Riferimento::Allineato(base_h, 0),
                _ => Riferimento::Spostato(base_h, tw("x").unwrap_or(0.0)),
            };
            let y = match attr(fp, "yAlign") {
                Some("center") => Riferimento::Allineato(base_v, 1),
                Some("bottom") => Riferimento::Allineato(base_v, 2),
                Some("top") => Riferimento::Allineato(base_v, 0),
                _ => Riferimento::Spostato(base_v, tw("y").unwrap_or(0.0)),
            };
            let avvolge = match attr(fp, "wrap") {
                Some("notBeside") => Avvolge::SopraSotto,
                Some("none") => Avvolge::Nessuno,
                _ => Avvolge::Lati,
            };
            self.last_frame = Some(key);
            out.push(Blocco::Mobile(Mobile { contenuto: MobileContenuto::Cornice(vec![Blocco::Par(par)]), w: tw("w").unwrap_or(0.0), h: if attr(fp, "hRule") == Some("exact") { -h } else { h }, x, y, avvolge, dietro: false }));
            return;
        }
        self.last_frame = None;
        out.push(Blocco::Par(par));
    }

    fn inline(&mut self, n: roxmltree::Node, part: &Part, base: &RProps, link: Option<&Target>, par: &mut Paragrafo, anchored: &mut Vec<Mobile>) {
        for c in n.children().filter(|c| c.is_element()) {
            let name = c.tag_name().name();
            if c.tag_name().namespace() == Some(W) {
                match name {
                    "r" => self.run(c, part, base, link, par, anchored),
                    "hyperlink" => {
                        let target = attr(c, "id")
                            .and_then(|id| part.rels.get(id))
                            .and_then(|t| t.strip_prefix("ext:"))
                            .map(|u| Target::Uri(match attr(c, "anchor") {
                                Some(a) => format!("{u}#{a}"),
                                None => u.to_string(),
                            }))
                            .or_else(|| attr(c, "anchor").map(|a| Target::Anchor(a.to_string())));
                        self.inline(c, part, base, target.as_ref().or(link), par, anchored);
                    }
                    "ins" | "smartTag" | "customXml" | "dir" | "bdo" => self.inline(c, part, base, link, par, anchored),
                    "sdt" => {
                        if let Some(ct) = wchild(c, "sdtContent") {
                            self.inline(ct, part, base, link, par, anchored);
                        }
                    }
                    "fldSimple" => {
                        let instr = attr(c, "instr").unwrap_or("").trim().to_string();
                        self.fields.push(Field { instr, result: true, check: None });
                        let before = par.runs.len();
                        self.field_start(par, base);
                        if par.runs.len() == before || !is_page_field(&self.fields.last().map(|f| f.instr.clone()).unwrap_or_default()) {
                            let l = field_link(&self.fields.last().map(|f| f.instr.clone()).unwrap_or_default());
                            self.inline(c, part, base, l.as_ref().or(link), par, anchored);
                        }
                        self.fields.pop();
                    }
                    "bookmarkStart" => {
                        if let Some(nm) = attr(c, "name") {
                            if nm != "_GoBack" {
                                par.segnalibri.push(nm.to_string());
                            }
                        }
                    }
                    _ => {}
                }
            } else if name == "AlternateContent" {
                if let Some(ch) = c.children().find(|x| x.tag_name().name() == "Choice").or_else(|| c.children().find(|x| x.tag_name().name() == "Fallback")) {
                    self.inline(ch, part, base, link, par, anchored);
                }
            } else if name == "oMath" || name == "oMathPara" {
                let text: String = c.descendants().filter(|d| d.tag_name().name() == "t").filter_map(|d| d.text()).collect();
                par.runs.push(Run { pezzo: Pezzo::Testo(text), font: base.font(&self.theme), link: link.cloned() });
            }
        }
    }

    /// All'inizio del risultato di un campo: il numero della pagina e il totale si calcolano qui.
    fn field_start(&mut self, par: &mut Paragrafo, r: &RProps) {
        let Some(f) = self.fields.last() else { return };
        let word = f.instr.split_whitespace().next().unwrap_or("").to_ascii_uppercase();
        let pezzo = match word.as_str() {
            "PAGE" => Pezzo::NumPagina,
            "NUMPAGES" | "SECTIONPAGES" => Pezzo::NumPagine,
            _ => return,
        };
        par.runs.push(Run { pezzo, font: r.font(&self.theme), link: None });
    }

    fn in_hidden_field(&self) -> bool {
        self.fields.iter().any(|f| !f.result || is_page_field(&f.instr))
    }

    fn run(&mut self, n: roxmltree::Node, part: &Part, base: &RProps, link: Option<&Target>, par: &mut Paragrafo, anchored: &mut Vec<Mobile>) {
        let mut r = base.clone();
        let rpr = wchild(n, "rPr");
        if let Some(st) = rpr.and_then(|x| wchild(x, "rStyle")).and_then(|s| wval(s)) {
            r.merge(&self.styles.run(st));
        }
        if let Some(x) = rpr {
            r.merge(&RProps::parse(x));
        }
        let hidden = r.hidden.unwrap_or(false);
        let font = r.font(&self.theme);
        let field_link = self.fields.iter().rev().find_map(|f| if f.result { field_link(&f.instr) } else { None });
        let link = link.cloned().or(field_link);
        let push = |par: &mut Paragrafo, pezzo: Pezzo| par.runs.push(Run { pezzo, font: font.clone(), link: link.clone() });
        for c in n.children().filter(|c| c.is_element()) {
            let name = c.tag_name().name();
            if c.tag_name().namespace() == Some(W) {
                match name {
                    "fldChar" => match attr(c, "fldCharType") {
                        Some("begin") => {
                            let cb = wchild(c, "ffData").and_then(|d| wchild(d, "checkBox"));
                            let check = cb.map(|b| on(wchild(b, "checked")).or_else(|| on(wchild(b, "default"))).unwrap_or(false));
                            self.fields.push(Field { instr: String::new(), result: false, check });
                        }
                        Some("separate") => {
                            if let Some(f) = self.fields.last_mut() {
                                f.result = true;
                            }
                            if self.fields.iter().rev().skip(1).all(|f| f.result && !is_page_field(&f.instr)) {
                                self.field_start(par, &r);
                            }
                        }
                        Some("end") => {
                            // Un campo senza risultato (PAGE scritto senza «separate»): il numero va messo ora.
                            if let Some(f) = self.fields.pop() {
                                if let Some(v) = f.check.filter(|_| f.instr.trim_start().to_uppercase().starts_with("FORMCHECKBOX")) {
                                    if self.fields.iter().all(|f| f.result && !is_page_field(&f.instr)) && !hidden {
                                        push(par, Pezzo::Testo(if v { "\u{2612}" } else { "\u{2610}" }.into()));
                                    }
                                }
                                if !f.result && is_page_field(&f.instr) && self.fields.iter().all(|f| f.result && !is_page_field(&f.instr)) {
                                    self.fields.push(f);
                                    self.field_start(par, &r);
                                    self.fields.pop();
                                }
                            }
                        }
                        _ => {}
                    },
                    "instrText" => {
                        if let Some(f) = self.fields.last_mut() {
                            if !f.result {
                                f.instr.push_str(c.text().unwrap_or(""));
                            }
                        }
                    }
                    _ if self.in_hidden_field() || hidden => {}
                    "t" => push(par, Pezzo::Testo(c.text().unwrap_or("").to_string())),
                    "tab" | "ptab" => push(par, Pezzo::Tab),
                    "br" => push(
                        par,
                        match attr(c, "type") {
                            Some("page") => Pezzo::SaltoPagina,
                            Some("column") => Pezzo::SaltoColonna,
                            _ => Pezzo::ACapo,
                        },
                    ),
                    "cr" => push(par, Pezzo::ACapo),
                    "noBreakHyphen" => push(par, Pezzo::Testo("\u{2011}".into())),
                    "softHyphen" => {}
                    "sym" => {
                        let ch = attr(c, "char").and_then(|h| u32::from_str_radix(h, 16).ok()).and_then(char::from_u32).map(|ch| {
                            let code = ch as u32;
                            if code < 0x100 {
                                bullet_char(char::from_u32(0xF000 + code).unwrap_or(ch))
                            } else {
                                bullet_char(ch)
                            }
                        });
                        if let Some(ch) = ch {
                            push(par, Pezzo::Testo(ch.to_string()));
                        }
                    }
                    "footnoteReference" | "endnoteReference" => {
                        let end = name == "endnoteReference";
                        if let Some(id) = attr(c, "id") {
                            if let Some(i) = self.note(id, end) {
                                let mut f = font.clone();
                                f.vert = 1;
                                par.runs.push(Run { pezzo: if end { Pezzo::NotaFine(i) } else { Pezzo::Nota(i) }, font: f, link: None });
                            }
                        }
                    }
                    "footnoteRef" | "endnoteRef" => {
                        // Dentro la nota: il suo numero.
                        let mut f = font.clone();
                        f.vert = 1;
                        par.runs.push(Run { pezzo: Pezzo::Testo(format!("{}", self.footnote_count)), font: f, link: None });
                    }
                    "drawing" => self.drawing(c, part, &font, par, anchored),
                    "pict" | "object" => self.vml(c, part, &font, par, anchored),
                    "lastRenderedPageBreak" | "rPr" | "delText" => {}
                    _ => {}
                }
            } else if name == "AlternateContent" && !self.in_hidden_field() && !hidden {
                if let Some(ch) = c.children().find(|x| x.tag_name().name() == "Choice").or_else(|| c.children().find(|x| x.tag_name().name() == "Fallback")) {
                    let fake = ch;
                    for d in fake.children().filter(|d| d.is_element()) {
                        match d.tag_name().name() {
                            "drawing" => self.drawing(d, part, &font, par, anchored),
                            "pict" => self.vml(d, part, &font, par, anchored),
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    /// Una nota a piè di pagina o di chiusura: si legge una volta, con il suo numero.
    fn note(&mut self, id: &str, end: bool) -> Option<usize> {
        let file = if end { "word/endnotes.xml" } else { "word/footnotes.xml" };
        let slot = end as usize;
        if self.notes_xml[slot].is_none() {
            self.notes_xml[slot] = Some(std::rc::Rc::new(voce(&mut self.zip, file)?));
        }
        let xml = self.notes_xml[slot].clone()?;
        let doc = roxmltree::Document::parse(&xml).ok()?;
        let tag = if end { "endnote" } else { "footnote" };
        let node = doc.root_element().children().find(|c| is_w(*c, tag) && attr(*c, "id") == Some(id))?;
        let part = rels_of(&mut self.zip, file);
        let list_len = if end { self.doc.note_fine.len() } else { self.doc.note.len() };
        let number = if end { roman((list_len + 1) as u32) } else { (list_len + 1).to_string() };
        self.footnote_count = list_len + 1;
        let saved = std::mem::take(&mut self.fields);
        let mut blocks = Vec::new();
        self.blocks(node, &part, &mut blocks);
        self.fields = saved;
        // Il numero dentro la nota è quello scritto nel testo.
        for b in &mut blocks {
            if let Blocco::Par(p) = b {
                for r in &mut p.runs {
                    if r.font.vert == 1 {
                        if let Pezzo::Testo(t) = &mut r.pezzo {
                            if t.parse::<usize>().is_ok() {
                                *t = number.clone();
                            }
                        }
                    }
                }
            }
        }
        if end {
            self.doc.note_fine.push((number, blocks));
            Some(self.doc.note_fine.len() - 1)
        } else {
            self.doc.note.push((number, blocks));
            Some(self.doc.note.len() - 1)
        }
    }

    fn drawing(&mut self, d: roxmltree::Node, part: &Part, font: &Font, par: &mut Paragrafo, anchored: &mut Vec<Mobile>) {
        for frame in d.children().filter(|c| c.is_element()) {
            let kind = frame.tag_name().name();
            let ext = frame.children().find(|c| c.tag_name().name() == "extent");
            let (w, h) = (emu(ext.and_then(|e| attr(e, "cx"))), emu(ext.and_then(|e| attr(e, "cy"))));
            // Più immagini alternative (un PDF per il Mac e un PNG di riserva): la prima che si legge.
            let blips: Vec<String> = frame.descendants().filter(|c| c.tag_name().name() == "blip").filter_map(|b| b.attributes().find(|a| a.name() == "embed").map(|a| a.value().to_string())).collect();
            let blip = blips.iter().find(|r| self.image(part, r, w, h).is_some()).or(blips.first()).cloned();
            let txbx = frame.descendants().find(|c| c.tag_name().name() == "txbxContent");
            if kind == "inline" {
                if let Some(rid) = blip {
                    let img = self.image(part, &rid, w, h).unwrap_or(usize::MAX);
                    par.runs.push(Run { pezzo: Pezzo::Immagine { img, w, h }, font: font.clone(), link: None });
                }
                continue;
            }
            if kind != "anchor" {
                continue;
            }
            let content = if let Some(rid) = blip {
                match self.image(part, &rid, w, h) {
                    Some(i) => MobileContenuto::Immagine(i),
                    None => continue,
                }
            } else if let Some(tb) = txbx {
                let mut blocks = Vec::new();
                self.blocks(tb, part, &mut blocks);
                let sp = frame.descendants().find(|c| c.tag_name().name() == "spPr");
                let fill = sp.and_then(|s| s.children().find(|c| c.tag_name().name() == "solidFill")).and_then(|f| f.descendants().find(|c| c.tag_name().name() == "srgbClr")).and_then(|c| attr(c, "val")).and_then(hex_color);
                let line = sp.and_then(|s| s.children().find(|c| c.tag_name().name() == "ln")).filter(|l| l.children().all(|c| c.tag_name().name() != "noFill")).and_then(|l| {
                    let col = l.descendants().find(|c| c.tag_name().name() == "srgbClr").and_then(|c| attr(c, "val")).and_then(hex_color)?;
                    Some(Linea { width: emu(attr(l, "w")).max(0.5), color: col, dash: Dash::Solid, double: false, space: 0.0 })
                });
                MobileContenuto::Testo(blocks, fill, line)
            } else {
                continue;
            };
            let pos = |tag: &str| -> Riferimento {
                let Some(p) = frame.children().find(|c| c.tag_name().name() == tag) else { return Riferimento::Spostato(Base::Colonna, 0.0) };
                let base = match attr(p, "relativeFrom") {
                    Some("page") => Base::Pagina,
                    Some("margin" | "leftMargin" | "rightMargin" | "topMargin" | "bottomMargin" | "insideMargin" | "outsideMargin") => Base::Margine,
                    Some("paragraph" | "line") => Base::Paragrafo,
                    _ => Base::Colonna,
                };
                if let Some(o) = p.children().find(|c| c.tag_name().name() == "posOffset") {
                    return Riferimento::Spostato(base, emu(o.text()));
                }
                let a = p.children().find(|c| c.tag_name().name() == "align").and_then(|a| a.text()).unwrap_or("left");
                Riferimento::Allineato(
                    base,
                    match a {
                        "center" => 1,
                        "right" | "bottom" | "outside" => 2,
                        _ => 0,
                    },
                )
            };
            let wrap = frame.children().find(|c| c.tag_name().name().starts_with("wrap")).map(|c| c.tag_name().name()).unwrap_or("wrapNone");
            let avvolge = match wrap {
                "wrapSquare" | "wrapTight" | "wrapThrough" => Avvolge::Lati,
                "wrapTopAndBottom" => Avvolge::SopraSotto,
                _ => Avvolge::Nessuno,
            };
            anchored.push(Mobile { contenuto: content, w, h, x: pos("positionH"), y: pos("positionV"), avvolge, dietro: attr(frame, "behindDoc").is_some_and(|v| v == "1") });
        }
    }

    /// Le immagini e le caselle di testo del vecchio formato (VML).
    fn vml(&mut self, d: roxmltree::Node, part: &Part, font: &Font, par: &mut Paragrafo, anchored: &mut Vec<Mobile>) {
        for shape in d.children().filter(|c| matches!(c.tag_name().name(), "shape" | "rect" | "roundrect")) {
            let style = attr(shape, "style").unwrap_or("");
            let prop = |k: &str| style.split(';').find_map(|kv| kv.split_once(':').filter(|(a, _)| a.trim() == k).map(|(_, v)| v.trim().to_string()));
            let len = |k: &str| prop(k).and_then(|v| super::ods::length(&v)).unwrap_or(0.0);
            let (w, h) = (len("width"), len("height"));
            let absolute = prop("position").is_some_and(|p| p == "absolute");
            let img = shape.children().find(|c| c.tag_name().name() == "imagedata").and_then(|i| i.attributes().find(|a| a.name() == "id").map(|a| a.value().to_string()));
            let txbx = shape.descendants().find(|c| c.tag_name().name() == "txbxContent");
            // Una linea orizzontale (o:hr): diventa il bordo sotto del paragrafo.
            if shape.attributes().any(|a| a.name() == "hr" && (a.value() == "t" || a.value() == "true")) {
                let color = attr(shape, "fillcolor").and_then(hex_color).unwrap_or((0xA0, 0xA0, 0xA0));
                par.bordi[1] = Some(Linea { width: h.max(0.75), color, dash: Dash::Solid, double: false, space: 0.0 });
                continue;
            }
            let content = if let Some(rid) = img {
                match self.image(part, &rid, w, h) {
                    Some(i) if !absolute => {
                        par.runs.push(Run { pezzo: Pezzo::Immagine { img: i, w, h }, font: font.clone(), link: None });
                        continue;
                    }
                    Some(i) => MobileContenuto::Immagine(i),
                    None => continue,
                }
            } else if let Some(tb) = txbx {
                let mut blocks = Vec::new();
                self.blocks(tb, part, &mut blocks);
                MobileContenuto::Testo(blocks, attr(shape, "fillcolor").and_then(hex_color), None)
            } else {
                continue;
            };
            let base = |k: &str| match prop(k).as_deref() {
                Some("page") => Base::Pagina,
                Some("margin") => Base::Margine,
                Some("text" | "char" | "line") => Base::Paragrafo,
                _ => Base::Colonna,
            };
            anchored.push(Mobile {
                contenuto: content,
                w,
                h,
                x: Riferimento::Spostato(base("mso-position-horizontal-relative"), len("margin-left") + len("left")),
                y: Riferimento::Spostato(base("mso-position-vertical-relative"), len("margin-top") + len("top")),
                avvolge: Avvolge::Nessuno,
                dietro: prop("z-index").is_some_and(|z| z.starts_with('-')),
            });
        }
    }

    fn table(&mut self, n: roxmltree::Node, part: &Part, _out: &mut [Blocco]) -> Option<Tabella> {
        // Tabelle annidate senza fine (file rotti o fatti apposta): oltre un limite non si scende.
        if self.table_style.len() >= 32 {
            return None;
        }
        let tpr = wchild(n, "tblPr");
        let style_id = tpr.and_then(|t| wchild(t, "tblStyle")).and_then(|s| wval(s)).map(str::to_string).or_else(|| self.styles.default_table.clone());
        // Lo stile di tabella con le sue eredità.
        let mut st_p = PProps::default();
        let mut st_r = RProps::default();
        let mut st_tc = TcProps::default();
        let mut margins = [Some(5.4), Some(5.4), Some(0.0), Some(0.0)];
        let mut parts: HashMap<String, (PProps, RProps, TcProps)> = HashMap::new();
        if let Some(id) = &style_id {
            for s in self.styles.chain(id) {
                st_p.merge(&s.p);
                st_r.merge(&s.r);
                st_tc.merge(&s.tbl);
                if let Some(m) = s.cell_margins {
                    for i in 0..4 {
                        if m[i].is_some() {
                            margins[i] = m[i];
                        }
                    }
                }
                for (k, v) in &s.parts {
                    let e = parts.entry(k.clone()).or_default();
                    e.0.merge(&v.0);
                    e.1.merge(&v.1);
                    e.2.merge(&v.2);
                }
            }
        }
        if let Some(t) = tpr {
            st_tc.merge(&TcProps::parse(t));
            if let Some(m) = wchild(t, "tblCellMar") {
                let m = cell_margins(m);
                for i in 0..4 {
                    if m[i].is_some() {
                        margins[i] = m[i];
                    }
                }
            }
        }
        // tblLook: quali parti dello stile valgono.
        let look = tpr.and_then(|t| wchild(t, "tblLook"));
        let flag = |name: &str, bit: u32, default: bool| -> bool {
            let Some(l) = look else { return default };
            if let Some(v) = attr(l, name) {
                return v == "1" || v == "true";
            }
            attr(l, "val").and_then(|v| u32::from_str_radix(v, 16).ok()).map_or(default, |v| v & bit != 0)
        };
        let first_row = flag("firstRow", 0x20, true);
        let last_row = flag("lastRow", 0x40, false);
        let first_col = flag("firstColumn", 0x80, true);
        let last_col = flag("lastColumn", 0x100, false);
        let h_band = !flag("noHBand", 0x200, false);
        let v_band = !flag("noVBand", 0x400, true);
        let allinea = match tpr.and_then(|t| wchild(t, "jc")).and_then(|j| wval(j)) {
            Some("center") => 1,
            Some("right" | "end") => 2,
            _ => 0,
        };
        let rientro = tpr.and_then(|t| wchild(t, "tblInd")).and_then(|i| twips(attr(i, "w"))).unwrap_or(0.0);
        let mut griglia: Vec<f32> = wchild(n, "tblGrid").map(|g| g.children().filter(|c| is_w(*c, "gridCol")).map(|c| twips(attr(c, "w")).unwrap_or(0.0)).collect()).unwrap_or_default();

        let rows: Vec<_> = n.children().filter(|c| is_w(*c, "tr")).collect();
        let row_count = rows.len();
        let mut righe = Vec::new();
        let mut header_rows = 0usize;
        for (ri, tr) in rows.iter().enumerate() {
            let trpr = wchild(*tr, "trPr");
            let height = trpr.and_then(|t| wchild(t, "trHeight")).map(|h| {
                let v = twips(attr(h, "val")).unwrap_or(0.0);
                if attr(h, "hRule") == Some("exact") {
                    -v
                } else {
                    v
                }
            });
            let header = trpr.and_then(|t| on(wchild(t, "tblHeader"))).unwrap_or(false);
            if header && header_rows == ri {
                header_rows += 1;
            }
            let cant = trpr.and_then(|t| on(wchild(t, "cantSplit"))).unwrap_or(false);
            let before = trpr.and_then(|t| wchild(t, "gridBefore")).and_then(|g| wval(g)).and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
            let mut celle = Vec::new();
            if before > 0 {
                celle.push(Cella { blocchi: Vec::new(), span: before, vmerge: None, sfondo: None, bordi: [None; 4], valign: 0, margini: [0.0; 4], larghezza: None });
            }
            let tcs: Vec<_> = tr.children().filter(|c| is_w(*c, "tc") || is_w(*c, "sdt")).flat_map(|c| if is_w(c, "sdt") { wchild(c, "sdtContent").map(|s| s.children().filter(|x| is_w(*x, "tc")).collect::<Vec<_>>()).unwrap_or_default() } else { vec![c] }).collect();
            let ncols = tcs.len();
            let mut col = before;
            for (ci, tc) in tcs.iter().enumerate() {
                let tcpr = wchild(*tc, "tcPr");
                let span = tcpr.and_then(|t| wchild(t, "gridSpan")).and_then(|g| wval(g)).and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).max(1);
                let vmerge = tcpr.and_then(|t| wchild(t, "vMerge")).map(|v| wval(v) == Some("restart"));
                // Le parti dello stile per questa cella, dalla meno alla più importante.
                let mut kinds: Vec<&str> = vec!["wholeTable"];
                let body_row = ri.saturating_sub(if first_row { 1 } else { 0 });
                let is_first_row = first_row && ri == 0;
                let is_last_row = last_row && ri + 1 == row_count;
                if h_band && !is_first_row && !is_last_row {
                    kinds.push(if body_row % 2 == 0 { "band1Horz" } else { "band2Horz" });
                }
                if v_band {
                    kinds.push(if ci % 2 == 0 { "band1Vert" } else { "band2Vert" });
                }
                if first_col && ci == 0 {
                    kinds.push("firstCol");
                }
                if last_col && ci + 1 == ncols {
                    kinds.push("lastCol");
                }
                if is_last_row {
                    kinds.push("lastRow");
                }
                if is_first_row {
                    kinds.push("firstRow");
                }
                let mut cp = st_p.clone();
                let mut cr = st_r.clone();
                let mut ctc = TcProps::default();
                for k in &kinds {
                    if let Some((p, r, t)) = parts.get(*k) {
                        cp.merge(p);
                        cr.merge(r);
                        ctc.merge(t);
                    }
                }
                let direct = tcpr.map(TcProps::parse).unwrap_or_default();
                // I bordi: della cella, poi della parte di stile, poi della tabella (esterni o interni).
                let edge = |i: usize, outer: bool, inner: usize| -> Option<Linea> {
                    if let Some(b) = direct.borders[i] {
                        return b;
                    }
                    if let Some(b) = ctc.borders[i] {
                        return b;
                    }
                    if outer {
                        st_tc.borders[i].flatten()
                    } else {
                        ctc.borders[inner].flatten().or(st_tc.borders[inner].flatten())
                    }
                };
                let last_c = col + span >= griglia.len().max(col + span) && ci + 1 == ncols;
                let bordi = [edge(0, ri == 0, 4), edge(1, ri + 1 == row_count, 4), edge(2, col == 0, 5), edge(3, last_c, 5)];
                let sfondo = direct.shading.or(ctc.shading).or(st_tc.shading).flatten();
                let valign = direct.valign.or(ctc.valign).or(st_tc.valign).unwrap_or(0);
                let mut cm = margins;
                if let Some(m) = tcpr.and_then(|t| wchild(t, "tcMar")).map(cell_margins) {
                    for i in 0..4 {
                        if m[i].is_some() {
                            cm[i] = m[i];
                        }
                    }
                }
                let width = tcpr.and_then(|t| wchild(t, "tcW")).filter(|w| attr(*w, "type").is_none_or(|t| t == "dxa")).and_then(|w| twips(attr(w, "w")));
                self.table_style.push((cp, cr));
                let mut blocchi = Vec::new();
                self.blocks(*tc, part, &mut blocchi);
                self.table_style.pop();
                celle.push(Cella { blocchi, span, vmerge, sfondo, bordi, valign, margini: [cm[0].unwrap_or(5.4), cm[1].unwrap_or(5.4), cm[2].unwrap_or(0.0), cm[3].unwrap_or(0.0)], larghezza: width });
                col += span;
            }
            righe.push(Riga { celle, altezza: height.unwrap_or(0.0), intestazione: header, non_dividere: cant });
        }
        // Una griglia mancante o corta: dalle larghezze delle celle.
        let cols = righe.iter().map(|r| r.celle.iter().map(|c| c.span).sum::<usize>()).max().unwrap_or(0);
        if griglia.len() < cols {
            let mut g = vec![0.0f32; cols];
            for r in &righe {
                let mut c = 0;
                for cell in &r.celle {
                    if cell.span == 1 && c < cols {
                        if let Some(w) = cell.larghezza {
                            g[c] = g[c].max(w);
                        }
                    }
                    c += cell.span;
                }
            }
            for (i, v) in g.iter_mut().enumerate() {
                if *v == 0.0 {
                    *v = griglia.get(i).copied().unwrap_or(72.0);
                }
            }
            griglia = g;
        }
        if righe.is_empty() {
            return None;
        }
        Some(Tabella { griglia, righe, allinea, rientro, relativa: None })
    }

    /// Le intestazioni o i piè di una sezione, letti dalle loro parti.
    fn header_footer(&mut self, sect: roxmltree::Node, part: &Part, tag: &str, prev: &Intestazioni) -> Intestazioni {
        let mut out = prev.clone();
        for r in sect.children().filter(|c| is_w(*c, tag)) {
            let Some(id) = attr(r, "id") else { continue };
            let Some(target) = part.rels.get(id).cloned() else { continue };
            let Some(xml) = voce(&mut self.zip, &target) else { continue };
            let Ok(doc) = roxmltree::Document::parse(&xml) else { continue };
            let hp = rels_of(&mut self.zip, &target);
            let saved = std::mem::take(&mut self.fields);
            let mut blocks = Vec::new();
            self.blocks(doc.root_element(), &hp, &mut blocks);
            self.fields = saved;
            match attr(r, "type") {
                Some("first") => out.prima = Some(blocks),
                Some("even") => out.pari = Some(blocks),
                _ => out.normale = Some(blocks),
            }
        }
        out
    }

    fn section(&mut self, sect: Option<roxmltree::Node>, part: &Part, prev: Option<&Sezione>) -> Sezione {
        let mut s = Sezione::default();
        if let Some(p) = prev {
            s.testa = p.testa.clone();
            s.piede = p.piede.clone();
        }
        let Some(sect) = sect else { return s };
        if let Some(sz) = wchild(sect, "pgSz") {
            s.w = twips(attr(sz, "w")).unwrap_or(s.w);
            s.h = twips(attr(sz, "h")).unwrap_or(s.h);
        }
        if let Some(m) = wchild(sect, "pgMar") {
            let g = |k: &str, d: f32| twips(attr(m, k)).unwrap_or(d);
            let gutter = g("gutter", 0.0);
            s.margini = [g("left", s.margini[0]) + gutter, g("right", s.margini[1]), g("top", s.margini[2]).abs(), g("bottom", s.margini[3]).abs()];
            s.dist_testa = g("header", s.dist_testa);
            s.dist_piede = g("footer", s.dist_piede);
        }
        if let Some(c) = wchild(sect, "cols") {
            s.colonne = attr(c, "num").and_then(|v| v.parse::<u8>().ok()).unwrap_or(1).clamp(1, 12);
            s.spazio_colonne = twips(attr(c, "space")).unwrap_or(36.0);
        }
        s.prima_diversa = on(wchild(sect, "titlePg")).unwrap_or(false);
        let kind = wchild(sect, "type").and_then(|t| wval(t)).unwrap_or("nextPage");
        s.pagina_nuova = kind != "continuous";
        s.parita = match kind {
            "oddPage" => 1,
            "evenPage" => 2,
            _ => 0,
        };
        s.primo_numero = wchild(sect, "pgNumType").and_then(|p| attr(p, "start")).and_then(|v| v.parse().ok());
        let prev_t = s.testa.clone();
        let prev_p = s.piede.clone();
        s.testa = self.header_footer(sect, part, "headerReference", &prev_t);
        s.piede = self.header_footer(sect, part, "footerReference", &prev_p);
        s
    }
}

fn is_page_field(instr: &str) -> bool {
    matches!(instr.split_whitespace().next().unwrap_or("").to_ascii_uppercase().as_str(), "PAGE" | "NUMPAGES" | "SECTIONPAGES")
}

/// HYPERLINK "indirizzo" o HYPERLINK \l "segnalibro"; PAGEREF segnalibro (indici).
fn field_link(instr: &str) -> Option<Target> {
    let mut words = instr.split_whitespace();
    let first = words.next()?.to_ascii_uppercase();
    let quoted = |s: &str| s.trim_matches('"').to_string();
    match first.as_str() {
        "HYPERLINK" => {
            let rest: Vec<&str> = words.collect();
            if let Some(i) = rest.iter().position(|w| *w == "\\l") {
                return rest.get(i + 1).map(|a| Target::Anchor(quoted(a)));
            }
            rest.first().filter(|w| !w.starts_with('\\')).map(|u| Target::Uri(quoted(u)))
        }
        "PAGEREF" | "REF" => words.next().map(|a| Target::Anchor(quoted(a))),
        _ => None,
    }
}

/// «Heading1», «Titolo1», «heading 2»: i livelli dei titoli dai nomi degli stili.
fn heading_level(style: &str) -> Option<u8> {
    let s = style.to_ascii_lowercase().replace(' ', "");
    for prefix in ["heading", "titolo", "berschrift", "titre", "ttulo", "título"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            if let Ok(n) = rest.parse::<u8>() {
                if (1..=9).contains(&n) {
                    return Some(n);
                }
            }
        }
    }
    None
}

fn theme_fonts(xml: Option<&str>) -> (String, String) {
    let mut out = ("Calibri Light".to_string(), "Calibri".to_string());
    let Some(doc) = xml.and_then(|x| roxmltree::Document::parse(x).ok()) else { return out };
    let face = |name: &str| doc.descendants().find(|n| n.tag_name().name() == name).and_then(|n| n.children().find(|c| c.tag_name().name() == "latin")).and_then(|l| attr(l, "typeface")).filter(|t| !t.is_empty()).map(str::to_string);
    if let Some(m) = face("majorFont") {
        out.0 = m;
    }
    if let Some(m) = face("minorFont") {
        out.1 = m;
    }
    out
}

/// Legge un .docx.
pub fn leggi_docx(bytes: &[u8]) -> Result<Documento, String> {
    if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]) {
        return Err("il documento è protetto da password".into());
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| "il file non è un documento Word leggibile".to_string())?;
    // La parte principale: dalle relazioni del pacchetto (di solito word/document.xml).
    let main = voce(&mut zip, "_rels/.rels")
        .and_then(|x| {
            let d = roxmltree::Document::parse(&x).ok()?;
            let t = d.descendants().find(|n| attr(*n, "Type").is_some_and(|t| t.ends_with("/officeDocument")))?;
            Some(attr(t, "Target")?.trim_start_matches('/').to_string())
        })
        .unwrap_or_else(|| "word/document.xml".into());
    let xml = voce(&mut zip, &main).ok_or("nel file non c'è il testo del documento")?;
    let part = rels_of(&mut zip, &main);
    let find = |kind: &str| part.rels.values().find(|t| t.to_ascii_lowercase().ends_with(kind)).cloned();
    let styles_xml = find("styles.xml").and_then(|p| voce(&mut zip, &p)).or_else(|| voce(&mut zip, "word/styles.xml"));
    let numbering_xml = find("numbering.xml").and_then(|p| voce(&mut zip, &p)).or_else(|| voce(&mut zip, "word/numbering.xml"));
    let theme_xml = find("theme1.xml").and_then(|p| voce(&mut zip, &p)).or_else(|| voce(&mut zip, "word/theme/theme1.xml"));
    let settings_xml = find("settings.xml").and_then(|p| voce(&mut zip, &p)).or_else(|| voce(&mut zip, "word/settings.xml"));
    let mut ctx = Ctx {
        zip,
        styles: Styles::parse(styles_xml.as_deref()),
        numbering: Numbering::parse(numbering_xml.as_deref()),
        theme: theme_fonts(theme_xml.as_deref()),
        doc: Documento { tab_predefinita: 36.0, ..Default::default() },
        notes_xml: [None, None],
        fields: Vec::new(),
        table_style: Vec::new(),
        image_cache: HashMap::new(),
        footnote_count: 0,
        last_frame: None,
    };
    // Senza docDefaults il carattere di Word è Times New Roman 10; con il tema, quello del tema.
    if let Some(s) = settings_xml.as_deref().and_then(|x| roxmltree::Document::parse(x).ok()) {
        let root = s.root_element();
        ctx.doc.tab_predefinita = wchild(root, "defaultTabStop").and_then(|t| twips(wval(t))).filter(|v| *v > 1.0).unwrap_or(36.0);
        ctx.doc.pari_dispari = on(wchild(root, "evenAndOddHeaders")).unwrap_or(false);
    }
    let doc = super::foglio::parse(&xml)?;
    let body = doc.root_element().children().find(|c| is_w(*c, "body")).ok_or("documento senza corpo")?;
    let mut sections: Vec<Sezione> = Vec::new();
    let mut current: Vec<Blocco> = Vec::new();
    for c in body.children().filter(|c| c.is_element()) {
        if is_w(c, "sectPr") {
            continue;
        }
        let mut tmp = Vec::new();
        let wrapper = c;
        // I blocchi normali; un paragrafo con sectPr chiude la sezione.
        if is_w(c, "p") {
            ctx.paragraph(c, &part, &mut tmp);
        } else {
            let fake_parent_children = [wrapper];
            for x in fake_parent_children {
                if is_w(x, "tbl") {
                    if let Some(t) = ctx.table(x, &part, &mut tmp) {
                        tmp.push(Blocco::Tab(t));
                    }
                } else if is_w(x, "sdt") {
                    if let Some(ct) = wchild(x, "sdtContent") {
                        ctx.blocks(ct, &part, &mut tmp);
                    }
                } else if x.tag_name().name() == "AlternateContent" || is_w(x, "customXml") || is_w(x, "ins") {
                    ctx.blocks(x, &part, &mut tmp);
                }
            }
        }
        current.append(&mut tmp);
        // Un sectPr anche dentro un sdt: chiude la sezione.
        let sect = if is_w(c, "p") { wchild(c, "pPr").and_then(|p| wchild(p, "sectPr")) } else { c.descendants().find(|d| is_w(*d, "sectPr") && d.parent().is_some_and(|p| is_w(p, "pPr"))) };
        if let Some(sect) = sect {
            let mut s = ctx.section(Some(sect), &part, sections.last());
            // Il paragrafo che porta la fine della sezione non si vede se è vuoto (Word lo mostra però).
            s.blocchi = std::mem::take(&mut current);
            sections.push(s);
        }
    }
    let last_sect = wchild(body, "sectPr");
    let mut s = ctx.section(last_sect, &part, sections.last());
    s.blocchi = current;
    sections.push(s);
    ctx.doc.sezioni = sections;
    Ok(ctx.doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_labels_and_fields() {
        assert_eq!(number_text(4, "lowerRoman"), "iv");
        assert_eq!(number_text(28, "upperLetter"), "BB");
        assert_eq!(bullet_char('\u{F0B7}'), '•');
        assert!(matches!(field_link("HYPERLINK \"https://cedro.app\""), Some(Target::Uri(u)) if u == "https://cedro.app"));
        assert!(matches!(field_link("HYPERLINK \\l \"_Toc1\""), Some(Target::Anchor(a)) if a == "_Toc1"));
        assert_eq!(heading_level("Heading2"), Some(2));
        assert_eq!(heading_level("Titolo1"), Some(1));
        assert_eq!(resolve("word", "../customXml/item1.xml"), "customXml/item1.xml");
    }
}
