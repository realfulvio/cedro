//! PDF in Word modificabile: titoli, paragrafi, tabelle vere (celle, sfondi, bordi), elenchi e
//! immagini al loro posto nel testo, ricostruiti da testo, linee e riquadri del PDF.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use hayro::hayro_interpret::font::GlyphRun;
use hayro::hayro_interpret::{
    interpret_page, BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps, InterpreterCache,
    InterpreterSettings, Paint, SoftMask, TransformExt,
};
use hayro::hayro_syntax::Pdf;
use hayro::kurbo::{BezPath, PathEl, Rect, Shape};
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{render, PixmapSettings, RenderCache, RenderSettings};

use crate::zipstore::zip_archive;

/// Un carattere del PDF, in punti con l'origine in alto a sinistra della pagina.
#[derive(Clone)]
struct Glyph {
    text: String,
    x: f64,
    /// Linea di base.
    y: f64,
    size: f64,
    adv: f64,
    color: [u8; 3],
    bold: bool,
    italic: bool,
    font: String,
    font_key: Option<u128>,
}

/// Un disegno del PDF: riquadro pieno, linea o tratto.
struct Shape2 {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    fill: Option<[u8; 3]>,
    stroke: Option<([u8; 3], f64)>,
    /// Il tracciato è un rettangolo.
    rect: bool,
    /// Il tracciato è un segmento.
    line: bool,
}

struct Pic {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

#[derive(Default)]
struct Collector {
    glyphs: Vec<Glyph>,
    shapes: Vec<Shape2>,
    pics: Vec<Pic>,
    /// Spessore medio dei tratti per font senza nome (per riconoscere il grassetto).
    stems: HashMap<u128, (f64, u32)>,
    seen: HashSet<(u128, char)>,
}

fn rgb(c: &hayro::hayro_interpret::color::Color) -> Option<[u8; 3]> {
    let [r, g, b, a] = c.to_rgba().components();
    if a < 0.05 {
        return None;
    }
    Some([(r * 255.0).round() as u8, (g * 255.0).round() as u8, (b * 255.0).round() as u8])
}

impl<'a> Device<'a> for Collector {
    fn draw_path(&mut self, p: &BezPath, props: DrawProps<'a>, mode: &DrawMode) {
        let scale = props.transform.determinant().abs().sqrt();
        let (filled, width) = match mode {
            DrawMode::Fill(_) => (true, None),
            DrawMode::Stroke(s) => (false, Some(s.line_width as f64 * scale)),
            DrawMode::FillAndStroke(_, s) => (true, Some(s.line_width as f64 * scale)),
            DrawMode::Invisible => return,
        };
        let color = match &props.paint {
            Paint::Color(c) => rgb(c),
            _ => Some([128, 128, 128]),
        };
        let q = props.transform * p.clone();
        let b = q.bounding_box();
        let near = |a: f64, b: f64| (a - b).abs() < 0.05;
        let (mut pts, mut simple) = (Vec::new(), true);
        for el in q.elements() {
            match el {
                PathEl::MoveTo(p) | PathEl::LineTo(p) => pts.push(*p),
                PathEl::ClosePath => {}
                _ => simple = false,
            }
        }
        let rect = simple
            && (4..=5).contains(&pts.len())
            && pts.iter().all(|p| (near(p.x, b.x0) || near(p.x, b.x1)) && (near(p.y, b.y0) || near(p.y, b.y1)));
        let line = simple && pts.len() == 2 && (near(b.x0, b.x1) || near(b.y0, b.y1));
        self.shapes.push(Shape2 {
            x0: b.x0,
            y0: b.y0,
            x1: b.x1,
            y1: b.y1,
            fill: if filled { color } else { None },
            stroke: width.map(|w| (color.unwrap_or([0, 0, 0]), w)),
            rect,
            line,
        });
    }
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}

    fn draw_image(&mut self, image: Image<'a, '_>, props: ImageDrawProps<'a>) {
        let r = Rect::new(0.0, 0.0, image.width() as f64, image.height() as f64);
        let b = (props.transform * r.to_path(0.1)).bounding_box();
        self.pics.push(Pic { x0: b.x0, y0: b.y0, x1: b.x1, y1: b.y1 });
    }

    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, mode: &DrawMode) {
        if matches!(mode, DrawMode::Invisible) {
            return;
        }
        let color = match &props.paint {
            Paint::Color(c) => rgb(c).unwrap_or([0, 0, 0]),
            _ => [0, 0, 0],
        };
        for glyph in run.glyphs() {
            let Some(unicode) = glyph.as_unicode() else { continue };
            let text = match unicode {
                hayro::hayro_interpret::hayro_cmap::BfString::Char(c) => c.to_string(),
                hayro::hayro_interpret::hayro_cmap::BfString::String(s) => s,
            };
            if text.chars().all(|c| c == '\0') {
                continue;
            }
            // L'outline ha 1000 unità per em: la scala della trasformazione è il corpo in punti.
            let t = props.transform * glyph.transform();
            let [a, b, c, d, e, f] = t.as_coeffs();
            if b.abs() > 0.01 * a.abs() || c.abs() > 0.01 * d.abs() {
                continue; // testo ruotato: raro, non si ricostruisce
            }
            let size = (a * d - b * c).abs().sqrt() * 1000.0;
            if size < 1.0 {
                continue;
            }
            let (mut font, mut bold, mut italic, mut adv, mut font_key) = (String::new(), false, false, 0.0, None);
            if let hayro::hayro_interpret::font::Glyph::Outline(o) = &**glyph {
                font_key = Some(o.font_cache_key());
                adv = o.advance_width().map_or(0.0, |w| w as f64) * size / 1000.0;
                if let Some(data) = o.font_data() {
                    font = data.postscript_name.unwrap_or_default();
                    bold = data.weight.is_some_and(|w| w >= 600);
                    italic = data.is_italic;
                } else if let Some(ch) = text.chars().next().filter(|c| c.is_alphabetic()) {
                    // Font Type1 senza nome: il grassetto si vede dallo spessore dei tratti.
                    let key = o.font_cache_key();
                    if self.seen.insert((key, ch)) {
                        let outline = o.outline();
                        let (area, perimeter) = (outline.area().abs(), outline.perimeter(0.5));
                        if perimeter > 0.0 {
                            let e = self.stems.entry(key).or_insert((0.0, 0));
                            e.0 += 2.0 * area / perimeter;
                            e.1 += 1;
                        }
                    }
                }
            }
            if adv <= 0.0 {
                adv = size * 0.5 * text.chars().count() as f64;
            }
            let low = font.to_ascii_lowercase();
            bold |= ["bold", "black", "heavy", "semibold", "demi"].iter().any(|k| low.contains(k));
            italic |= low.contains("italic") || low.contains("oblique");
            self.glyphs.push(Glyph { text, x: e, y: f, size, adv, color, bold, italic, font, font_key });
        }
    }
}

/// Una riga di testo.
#[derive(Clone)]
struct Line {
    x: f64,
    end: f64,
    baseline: f64,
    size: f64,
    runs: Vec<Run>,
    /// Dove comincia la seconda parola: il testo dopo il punto elenco o il numero.
    word2_x: Option<f64>,
    /// Pezzi staccati sulla stessa riga (colonne senza bordi): dove comincia ciascuno.
    tabs: Vec<f64>,
}

#[derive(Clone)]
struct Run {
    text: String,
    font: String,
    size: f64,
    color: [u8; 3],
    bold: bool,
    italic: bool,
}

impl Run {
    fn same_style(&self, o: &Run) -> bool {
        self.font == o.font && (self.size - o.size).abs() < 0.2 && self.color == o.color && self.bold == o.bold && self.italic == o.italic
    }
}

impl Line {
    fn all_bold(&self) -> bool {
        self.runs.iter().filter(|r| !r.text.trim().is_empty()).all(|r| r.bold)
    }
    fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
}

/// Le righe di base: i caratteri con la stessa linea di base, dall'alto in basso.
fn split_rows(mut glyphs: Vec<Glyph>) -> Vec<Vec<Glyph>> {
    glyphs.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
    let mut rows: Vec<Vec<Glyph>> = Vec::new();
    let mut row_y = 0.0;
    for g in glyphs {
        match rows.last_mut() {
            Some(r) if (g.y - row_y).abs() <= 0.35 * g.size => r.push(g),
            _ => {
                row_y = g.y;
                rows.push(vec![g]);
            }
        }
    }
    rows
}

/// Raggruppa i caratteri in righe, da sinistra a destra e dall'alto in basso.
fn group_lines(glyphs: Vec<Glyph>) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    for mut row in split_rows(glyphs) {
        row.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
        let mut cur: Option<Line> = None;
        let mut last_end = 0.0;
        let mut after_space = false;
        for g in row {
            let split = cur.as_ref().is_some_and(|l| g.x - last_end > 2.0 * g.size.max(l.size) || g.x < last_end - 0.6 * g.size);
            if split {
                lines.extend(cur.take());
                after_space = false;
            }
            let line = cur.get_or_insert_with(|| Line { x: g.x, end: g.x, baseline: g.y, size: g.size, runs: Vec::new(), word2_x: None, tabs: Vec::new() });
            let blank = g.text.trim().is_empty();
            let has_text = line.runs.iter().any(|r| !r.text.trim().is_empty());
            // Un vuoto largo come una parola, senza spazio nel PDF, diventa uno spazio.
            let gap = g.x - last_end > 0.2 * g.size && line.end > line.x;
            if gap && !g.text.starts_with(' ') {
                if let Some(run) = line.runs.last_mut() {
                    if !run.text.ends_with(' ') {
                        run.text.push(' ');
                    }
                }
            }
            if !blank && has_text && (gap || after_space) && line.word2_x.is_none() {
                line.word2_x = Some(g.x);
            }
            if !blank || has_text {
                after_space = blank;
            }
            let run = Run { text: g.text.clone(), font: g.font.clone(), size: g.size, color: g.color, bold: g.bold, italic: g.italic };
            match line.runs.last_mut() {
                Some(r) if r.same_style(&run) => r.text.push_str(&g.text),
                _ => line.runs.push(run),
            }
            line.size = line.size.max(g.size);
            last_end = g.x + g.adv;
            line.end = last_end;
        }
        lines.extend(cur);
    }
    for l in &mut lines {
        // Niente spazi a inizio e fine riga.
        if let Some(first) = l.runs.first_mut() {
            first.text = first.text.trim_start().to_string();
        }
        if let Some(last) = l.runs.last_mut() {
            last.text = last.text.trim_end().to_string();
        }
        l.runs.retain(|r| !r.text.is_empty());
    }
    lines.retain(|l| l.runs.iter().any(|r| !r.text.trim().is_empty()));
    lines
}

/// Il nome del carattere per Word: senza il prefisso del sottoinsieme e senza lo stile.
fn family(postscript: &str) -> String {
    let name = postscript.split('+').next_back().unwrap_or(postscript);
    let base = name.split(['-', ',']).next().unwrap_or(name);
    let low = base.to_ascii_lowercase();
    let mapped = if low.contains("times") {
        "Times New Roman"
    } else if low.contains("arial") || low.contains("helvetica") || low.is_empty() {
        "Arial"
    } else if low.contains("courier") {
        "Courier New"
    } else if low.contains("calibri") {
        "Calibri"
    } else if low.contains("verdana") {
        "Verdana"
    } else if low.contains("tahoma") {
        "Tahoma"
    } else if low.contains("georgia") {
        "Georgia"
    } else {
        // «OpenSans» diventa «Open Sans».
        let mut out = String::new();
        for (i, c) in base.chars().enumerate() {
            if i > 0 && c.is_uppercase() && out.chars().last().is_some_and(char::is_lowercase) {
                out.push(' ');
            }
            out.push(c);
        }
        return out;
    };
    mapped.to_string()
}

fn xml(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if (c as u32) < 0x20 || c == '\u{fffe}' || c == '\u{ffff}' => {}
            _ => out.push(ch),
        }
    }
    out
}

// ---------------------------------------------------------------- tabelle

/// Un segmento di griglia: `pos` è y (orizzontale) o x (verticale), da `a` a `b` lungo la linea.
struct Seg {
    pos: f64,
    a: f64,
    b: f64,
    /// Linea vera (non il bordo di un riquadro colorato).
    visible: bool,
    width: f64,
    color: [u8; 3],
}

struct Cell {
    /// Colonne occupate (indici delle colonne della griglia).
    span: usize,
    paras: Vec<Para>,
    fill: Option<[u8; 3]>,
}

struct Table {
    x0: f64,
    top: f64,
    bottom: f64,
    cols: Vec<f64>,
    rows: Vec<(f64, Vec<Cell>)>,
    border: Option<([u8; 3], f64)>,
    header: bool,
    pad: f64,
}

fn is_white(c: [u8; 3]) -> bool {
    c.iter().all(|v| *v >= 250)
}

/// Cerca le tabelle nei disegni della pagina. Restituisce anche i riquadri che le compongono.
fn find_tables(shapes: &[Shape2], page: (f64, f64), glyphs: &mut Vec<Glyph>) -> Vec<Table> {
    let mut hs: Vec<Seg> = Vec::new();
    let mut vs: Vec<Seg> = Vec::new();
    let mut fills: Vec<&Shape2> = Vec::new();
    for s in shapes {
        let (w, h) = (s.x1 - s.x0, s.y1 - s.y0);
        if w * h > 0.85 * page.0 * page.1 {
            continue;
        }
        let thin = |t: f64| t < 1.6;
        if s.line || (s.rect && s.fill.is_some() && (thin(h) || thin(w))) {
            let color = s.stroke.map(|x| x.0).or(s.fill).unwrap_or([0, 0, 0]);
            if h < w && thin(h) && w > 3.0 {
                let width = s.stroke.map_or(h, |x| x.1).max(0.1);
                hs.push(Seg { pos: (s.y0 + s.y1) / 2.0, a: s.x0, b: s.x1, visible: true, width, color });
            } else if w < h && thin(w) && h > 3.0 {
                let width = s.stroke.map_or(w, |x| x.1).max(0.1);
                vs.push(Seg { pos: (s.x0 + s.x1) / 2.0, a: s.y0, b: s.y1, visible: true, width, color });
            }
        } else if s.rect {
            if let Some((color, width)) = s.stroke {
                let color_width = (color, width.max(0.1));
                for y in [s.y0, s.y1] {
                    hs.push(Seg { pos: y, a: s.x0, b: s.x1, visible: true, width: color_width.1, color: color_width.0 });
                }
                for x in [s.x0, s.x1] {
                    vs.push(Seg { pos: x, a: s.y0, b: s.y1, visible: true, width: color_width.1, color: color_width.0 });
                }
            }
            if s.fill.is_some() && h < 150.0 && w >= 30.0 {
                fills.push(s);
                for y in [s.y0, s.y1] {
                    hs.push(Seg { pos: y, a: s.x0, b: s.x1, visible: false, width: 0.0, color: [0; 3] });
                }
                for x in [s.x0, s.x1] {
                    vs.push(Seg { pos: x, a: s.y0, b: s.y1, visible: false, width: 0.0, color: [0; 3] });
                }
            }
        }
    }
    // Segmenti orizzontali con la stessa estensione: una griglia.
    let mut groups: Vec<(f64, f64, Vec<usize>)> = Vec::new();
    for (i, s) in hs.iter().enumerate() {
        match groups.iter_mut().find(|g| (g.0 - s.a).abs() < 2.5 && (g.1 - s.b).abs() < 2.5) {
            Some(g) => g.2.push(i),
            None => groups.push((s.a, s.b, vec![i])),
        }
    }
    let mut tables = Vec::new();
    for (gx0, gx1, members) in groups {
        let mut ys: Vec<f64> = members.iter().map(|&i| hs[i].pos).collect();
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        ys.dedup_by(|b, a| (*b - *a).abs() < 1.0);
        if ys.len() < 3 {
            continue;
        }
        let inside_x = |x: f64| x >= gx0 - 1.5 && x <= gx1 + 1.5;
        let verticals: Vec<&Seg> = vs.iter().filter(|v| inside_x(v.pos)).collect();
        let covered = |y0: f64, y1: f64| {
            let mid = (y0 + y1) / 2.0;
            if verticals.is_empty() {
                return y1 - y0 < 40.0;
            }
            verticals.iter().any(|v| v.a - 1.0 <= mid && mid <= v.b + 1.0)
        };
        // Le fasce di righe consecutive coperte da linee verticali o da sfondi.
        let mut bands: Vec<Vec<f64>> = Vec::new();
        let mut cur: Vec<f64> = Vec::new();
        for w in ys.windows(2) {
            if covered(w[0], w[1]) {
                if cur.is_empty() {
                    cur.push(w[0]);
                }
                cur.push(w[1]);
            } else if !cur.is_empty() {
                bands.push(std::mem::take(&mut cur));
            }
        }
        if !cur.is_empty() {
            bands.push(cur);
        }
        // Righe brevi una sopra l'altra (le tacche di un asse) non sono una tabella.
        if gx1 - gx0 < 30.0 {
            continue;
        }
        for ys in bands.into_iter().filter(|b| b.len() >= 3) {
            let (top, bottom) = (ys[0], *ys.last().unwrap());
            let mut cols: Vec<f64> = vec![gx0, gx1];
            for v in verticals.iter().filter(|v| v.a < bottom - 1.0 && v.b > top + 1.0) {
                cols.push(v.pos);
            }
            cols.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            cols.dedup_by(|b, a| (*b - *a).abs() < 1.5);
            if cols.len() < 2 {
                continue;
            }
            tables.push(build_table(&ys, &cols, &hs, &verticals, &fills, glyphs));
        }
    }
    tables
}

fn build_table(ys: &[f64], cols: &[f64], hs: &[Seg], verticals: &[&Seg], fills: &[&Shape2], glyphs: &mut Vec<Glyph>) -> Table {
    let (x0, x1) = (cols[0], *cols.last().unwrap());
    let (top, bottom) = (ys[0], *ys.last().unwrap());
    // Testo della tabella: i caratteri la cui origine è dentro il riquadro.
    let (mut mine, rest): (Vec<Glyph>, Vec<Glyph>) =
        std::mem::take(glyphs).into_iter().partition(|g| g.x >= x0 - 1.0 && g.x <= x1 + 1.0 && g.y > top && g.y < bottom + 1.5);
    *glyphs = rest;
    let mut rows = Vec::new();
    let mut pads: Vec<f64> = Vec::new();
    for w in ys.windows(2) {
        let (ry0, ry1) = (w[0], w[1]);
        let mid = (ry0 + ry1) / 2.0;
        // Limiti di colonna aperti in questa riga: la cella si unisce alla vicina.
        let open: Vec<bool> = (1..cols.len() - 1)
            .map(|j| !verticals.iter().any(|v| (v.pos - cols[j]).abs() < 1.5 && v.a - 1.0 <= mid && mid <= v.b + 1.0))
            .collect();
        let mut bounds: Vec<(usize, usize)> = Vec::new(); // (prima colonna, numero di colonne)
        for c in 0..cols.len() - 1 {
            if c > 0 && open[c - 1] {
                bounds.last_mut().unwrap().1 += 1;
            } else {
                bounds.push((c, 1));
            }
        }
        let (in_row, rest): (Vec<Glyph>, Vec<Glyph>) = mine.into_iter().partition(|g| g.y > ry0 && g.y <= ry1 + 1.5);
        mine = rest;
        let mut cells = Vec::new();
        for (c0, span) in bounds {
            let (cx0, cx1) = (cols[c0], cols[c0 + span]);
            let mut text: Vec<Glyph> = Vec::new();
            for g in &in_row {
                if g.x >= cx0 - 1.0 && g.x < cx1 - 0.5 {
                    text.push(g.clone());
                }
            }
            let lines = group_lines(text);
            if let Some(l) = lines.first() {
                pads.push(l.x - cx0);
            }
            let fill = fills
                .iter()
                .rev()
                .find(|f| {
                    f.fill.is_some_and(|c| !is_white(c))
                        && f.x0 <= (cx0 + cx1) / 2.0
                        && f.x1 >= (cx0 + cx1) / 2.0
                        && f.y0 <= mid
                        && f.y1 >= mid
                        && f.x0 >= x0 - 1.5
                        && f.x1 <= x1 + 1.5
                })
                .and_then(|f| f.fill);
            let refs: Vec<&Line> = lines.iter().collect();
            let paras = match refs.is_empty() {
                true => Vec::new(),
                false => vec![para_from_lines(&refs, cx0)],
            };
            cells.push(Cell { span, paras, fill });
        }
        rows.push((ry1 - ry0, cells));
    }
    let visible: Vec<&Seg> = hs.iter().filter(|s| s.visible && s.pos >= top - 1.0 && s.pos <= bottom + 1.0).collect();
    let border = visible.first().map(|s| (s.color, s.width));
    let header = rows.len() > 1 && rows[0].1.iter().all(|c| c.fill.is_some()) && rows[0].1.first().map(|c| c.fill) != rows[1].1.first().map(|c| c.fill);
    pads.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let pad = pads.get(pads.len() / 2).copied().unwrap_or(4.0).clamp(1.0, 12.0);
    Table { x0, top, bottom, cols: cols.to_vec(), rows, border, header, pad }
}

// ---------------------------------------------------------------- paragrafi

struct Para {
    runs: Vec<Run>,
    x: f64,
    end: f64,
    top: f64,
    bottom: f64,
    first_x: f64,
    /// Passo tra le righe, se la riga va a capo.
    pitch: Option<f64>,
    heading: usize,
    /// Linea di base e corpo della prima riga.
    baseline: f64,
    size: f64,
    /// Punto elenco o numero davanti al testo.
    marker: Option<Marker>,
    /// Dove comincia il testo dopo il punto elenco o il numero.
    text_x: Option<f64>,
    /// Voce di un elenco di Word.
    list: Option<ListRef>,
    /// Tabulazioni: i pezzi della riga allineati come nel PDF.
    tabs: Vec<f64>,
}

#[derive(Clone, Copy, PartialEq)]
enum NumFmt {
    Decimal,
    LowerLetter,
    UpperLetter,
}

#[derive(Clone, Copy)]
enum Marker {
    Bullet(char),
    /// Numero, formato, segno dopo il numero ('.' o ')') e lunghezza del prefisso nel testo.
    Number(u32, NumFmt, char, usize),
}

/// La voce di un elenco: `num` è l'elenco di Word, `level` il rientro.
#[derive(Clone, Copy)]
struct ListRef {
    num: u32,
    level: u32,
    kind: ListKind,
}

#[derive(Clone, Copy)]
enum ListKind {
    Bullet(char),
    Number { fmt: NumFmt, delim: char, start: u32 },
}

fn is_bullet(text: &str) -> Option<usize> {
    let c = text.chars().next()?;
    let bullets = ['•', '▪', '◦', '●', '·', '■', '□', '○', '–', '-', '➢', '►', '✓', '\u{f0b7}', '\u{f0a7}', '\u{f0a8}', '\u{f076}', '\u{f0d8}', '\u{f0fc}', '\u{f06e}'];
    if bullets.contains(&c) && text.chars().nth(1).is_some_and(char::is_whitespace) {
        return Some(c.len_utf8());
    }
    None
}

/// «3. », «12) », «a) », «b. »: il numero di una voce di elenco.
fn number_prefix(text: &str) -> Option<Marker> {
    let digits: String = text.chars().take_while(char::is_ascii_digit).collect();
    let (n, fmt, rest) = if !digits.is_empty() {
        if digits.len() > 3 {
            return None;
        }
        (digits.parse::<u32>().ok()?, NumFmt::Decimal, &text[digits.len()..])
    } else {
        let c = text.chars().next()?;
        let rest = &text[c.len_utf8()..];
        match c {
            'a'..='z' => (c as u32 - 'a' as u32 + 1, NumFmt::LowerLetter, rest),
            'A'..='Z' if rest.starts_with(')') => (c as u32 - 'A' as u32 + 1, NumFmt::UpperLetter, rest),
            _ => return None,
        }
    };
    let delim = rest.chars().next().filter(|c| *c == '.' || *c == ')')?;
    let after = &rest[1..];
    if n == 0 || !after.chars().next().is_some_and(char::is_whitespace) || after.trim().is_empty() {
        return None;
    }
    let len = text.len() - after.len() + (after.len() - after.trim_start().len());
    Some(Marker::Number(n, fmt, delim, len))
}

fn para_from_lines(lines: &[&Line], _col_x: f64) -> Para {
    let mut runs: Vec<Run> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let mut next: Vec<Run> = line.runs.clone();
        if i > 0 {
            let first = next.first().and_then(|r| r.text.chars().next());
            let glued = runs.last().is_some_and(|r| {
                let mut rev = r.text.chars().rev();
                rev.next() == Some('-') && rev.next().is_some_and(char::is_alphabetic)
            }) && first.is_some_and(char::is_lowercase);
            if let Some(last) = runs.last_mut() {
                if glued {
                    last.text.pop();
                } else if !last.text.ends_with(' ') {
                    last.text.push(' ');
                }
            }
        }
        for r in next.drain(..) {
            match runs.last_mut() {
                Some(l) if l.same_style(&r) => l.text.push_str(&r.text),
                _ => runs.push(r),
            }
        }
    }
    let first = lines[0];
    let last = lines[lines.len() - 1];
    let pitch = (lines.len() > 1).then(|| (last.baseline - first.baseline) / (lines.len() - 1) as f64);
    Para {
        runs,
        x: lines.iter().map(|l| l.x).fold(f64::MAX, f64::min),
        first_x: first.x,
        end: lines.iter().map(|l| l.end).fold(0.0, f64::max),
        top: first.baseline - first.size * 0.9,
        bottom: last.baseline + last.size * 0.22,
        pitch,
        heading: 0,
        baseline: first.baseline,
        size: first.size,
        marker: None,
        text_x: first.word2_x,
        list: None,
        tabs: if lines.len() == 1 { first.tabs.clone() } else { Vec::new() },
    }
}

/// Toglie i primi `len` byte del testo (il punto elenco o il numero) e gli spazi dopo.
fn strip_front(runs: &mut Vec<Run>, mut len: usize) {
    while len > 0 && !runs.is_empty() {
        if runs[0].text.len() <= len {
            len -= runs[0].text.len();
            runs.remove(0);
        } else {
            let rest = runs[0].text.get(len..).unwrap_or_default().to_string();
            runs[0].text = rest;
            len = 0;
        }
    }
    while let Some(first) = runs.first_mut() {
        first.text = first.text.trim_start().to_string();
        if !first.text.is_empty() {
            break;
        }
        runs.remove(0);
    }
}

/// Le dimensioni del corpo e dei titoli, uguali in tutto il documento.
struct TextSizes {
    body: f64,
    /// Corpi dei titoli, dal più grande: titolo 1, 2, 3.
    headings: Vec<f64>,
}

impl TextSizes {
    fn is_heading(&self, l: &Line) -> bool {
        (l.size >= self.body * 1.15 && l.all_bold()) || l.size >= self.body * 1.6
    }
    fn rank(&self, l: &Line) -> usize {
        let key = (l.size * 2.0).round() / 2.0;
        self.headings.iter().position(|s| (*s - key).abs() < 0.01).map_or(1, |p| (p + 1).min(3))
    }
}

/// I pezzi staccati sulla stessa linea di base (una tabella senza bordi, «Nome    Data»)
/// diventano una riga sola con le tabulazioni, invece di righe una sotto l'altra.
fn join_tabbed(lines: &[Line]) -> Vec<Line> {
    let mut out: Vec<Line> = Vec::new();
    for l in lines {
        match out.last_mut() {
            Some(prev) if (l.baseline - prev.baseline).abs() <= 0.35 * l.size.max(prev.size) && l.x > prev.end => {
                let text = prev.text();
                // Dopo un punto elenco o un numero basta uno spazio.
                let marker = is_bullet(&format!("{text} x")).is_some() || number_prefix(&format!("{text} x")).is_some_and(|_| !text.contains(' '));
                let sep = if marker { " " } else { "\t" };
                if let Some(last) = prev.runs.last_mut() {
                    last.text.push_str(sep);
                }
                if marker {
                    prev.word2_x = prev.word2_x.or(Some(l.x));
                } else {
                    prev.tabs.push(l.x);
                }
                prev.runs.extend(l.runs.iter().cloned());
                prev.end = l.end;
                prev.size = prev.size.max(l.size);
            }
            _ => out.push(l.clone()),
        }
    }
    out
}

/// Mette in paragrafi le righe di testo di una colonna, larga da `left` a `right`.
fn paragraphs(lines: &[Line], left: f64, right: f64, sizes: &TextSizes) -> Vec<Para> {
    let lines = join_tabbed(lines);
    let lines = lines.as_slice();
    let width = (right - left).max(1.0);
    let starts_item = |l: &Line| {
        let text = l.text();
        is_bullet(&text).is_some() || number_prefix(&text).is_some()
    };
    let mut out: Vec<Para> = Vec::new();
    let mut group: Vec<&Line> = Vec::new();
    let flush = |group: &mut Vec<&Line>, out: &mut Vec<Para>| {
        if group.is_empty() {
            return;
        }
        let mut p = para_from_lines(group, left);
        if sizes.is_heading(group[0]) {
            p.heading = sizes.rank(group[0]);
        }
        // Elenco puntato: via il simbolo, lo rimette Word.
        let text: String = p.runs.iter().map(|r| r.text.as_str()).collect();
        if let Some(n) = is_bullet(&text) {
            p.marker = text.chars().next().map(Marker::Bullet);
            strip_front(&mut p.runs, n);
        } else {
            p.marker = number_prefix(&text);
        }
        out.push(p);
        group.clear();
    };
    for line in lines {
        let continues = group.last().is_some_and(|prev| {
            let dy = line.baseline - prev.baseline;
            let short_prev = prev.end < right - 0.15 * width;
            // Le righe dopo la prima di una voce d'elenco cominciano sotto il testo, non sotto il punto.
            let under_item_text = starts_item(group[0]) && group[0].word2_x.is_some_and(|x| (line.x - x).abs() <= 0.8 * line.size);
            let same_x = (line.x - prev.x).abs() <= 0.8 * line.size || (group.len() == 1 && line.x < prev.x) || under_item_text;
            dy > 0.6 * line.size
                && dy < 1.7 * line.size.max(prev.size)
                && (line.size - prev.size).abs() < 0.6
                && sizes.is_heading(line) == sizes.is_heading(prev)
                && !short_prev
                && same_x
                && !starts_item(line)
                && line.tabs.is_empty()
                && prev.tabs.is_empty()
        });
        if !continues {
            flush(&mut group, &mut out);
        }
        group.push(line);
    }
    flush(&mut group, &mut out);
    out
}

// ---------------------------------------------------------------- figure

struct Figure {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

/// Grafici e immagini: gruppi di disegni vicini, o immagini incorporate.
fn find_figures(shapes: &[Shape2], pics: &[Pic], tables: &[Table], page: (f64, f64)) -> Vec<Figure> {
    struct Item {
        b: [f64; 4],
        pic: bool,
    }
    let mut items: Vec<Item> = Vec::new();
    for s in shapes {
        if s.fill.is_some_and(is_white) && s.stroke.is_none() {
            continue;
        }
        let (w, h) = (s.x1 - s.x0, s.y1 - s.y0);
        if w * h > 0.85 * page.0 * page.1 {
            continue;
        }
        if tables.iter().any(|t| s.x0 >= t.x0 - 2.0 && s.x1 <= t.cols.last().unwrap() + 2.0 && s.y0 >= t.top - 2.0 && s.y1 <= t.bottom + 2.0) {
            continue;
        }
        items.push(Item { b: [s.x0, s.y0, s.x1, s.y1], pic: false });
    }
    for p in pics {
        if p.x1 - p.x0 >= 20.0 && p.y1 - p.y0 >= 20.0 {
            items.push(Item { b: [p.x0, p.y0, p.x1, p.y1], pic: true });
        }
    }
    // Unisce i riquadri che si toccano.
    let mut clusters: Vec<(f64, f64, f64, f64, usize, bool)> = Vec::new();
    for it in &items {
        let mut cur = (it.b[0], it.b[1], it.b[2], it.b[3], 1usize, it.pic);
        loop {
            let hit = clusters.iter().position(|c| c.0 - 3.0 <= cur.2 && cur.0 - 3.0 <= c.2 && c.1 - 3.0 <= cur.3 && cur.1 - 3.0 <= c.3);
            match hit {
                Some(i) => {
                    let c = clusters.swap_remove(i);
                    cur = (cur.0.min(c.0), cur.1.min(c.1), cur.2.max(c.2), cur.3.max(c.3), cur.4 + c.4, cur.5 || c.5);
                }
                None => break,
            }
        }
        clusters.push(cur);
    }
    let mut figs: Vec<Figure> = clusters
        .into_iter()
        .filter(|c| c.5 || (c.4 >= 5 && c.2 - c.0 >= 40.0 && c.3 - c.1 >= 40.0))
        .map(|c| Figure { x0: (c.0 - 2.0).max(0.0), y0: (c.1 - 2.0).max(0.0), x1: (c.2 + 2.0).min(page.0), y1: (c.3 + 2.0).min(page.1) })
        .collect();
    figs.sort_by(|a, b| a.y0.partial_cmp(&b.y0).unwrap_or(std::cmp::Ordering::Equal));
    figs
}

/// Le scritte attaccate a un grafico (valori degli assi, etichette) fanno parte del grafico.
fn absorb_labels(figs: &mut [Figure], glyphs: &[Glyph], body: f64) {
    if figs.is_empty() {
        return;
    }
    let lines = group_lines(glyphs.to_vec());
    let mut taken = vec![false; lines.len()];
    loop {
        let mut changed = false;
        for (i, l) in lines.iter().enumerate() {
            let text = l.text();
            // Le frasi e i titoli restano testo.
            if taken[i] || text.chars().count() > 30 || (l.all_bold() && l.size >= body * 1.15) {
                continue;
            }
            let (x0, x1, y0, y1) = (l.x, l.end, l.baseline - l.size * 0.8, l.baseline + l.size * 0.25);
            let d = l.size + 2.0;
            for f in figs.iter_mut() {
                let above_below = x0 >= f.x0 - 2.0 * d && x1 <= f.x1 + 2.0 * d && y1 >= f.y0 - d && y0 <= f.y1 + d;
                let beside = y0 >= f.y0 - d && y1 <= f.y1 + d && x1 >= f.x0 - d && x0 <= f.x1 + d;
                if above_below || beside {
                    f.x0 = f.x0.min(x0 - 2.0).max(0.0);
                    f.x1 = f.x1.max(x1 + 2.0);
                    f.y0 = f.y0.min(y0 - 2.0).max(0.0);
                    f.y1 = f.y1.max(y1 + 2.0);
                    taken[i] = true;
                    changed = true;
                    break;
                }
            }
        }
        if !changed {
            break;
        }
    }
}

/// Il ritaglio di un grafico: PNG con la tavolozza se i colori sono pochi (grafici, schemi),
/// JPEG per le foto. Restituisce i byte e l'estensione.
fn encode_figure(img: &image::RgbImage) -> Result<(Vec<u8>, &'static str), String> {
    let mut counts: HashMap<[u8; 3], u32> = HashMap::new();
    for p in img.pixels() {
        *counts.entry(p.0).or_default() += 1;
        if counts.len() > 65536 {
            break;
        }
    }
    let total = (img.width() * img.height()).max(1) as f64;
    let mut by_count: Vec<([u8; 3], u32)> = counts.into_iter().collect();
    by_count.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let palette: Vec<[u8; 3]> = by_count.iter().take(256).map(|c| c.0).collect();
    let covered: u32 = by_count.iter().take(256).map(|c| c.1).sum();
    if by_count.len() > 65536 || (covered as f64) < 0.9 * total {
        // Una foto: il JPEG pesa molto meno.
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85).encode_image(img).map_err(|e| e.to_string())?;
        return Ok((out, "jpg"));
    }
    // Ogni colore va al più vicino della tavolozza: i bordi sfumati restano puliti.
    let mut index: HashMap<[u8; 3], u8> = palette.iter().enumerate().map(|(i, c)| (*c, i as u8)).collect();
    let mut data = Vec::with_capacity((img.width() * img.height()) as usize);
    for p in img.pixels() {
        let i = *index.entry(p.0).or_insert_with(|| {
            let dist = |c: &[u8; 3]| (0..3).map(|k| (c[k] as i32 - p.0[k] as i32).pow(2)).sum::<i32>();
            palette.iter().enumerate().min_by_key(|(_, c)| dist(c)).map_or(0, |(i, _)| i as u8)
        });
        data.push(i);
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, img.width(), img.height());
        enc.set_color(png::ColorType::Indexed);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_palette(palette.iter().flatten().copied().collect::<Vec<u8>>());
        enc.set_compression(png::Compression::High);
        let mut w = enc.write_header().map_err(|e| e.to_string())?;
        w.write_image_data(&data).map_err(|e| e.to_string())?;
    }
    Ok((out, "png"))
}

// ---------------------------------------------------------------- colonne

/// Una fascia della pagina: testo su una colonna o su più colonne affiancate.
struct Region {
    top: f64,
    bottom: f64,
    /// Più di una colonna: da dove a dove va ciascuna.
    cols: Vec<(f64, f64)>,
    glyphs: Vec<Vec<Glyph>>,
}

/// I tratti occupati da testo in una riga: le parole vicine si uniscono.
fn row_spans(row: &[Glyph]) -> Vec<(f64, f64)> {
    let mut gs: Vec<&Glyph> = row.iter().filter(|g| !g.text.trim().is_empty()).collect();
    gs.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<(f64, f64)> = Vec::new();
    for g in gs {
        let (a, b) = (g.x, g.x + g.adv.max(0.0));
        match out.last_mut() {
            Some(l) if a - l.1 <= 0.8 * g.size => l.1 = l.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    v[v.len() / 2]
}

/// Divide il testo della pagina in fasce: a una colonna, o a più colonne come in un giornale.
fn regions(glyphs: Vec<Glyph>, carry: Option<&[(f64, f64)]>) -> Vec<Region> {
    let rows = split_rows(glyphs);
    let spans: Vec<Vec<(f64, f64)>> = rows.iter().map(|r| row_spans(r)).collect();
    let left = spans.iter().flatten().map(|s| s.0).fold(f64::MAX, f64::min);
    let right = spans.iter().flatten().map(|s| s.1).fold(0.0, f64::max);
    let single = |rows: Vec<Vec<Glyph>>| -> Region {
        let top = rows.first().and_then(|r| r.first()).map_or(0.0, |g| g.y - g.size);
        let bottom = rows.last().and_then(|r| r.first()).map_or(0.0, |g| g.y + g.size * 0.25);
        Region { top, bottom, cols: Vec::new(), glyphs: vec![rows.into_iter().flatten().collect()] }
    };
    let width = right - left;
    let heights: Vec<(f64, f64)> = rows.iter().map(|r| (r[0].y, r.iter().map(|g| g.size).fold(0.0, f64::max))).collect();
    let mut bands = if width > 150.0 && rows.len() >= 6 { column_bands(&spans, &heights, left, right) } else { Vec::new() };
    // Le colonne della pagina prima che continuano qui, anche se a destra non c'è più testo.
    if let Some(cols) = carry.filter(|c| c.len() > 1) {
        let fits = |row: &Vec<(f64, f64)>| row.iter().all(|s| cols.iter().any(|c| s.0 >= c.0 - 3.0 && s.1 <= c.1 + 3.0));
        let first_band = bands.first().map_or(rows.len(), |b| b.0);
        let n = spans[..first_band].iter().take_while(|r| fits(r)).count();
        if n >= 3 {
            bands.insert(0, (0, n - 1, cols.to_vec()));
        }
    }
    if bands.is_empty() {
        return vec![single(rows)];
    }
    let mut out = Vec::new();
    let mut rest: Vec<Vec<Glyph>> = Vec::new();
    let mut band_of = vec![None; rows.len()];
    for (b, (r0, r1, _)) in bands.iter().enumerate() {
        for slot in band_of.iter_mut().take(*r1 + 1).skip(*r0) {
            *slot = Some(b);
        }
    }
    let mut current: Option<(usize, Vec<Vec<Glyph>>)> = None;
    for (i, row) in rows.into_iter().enumerate() {
        match band_of[i] {
            None => {
                if let Some((b, cols)) = current.take() {
                    out.push(band_region(&bands[b].2, cols));
                }
                rest.push(row);
            }
            Some(b) => {
                if !rest.is_empty() {
                    out.push(single(std::mem::take(&mut rest)));
                }
                let entry = current.get_or_insert_with(|| (b, vec![Vec::new(); bands[b].2.len()]));
                for g in row {
                    let mid = g.x + g.adv / 2.0;
                    let col = bands[b].2.iter().position(|c| mid < c.1 + 1.0).unwrap_or(bands[b].2.len() - 1);
                    entry.1[col].push(g);
                }
            }
        }
    }
    if let Some((b, cols)) = current.take() {
        out.push(band_region(&bands[b].2, cols));
    }
    if !rest.is_empty() {
        out.push(single(rest));
    }
    out
}

fn band_region(cols: &[(f64, f64)], glyphs: Vec<Vec<Glyph>>) -> Region {
    let all = glyphs.iter().flatten();
    let top = all.clone().map(|g| g.y - g.size).fold(f64::MAX, f64::min);
    let bottom = all.map(|g| g.y + g.size * 0.25).fold(0.0, f64::max);
    Region { top, bottom, cols: cols.to_vec(), glyphs }
}

/// Cerca le fasce a più colonne: righe consecutive che lasciano libero lo stesso corridoio
/// verticale, con testo pieno ai due lati. Restituisce (prima riga, ultima riga, colonne).
fn column_bands(spans: &[Vec<(f64, f64)>], heights: &[(f64, f64)], left: f64, right: f64) -> Vec<(usize, usize, Vec<(f64, f64)>)> {
    let width = right - left;
    let n = width.ceil() as usize + 1;
    let mut cover = vec![0usize; n];
    for row in spans {
        for (a, b) in row {
            let (i0, i1) = (((a - left).max(0.0)) as usize, ((b - left).ceil() as usize).min(n - 1));
            for c in cover.iter_mut().take(i1 + 1).skip(i0) {
                *c += 1;
            }
        }
    }
    // Corridoi candidati nella parte centrale, dai più vuoti.
    let limit = (spans.len() as f64 * 0.3) as usize;
    let (lo, hi) = ((0.15 * width) as usize, (0.85 * width) as usize);
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut i = lo;
    while i < hi {
        if cover[i] <= limit {
            let start = i;
            while i < hi && cover[i] <= limit {
                i += 1;
            }
            if i - start >= 6 {
                runs.push((start, i));
            }
        }
        i += 1;
    }
    runs.sort_by_key(|&(a, b)| (cover[a..b].iter().min().copied().unwrap_or(0), usize::MAX - (b - a)));
    for (a, b) in runs {
        let g = left + (a + b) as f64 / 2.0;
        let crosses = |row: &Vec<(f64, f64)>| row.iter().any(|s| s.0 < g - 1.0 && s.1 > g + 1.0);
        let mut bands = Vec::new();
        let mut r = 0;
        while r < spans.len() {
            if crosses(&spans[r]) || spans[r].is_empty() {
                r += 1;
                continue;
            }
            let start = r;
            while r < spans.len() && !crosses(&spans[r]) {
                r += 1;
            }
            // Le righe corte prima che cominci la colonna di destra (un elenco, la fine di un
            // paragrafo) non fanno parte delle colonne.
            let mut first = start;
            if let Some(right_row) = (start..r).find(|&i| spans[i].iter().any(|s| s.0 >= g)) {
                let (y, size) = heights[right_row];
                while first < right_row && heights[first].0 < y - 2.0 * size {
                    first += 1;
                }
            }
            if let Some(cols) = band_columns(&spans[first..r], g) {
                bands.push((first, r - 1, cols));
            }
        }
        if !bands.is_empty() {
            return bands;
        }
    }
    Vec::new()
}

/// Le colonne di una fascia, se il testo sembra davvero impaginato in colonne.
fn band_columns(rows: &[Vec<(f64, f64)>], g: f64) -> Option<Vec<(f64, f64)>> {
    let mut union: Vec<(f64, f64)> = rows.iter().flatten().copied().collect();
    union.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut cols: Vec<(f64, f64)> = Vec::new();
    for (a, b) in union {
        match cols.last_mut() {
            Some(c) if a - c.1 < 6.0 => c.1 = c.1.max(b),
            _ => cols.push((a, b)),
        }
    }
    if cols.len() < 2 || !cols.iter().any(|c| c.1 <= g) || !cols.iter().any(|c| c.0 >= g) {
        return None;
    }
    // Ogni colonna ha almeno tre righe, e le righe la riempiono quasi tutta (non è un modulo).
    for c in &cols {
        let widths: Vec<f64> = rows
            .iter()
            .filter_map(|row| {
                let mine: Vec<&(f64, f64)> = row.iter().filter(|s| s.0 >= c.0 - 0.5 && s.1 <= c.1 + 0.5).collect();
                let a = mine.iter().map(|s| s.0).fold(f64::MAX, f64::min);
                let b = mine.iter().map(|s| s.1).fold(0.0, f64::max);
                (!mine.is_empty()).then_some(b - a)
            })
            .collect();
        if widths.len() < 3 || c.1 - c.0 < 60.0 || median(widths) < 0.6 * (c.1 - c.0) {
            return None;
        }
    }
    Some(cols)
}

// ---------------------------------------------------------------- pagine

enum Block {
    Para(Para),
    Table(Table),
    Image { fig: Figure, data: Vec<u8>, ext: &'static str },
}

impl Block {
    fn top(&self) -> f64 {
        match self {
            Block::Para(p) => p.top,
            Block::Table(t) => t.top,
            Block::Image { fig, .. } => fig.y0,
        }
    }
    fn bottom(&self) -> f64 {
        match self {
            Block::Para(p) => p.bottom,
            Block::Table(t) => t.bottom,
            Block::Image { fig, .. } => fig.y1,
        }
    }
    fn span(&self) -> (f64, f64) {
        match self {
            Block::Para(q) => (q.x, q.end),
            Block::Table(t) => (t.x0, *t.cols.last().unwrap()),
            Block::Image { fig, .. } => (fig.x0, fig.x1),
        }
    }
}

/// Un pezzo di pagina: una colonna sola (`cols` vuoto) o più colonne affiancate.
struct Segment {
    cols: Vec<(f64, f64)>,
    columns: Vec<Vec<Block>>,
}

/// Una pagina del Word.
pub enum Page {
    /// Pagina ricostruita: titoli, paragrafi, tabelle, immagini.
    Flow(FlowPage),
    /// Pagina senza testo da copiare: non c'è niente da ricostruire.
    Empty,
    /// Pagina finita dentro la precedente (una tabella che continua).
    Merged,
}

pub struct FlowPage {
    width: f64,
    height: f64,
    segments: Vec<Segment>,
}

impl FlowPage {
    fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.segments.iter().flat_map(|s| s.columns.iter().flatten())
    }
}

/// Quel che si legge da una pagina prima di sapere com'è fatto il resto del documento.
struct RawPage {
    width: f64,
    height: f64,
    regions: Vec<(Region, Vec<Vec<Line>>)>,
    others: Vec<Block>,
}

/// Legge le pagine del PDF. `Page::Empty` per quelle senza testo incorporato.
pub fn read_pages(bytes: &[u8]) -> Result<Vec<Page>, String> {
    let original = Pdf::new(bytes.to_vec()).map_err(|e| format!("PDF non letto: {e:?}"))?;
    let cache = RenderCache::new();
    let interp_cache = InterpreterCache::new();
    let settings = InterpreterSettings::default();
    let mut raw: Vec<Option<RawPage>> = Vec::new();
    let mut size_chars: HashMap<i64, usize> = HashMap::new();
    for page in original.pages().iter() {
        let (w, h) = page.render_dimensions();
        let (w, h) = (w as f64, h as f64);
        let ctm = page.initial_transform(true).to_kurbo();
        let mut ctx = Context::new(ctm, Rect::new(0.0, 0.0, w, h), &interp_cache, page.xref(), settings.clone());
        let mut collector = Collector::default();
        interpret_page(page, &mut ctx, &mut collector);
        let Collector { mut glyphs, shapes, pics, stems, .. } = collector;
        if glyphs.is_empty() {
            raw.push(None);
            continue;
        }
        for g in &mut glyphs {
            if let (Some(key), true) = (g.font_key, g.font.is_empty()) {
                if let Some((sum, n)) = stems.get(&key) {
                    g.bold = *n > 0 && sum / *n as f64 >= 96.0;
                }
            }
        }
        let tables = find_tables(&shapes, (w, h), &mut glyphs);
        let mut figures = find_figures(&shapes, &pics, &tables, (w, h));
        let page_body = median(glyphs.iter().map(|g| g.size).collect());
        absorb_labels(&mut figures, &glyphs, page_body);
        // Il testo dentro una figura fa parte dell'immagine.
        glyphs.retain(|g| !figures.iter().any(|f| g.x >= f.x0 && g.x <= f.x1 && g.y >= f.y0 && g.y <= f.y1));
        for g in &glyphs {
            *size_chars.entry((g.size * 4.0).round() as i64).or_default() += g.text.chars().count();
        }
        let mut others: Vec<Block> = tables.into_iter().map(Block::Table).collect();
        if !figures.is_empty() {
            let scale = (200.0 / 72.0f32).min(3200.0 / (w.max(h).max(1.0) as f32));
            let pixmap = PixmapSettings { x_scale: scale, y_scale: scale, bg_color: WHITE };
            let rendered = render(page, &cache, &settings, &RenderSettings::default(), &pixmap);
            let png = rendered.into_png().map_err(|e| format!("pagina non disegnata: {e}"))?;
            let full = image::load_from_memory(&png).map_err(|e| e.to_string())?.to_rgb8();
            for fig in figures {
                let s = scale as f64;
                let (x, y) = ((fig.x0 * s) as u32, (fig.y0 * s) as u32);
                let cw = (((fig.x1 - fig.x0) * s) as u32).min(full.width().saturating_sub(x)).max(1);
                let ch = (((fig.y1 - fig.y0) * s) as u32).min(full.height().saturating_sub(y)).max(1);
                let crop = image::imageops::crop_imm(&full, x.min(full.width() - 1), y.min(full.height() - 1), cw, ch).to_image();
                let (data, ext) = encode_figure(&crop)?;
                others.push(Block::Image { fig, data, ext });
            }
        }
        let carry = raw.last().and_then(|p| p.as_ref()).and_then(|p| p.regions.last()).map(|r| r.0.cols.clone());
        let regions = regions(glyphs, carry.as_deref())
            .into_iter()
            .map(|r| {
                let lines = r.glyphs.iter().map(|g| group_lines(g.clone())).collect();
                (r, lines)
            })
            .collect();
        raw.push(Some(RawPage { width: w, height: h, regions, others }));
    }
    // Corpo e titoli si decidono sul documento intero: una pagina di sole tabelle non ha corpo.
    let body = size_chars.into_iter().max_by_key(|(s, n)| (*n, -*s)).map_or(11.0, |(s, _)| s as f64 / 4.0);
    let mut sizes = TextSizes { body, headings: Vec::new() };
    let mut heads: Vec<f64> = Vec::new();
    for p in raw.iter().flatten() {
        for (_, cols) in &p.regions {
            for l in cols.iter().flatten() {
                if sizes.is_heading(l) {
                    heads.push((l.size * 2.0).round() / 2.0);
                }
            }
        }
    }
    heads.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    heads.dedup();
    sizes.headings = heads;

    let mut pages = Vec::new();
    for p in raw {
        let Some(p) = p else {
            pages.push(Page::Empty);
            continue;
        };
        pages.push(Page::Flow(layout_page(p, &sizes)));
    }
    merge_split_tables(&mut pages);
    number_lists(&mut pages);
    Ok(pages)
}

/// Mette in ordine di lettura paragrafi, tabelle e immagini di una pagina.
fn layout_page(p: RawPage, sizes: &TextSizes) -> FlowPage {
    // Larghezza del testo a una colonna: serve per capire quando una riga va a capo.
    let mut left = f64::MAX;
    let mut right = 0.0f64;
    for (_, cols) in &p.regions {
        for l in cols.iter().flatten() {
            left = left.min(l.x);
            right = right.max(l.end);
        }
    }
    let mut singles: Vec<Block> = Vec::new();
    let mut bands: Vec<(f64, f64, Vec<(f64, f64)>, Vec<Vec<Block>>)> = Vec::new();
    for (region, cols) in p.regions {
        if region.cols.is_empty() {
            for l in cols {
                singles.extend(paragraphs(&l, left, right, sizes).into_iter().map(Block::Para));
            }
        } else {
            let columns =
                cols.iter().zip(&region.cols).map(|(l, c)| paragraphs(l, c.0, c.1, sizes).into_iter().map(Block::Para).collect()).collect();
            bands.push((region.top, region.bottom, region.cols, columns));
        }
    }
    // Tabelle e immagini: nella colonna che le contiene, se stanno in una colonna.
    for b in p.others {
        let (x0, x1) = b.span();
        let mid = (b.top() + b.bottom()) / 2.0;
        let home = bands.iter_mut().find_map(|band| {
            if mid < band.0 - 5.0 || mid > band.1 + 5.0 {
                return None;
            }
            let i = band.2.iter().position(|c| x0 >= c.0 - 8.0 && x1 <= c.1 + 8.0)?;
            Some(&mut band.3[i])
        });
        match home {
            Some(col) => col.push(b),
            None => singles.push(b),
        }
    }
    let by_top = |a: &Block, b: &Block| a.top().partial_cmp(&b.top()).unwrap_or(std::cmp::Ordering::Equal);
    singles.sort_by(by_top);
    bands.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut segments: Vec<Segment> = Vec::new();
    let mut singles = singles.into_iter().peekable();
    for (top, _, cols, mut columns) in bands {
        let before: Vec<Block> = std::iter::from_fn(|| singles.next_if(|b| b.top() < top)).collect();
        if !before.is_empty() {
            segments.push(Segment { cols: Vec::new(), columns: vec![before] });
        }
        for c in &mut columns {
            c.sort_by(by_top);
        }
        segments.push(Segment { cols, columns });
    }
    let after: Vec<Block> = singles.collect();
    if !after.is_empty() || segments.is_empty() {
        segments.push(Segment { cols: Vec::new(), columns: vec![after] });
    }
    FlowPage { width: p.width, height: p.height, segments }
}

/// Una tabella che continua sulla pagina dopo diventa una tabella sola, con la riga
/// d'intestazione ripetuta da Word invece che copiata. La pagina svuotata resta al suo
/// posto come `Page::Merged`, così gli indici restano quelli del PDF (per l'OCR).
fn merge_split_tables(pages: &mut [Page]) {
    let mut i = 0;
    let mut j = 1;
    while j < pages.len() {
        let joinable = match (&pages[i], &pages[j]) {
            (Page::Flow(a), Page::Flow(b)) => {
                let last = a.segments.last().filter(|s| s.cols.is_empty()).and_then(|s| s.columns[0].last());
                let first = b.segments.first().filter(|s| s.cols.is_empty()).and_then(|s| s.columns[0].first());
                match (last, first) {
                    (Some(Block::Table(t)), Some(Block::Table(u))) => {
                        (a.width - b.width).abs() < 1.0
                            && (a.height - b.height).abs() < 1.0
                            && t.cols.len() == u.cols.len()
                            && t.cols.iter().zip(&u.cols).all(|(x, y)| (x - y).abs() < 2.0)
                            && t.bottom > 0.6 * a.height
                            && u.top < 0.4 * b.height
                    }
                    _ => false,
                }
            }
            _ => false,
        };
        if !joinable {
            i = j;
            j += 1;
            continue;
        }
        let Page::Flow(mut next) = std::mem::replace(&mut pages[j], Page::Merged) else { unreachable!() };
        j += 1;
        let Page::Flow(cur) = &mut pages[i] else { unreachable!() };
        let Some(Block::Table(mut u)) = Some(next.segments[0].columns[0].remove(0)) else { unreachable!() };
        let Some(Block::Table(t)) = cur.segments.last_mut().and_then(|s| s.columns[0].last_mut()) else { unreachable!() };
        let text = |cells: &[Cell]| -> String { cells.iter().flat_map(|c| &c.paras).flat_map(|p| &p.runs).map(|r| r.text.as_str()).collect() };
        if t.header && !u.rows.is_empty() && text(&t.rows[0].1) == text(&u.rows[0].1) {
            u.rows.remove(0);
        }
        t.rows.append(&mut u.rows);
        t.bottom = u.bottom;
        if next.segments[0].columns[0].is_empty() {
            next.segments.remove(0);
        }
        cur.segments.append(&mut next.segments);
    }
}

/// Le voci con punto elenco o numero diventano elenchi di Word. I numeri diventano un
/// elenco solo se si susseguono (1, 2, 3…): un «3.» da solo resta testo.
fn number_lists(pages: &mut [Page]) {
    let mut paras: Vec<(&mut Para, f64)> = Vec::new();
    for page in pages.iter_mut() {
        if let Page::Flow(p) = page {
            for seg in &mut p.segments {
                for (ci, col) in seg.columns.iter_mut().enumerate() {
                    let col_left = seg.cols.get(ci).map_or(0.0, |c| c.0);
                    for b in col {
                        if let Block::Para(q) = b {
                            paras.push((q, col_left));
                        }
                    }
                }
            }
        }
    }
    // Punti elenco: il livello viene dal rientro.
    let mut indents: Vec<f64> = paras.iter().filter(|(p, _)| matches!(p.marker, Some(Marker::Bullet(_)))).map(|(p, l)| p.first_x - l).collect();
    indents.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    indents.dedup_by(|b, a| (*b - *a).abs() < 4.0);
    for (p, l) in paras.iter_mut() {
        if let Some(Marker::Bullet(c)) = p.marker {
            let level = indents.iter().position(|x| (p.first_x - *l - x).abs() < 4.0).unwrap_or(0).min(8) as u32;
            p.list = Some(ListRef { num: 1, level, kind: ListKind::Bullet(c) });
        }
    }
    // Numeri: un elenco per titolo/corpo, formato e rientro.
    let mut open: Vec<((usize, u8, char, i64), u32, Vec<usize>)> = Vec::new();
    let mut lists: Vec<Vec<usize>> = Vec::new();
    for (i, (p, l)) in paras.iter().enumerate() {
        let Some(Marker::Number(n, fmt, delim, _)) = p.marker else { continue };
        let key = (p.heading, fmt as u8, delim, ((p.first_x - l) / 6.0).round() as i64);
        match open.iter_mut().find(|o| o.0 == key) {
            Some(o) if n == o.1 + 1 => {
                o.1 = n;
                o.2.push(i);
            }
            Some(o) => {
                lists.push(std::mem::replace(&mut o.2, vec![i]));
                o.1 = n;
            }
            None => open.push((key, n, vec![i])),
        }
    }
    lists.extend(open.into_iter().map(|o| o.2));
    let mut next_num = 2;
    for members in lists.into_iter().filter(|m| m.len() >= 2) {
        let Some(Marker::Number(start, fmt, delim, _)) = paras[members[0]].0.marker else { continue };
        for i in members {
            let p = &mut paras[i].0;
            if let Some(Marker::Number(_, _, _, len)) = p.marker {
                strip_front(&mut p.runs, len);
            }
            p.list = Some(ListRef { num: next_num, level: 0, kind: ListKind::Number { fmt, delim, start } });
        }
        next_num += 1;
    }
}

// ---------------------------------------------------------------- Word

const EMU_PER_PT: f64 = 12700.0;

fn twips(pt: f64) -> i64 {
    (pt * 20.0).round() as i64
}

fn hex(c: [u8; 3]) -> String {
    format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

/// L'aspetto del testo: quello più comune diventa lo stile, i paragrafi dicono solo le differenze.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Look {
    font: String,
    half: i64,
    color: [u8; 3],
    bold: bool,
    italic: bool,
}

impl Look {
    fn of(r: &Run) -> Look {
        Look { font: family(&r.font), half: (r.size * 2.0).round().max(2.0) as i64, color: r.color, bold: r.bold, italic: r.italic }
    }

    /// Le proprietà di `run` diverse da quelle dello stile.
    fn diff(&self, run: &Look) -> String {
        let mut out = String::new();
        if run.font != self.font {
            let font = xml(&run.font);
            let _ = write!(out, "<w:rFonts w:ascii=\"{font}\" w:hAnsi=\"{font}\" w:cs=\"{font}\"/>");
        }
        match (run.bold, self.bold) {
            (true, false) => out.push_str("<w:b/>"),
            (false, true) => out.push_str("<w:b w:val=\"0\"/>"),
            _ => {}
        }
        match (run.italic, self.italic) {
            (true, false) => out.push_str("<w:i/>"),
            (false, true) => out.push_str("<w:i w:val=\"0\"/>"),
            _ => {}
        }
        if run.color != self.color {
            let _ = write!(out, "<w:color w:val=\"{}\"/>", hex(run.color));
        }
        if run.half != self.half {
            let _ = write!(out, "<w:sz w:val=\"{0}\"/><w:szCs w:val=\"{0}\"/>", run.half);
        }
        out
    }

    /// Le proprietà complete, per la definizione dello stile.
    fn full(&self) -> String {
        let font = xml(&self.font);
        let mut out = format!("<w:rFonts w:ascii=\"{font}\" w:hAnsi=\"{font}\" w:cs=\"{font}\"/>");
        if self.bold {
            out.push_str("<w:b/><w:bCs/>");
        }
        if self.italic {
            out.push_str("<w:i/><w:iCs/>");
        }
        if self.color != [0, 0, 0] {
            let _ = write!(out, "<w:color w:val=\"{}\"/>", hex(self.color));
        }
        let _ = write!(out, "<w:sz w:val=\"{0}\"/><w:szCs w:val=\"{0}\"/>", self.half);
        out
    }
}

/// L'aspetto più diffuso, pesato sul numero di caratteri.
fn dominant<'a>(runs: impl Iterator<Item = &'a Run>) -> Option<Look> {
    let mut counts: HashMap<Look, usize> = HashMap::new();
    for r in runs {
        *counts.entry(Look::of(r)).or_default() += r.text.chars().count();
    }
    counts.into_iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.half.cmp(&a.0.half))).map(|(l, _)| l)
}

/// Gli stili del documento: corpo, titoli 1-3, celle delle tabelle.
struct Styles {
    body: Look,
    headings: [Look; 3],
    table: Look,
}

impl Styles {
    fn of(pages: &[Page]) -> Styles {
        let blocks = || pages.iter().filter_map(|p| if let Page::Flow(f) = p { Some(f) } else { None }).flat_map(|f| f.blocks());
        let paras = || blocks().filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None });
        let body = dominant(paras().filter(|p| p.heading == 0).flat_map(|p| &p.runs))
            .unwrap_or(Look { font: "Arial".into(), half: 22, color: [0, 0, 0], bold: false, italic: false });
        let heading = |n: usize| {
            dominant(paras().filter(|p| p.heading == n).flat_map(|p| &p.runs))
                .unwrap_or(Look { half: body.half + 4 * (4 - n as i64), bold: true, ..body.clone() })
        };
        let table = dominant(
            blocks().filter_map(|b| if let Block::Table(t) = b { Some(t) } else { None }).flat_map(|t| &t.rows).flat_map(|r| &r.1).flat_map(|c| &c.paras).flat_map(|p| &p.runs),
        )
        .unwrap_or_else(|| body.clone());
        Styles { headings: [heading(1), heading(2), heading(3)], body, table }
    }

    fn base(&self, style: &str) -> &Look {
        match style {
            "Heading1" => &self.headings[0],
            "Heading2" => &self.headings[1],
            "Heading3" => &self.headings[2],
            "TableParagraph" => &self.table,
            _ => &self.body,
        }
    }
}

fn run_xml(run: &Run, base: &Look) -> String {
    let props = base.diff(&Look::of(run));
    let rpr = if props.is_empty() { String::new() } else { format!("<w:rPr>{props}</w:rPr>") };
    let text: Vec<String> = run.text.split('\t').map(|t| format!("<w:t xml:space=\"preserve\">{}</w:t>", xml(t))).collect();
    format!("<w:r>{rpr}{}</w:r>", text.join("<w:tab/>"))
}

struct Geometry {
    left: f64,
    right: f64,
}

fn para_xml(p: &Para, style: &str, before: f64, line_h: Option<f64>, geo: &Geometry, pad: Option<(f64, f64)>, styles: &Styles) -> String {
    let base = styles.base(style);
    let mut ppr = format!("<w:pStyle w:val=\"{style}\"/>");
    if p.heading > 0 {
        ppr.push_str("<w:keepNext/>");
    }
    if let Some(l) = p.list {
        let _ = write!(ppr, "<w:numPr><w:ilvl w:val=\"{}\"/><w:numId w:val=\"{}\"/></w:numPr>", l.level, l.num);
    }
    if !p.tabs.is_empty() {
        let (col_left, _) = pad.unwrap_or((geo.left, geo.right));
        ppr.push_str("<w:tabs>");
        for t in &p.tabs {
            let _ = write!(ppr, "<w:tab w:val=\"left\" w:pos=\"{}\"/>", twips((t - col_left).max(0.0)));
        }
        ppr.push_str("</w:tabs>");
    }
    let line = match line_h.or(p.pitch) {
        Some(h) => format!(" w:line=\"{}\" w:lineRule=\"atLeast\"", twips(h)),
        None => String::new(),
    };
    let _ = write!(ppr, "<w:spacing w:before=\"{}\" w:after=\"0\"{line}/>", twips(before));
    // Rientro e allineamento rispetto alla colonna di testo.
    let (col_left, col_right) = pad.unwrap_or((geo.left, geo.right));
    let mut jc = "";
    let indent = (p.x - col_left).max(0.0);
    if p.list.is_some() && p.tabs.is_empty() {
        // Il numero o il punto sta dove era nel PDF, il testo dove cominciava.
        let marker = (p.first_x - col_left).max(0.0);
        let text = p.text_x.map_or(marker + 18.0, |x| (x - col_left).max(marker + 6.0));
        let _ = write!(ppr, "<w:ind w:left=\"{}\" w:hanging=\"{}\"/>", twips(text), twips(text - marker));
    } else if p.tabs.is_empty() {
        let width = col_right - col_left;
        let center_off = ((p.x + p.end) / 2.0 - (col_left + col_right) / 2.0).abs();
        if p.pitch.is_none() && p.x - col_left > 0.04 * width && center_off < 3.0 {
            jc = "center";
        } else if p.pitch.is_none() && p.x - col_left > 0.3 * width && (col_right - p.end).abs() < 3.0 {
            jc = "right";
        }
        if jc.is_empty() && indent > 2.0 {
            let first = (p.first_x - col_left).max(0.0);
            if (first - indent).abs() > 2.0 && p.first_x > p.x {
                let _ = write!(ppr, "<w:ind w:left=\"{}\" w:firstLine=\"{}\"/>", twips(indent), twips(first - indent));
            } else {
                let _ = write!(ppr, "<w:ind w:left=\"{}\"/>", twips(indent));
            }
        }
    }
    if !jc.is_empty() {
        let _ = write!(ppr, "<w:jc w:val=\"{jc}\"/>");
    }
    // Il numero dell'elenco prende l'aspetto del testo che lo segue.
    if p.list.is_some() {
        if let Some(first) = p.runs.first() {
            let props = base.diff(&Look::of(first));
            if !props.is_empty() {
                let _ = write!(ppr, "<w:rPr>{props}</w:rPr>");
            }
        }
    }
    let mut out = format!("<w:p><w:pPr>{ppr}</w:pPr>");
    for r in &p.runs {
        out.push_str(&run_xml(r, base));
    }
    out.push_str("</w:p>");
    out
}

fn table_xml(t: &Table, geo: &Geometry, styles: &Styles) -> String {
    let border = match t.border {
        Some((c, w)) => {
            let sz = ((w * 8.0).round() as i64).clamp(2, 24);
            let col = hex(c);
            let one = |name: &str| format!("<w:{name} w:val=\"single\" w:sz=\"{sz}\" w:space=\"0\" w:color=\"{col}\"/>");
            format!("<w:tblBorders>{}{}{}{}{}{}</w:tblBorders>", one("top"), one("left"), one("bottom"), one("right"), one("insideH"), one("insideV"))
        }
        None => String::new(),
    };
    let widths: Vec<i64> = t.cols.windows(2).map(|w| twips(w[1] - w[0])).collect();
    let total: i64 = widths.iter().sum();
    let pad = twips(t.pad);
    let mut out = format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"{total}\" w:type=\"dxa\"/><w:tblInd w:w=\"{}\" w:type=\"dxa\"/>{border}<w:tblLayout w:type=\"fixed\"/>\
<w:tblCellMar><w:top w:w=\"0\" w:type=\"dxa\"/><w:left w:w=\"{pad}\" w:type=\"dxa\"/><w:bottom w:w=\"0\" w:type=\"dxa\"/><w:right w:w=\"{pad}\" w:type=\"dxa\"/></w:tblCellMar></w:tblPr><w:tblGrid>",
        twips(t.x0 - geo.left)
    );
    for w in &widths {
        let _ = write!(out, "<w:gridCol w:w=\"{w}\"/>");
    }
    out.push_str("</w:tblGrid>");
    for (i, (height, cells)) in t.rows.iter().enumerate() {
        let header = if i == 0 && t.header { "<w:tblHeader/>" } else { "" };
        let _ = write!(out, "<w:tr><w:trPr><w:cantSplit/><w:trHeight w:val=\"{}\" w:hRule=\"atLeast\"/>{header}</w:trPr>", twips(*height));
        let mut col = 0;
        for cell in cells {
            let w: i64 = widths[col..col + cell.span].iter().sum();
            let (cx0, cx1) = (t.cols[col], t.cols[col + cell.span]);
            let span = if cell.span > 1 { format!("<w:gridSpan w:val=\"{}\"/>", cell.span) } else { String::new() };
            let shd = cell.fill.map_or(String::new(), |c| format!("<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{}\"/>", hex(c)));
            let _ = write!(out, "<w:tc><w:tcPr><w:tcW w:w=\"{w}\" w:type=\"dxa\"/>{span}{shd}<w:vAlign w:val=\"center\"/></w:tcPr>");
            if cell.paras.is_empty() {
                out.push_str("<w:p><w:pPr><w:pStyle w:val=\"TableParagraph\"/></w:pPr></w:p>");
            }
            for p in &cell.paras {
                let inner = (cx0 + t.pad, cx1 - t.pad);
                let single = Para { runs: p.runs.clone(), tabs: p.tabs.clone(), list: None, marker: None, ..*p };
                out.push_str(&para_xml(&single, "TableParagraph", 0.0, None, geo, Some(inner), styles));
            }
            out.push_str("</w:tc>");
            col += cell.span;
        }
        out.push_str("</w:tr>");
    }
    out.push_str("</w:tbl>");
    out
}

fn image_xml(id: usize, ext: &str, fig: &Figure, before: f64, geo: &Geometry) -> String {
    let (cx, cy) = (((fig.x1 - fig.x0) * EMU_PER_PT) as i64, ((fig.y1 - fig.y0) * EMU_PER_PT) as i64);
    let center = ((fig.x0 + fig.x1) / 2.0 - (geo.left + geo.right) / 2.0).abs() < 4.0 && fig.x0 - geo.left > 4.0;
    let (jc, ind) = if center {
        ("<w:jc w:val=\"center\"/>".to_string(), String::new())
    } else {
        (String::new(), format!("<w:ind w:left=\"{}\"/>", twips((fig.x0 - geo.left).max(0.0))))
    };
    format!(
        "<w:p><w:pPr><w:spacing w:before=\"{}\" w:after=\"0\"/>{ind}{jc}</w:pPr><w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\">\
<wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:docPr id=\"{id}\" name=\"Immagine {id}\"/><wp:cNvGraphicFramePr/>\
<a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
<pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:nvPicPr><pic:cNvPr id=\"{id}\" name=\"immagine{id}.{ext}\"/><pic:cNvPicPr/></pic:nvPicPr>\
<pic:blipFill><a:blip r:embed=\"rIdImg{id}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>\
<pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>\
</a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>",
        twips(before)
    )
}

/// Le impostazioni di una sezione: pagina, margini e colonne. `continuous` se la sezione
/// comincia sulla stessa pagina della precedente.
struct Sect {
    w: f64,
    h: f64,
    top: f64,
    bottom: f64,
    left: f64,
    right: f64,
    /// Colonne: (inizio, larghezza) in punti dal bordo della pagina.
    cols: Vec<(f64, f64)>,
    continuous: bool,
}

fn sect_xml(s: &Sect) -> String {
    let kind = if s.continuous { "<w:type w:val=\"continuous\"/>" } else { "" };
    let mut cols = String::new();
    if s.cols.len() > 1 {
        let _ = write!(cols, "<w:cols w:num=\"{}\" w:equalWidth=\"0\">", s.cols.len());
        for (i, (x, w)) in s.cols.iter().enumerate() {
            let space = s.cols.get(i + 1).map_or(0.0, |n| n.0 - x - w);
            let _ = write!(cols, "<w:col w:w=\"{}\" w:space=\"{}\"/>", twips(*w), twips(space.max(0.0)));
        }
        cols.push_str("</w:cols>");
    }
    format!(
        "<w:sectPr>{kind}<w:pgSz w:w=\"{}\" w:h=\"{}\"/><w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\" w:header=\"0\" w:footer=\"0\" w:gutter=\"0\"/>{cols}</w:sectPr>",
        twips(s.w),
        twips(s.h),
        twips(s.top),
        twips(s.right),
        twips(s.bottom),
        twips(s.left)
    )
}

/// Chiude una sezione: le sue impostazioni vanno nell'ultimo paragrafo.
fn close_section(parts: &mut Vec<String>, sect: &str) {
    match parts.last_mut().filter(|l| l.starts_with("<w:p>") && !l.contains("<w:drawing>")) {
        Some(last) => *last = last.replacen("</w:pPr>", &format!("{sect}</w:pPr>"), 1),
        None => parts.push(format!("<w:p><w:pPr><w:spacing w:before=\"0\" w:after=\"0\" w:line=\"20\" w:lineRule=\"exact\"/>{sect}</w:pPr></w:p>")),
    }
}

/// La definizione degli elenchi usati nel documento.
fn numbering_xml(lists: &[ListRef]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:numbering xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
    );
    // Elenco puntato: il simbolo del PDF per ogni livello, se Word lo sa scrivere.
    let mut symbols = ['•', '◦', '▪', '•', '◦', '▪', '•', '◦', '▪'];
    for l in lists {
        if let ListKind::Bullet(c) = l.kind {
            if !('\u{e000}'..='\u{f8ff}').contains(&c) && l.level < 9 {
                symbols[l.level as usize] = c;
            }
        }
    }
    out.push_str("<w:abstractNum w:abstractNumId=\"1\"><w:multiLevelType w:val=\"hybridMultilevel\"/>");
    for (i, c) in symbols.iter().enumerate() {
        let _ = write!(
            out,
            "<w:lvl w:ilvl=\"{i}\"><w:start w:val=\"1\"/><w:numFmt w:val=\"bullet\"/><w:lvlText w:val=\"{}\"/><w:lvlJc w:val=\"left\"/>\
<w:pPr><w:ind w:left=\"{}\" w:hanging=\"360\"/></w:pPr></w:lvl>",
            xml(&c.to_string()),
            360 * (i + 1)
        );
    }
    out.push_str("</w:abstractNum>");
    let mut seen: Vec<u32> = Vec::new();
    let mut nums = String::from("<w:num w:numId=\"1\"><w:abstractNumId w:val=\"1\"/></w:num>");
    for l in lists {
        let ListKind::Number { fmt, delim, start } = l.kind else { continue };
        if seen.contains(&l.num) {
            continue;
        }
        seen.push(l.num);
        let fmt = match fmt {
            NumFmt::Decimal => "decimal",
            NumFmt::LowerLetter => "lowerLetter",
            NumFmt::UpperLetter => "upperLetter",
        };
        let _ = write!(
            out,
            "<w:abstractNum w:abstractNumId=\"{0}\"><w:multiLevelType w:val=\"singleLevel\"/><w:lvl w:ilvl=\"0\"><w:start w:val=\"{start}\"/>\
<w:numFmt w:val=\"{fmt}\"/><w:lvlText w:val=\"%1{delim}\"/><w:lvlJc w:val=\"left\"/><w:pPr><w:ind w:left=\"360\" w:hanging=\"360\"/></w:pPr></w:lvl></w:abstractNum>",
            l.num
        );
        let _ = write!(nums, "<w:num w:numId=\"{0}\"><w:abstractNumId w:val=\"{0}\"/></w:num>", l.num);
    }
    out.push_str(&nums);
    out.push_str("</w:numbering>");
    out
}

/// Il file Word. Le pagine con testo diventano titoli, paragrafi e tabelle; per le altre,
/// `fallback` dà il testo letto con l'OCR, in paragrafi, su una pagina normale.
pub fn docx(pages: &[Page], fallback: impl Fn(usize) -> String) -> Vec<u8> {
    // Margini comuni: dove comincia e finisce il contenuto di tutte le pagine.
    let (mut left, mut right) = (f64::MAX, 0.0f64);
    for page in pages {
        if let Page::Flow(p) = page {
            for b in p.blocks() {
                let (x0, x1) = b.span();
                left = left.min(x0);
                right = right.max(x1);
            }
        }
    }
    let styles = Styles::of(pages);
    // L'interlinea tipica del documento, per i paragrafi di una riga sola.
    let ratio = median(
        pages
            .iter()
            .filter_map(|p| if let Page::Flow(f) = p { Some(f) } else { None })
            .flat_map(|f| f.blocks())
            .filter_map(|b| if let Block::Para(p) = b { p.pitch.map(|h| h / p.size) } else { None })
            .filter(|r| (1.0..1.8).contains(r))
            .collect(),
    );
    let ratio = if ratio == 0.0 { 1.2 } else { ratio };
    let leading = |size: f64| size * ratio;
    let mut lists: Vec<ListRef> = Vec::new();
    let mut media: Vec<(String, Vec<u8>)> = Vec::new();
    let mut rels = String::new();
    let mut body = String::new();
    let mut next_id = 1;
    for (i, page) in pages.iter().enumerate() {
        let last_page = pages.iter().rposition(|p| !matches!(p, Page::Merged)) == Some(i);
        match page {
            Page::Flow(p) => {
                let page_right = (p.width - right).max(20.0);
                let geo = Geometry { left: left.max(0.0), right: p.width - page_right };
                let eff_top = |b: &Block| match b {
                    Block::Para(q) => q.baseline - q.pitch.unwrap_or_else(|| leading(q.size)) + 0.22 * q.size,
                    _ => b.top(),
                };
                let first_top = p.blocks().map(eff_top).fold(f64::MAX, f64::min);
                let top = if first_top == f64::MAX { 40.0 } else { first_top.clamp(20.0, 150.0) };
                let mut cursor = top;
                for (si, seg) in p.segments.iter().enumerate() {
                    // Le colonne di Word cominciano dal margine e arrivano al margine destro.
                    let mut cols: Vec<(f64, f64)> = Vec::new();
                    for (ci, c) in seg.cols.iter().enumerate() {
                        let start = if ci == 0 { geo.left } else { c.0.max(cols.last().map_or(geo.left, |p| p.0 + p.1) + 6.0) };
                        let end = match seg.cols.get(ci + 1) {
                            Some(next) => c.1.min(next.0 - 6.0).max(start + 20.0),
                            None => geo.right.max(start + 20.0),
                        };
                        cols.push((start, end - start));
                    }
                    let mut parts: Vec<String> = Vec::new();
                    // Lo spazio sopra le colonne va dopo il paragrafo che le precede: le colonne
                    // cominciano tutte alla stessa altezza.
                    if seg.cols.len() > 1 {
                        let band_top = seg.columns.iter().filter_map(|c| c.first()).map(eff_top).fold(f64::MAX, f64::min);
                        if band_top < f64::MAX && band_top > cursor + 0.5 {
                            if let Some(at) = body.rfind("w:after=\"0\"") {
                                body.replace_range(at..at + "w:after=\"0\"".len(), &format!("w:after=\"{}\"", twips(band_top - cursor)));
                                cursor = band_top;
                            }
                        }
                    }
                    let mut seg_end = cursor;
                    for (ci, col) in seg.columns.iter().enumerate() {
                        let cgeo = match cols.get(ci) {
                            Some((x, w)) => Geometry { left: *x, right: x + w },
                            None => Geometry { left: geo.left, right: geo.right },
                        };
                        let mut prev_bottom = cursor;
                        let mut col_parts: Vec<String> = Vec::new();
                        for block in col {
                            let gap = (block.top() - prev_bottom).max(0.0);
                            match block {
                                Block::Para(q) => {
                                    let style = match (q.heading, q.list.is_some()) {
                                        (1, _) => "Heading1",
                                        (2, _) => "Heading2",
                                        (3, _) => "Heading3",
                                        (_, true) => "ListParagraph",
                                        _ => "BodyText",
                                    };
                                    if let Some(l) = q.list {
                                        lists.push(l);
                                    }
                                    // Ogni riga alta come nel PDF: lo spazio prima si misura tra le
                                    // linee di base, così il testo non scivola giù pagina dopo pagina.
                                    let line_h = q.pitch.unwrap_or_else(|| leading(q.size));
                                    let top = q.baseline - line_h + 0.22 * q.size;
                                    let gap = (top - prev_bottom).max(0.0);
                                    col_parts.push(para_xml(q, style, gap, Some(line_h), &cgeo, None, &styles));
                                }
                                Block::Table(t) => {
                                    // Lo spazio sopra la tabella va dopo il paragrafo che la precede.
                                    if gap > 0.5 {
                                        if let Some(last) = col_parts.last_mut().filter(|l| l.starts_with("<w:p>") && !l.contains("<w:drawing>")) {
                                            *last = last.replacen("w:after=\"0\"", &format!("w:after=\"{}\"", twips(gap)), 1);
                                        }
                                    }
                                    col_parts.push(table_xml(t, &cgeo, &styles));
                                }
                                Block::Image { fig, data, ext } => {
                                    let id = next_id;
                                    next_id += 1;
                                    media.push((format!("word/media/immagine{id}.{ext}"), data.clone()));
                                    let _ = write!(
                                        rels,
                                        "<Relationship Id=\"rIdImg{id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/immagine{id}.{ext}\"/>"
                                    );
                                    col_parts.push(image_xml(id, ext, fig, gap, &cgeo));
                                }
                            }
                            prev_bottom = block.bottom();
                            seg_end = seg_end.max(prev_bottom);
                        }
                        // Fine colonna: il resto va nella colonna dopo. L'interruzione sta in un
                        // paragrafo alto un punto, così le colonne cominciano alla stessa altezza.
                        if ci + 1 < seg.columns.len() {
                            col_parts.push(
                                "<w:p><w:pPr><w:spacing w:before=\"0\" w:after=\"0\" w:line=\"20\" w:lineRule=\"exact\"/></w:pPr><w:r><w:br w:type=\"column\"/></w:r></w:p>"
                                    .to_string(),
                            );
                        }
                        parts.extend(col_parts);
                    }
                    cursor = seg_end;
                    let sect = sect_xml(&Sect {
                        w: p.width,
                        h: p.height,
                        top,
                        bottom: 36.0,
                        left: geo.left,
                        right: page_right,
                        cols,
                        continuous: si > 0,
                    });
                    let last_seg = si + 1 == p.segments.len();
                    if last_page && last_seg {
                        if parts.last().is_some_and(|l| l.starts_with("<w:tbl>")) {
                            parts.push("<w:p><w:pPr><w:spacing w:before=\"0\" w:after=\"0\"/></w:pPr></w:p>".to_string());
                        }
                        parts.push(sect);
                    } else {
                        close_section(&mut parts, &sect);
                    }
                    for part in parts {
                        body.push_str(&part);
                    }
                }
            }
            Page::Merged => {}
            Page::Empty => {
                let mut parts: Vec<String> = fallback(i)
                    .split('\n')
                    .map(|para| format!("<w:p><w:pPr><w:pStyle w:val=\"BodyText\"/><w:spacing w:before=\"0\" w:after=\"120\"/></w:pPr><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>", xml(para)))
                    .collect();
                let sect = sect_xml(&Sect { w: 595.3, h: 841.9, top: 56.7, bottom: 56.7, left: 56.7, right: 56.7, cols: Vec::new(), continuous: false });
                if last_page {
                    parts.push(sect);
                } else {
                    close_section(&mut parts, &sect);
                }
                for part in parts {
                    body.push_str(&part);
                }
            }
        }
    }
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\"><w:body>{body}</w:body></w:document>"
    );
    let types = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
<Default Extension=\"xml\" ContentType=\"application/xml\"/>\
<Default Extension=\"png\" ContentType=\"image/png\"/>\
<Default Extension=\"jpg\" ContentType=\"image/jpeg\"/>\
<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
<Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>\
<Override PartName=\"/word/numbering.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml\"/>\
</Types>";
    let root_rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
</Relationships>";
    let doc_rels = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>\
<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering\" Target=\"numbering.xml\"/>{rels}</Relationships>"
    );
    let heading = |n: usize| {
        format!(
            "<w:style w:type=\"paragraph\" w:styleId=\"Heading{n}\"><w:name w:val=\"heading {n}\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"BodyText\"/><w:uiPriority w:val=\"9\"/><w:qFormat/>\
<w:pPr><w:keepNext/><w:outlineLvl w:val=\"{}\"/></w:pPr><w:rPr>{}</w:rPr></w:style>",
            n - 1,
            styles.headings[n - 1].full()
        )
    };
    let font = xml(&styles.body.font);
    let styles_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"{font}\" w:hAnsi=\"{font}\" w:cs=\"{font}\"/>\
<w:sz w:val=\"{half}\"/><w:szCs w:val=\"{half}\"/><w:lang w:val=\"it-IT\"/></w:rPr></w:rPrDefault>\
<w:pPrDefault><w:pPr><w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults>\
<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"BodyText\"><w:name w:val=\"Body Text\"/><w:basedOn w:val=\"Normal\"/><w:qFormat/><w:rPr>{}</w:rPr></w:style>\
{}{}{}\
<w:style w:type=\"paragraph\" w:styleId=\"ListParagraph\"><w:name w:val=\"List Paragraph\"/><w:basedOn w:val=\"BodyText\"/><w:uiPriority w:val=\"34\"/><w:qFormat/></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"TableParagraph\"><w:name w:val=\"Table Paragraph\"/><w:basedOn w:val=\"Normal\"/><w:qFormat/><w:rPr>{}</w:rPr></w:style>\
</w:styles>",
        styles.body.full(),
        heading(1),
        heading(2),
        heading(3),
        styles.table.full(),
        half = styles.body.half,
    );
    let numbering = numbering_xml(&lists);
    let mut entries: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", root_rels.as_bytes()),
        ("word/document.xml", document.as_bytes()),
        ("word/_rels/document.xml.rels", doc_rels.as_bytes()),
        ("word/styles.xml", styles_xml.as_bytes()),
        ("word/numbering.xml", numbering.as_bytes()),
    ];
    for (name, data) in &media {
        entries.push((name.as_str(), data.as_slice()));
    }
    zip_archive(&entries)
}
