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
struct Line {
    x: f64,
    end: f64,
    baseline: f64,
    size: f64,
    runs: Vec<Run>,
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

/// Raggruppa i caratteri in righe, da sinistra a destra e dall'alto in basso.
fn group_lines(mut glyphs: Vec<Glyph>) -> Vec<Line> {
    glyphs.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
    // Righe di base: i caratteri con la stessa linea di base vanno insieme.
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
    let mut lines: Vec<Line> = Vec::new();
    for mut row in rows {
        row.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
        let mut cur: Option<Line> = None;
        let mut last_end = 0.0;
        for g in row {
            let split = cur.as_ref().is_some_and(|l| g.x - last_end > 2.0 * g.size.max(l.size) || g.x < last_end - 0.6 * g.size);
            if split {
                lines.extend(cur.take());
            }
            let line = cur.get_or_insert_with(|| Line { x: g.x, end: g.x, baseline: g.y, size: g.size, runs: Vec::new() });
            // Un vuoto largo come una parola, senza spazio nel PDF, diventa uno spazio.
            if g.x - last_end > 0.2 * g.size && !g.text.starts_with(' ') && line.end > line.x {
                if let Some(run) = line.runs.last_mut() {
                    if !run.text.ends_with(' ') {
                        run.text.push(' ');
                    }
                }
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
                    text.push(Glyph { text: g.text.clone(), font: g.font.clone(), ..*g });
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
    bullet: bool,
}

fn is_bullet(text: &str) -> Option<usize> {
    let c = text.chars().next()?;
    if matches!(c, '•' | '▪' | '◦' | '●' | '·' | '■' | '–' | '\u{f0b7}' | '\u{f0a7}') && text.chars().nth(1).is_some_and(char::is_whitespace) {
        return Some(c.len_utf8());
    }
    None
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
        bullet: false,
    }
}

/// Mette in paragrafi le righe di testo fuori dalle tabelle.
fn paragraphs(lines: &[Line], body: f64) -> Vec<Para> {
    let right = lines.iter().map(|l| l.end).fold(0.0, f64::max);
    let left = lines.iter().map(|l| l.x).fold(f64::MAX, f64::min);
    let width = (right - left).max(1.0);
    let heading_of = |l: &Line| l.size >= body * 1.15 && l.all_bold();
    // Le dimensioni dei titoli, dalla più grande: titolo 1, 2, 3.
    let mut sizes: Vec<f64> = lines.iter().filter(|l| heading_of(l)).map(|l| (l.size * 2.0).round() / 2.0).collect();
    sizes.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    sizes.dedup();
    let rank = |l: &Line| sizes.iter().position(|s| (*s - (l.size * 2.0).round() / 2.0).abs() < 0.01).map_or(0, |p| (p + 1).min(3));

    let mut out: Vec<Para> = Vec::new();
    let mut group: Vec<&Line> = Vec::new();
    let flush = |group: &mut Vec<&Line>, out: &mut Vec<Para>| {
        if group.is_empty() {
            return;
        }
        let mut p = para_from_lines(group, left);
        if heading_of(group[0]) {
            p.heading = rank(group[0]);
        }
        // Elenco puntato: via il simbolo.
        let text: String = p.runs.iter().map(|r| r.text.as_str()).collect();
        if let Some(n) = is_bullet(&text) {
            p.bullet = true;
            if let Some(first) = p.runs.first_mut() {
                first.text = first.text[n..].trim_start().to_string();
            }
        }
        out.push(p);
        group.clear();
    };
    for line in lines {
        let continues = group.last().is_some_and(|prev| {
            let dy = line.baseline - prev.baseline;
            let short_prev = prev.end < right - 0.15 * width;
            let same_x = (line.x - prev.x).abs() <= 0.8 * line.size || (group.len() == 1 && line.x < prev.x);
            dy > 0.6 * line.size
                && dy < 1.7 * line.size.max(prev.size)
                && (line.size - prev.size).abs() < 0.6
                && heading_of(line) == heading_of(prev)
                && !short_prev
                && same_x
                && is_bullet(&line.text()).is_none()
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

// ---------------------------------------------------------------- pagine

enum Block {
    Para(Para),
    Table(Table),
    Image { fig: Figure, png: Vec<u8> },
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
}

/// Una pagina del Word.
pub enum Page {
    /// Pagina ricostruita: titoli, paragrafi, tabelle, immagini.
    Flow(FlowPage),
    /// Pagina senza testo da copiare: non c'è niente da ricostruire.
    Empty,
}

pub struct FlowPage {
    width: f64,
    height: f64,
    blocks: Vec<Block>,
}

/// Legge le pagine del PDF. `Page::Empty` per quelle senza testo incorporato.
/// Con `tables` falso le tabelle non si riconoscono: il loro testo resta in paragrafi.
pub fn read_pages(bytes: &[u8], with_tables: bool) -> Result<Vec<Page>, String> {
    let original = Pdf::new(bytes.to_vec()).map_err(|e| format!("PDF non letto: {e:?}"))?;
    let cache = RenderCache::new();
    let interp_cache = InterpreterCache::new();
    let settings = InterpreterSettings::default();
    let mut pages = Vec::new();
    for page in original.pages().iter() {
        let (w, h) = page.render_dimensions();
        let (w, h) = (w as f64, h as f64);
        let ctm = page.initial_transform(true).to_kurbo();
        let mut ctx = Context::new(ctm, Rect::new(0.0, 0.0, w, h), &interp_cache, page.xref(), settings.clone());
        let mut collector = Collector::default();
        interpret_page(page, &mut ctx, &mut collector);
        let Collector { mut glyphs, shapes, pics, stems, .. } = collector;
        if glyphs.is_empty() {
            pages.push(Page::Empty);
            continue;
        }
        for g in &mut glyphs {
            if let (Some(key), true) = (g.font_key, g.font.is_empty()) {
                if let Some((sum, n)) = stems.get(&key) {
                    g.bold = *n > 0 && sum / *n as f64 >= 96.0;
                }
            }
        }
        let tables = if with_tables { find_tables(&shapes, (w, h), &mut glyphs) } else { Vec::new() };
        // Senza tabelle anche i riquadri colorati non diventano figure: il testo resta testo.
        let figures = find_figures(if with_tables { &shapes[..] } else { &[] }, &pics, &tables, (w, h));
        // Il testo dentro una figura fa parte dell'immagine.
        glyphs.retain(|g| !figures.iter().any(|f| g.x >= f.x0 && g.x <= f.x1 && g.y >= f.y0 && g.y <= f.y1));

        let lines = group_lines(glyphs);
        let mut sizes: HashMap<i64, usize> = HashMap::new();
        for l in &lines {
            *sizes.entry((l.size * 4.0).round() as i64).or_default() += l.text().chars().count();
        }
        let body = sizes.into_iter().max_by_key(|(_, n)| *n).map_or(11.0, |(s, _)| s as f64 / 4.0);
        let mut blocks: Vec<Block> = paragraphs(&lines, body).into_iter().map(Block::Para).collect();
        blocks.extend(tables.into_iter().map(Block::Table));
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
                let mut out = Vec::new();
                crop.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).map_err(|e| e.to_string())?;
                blocks.push(Block::Image { fig, png: out });
            }
        }
        blocks.sort_by(|a, b| a.top().partial_cmp(&b.top()).unwrap_or(std::cmp::Ordering::Equal));
        pages.push(Page::Flow(FlowPage { width: w, height: h, blocks }));
    }
    Ok(pages)
}

// ---------------------------------------------------------------- Word

const EMU_PER_PT: f64 = 12700.0;

fn twips(pt: f64) -> i64 {
    (pt * 20.0).round() as i64
}

fn run_xml(run: &Run, in_heading: bool) -> String {
    let half_points = (run.size * 2.0).round().max(2.0) as i64;
    let font = xml(&family(&run.font));
    let color = if run.color == [0, 0, 0] { String::new() } else { format!("<w:color w:val=\"{:02X}{:02X}{:02X}\"/>", run.color[0], run.color[1], run.color[2]) };
    let bold = if run.bold || in_heading { "<w:b/>" } else { "" };
    format!(
        "<w:r><w:rPr><w:rFonts w:ascii=\"{font}\" w:hAnsi=\"{font}\" w:cs=\"{font}\"/>{bold}{}{color}<w:sz w:val=\"{half_points}\"/><w:szCs w:val=\"{half_points}\"/></w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r>",
        if run.italic { "<w:i/>" } else { "" },
        xml(&run.text)
    )
}

struct Geometry {
    left: f64,
    right: f64,
}

fn para_xml(p: &Para, style: &str, before: f64, geo: &Geometry, pad: Option<(f64, f64)>) -> String {
    let mut ppr = format!("<w:pStyle w:val=\"{style}\"/>");
    if p.bullet {
        ppr.push_str("<w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr>");
    }
    if p.heading > 0 {
        ppr.push_str("<w:keepNext/>");
    }
    let line = match p.pitch {
        Some(pitch) => format!(" w:line=\"{}\" w:lineRule=\"atLeast\"", twips(pitch)),
        None => String::new(),
    };
    let _ = write!(ppr, "<w:spacing w:before=\"{}\" w:after=\"0\"{line}/>", twips(before));
    // Rientro e allineamento rispetto alla colonna di testo.
    let (col_left, col_right) = pad.unwrap_or((geo.left, geo.right));
    let mut jc = "";
    let indent = (p.x - col_left).max(0.0);
    if pad.is_some() || !p.bullet {
        let width = col_right - col_left;
        let center_off = ((p.x + p.end) / 2.0 - (col_left + col_right) / 2.0).abs();
        if p.pitch.is_none() && p.x - col_left > 0.04 * width && center_off < 3.0 && !p.bullet {
            jc = "center";
        } else if p.pitch.is_none() && p.x - col_left > 0.3 * width && (col_right - p.end).abs() < 3.0 {
            jc = "right";
        }
    }
    if jc.is_empty() && indent > 2.0 && !p.bullet {
        let first = (p.first_x - col_left).max(0.0);
        if (first - indent).abs() > 2.0 && p.first_x > p.x {
            let _ = write!(ppr, "<w:ind w:left=\"{}\" w:firstLine=\"{}\"/>", twips(indent), twips(first - indent));
        } else {
            let _ = write!(ppr, "<w:ind w:left=\"{}\"/>", twips(indent));
        }
    }
    if !jc.is_empty() {
        let _ = write!(ppr, "<w:jc w:val=\"{jc}\"/>");
    }
    let mut out = format!("<w:p><w:pPr>{ppr}</w:pPr>");
    for r in &p.runs {
        out.push_str(&run_xml(r, p.heading > 0));
    }
    out.push_str("</w:p>");
    out
}

fn table_xml(t: &Table, geo: &Geometry) -> String {
    let border = match t.border {
        Some((c, w)) => {
            let sz = ((w * 8.0).round() as i64).clamp(2, 24);
            let col = format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2]);
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
            let shd = cell.fill.map_or(String::new(), |c| format!("<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{:02X}{:02X}{:02X}\"/>", c[0], c[1], c[2]));
            let _ = write!(out, "<w:tc><w:tcPr><w:tcW w:w=\"{w}\" w:type=\"dxa\"/>{span}{shd}<w:vAlign w:val=\"center\"/></w:tcPr>");
            if cell.paras.is_empty() {
                out.push_str("<w:p><w:pPr><w:pStyle w:val=\"TableParagraph\"/></w:pPr></w:p>");
            }
            for p in &cell.paras {
                let inner = (cx0 + t.pad, cx1 - t.pad);
                let mut single = Para { runs: p.runs.clone(), ..*p };
                single.bullet = false;
                out.push_str(&para_xml(&single, "TableParagraph", 0.0, geo, Some(inner)));
            }
            out.push_str("</w:tc>");
            col += cell.span;
        }
        out.push_str("</w:tr>");
    }
    out.push_str("</w:tbl>");
    out
}

fn image_xml(id: usize, fig: &Figure, before: f64, geo: &Geometry, width_pt: f64) -> String {
    let (cx, cy) = (((fig.x1 - fig.x0) * EMU_PER_PT) as i64, ((fig.y1 - fig.y0) * EMU_PER_PT) as i64);
    let center = ((fig.x0 + fig.x1) / 2.0 - (geo.left + geo.right) / 2.0).abs() < 4.0 && fig.x0 - geo.left > 4.0;
    let (jc, ind) = if center {
        ("<w:jc w:val=\"center\"/>".to_string(), String::new())
    } else {
        (String::new(), format!("<w:ind w:left=\"{}\"/>", twips((fig.x0 - geo.left).max(0.0))))
    };
    let _ = width_pt;
    format!(
        "<w:p><w:pPr><w:spacing w:before=\"{}\" w:after=\"0\"/>{ind}{jc}</w:pPr><w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\">\
<wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:docPr id=\"{id}\" name=\"Immagine {id}\"/><wp:cNvGraphicFramePr/>\
<a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
<pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:nvPicPr><pic:cNvPr id=\"{id}\" name=\"immagine{id}.png\"/><pic:cNvPicPr/></pic:nvPicPr>\
<pic:blipFill><a:blip r:embed=\"rIdImg{id}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>\
<pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>\
</a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>",
        twips(before)
    )
}

fn sect_xml(w: f64, h: f64, top: f64, bottom: f64, left: f64, right: f64) -> String {
    format!(
        "<w:sectPr><w:pgSz w:w=\"{}\" w:h=\"{}\"/><w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\" w:header=\"0\" w:footer=\"0\" w:gutter=\"0\"/></w:sectPr>",
        twips(w),
        twips(h),
        twips(top),
        twips(right),
        twips(bottom),
        twips(left)
    )
}

/// Il file Word. Le pagine con testo diventano titoli, paragrafi e tabelle; per le altre,
/// `fallback` dà il testo letto con l'OCR, in paragrafi, su una pagina normale.
pub fn docx(pages: &[Page], fallback: impl Fn(usize) -> String) -> Vec<u8> {
    // Margini comuni: dove comincia e finisce il contenuto di tutte le pagine.
    let (mut left, mut right) = (f64::MAX, 0.0f64);
    for page in pages {
        if let Page::Flow(p) = page {
            for b in &p.blocks {
                let (x0, x1) = match b {
                    Block::Para(q) => (q.x, q.end),
                    Block::Table(t) => (t.x0, *t.cols.last().unwrap()),
                    Block::Image { fig, .. } => (fig.x0, fig.x1),
                };
                left = left.min(x0);
                right = right.max(x1);
            }
        }
    }
    let mut media: Vec<(String, Vec<u8>)> = Vec::new();
    let mut rels = String::new();
    let mut body = String::new();
    let mut next_id = 1;
    for (i, page) in pages.iter().enumerate() {
        let last_page = i + 1 == pages.len();
        let (mut parts, sect): (Vec<String>, String) = match page {
            Page::Flow(p) => {
                let page_right = (p.width - right).max(20.0);
                let geo = Geometry { left: left.max(0.0), right: p.width - page_right };
                let mut parts: Vec<String> = Vec::new();
                let mut prev_bottom: Option<f64> = None;
                for block in &p.blocks {
                    let gap = prev_bottom.map_or(0.0, |b| (block.top() - b).max(0.0));
                    match block {
                        Block::Para(q) => {
                            let style = match q.heading {
                                1 => "Heading1",
                                2 => "Heading2",
                                3 => "Heading3",
                                _ => "BodyText",
                            };
                            parts.push(para_xml(q, style, gap, &geo, None));
                        }
                        Block::Table(t) => {
                            // Lo spazio sopra la tabella va dopo il paragrafo che la precede.
                            if gap > 0.5 {
                                if let Some(last) = parts.last_mut().filter(|l| l.starts_with("<w:p>") && !l.contains("<w:drawing>")) {
                                    *last = last.replacen("w:after=\"0\"", &format!("w:after=\"{}\"", twips(gap)), 1);
                                }
                            }
                            parts.push(table_xml(t, &geo));
                        }
                        Block::Image { fig, png } => {
                            let id = next_id;
                            next_id += 1;
                            media.push((format!("word/media/immagine{id}.png"), png.clone()));
                            let _ = write!(
                                rels,
                                "<Relationship Id=\"rIdImg{id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/immagine{id}.png\"/>"
                            );
                            parts.push(image_xml(id, fig, gap, &geo, p.width));
                        }
                    }
                    prev_bottom = Some(block.bottom());
                }
                let top = p.blocks.first().map_or(40.0, |b| b.top()).clamp(20.0, 150.0);
                (parts, sect_xml(p.width, p.height, top, 36.0, geo.left, page_right))
            }
            Page::Empty => {
                let parts = fallback(i)
                    .split('\n')
                    .map(|para| format!("<w:p><w:pPr><w:pStyle w:val=\"BodyText\"/><w:spacing w:before=\"0\" w:after=\"120\"/></w:pPr><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>", xml(para)))
                    .collect();
                (parts, sect_xml(595.3, 841.9, 56.7, 56.7, 56.7, 56.7))
            }
        };
        if !last_page {
            // La fine della sezione sta nell'ultimo paragrafo della pagina.
            match parts.last_mut().filter(|l| l.starts_with("<w:p>") && !l.contains("<w:drawing>")) {
                Some(last) => *last = last.replacen("</w:pPr>", &format!("{sect}</w:pPr>"), 1),
                None => parts.push(format!(
                    "<w:p><w:pPr><w:spacing w:before=\"0\" w:after=\"0\" w:line=\"20\" w:lineRule=\"exact\"/>{sect}</w:pPr></w:p>"
                )),
            }
        } else if parts.last().is_some_and(|l| l.starts_with("<w:tbl>")) {
            parts.push("<w:p><w:pPr><w:spacing w:before=\"0\" w:after=\"0\"/></w:pPr></w:p>".to_string());
        }
        for part in parts {
            body.push_str(&part);
        }
        if last_page {
            body.push_str(&sect);
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
    let heading = |id: &str, name: &str, level: u32| {
        format!(
            "<w:style w:type=\"paragraph\" w:styleId=\"{id}\"><w:name w:val=\"{name}\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"BodyText\"/><w:qFormat/>\
<w:pPr><w:keepNext/><w:outlineLvl w:val=\"{level}\"/></w:pPr><w:rPr><w:b/></w:rPr></w:style>"
        )
    };
    let styles = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Arial\" w:hAnsi=\"Arial\" w:cs=\"Arial\"/>\
<w:sz w:val=\"22\"/><w:lang w:val=\"it-IT\"/></w:rPr></w:rPrDefault>\
<w:pPrDefault><w:pPr><w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults>\
<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"BodyText\"><w:name w:val=\"Body Text\"/><w:basedOn w:val=\"Normal\"/><w:qFormat/></w:style>\
{}{}{}\
<w:style w:type=\"paragraph\" w:styleId=\"TableParagraph\"><w:name w:val=\"Table Paragraph\"/><w:basedOn w:val=\"Normal\"/><w:qFormat/></w:style>\
</w:styles>",
        heading("Heading1", "heading 1", 0),
        heading("Heading2", "heading 2", 1),
        heading("Heading3", "heading 3", 2)
    );
    let numbering = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:numbering xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:abstractNum w:abstractNumId=\"0\"><w:multiLevelType w:val=\"hybridMultilevel\"/>\
<w:lvl w:ilvl=\"0\"><w:start w:val=\"1\"/><w:numFmt w:val=\"bullet\"/><w:lvlText w:val=\"&#8226;\"/><w:lvlJc w:val=\"left\"/>\
<w:pPr><w:ind w:left=\"360\" w:hanging=\"360\"/></w:pPr></w:lvl></w:abstractNum>\
<w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num></w:numbering>";
    let mut entries: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", root_rels.as_bytes()),
        ("word/document.xml", document.as_bytes()),
        ("word/_rels/document.xml.rels", doc_rels.as_bytes()),
        ("word/styles.xml", styles.as_bytes()),
        ("word/numbering.xml", numbering.as_bytes()),
    ];
    for (name, data) in &media {
        entries.push((name.as_str(), data.as_slice()));
    }
    zip_archive(&entries)
}
