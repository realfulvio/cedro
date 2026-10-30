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
    /// Larghezza dei caratteri in percento (testo stretto o largo, Tz).
    stretch: u16,
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
    /// Il tracciato in punti della pagina, per ridisegnarlo in Word.
    path: BezPath,
    /// Posizione nell'ordine di disegno (anche rispetto alle immagini).
    order: usize,
    /// Il disegno di una lettera ruotata.
    text: bool,
    /// Tratteggio: lunghezze di tratti e vuoti, in punti.
    dash: Vec<f64>,
    /// Non si ridisegna come forma (sfumatura, motivo, ritaglio non rettangolare): nel Word
    /// va il pezzo di pagina disegnato.
    raster: bool,
}

struct Pic {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    order: usize,
}

#[derive(Default)]
struct Collector {
    glyphs: Vec<Glyph>,
    shapes: Vec<Shape2>,
    pics: Vec<Pic>,
    /// Spessore medio dei tratti per font senza nome (per riconoscere il grassetto).
    stems: HashMap<u128, (f64, u32)>,
    seen: HashSet<(u128, char)>,
    /// Le aree di ritaglio aperte, e se sono rettangoli: quel che sta fuori non si vede.
    clips: Vec<(Rect, bool)>,
    drawn: usize,
}

impl Collector {
    fn visible(&self, b: &Rect) -> bool {
        self.clips.last().is_none_or(|(c, _)| b.x1 >= c.x0 - 0.5 && b.x0 <= c.x1 + 0.5 && b.y1 >= c.y0 - 0.5 && b.y0 <= c.y1 + 0.5)
    }
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
        let (filled, width, dash) = match mode {
            DrawMode::Fill(_) => (true, None, Vec::new()),
            DrawMode::Stroke(s) => (false, Some(s.line_width as f64 * scale), s.dash_array.iter().map(|d| *d as f64 * scale).collect()),
            DrawMode::FillAndStroke(_, s) => (true, Some(s.line_width as f64 * scale), s.dash_array.iter().map(|d| *d as f64 * scale).collect()),
            DrawMode::Invisible => return,
        };
        let color = match &props.paint {
            Paint::Color(c) => rgb(c),
            _ => Some([128, 128, 128]),
        };
        let q = props.transform * p.clone();
        let b = q.bounding_box();
        if !self.visible(&b) {
            return;
        }
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
        // Quel che esce dal ritaglio: un rettangolo si accorcia, il resto si prende disegnato.
        let mut raster = !matches!(props.paint, Paint::Color(_));
        let (mut q, mut b) = (q, b);
        if let Some((c, clip_rect)) = self.clips.last() {
            let inside = b.x0 >= c.x0 - 0.5 && b.x1 <= c.x1 + 0.5 && b.y0 >= c.y0 - 0.5 && b.y1 <= c.y1 + 0.5;
            if !inside {
                if *clip_rect && rect {
                    b = b.intersect(*c);
                    q = b.to_path(0.1);
                } else {
                    raster = true;
                }
            }
        }
        self.shapes.push(Shape2 {
            x0: b.x0,
            y0: b.y0,
            x1: b.x1,
            y1: b.y1,
            fill: if filled { color } else { None },
            stroke: width.map(|w| (color.unwrap_or([0, 0, 0]), w)),
            rect,
            line,
            path: q,
            order: self.drawn,
            text: false,
            dash,
            raster,
        });
        self.drawn += 1;
    }
    fn push_clip_path(&mut self, clip: &ClipPath) {
        let b = clip.path.bounding_box();
        let near = |a: f64, v: f64| (a - v).abs() < 0.05;
        let mut pts = Vec::new();
        let mut simple = true;
        for el in clip.path.elements() {
            match el {
                PathEl::MoveTo(p) | PathEl::LineTo(p) => pts.push(*p),
                PathEl::ClosePath => {}
                _ => simple = false,
            }
        }
        let rect = simple && (4..=5).contains(&pts.len()) && pts.iter().all(|p| (near(p.x, b.x0) || near(p.x, b.x1)) && (near(p.y, b.y0) || near(p.y, b.y1)));
        let top = match self.clips.last() {
            Some((c, r)) => (c.intersect(b), *r && rect),
            None => (b, rect),
        };
        self.clips.push(top);
    }
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn pop_clip(&mut self) {
        self.clips.pop();
    }
    fn pop_transparency_group(&mut self) {}

    fn draw_image(&mut self, image: Image<'a, '_>, props: ImageDrawProps<'a>) {
        let r = Rect::new(0.0, 0.0, image.width() as f64, image.height() as f64);
        let b = (props.transform * r.to_path(0.1)).bounding_box();
        if !self.visible(&b) {
            return;
        }
        let c = self.clips.last().map_or(b, |c| c.0);
        let v = b.intersect(c);
        self.pics.push(Pic { x0: v.x0, y0: v.y0, x1: v.x1, y1: v.y1, order: self.drawn });
        self.drawn += 1;
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
                // Testo ruotato (le scritte a margine dei moduli): resta un disegno, come si vede.
                if let hayro::hayro_interpret::font::Glyph::Outline(o) = &**glyph {
                    let q = t * o.outline();
                    let bb = q.bounding_box();
                    if self.visible(&bb) && bb.area() > 0.0 {
                        self.shapes.push(Shape2 {
                            x0: bb.x0,
                            y0: bb.y0,
                            x1: bb.x1,
                            y1: bb.y1,
                            fill: Some(color),
                            stroke: None,
                            rect: false,
                            line: false,
                            path: q,
                            order: self.drawn,
                            text: true,
                            dash: Vec::new(),
                            raster: false,
                        });
                        self.drawn += 1;
                    }
                }
                continue;
            }
            // Il corpo è l'altezza; il testo stretto o largo ha la sua percentuale di larghezza.
            let size = d.abs() * 1000.0;
            if size < 1.0 {
                continue;
            }
            let stretch = ((a.abs() / d.abs()) * 100.0).round().clamp(1.0, 600.0) as u16;
            let stretch = if (95..=105).contains(&stretch) { 100 } else { stretch };
            let (mut font, mut bold, mut italic, mut adv, mut font_key) = (String::new(), false, false, 0.0, None);
            if let hayro::hayro_interpret::font::Glyph::Outline(o) = &**glyph {
                font_key = Some(o.font_cache_key());
                adv = o.advance_width().map_or(0.0, |w| w as f64) * a.abs();
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
            self.glyphs.push(Glyph { text, x: e, y: f, size, adv, stretch, color, bold, italic, font, font_key });
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
    /// Larghezza che avrebbe la riga con le larghezze normali dei caratteri.
    natural: f64,
}

#[derive(Clone)]
struct Run {
    text: String,
    font: String,
    size: f64,
    stretch: u16,
    /// Spazio in più dopo ogni carattere, in punti (testo spaziato nel PDF).
    tracking: f64,
    color: [u8; 3],
    bold: bool,
    italic: bool,
}

impl Run {
    fn same_style(&self, o: &Run) -> bool {
        self.font == o.font
            && (self.size - o.size).abs() < 0.2
            && self.stretch == o.stretch
            && self.color == o.color
            && self.bold == o.bold
            && self.italic == o.italic
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
/// `gap`: il vuoto, in corpi, oltre il quale una riga si spezza in due pezzi.
fn group_lines(glyphs: Vec<Glyph>, gap: f64) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    for mut row in split_rows(glyphs) {
        row.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
        let mut cur: Option<Line> = None;
        let mut last_end = 0.0;
        let mut after_space = false;
        // Gli spazi si rimettono dai vuoti tra le parole: quelli del PDF possono riempire una
        // casella intera (moduli scritti a spazi) e mescolarsi con le scritte vicine.
        row.retain(|g| !g.text.trim().is_empty());
        for g in row {
            // Nel testo a passo fisso gli spazi tengono le colonne: si rimettono tutti.
            let cell = 0.6 * g.size * g.stretch as f64 / 100.0;
            let mono = g.font == "Courier" && cur.as_ref().and_then(|l| l.runs.last()).is_some_and(|r| r.font == "Courier");
            // Nei moduli conta la scritta più piccola: un valore grande accanto a un'etichetta
            // piccola resta separato.
            let split = cur.as_ref().is_some_and(|l| {
                let size = if gap < 1.0 { g.size.min(l.size) } else { g.size.max(l.size) };
                (!mono && g.x - last_end > gap * size) || g.x < last_end - 0.6 * g.size
            });
            if split {
                lines.extend(cur.take());
                after_space = false;
            }
            let line = cur.get_or_insert_with(|| Line { x: g.x, end: g.x, baseline: g.y, size: g.size, runs: Vec::new(), word2_x: None, tabs: Vec::new(), natural: 0.0 });
            let blank = g.text.trim().is_empty();
            let has_text = line.runs.iter().any(|r| !r.text.trim().is_empty());
            // Un vuoto largo come una parola, senza spazio nel PDF, diventa uno spazio.
            let gap = g.x - last_end > 0.2 * g.size && line.end > line.x;
            if gap && !g.text.starts_with(' ') {
                if let Some(run) = line.runs.last_mut() {
                    if mono {
                        // Al più una riga lunghissima di spazi: con coordinate assurde (PDF ostile) non si chiedono GB.
                        let n = ((g.x - last_end) / cell).round().clamp(1.0, 1000.0) as usize;
                        run.text.push_str(&" ".repeat(n));
                        line.natural += n as f64 * cell;
                    } else if !run.text.ends_with(' ') {
                        run.text.push(' ');
                        line.natural += 0.278 * g.size;
                    }
                }
            }
            if !blank && has_text && (gap || after_space) && line.word2_x.is_none() {
                line.word2_x = Some(g.x);
            }
            if !blank || has_text {
                after_space = blank;
            }
            let run = Run { text: g.text.clone(), font: g.font.clone(), size: g.size, stretch: g.stretch, tracking: 0.0, color: g.color, bold: g.bold, italic: g.italic };
            match line.runs.last_mut() {
                Some(r) if r.same_style(&run) => r.text.push_str(&g.text),
                _ => line.runs.push(run),
            }
            line.size = line.size.max(g.size);
            line.natural += g.adv;
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
    /// Pagina del PDF (da 0) da cui viene, se è finita dentro la pagina prima con una tabella spezzata.
    from: Option<usize>,
    /// Ultima pagina del PDF (da 0) se la tabella continua sulle pagine dopo.
    until: Option<usize>,
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
    // Il primo gruppo che va bene vince; l'indice a celle di 2,5 punti evita di confrontare ogni
    // segmento con tutti i gruppi (i disegni tecnici ne hanno decine di migliaia).
    let mut groups: Vec<(f64, f64, Vec<usize>)> = Vec::new();
    let mut index: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    let cell = |v: f64| (v / 2.5).floor() as i64;
    for (i, s) in hs.iter().enumerate() {
        let (ka, kb) = (cell(s.a), cell(s.b));
        let hit = (-1..=1)
            .flat_map(|da: i64| (-1..=1).map(move |db: i64| (ka.saturating_add(da), kb.saturating_add(db))))
            .filter_map(|k| index.get(&k))
            .flatten()
            .copied()
            .filter(|&g| (groups[g].0 - s.a).abs() < 2.5 && (groups[g].1 - s.b).abs() < 2.5)
            .min();
        match hit {
            Some(g) => groups[g].2.push(i),
            None => {
                index.entry((ka, kb)).or_default().push(groups.len());
                groups.push((s.a, s.b, vec![i]));
            }
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
            let lines = group_lines(text, 2.0);
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
    Table { x0, top, bottom, cols: cols.to_vec(), rows, border, header, pad, from: None, until: None }
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
    // Il testo a passo fisso è già impaginato a spazi: ogni riga resta com'è.
    let mono = |l: &Line| l.runs.iter().all(|r| r.font == "Courier");
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
        if mono(group[0]) {
        } else if let Some(n) = is_bullet(&text) {
            p.marker = text.chars().next().map(Marker::Bullet);
            strip_front(&mut p.runs, n);
        } else {
            p.marker = number_prefix(&text);
        }
        out.push(p);
        group.clear();
    };
    for line in lines {
        let continues = !mono(line) && group.last().is_some_and(|prev| {
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
    let clusters = merge_touching(items.iter().map(|it| (it.b, it.pic)), page);
    let mut figs: Vec<Figure> = clusters
        .into_iter()
        .filter(|c| c.5 || (c.4 >= 5 && c.2 - c.0 >= 40.0 && c.3 - c.1 >= 40.0))
        .map(|c| Figure { x0: (c.0 - 2.0).max(0.0), y0: (c.1 - 2.0).max(0.0), x1: (c.2 + 2.0).min(page.0), y1: (c.3 + 2.0).min(page.1) })
        .collect();
    figs.sort_by(|a, b| a.y0.partial_cmp(&b.y0).unwrap_or(std::cmp::Ordering::Equal));
    figs
}

/// Un gruppo di riquadri che si toccano: estremi, quanti riquadri, se c'è un'immagine.
type Cluster = (f64, f64, f64, f64, usize, bool);

/// Unisce i riquadri che si toccano (entro 3 punti). Ogni riquadro cerca nella lista il primo
/// gruppo che tocca, lo toglie (`swap_remove`) e lo assorbe, finché ne tocca: un indice a celle
/// trova i gruppi vicini senza scorrere tutta la lista (le mappe e i disegni tecnici hanno decine
/// di migliaia di forme), con lo stesso risultato e lo stesso ordine.
fn merge_touching(items: impl Iterator<Item = ([f64; 4], bool)>, page: (f64, f64)) -> Vec<Cluster> {
    const DEAD: usize = usize::MAX;
    /// Oltre queste celle un gruppo va nella lista dei grandi, che si guarda sempre.
    const BIG: usize = 16;
    let touches = |c: &Cluster, cur: &Cluster| c.0 - 3.0 <= cur.2 && cur.0 - 3.0 <= c.2 && c.1 - 3.0 <= cur.3 && cur.1 - 3.0 <= c.3;
    // Un riquadro rovesciato o con NaN non si mette nelle celle.
    let sane = |c: &Cluster| c.0 <= c.2 && c.1 <= c.3;
    // Celle di 1/32 della pagina; quel che sta fuori va nelle celle del bordo.
    let side = page.0.max(page.1).max(1.0) / 32.0;
    let nx = ((page.0 / side).ceil() as usize).clamp(1, 64);
    let ny = ((page.1 / side).ceil() as usize).clamp(1, 64);
    let cell = |v: f64, n: usize| ((v / side).floor().max(0.0) as usize).min(n - 1);
    let range = |c: &Cluster| (cell(c.0 - 3.0, nx), cell(c.2 + 3.0, nx), cell(c.1 - 3.0, ny), cell(c.3 + 3.0, ny));
    let mut grid: Vec<Vec<usize>> = vec![Vec::new(); nx * ny];
    let mut big: Vec<usize> = Vec::new();
    // La lista nell'ordine voluto, con l'id di ogni gruppo; `pos`: da id a posto nella lista.
    let mut clusters: Vec<(Cluster, usize)> = Vec::new();
    let mut pos: Vec<usize> = Vec::new();
    for (b, pic) in items {
        let mut cur: Cluster = (b[0], b[1], b[2], b[3], 1, pic);
        loop {
            let (x0, x1, y0, y1) = range(&cur);
            let cells = (x1 + 1).saturating_sub(x0) * (y1 + 1).saturating_sub(y0);
            let hit = if !sane(&cur) || cells + big.len() >= clusters.len() {
                clusters.iter().position(|(c, _)| touches(c, &cur))
            } else {
                let mut best: Option<usize> = None;
                let mut look = |list: &mut Vec<usize>| {
                    list.retain(|id| pos[*id] != DEAD);
                    for id in list.iter() {
                        let p = pos[*id];
                        if best.is_none_or(|b| p < b) && touches(&clusters[p].0, &cur) {
                            best = Some(p);
                        }
                    }
                };
                look(&mut big);
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        look(&mut grid[y * nx + x]);
                    }
                }
                best
            };
            let Some(p) = hit else { break };
            let (c, id) = clusters.swap_remove(p);
            pos[id] = DEAD;
            if let Some((_, moved)) = clusters.get(p) {
                pos[*moved] = p;
            }
            cur = (cur.0.min(c.0), cur.1.min(c.1), cur.2.max(c.2), cur.3.max(c.3), cur.4 + c.4, cur.5 || c.5);
        }
        let id = pos.len();
        pos.push(clusters.len());
        let (x0, x1, y0, y1) = range(&cur);
        if !sane(&cur) || (x1 + 1 - x0) * (y1 + 1 - y0) > BIG {
            big.push(id);
        } else {
            for y in y0..=y1 {
                for x in x0..=x1 {
                    grid[y * nx + x].push(id);
                }
            }
        }
        clusters.push((cur, id));
    }
    clusters.into_iter().map(|(c, _)| c).collect()
}

/// Le scritte attaccate a un grafico (valori degli assi, etichette) fanno parte del grafico.
fn absorb_labels(figs: &mut [Figure], glyphs: &[Glyph], body: f64) {
    if figs.is_empty() {
        return;
    }
    let lines = group_lines(glyphs.to_vec(), 2.0);
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
    // Un vettore per ogni punto di larghezza: nessuna pagina vera ha colonne più larghe di 7 m.
    if !width.is_finite() || width > 20_000.0 {
        return Vec::new();
    }
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
    /// Pagina senza testo da copiare (una scansione): va nel Word come immagine.
    Scan(ScanPage),
    /// Modulo fitto di caselle (CU, 730, F24): righe e riquadri come sfondo, il testo sopra
    /// ciascuno al suo posto.
    Form(FormPage),
    /// Pagina finita dentro la precedente (una tabella che continua).
    Merged,
}

pub struct FormPage {
    width: f64,
    height: f64,
    /// Righe, riquadri e loghi, ridisegnati in Word come forme.
    shapes: Vec<Shape2>,
    /// Le immagini vere della pagina, con la loro posizione.
    pics: Vec<(Pic, Vec<u8>, &'static str)>,
    lines: Vec<Line>,
}

/// Una pagina di modulo: tante scritte brevi e piccole dentro tante caselle.
fn is_form(lines: &[Line], shapes: &[Shape2]) -> bool {
    let boxes = shapes.iter().filter(|s| s.rect || s.line).count();
    let small = lines.iter().filter(|l| l.size < 7.0).count();
    let chars = median(lines.iter().map(|l| l.text().chars().count() as f64).collect());
    lines.len() >= 80 && boxes >= 40 && chars <= 10.0 && small * 10 >= lines.len() * 3
}

pub struct ScanPage {
    width: f64,
    height: f64,
    jpeg: Vec<u8>,
}

impl Page {
    /// La pagina è una scansione, senza testo da copiare.
    pub fn is_scan(&self) -> bool {
        matches!(self, Page::Scan(_))
    }
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

/// Quel che si raccoglie da una pagina: testo, disegni, immagini.
struct Gathered {
    w: f64,
    h: f64,
    glyphs: Vec<Glyph>,
    shapes: Vec<Shape2>,
    pics: Vec<Pic>,
}

fn gather<'a>(page: &'a hayro::hayro_syntax::page::Page<'a>, interp_cache: &InterpreterCache<'a>, settings: &InterpreterSettings) -> Gathered {
    let (w, h) = page.render_dimensions();
    let (w, h) = (w as f64, h as f64);
    let ctm = page.initial_transform(true).to_kurbo();
    let mut ctx = Context::new(ctm, Rect::new(0.0, 0.0, w, h), interp_cache, page.xref(), settings.clone());
    let mut collector = Collector::default();
    interpret_page(page, &mut ctx, &mut collector);
    let Collector { mut glyphs, shapes, pics, stems, .. } = collector;
    // Il testo scritto fuori dal foglio non si vede: non va nel Word.
    glyphs.retain(|g| g.x >= -1.0 && g.x < w && g.y > 0.0 && g.y - g.size < h);
    for g in &mut glyphs {
        // Un carattere non è più largo della pagina (Tz o Tm esagerati in un PDF ostile).
        g.adv = g.adv.min(w).max(0.0);
        if let (Some(key), true) = (g.font_key, g.font.is_empty()) {
            if let Some((sum, n)) = stems.get(&key) {
                g.bold = *n > 0 && sum / *n as f64 >= 96.0;
            }
        }
    }
    // Un font senza nome a passo fisso (il Courier dei moduli): ogni carattere largo 0,6 em.
    let mut mono: HashMap<u128, (usize, usize)> = HashMap::new();
    for g in glyphs.iter().filter(|g| g.font.is_empty()) {
        if let Some(key) = g.font_key {
            let e = mono.entry(key).or_default();
            e.1 += 1;
            if (g.adv / (g.size * g.stretch as f64 / 100.0) - 0.6).abs() < 0.02 {
                e.0 += 1;
            }
        }
    }
    for g in glyphs.iter_mut().filter(|g| g.font.is_empty()) {
        if let Some((m, n)) = g.font_key.and_then(|k| mono.get(&k)) {
            if *n >= 5 && m * 10 >= n * 9 {
                g.font = "Courier".into();
            }
        }
    }
    Gathered { w, h, glyphs, shapes, pics }
}

/// Cosa ricostruire leggendo il PDF (le opzioni di «PDF in Word»).
pub struct ReadOptions {
    /// Le tabelle diventano tabelle di Word; spenta, il loro testo resta in paragrafi.
    pub tables: bool,
    /// Le colonne restano colonne; spenta, una colonna sola in ordine di lettura.
    pub columns: bool,
    /// Una tabella che continua sulla pagina dopo diventa una tabella sola.
    pub merge: bool,
}

impl Default for ReadOptions {
    fn default() -> Self {
        ReadOptions { tables: true, columns: true, merge: true }
    }
}

/// Legge le pagine del PDF. `Page::Scan` per quelle senza testo incorporato.
/// Le pagine per cui `want` (da 0) è falso non si leggono neppure: restano `Page::Merged`.
pub fn read_pages(bytes: &[u8], read: &ReadOptions, want: impl Fn(usize) -> bool) -> Result<Vec<Page>, String> {
    let original = Pdf::new(bytes.to_vec()).map_err(|e| format!("PDF non letto: {e:?}"))?;
    let cache = RenderCache::new();
    let interp_cache = InterpreterCache::new();
    let settings = InterpreterSettings::default();
    let mut raw: Vec<Option<RawPage>> = Vec::new();
    let mut special: Vec<Option<Page>> = Vec::new();
    let mut size_chars: HashMap<i64, usize> = HashMap::new();
    // Prima tutte le pagine: se il documento è un modulo, lo sono anche le sue pagine più semplici.
    let gathered: Vec<Option<Gathered>> =
        original.pages().iter().enumerate().map(|(i, page)| want(i).then(|| gather(page, &interp_cache, &settings))).collect();
    let text_pages = gathered.iter().flatten().filter(|g| !g.glyphs.is_empty()).count();
    // Una volta sola per pagina: serve qui per il documento e sotto per la pagina.
    let page_form: Vec<bool> =
        gathered.iter().map(|g| g.as_ref().is_some_and(|g| !g.glyphs.is_empty() && is_form(&group_lines(g.glyphs.clone(), 2.0), &g.shapes))).collect();
    let form_pages = page_form.iter().filter(|f| **f).count();
    let form_doc = form_pages > 0 && form_pages * 2 > text_pages;
    for ((page, g), page_is_form) in original.pages().iter().zip(gathered).zip(page_form) {
        let Some(Gathered { w, h, mut glyphs, mut shapes, pics }) = g else {
            raw.push(None);
            special.push(Some(Page::Merged));
            continue;
        };
        if glyphs.is_empty() {
            // Una scansione: la pagina intera diventa un'immagine, come la vede chi legge.
            let scale = (2000.0 / w.max(h).max(1.0)) as f32;
            let pixmap = PixmapSettings { x_scale: scale, y_scale: scale, bg_color: WHITE };
            let rendered = render(page, &cache, &settings, &RenderSettings::default(), &pixmap);
            // Una pagina lunghissima e sottile può dare una pixmap vuota: diventa una pagina bianca, non un errore.
            let rgb = crate::ocr::pixmap_image(rendered).map(|i| i.into_rgb8()).unwrap_or_default();
            if rgb.pixels().all(|p| p.0.iter().all(|&c| c >= 250)) {
                // Pagina bianca: resta una pagina vuota, non un'immagine.
                special.push(None);
                raw.push(Some(RawPage { width: w, height: h, regions: Vec::new(), others: Vec::new() }));
                continue;
            }
            let mut jpeg = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 80).encode_image(&rgb).map_err(|e| e.to_string())?;
            raw.push(None);
            special.push(Some(Page::Scan(ScanPage { width: w, height: h, jpeg })));
            continue;
        }
        if form_doc || page_is_form {
            let mut pics = pics;
            for s in shapes.iter().filter(|s| s.raster) {
                pics.push(Pic { x0: s.x0, y0: s.y0, x1: s.x1, y1: s.y1, order: s.order });
            }
            shapes.retain(|s| !s.raster);
            // Nel modulo ogni scritta sta al suo posto: le scritte vicine restano separate.
            let mut lines = group_lines(glyphs, 0.45);
            // Ogni scritta tiene in Word la larghezza del PDF: Word arrotonda il corpo ai mezzi
            // punti e non conosce la spaziatura del PDF, la differenza va tra i caratteri.
            for l in &mut lines {
                let chars = l.text().chars().count() as f64;
                let actual = l.end - l.x;
                let word_size = (l.size * 2.0).round().max(2.0) / 2.0;
                let expected = l.natural * word_size / l.size;
                if chars > 1.0 && l.runs.iter().all(|r| r.font != "Courier") && (actual - expected).abs() > 0.5 {
                    let t = (actual - expected) / chars;
                    for r in &mut l.runs {
                        r.tracking = t;
                    }
                }
            }
            // Le immagini vere si ritagliano dalla pagina disegnata senza testo.
            let mut cut = Vec::new();
            if !pics.is_empty() {
                // 200 dpi, ma una tavola A0 o una mappa non diventa una pixmap da gigabyte (come per le figure).
                let scale = (200.0 / 72.0f32).min(3200.0 / (w.max(h).max(1.0) as f32));
                let pixmap = PixmapSettings { x_scale: scale, y_scale: scale, bg_color: WHITE };
                let no_text = RenderSettings { skip_text: true, ..Default::default() };
                let rendered = render(page, &cache, &settings, &no_text, &pixmap);
                let full = crate::ocr::pixmap_image(rendered).map(|i| i.into_rgb8()).unwrap_or_default();
                for pic in pics {
                    let s = scale as f64;
                    let (x, y) = (((pic.x0.max(0.0)) * s) as u32, ((pic.y0.max(0.0)) * s) as u32);
                    if x >= full.width() || y >= full.height() {
                        continue;
                    }
                    let cw = (((pic.x1 - pic.x0) * s) as u32).min(full.width() - x).max(1);
                    let ch = (((pic.y1 - pic.y0) * s) as u32).min(full.height() - y).max(1);
                    let crop = image::imageops::crop_imm(&full, x, y, cw, ch).to_image();
                    let (data, ext) = encode_figure(&crop)?;
                    cut.push((pic, data, ext));
                }
            }
            raw.push(None);
            special.push(Some(Page::Form(FormPage { width: w, height: h, shapes, pics: cut, lines })));
            continue;
        }
        shapes.retain(|s| !s.text);
        special.push(None);
        let tables = if read.tables { find_tables(&shapes, (w, h), &mut glyphs) } else { Vec::new() };
        // Senza tabelle anche i riquadri e i bordi non diventano figure: il testo resta testo.
        let mut figures = find_figures(if read.tables { &shapes[..] } else { &[] }, &pics, &tables, (w, h));
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
            let full = crate::ocr::pixmap_image(rendered).map(|i| i.into_rgb8()).unwrap_or_default();
            for fig in figures.into_iter().filter(|_| full.width() > 0 && full.height() > 0) {
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
                let lines = r.glyphs.iter().map(|g| group_lines(g.clone(), 2.0)).collect();
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
    for (p, other) in raw.into_iter().zip(special) {
        let Some(p) = p else {
            pages.push(other.unwrap_or(Page::Merged));
            continue;
        };
        pages.push(Page::Flow(layout_page(p, &sizes, read.columns)));
    }
    if read.merge {
        merge_split_tables(&mut pages);
    }
    number_lists(&mut pages);
    Ok(pages)
}

/// `b` continua il paragrafo `a` (in fondo alla colonna prima): `a` non finisce la frase e `b`
/// comincia in minuscolo, senza essere un titolo o una voce d'elenco.
fn continues(a: &Para, b: &Para) -> bool {
    let last = a.runs.iter().rev().flat_map(|r| r.text.chars().rev()).find(|c| !c.is_whitespace());
    let first = b.runs.iter().flat_map(|r| r.text.chars()).find(|c| !c.is_whitespace());
    a.heading == 0
        && b.heading == 0
        && a.marker.is_none() == b.marker.is_none()
        && b.marker.is_none()
        && last.is_some_and(|c| !matches!(c, '.' | ':' | ';' | '!' | '?'))
        && first.is_some_and(char::is_lowercase)
}

fn join_para(a: &mut Para, b: Para) {
    if let Some(last) = a.runs.last_mut() {
        let mut rev = last.text.chars().rev();
        let hyphen = rev.next() == Some('-') && rev.next().is_some_and(char::is_alphabetic);
        if hyphen {
            last.text.pop();
        } else if !last.text.ends_with(' ') {
            last.text.push(' ');
        }
    }
    for r in b.runs {
        match a.runs.last_mut() {
            Some(l) if l.same_style(&r) => l.text.push_str(&r.text),
            _ => a.runs.push(r),
        }
    }
    a.end = a.end.max(b.end);
    a.bottom = b.bottom;
}

/// Sposta un blocco in orizzontale di `dx` punti.
fn shift_para(q: &mut Para, dx: f64) {
    q.x += dx;
    q.end += dx;
    q.first_x += dx;
    if let Some(t) = &mut q.text_x {
        *t += dx;
    }
    for t in &mut q.tabs {
        *t += dx;
    }
}

fn shift(b: &mut Block, dx: f64) {
    match b {
        Block::Para(q) => shift_para(q, dx),
        Block::Table(t) => {
            t.x0 += dx;
            for c in &mut t.cols {
                *c += dx;
            }
            for q in t.rows.iter_mut().flat_map(|r| r.1.iter_mut()).flat_map(|c| c.paras.iter_mut()) {
                shift_para(q, dx);
            }
        }
        Block::Image { fig, .. } => {
            fig.x0 += dx;
            fig.x1 += dx;
        }
    }
}

/// Mette in ordine di lettura paragrafi, tabelle e immagini di una pagina. Con `keep_columns`
/// spento le colonne diventano una colonna sola: prima tutta la sinistra, poi la destra.
fn layout_page(p: RawPage, sizes: &TextSizes, keep_columns: bool) -> FlowPage {
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
        if keep_columns {
            segments.push(Segment { cols, columns });
            continue;
        }
        // Una colonna sola: ogni colonna va sotto la precedente, allineata a sinistra come la prima.
        let mut flat: Vec<Block> = Vec::new();
        for (c, col) in cols.iter().zip(columns) {
            for (i, mut b) in col.into_iter().enumerate() {
                shift(&mut b, cols[0].0 - c.0);
                // Il paragrafo spezzato dal cambio di colonna torna intero.
                if let (0, Some(Block::Para(a)), Block::Para(q)) = (i, flat.last_mut(), &b) {
                    if continues(a, q) {
                        let Block::Para(q) = b else { unreachable!() };
                        join_para(a, q);
                        continue;
                    }
                }
                flat.push(b);
            }
        }
        match segments.last_mut().filter(|s| s.cols.is_empty()) {
            Some(prev) => prev.columns[0].extend(flat),
            None => segments.push(Segment { cols: Vec::new(), columns: vec![flat] }),
        }
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
        let last = j;
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
        t.until = Some(last);
        if next.segments[0].columns[0].is_empty() {
            next.segments.remove(0);
        }
        for b in next.segments.iter_mut().flat_map(|s| s.columns.iter_mut().flatten()) {
            if let Block::Table(t) = b {
                t.from.get_or_insert(last);
            }
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
    stretch: u16,
    /// Spazio in più tra i caratteri, in ventesimi di punto.
    spacing: i64,
    color: [u8; 3],
    bold: bool,
    italic: bool,
}

impl Look {
    fn of(r: &Run) -> Look {
        let half = (r.size * 2.0).round().max(2.0) as i64;
        // Word ha solo i mezzi punti: nel passo fisso la differenza si recupera tra i caratteri,
        // altrimenti le colonne scritte a spazi scivolano.
        let mono = if r.font == "Courier" { 0.6 * (r.size - half as f64 / 2.0) * r.stretch as f64 / 100.0 } else { 0.0 };
        let spacing = ((mono + r.tracking) * 20.0).round() as i64;
        Look { font: family(&r.font), half, stretch: r.stretch, spacing, color: r.color, bold: r.bold, italic: r.italic }
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
        if run.spacing != self.spacing {
            let _ = write!(out, "<w:spacing w:val=\"{}\"/>", run.spacing);
        }
        if run.stretch != self.stretch {
            let _ = write!(out, "<w:w w:val=\"{}\"/>", run.stretch);
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
        if self.spacing != 0 {
            let _ = write!(out, "<w:spacing w:val=\"{}\"/>", self.spacing);
        }
        if self.stretch != 100 {
            let _ = write!(out, "<w:w w:val=\"{}\"/>", self.stretch);
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
            .unwrap_or(Look { font: "Arial".into(), half: 22, stretch: 100, spacing: 0, color: [0, 0, 0], bold: false, italic: false });
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
    // Il testo a passo fisso è lungo esattamente come nel PDF: un po' di margine a destra
    // evita che l'ultima parola vada a capo per un arrotondamento.
    if p.runs.iter().all(|r| r.font == "Courier") {
        let extra = format!("w:right=\"-{}\"", twips(2.0 * p.size));
        match ppr.find("<w:ind ") {
            Some(at) => ppr.insert_str(at + 7, &format!("{extra} ")),
            None => {
                let _ = write!(ppr, "<w:ind {extra}/>");
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
/// La pagina intera come immagine dietro al testo.
fn page_picture(id: usize, ext: &str, width: f64, height: f64, number: usize, descr: &str) -> String {
    format!(
        "<w:p><w:pPr><w:spacing w:before=\"0\" w:after=\"0\" w:line=\"20\" w:lineRule=\"exact\"/></w:pPr>{}</w:p>",
        anchored(id, (0.0, 0.0, width, height), &format!("Pagina {number}"), descr, &picture_graphic(id, ext, width, height))
    )
}

/// Un oggetto dietro al testo, nel punto `(x, y)` della pagina, largo `w` e alto `h`.
fn anchored(id: usize, (x, y, w, h): (f64, f64, f64, f64), name: &str, descr: &str, graphic: &str) -> String {
    let emu = |v: f64| (v * EMU_PER_PT).round() as i64;
    let (cx, cy) = (emu(w).max(1), emu(h).max(1));
    format!(
        "<w:r><w:drawing><wp:anchor distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\" simplePos=\"0\" relativeHeight=\"{id}\" behindDoc=\"1\" locked=\"0\" layoutInCell=\"1\" allowOverlap=\"1\">\
<wp:simplePos x=\"0\" y=\"0\"/><wp:positionH relativeFrom=\"page\"><wp:posOffset>{}</wp:posOffset></wp:positionH>\
<wp:positionV relativeFrom=\"page\"><wp:posOffset>{}</wp:posOffset></wp:positionV><wp:extent cx=\"{cx}\" cy=\"{cy}\"/>\
<wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/><wp:wrapNone/><wp:docPr id=\"{id}\" name=\"{name}\" descr=\"{descr}\"/><wp:cNvGraphicFramePr/>\
<a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">{graphic}</a:graphic></wp:anchor></w:drawing></w:r>",
        emu(x),
        emu(y)
    )
}

fn picture_graphic(id: usize, ext: &str, w: f64, h: f64) -> String {
    let (cx, cy) = (((w * EMU_PER_PT) as i64).max(1), ((h * EMU_PER_PT) as i64).max(1));
    format!(
        "<a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
<pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:nvPicPr><pic:cNvPr id=\"{id}\" name=\"immagine{id}.{ext}\"/><pic:cNvPicPr/></pic:nvPicPr>\
<pic:blipFill><a:blip r:embed=\"rIdImg{id}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>\
<pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData>"
    )
}

/// Disegni consecutivi con lo stesso aspetto, come una sola forma di Word grande quanto la pagina:
/// righe e riquadri restano vettoriali e modificabili.
fn shapes_graphic(group: &[&Shape2], w: f64, h: f64) -> String {
    let emu = |v: f64| (v * EMU_PER_PT).round() as i64;
    let (cx, cy) = (emu(w), emu(h));
    let first = group[0];
    let mut paths = String::new();
    for s in group {
        let fill = if s.fill.is_some() { "" } else { " fill=\"none\"" };
        let stroke = if s.stroke.is_some() { "" } else { " stroke=\"0\"" };
        let _ = write!(paths, "<a:path w=\"{cx}\" h=\"{cy}\"{fill}{stroke}>");
        let pt = |p: &hayro::kurbo::Point| format!("<a:pt x=\"{}\" y=\"{}\"/>", emu(p.x), emu(p.y));
        for el in s.path.elements() {
            match el {
                PathEl::MoveTo(p) => {
                    let _ = write!(paths, "<a:moveTo>{}</a:moveTo>", pt(p));
                }
                PathEl::LineTo(p) => {
                    let _ = write!(paths, "<a:lnTo>{}</a:lnTo>", pt(p));
                }
                PathEl::QuadTo(a, b) => {
                    let _ = write!(paths, "<a:quadBezTo>{}{}</a:quadBezTo>", pt(a), pt(b));
                }
                PathEl::CurveTo(a, b, c) => {
                    let _ = write!(paths, "<a:cubicBezTo>{}{}{}</a:cubicBezTo>", pt(a), pt(b), pt(c));
                }
                PathEl::ClosePath => paths.push_str("<a:close/>"),
            }
        }
        paths.push_str("</a:path>");
    }
    let fill = match first.fill {
        Some(c) => format!("<a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>", hex(c)),
        None => "<a:noFill/>".to_string(),
    };
    let line = match first.stroke {
        Some((c, width)) => {
            let width = width.max(0.25);
            // Il tratteggio in Word si misura in centomillesimi dello spessore.
            let mut dash = String::new();
            if first.dash.iter().any(|d| *d > 0.0) {
                let mut ds = first.dash.clone();
                if ds.len() % 2 == 1 {
                    ds.extend(first.dash.clone());
                }
                dash.push_str("<a:custDash>");
                for pair in ds.chunks(2) {
                    let pct = |v: f64| ((v / width) * 100000.0).round().max(1.0) as i64;
                    let _ = write!(dash, "<a:ds d=\"{}\" sp=\"{}\"/>", pct(pair[0]), pct(pair[1]));
                }
                dash.push_str("</a:custDash>");
            }
            format!("<a:ln w=\"{}\"><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>{dash}</a:ln>", emu(width), hex(c))
        }
        None => "<a:ln><a:noFill/></a:ln>".to_string(),
    };
    format!(
        "<a:graphicData uri=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\"><wps:wsp><wps:cNvSpPr/><wps:spPr>\
<a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:custGeom><a:avLst/><a:gdLst/><a:ahLst/><a:cxnLst/>\
<a:rect l=\"0\" t=\"0\" r=\"r\" b=\"b\"/><a:pathLst>{paths}</a:pathLst></a:custGeom>{fill}{line}</wps:spPr><wps:bodyPr/></wps:wsp></a:graphicData>"
    )
}

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
    // I byte delle immagini restano nelle pagine: qui solo il nome nel Word e un riferimento.
    let mut media: Vec<(String, &[u8])> = Vec::new();
    let last_idx = pages.iter().rposition(|p| !matches!(p, Page::Merged));
    let mut rels = String::new();
    let mut body = String::new();
    let mut next_id = 1;
    for (i, page) in pages.iter().enumerate() {
        let last_page = last_idx == Some(i);
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
                                    media.push((format!("word/media/immagine{id}.{ext}"), data.as_slice()));
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
            Page::Scan(scan) => {
                // Come Foxit: la scansione occupa la pagina, dietro al testo. Il testo letto
                // con l'OCR va nel testo alternativo dell'immagine (si legge senza sporcare la pagina).
                let id = next_id;
                next_id += 1;
                media.push((format!("word/media/immagine{id}.jpg"), scan.jpeg.as_slice()));
                let _ = write!(
                    rels,
                    "<Relationship Id=\"rIdImg{id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/immagine{id}.jpg\"/>"
                );
                let ocr = fallback(i);
                // xml() toglie i caratteri di controllo: gli a capo tra i paragrafi si scrivono prima, come entità.
                let descr = ocr.split('\n').map(xml).collect::<Vec<_>>().join("&#10;");
                let picture = page_picture(id, "jpg", scan.width, scan.height, i + 1, &descr);
                let mut parts = vec![picture];
                let sect = sect_xml(&Sect { w: scan.width, h: scan.height, top: 20.0, bottom: 20.0, left: 20.0, right: 20.0, cols: Vec::new(), continuous: false });
                if last_page {
                    parts.push(sect);
                } else {
                    parts.push(format!("<w:p><w:pPr><w:spacing w:before=\"0\" w:after=\"0\" w:line=\"20\" w:lineRule=\"exact\"/>{sect}</w:pPr></w:p>"));
                }
                for part in parts {
                    body.push_str(&part);
                }
            }
            Page::Form(form) => {
                // Disegni e immagini nell'ordine del PDF, dietro al testo.
                enum Item<'a> {
                    Shape(&'a Shape2),
                    Pic(&'a (Pic, Vec<u8>, &'static str)),
                }
                let mut items: Vec<(usize, Item)> = form.shapes.iter().map(|s| (s.order, Item::Shape(s))).collect();
                items.extend(form.pics.iter().map(|p| (p.0.order, Item::Pic(p))));
                items.sort_by_key(|(o, _)| *o);
                let mut runs = String::new();
                let mut group: Vec<&Shape2> = Vec::new();
                let same = |a: &Shape2, b: &Shape2| {
                    a.fill == b.fill && a.dash == b.dash && a.stroke.map(|s| (s.0, (s.1 * 20.0).round() as i64)) == b.stroke.map(|s| (s.0, (s.1 * 20.0).round() as i64))
                };
                let flush = |group: &mut Vec<&Shape2>, runs: &mut String, next_id: &mut usize| {
                    if group.is_empty() {
                        return;
                    }
                    let id = *next_id;
                    *next_id += 1;
                    runs.push_str(&anchored(id, (0.0, 0.0, form.width, form.height), &format!("Disegno {id}"), "", &shapes_graphic(group, form.width, form.height)));
                    group.clear();
                };
                for (_, item) in items {
                    match item {
                        Item::Shape(s) => {
                            if group.last().is_some_and(|g| !same(g, s)) || group.len() >= 400 {
                                flush(&mut group, &mut runs, &mut next_id);
                            }
                            group.push(s);
                        }
                        Item::Pic((pic, data, ext)) => {
                            flush(&mut group, &mut runs, &mut next_id);
                            let id = next_id;
                            next_id += 1;
                            media.push((format!("word/media/immagine{id}.{ext}"), data.as_slice()));
                            let _ = write!(
                                rels,
                                "<Relationship Id=\"rIdImg{id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/immagine{id}.{ext}\"/>"
                            );
                            let (w, h) = (pic.x1 - pic.x0, pic.y1 - pic.y0);
                            runs.push_str(&anchored(id, (pic.x0, pic.y0, w, h), &format!("Immagine {id}"), "", &picture_graphic(id, ext, w, h)));
                        }
                    }
                }
                flush(&mut group, &mut runs, &mut next_id);
                let _ = write!(body, "<w:p><w:pPr><w:spacing w:before=\"0\" w:after=\"0\" w:line=\"20\" w:lineRule=\"exact\"/></w:pPr>{runs}</w:p>");
                // Ogni scritta è un riquadro di testo nel punto esatto della pagina.
                for l in &form.lines {
                    let line_h = l.size * 1.2;
                    let top = l.baseline - line_h + 0.22 * l.size;
                    let width = (l.end - l.x) * 1.15 + l.size;
                    let _ = write!(
                        body,
                        "<w:p><w:pPr><w:framePr w:w=\"{}\" w:wrap=\"none\" w:hAnchor=\"page\" w:vAnchor=\"page\" w:x=\"{}\" w:y=\"{}\"/>\
<w:spacing w:before=\"0\" w:after=\"0\" w:line=\"{}\" w:lineRule=\"exact\"/></w:pPr>",
                        twips(width),
                        twips(l.x),
                        twips(top.max(0.0)),
                        twips(line_h)
                    );
                    for r in &l.runs {
                        body.push_str(&run_xml(r, &styles.body));
                    }
                    body.push_str("</w:p>");
                }
                let sect = sect_xml(&Sect { w: form.width, h: form.height, top: 0.0, bottom: 0.0, left: 0.0, right: 0.0, cols: Vec::new(), continuous: false });
                if last_page {
                    body.push_str(&sect);
                } else {
                    let _ = write!(body, "<w:p><w:pPr><w:spacing w:before=\"0\" w:after=\"0\" w:line=\"20\" w:lineRule=\"exact\"/>{sect}</w:pPr></w:p>");
                }
            }
        }
    }
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" \
xmlns:wps=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\"><w:body>{body}</w:body></w:document>"
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
        entries.push((name.as_str(), *data));
    }
    zip_archive(&entries)
}

// ---------------------------------------------------------------- fogli di calcolo

/// Una cella per il foglio di calcolo: il testo e quante colonne occupa.
pub struct SheetCell {
    pub text: String,
    pub span: usize,
}

/// Una tabella del PDF pronta per il foglio di calcolo.
pub struct SheetTable {
    /// Pagine del PDF (da 0) dove comincia e dove finisce.
    pub pages: (usize, usize),
    pub rows: Vec<Vec<SheetCell>>,
    /// La prima riga è l'intestazione.
    pub header: bool,
    /// Tabella con i bordi; falsa per le colonne di testo allineate senza bordi.
    pub ruled: bool,
}

/// Il testo di una pagina fuori dalle tabelle, riga per riga.
pub struct SheetText {
    pub page: usize,
    pub lines: Vec<String>,
}

fn para_text(p: &Para) -> String {
    let mut text: String = p.runs.iter().map(|r| r.text.as_str()).collect();
    if let Some(Marker::Number(n, NumFmt::Decimal, delim, _)) = p.marker {
        text = format!("{n}{delim} {text}");
    }
    text.trim().to_string()
}

fn para_bold(p: &Para) -> bool {
    let mut words = p.runs.iter().filter(|r| !r.text.trim().is_empty()).peekable();
    words.peek().is_some() && words.all(|r| r.bold)
}

/// Una riga di testo a pezzi staccati (colonne senza bordi): dove comincia ogni pezzo e cosa dice.
struct Tabbed {
    starts: Vec<f64>,
    cells: Vec<String>,
    bold: bool,
}

fn tabbed(p: &Para) -> Option<Tabbed> {
    if p.tabs.is_empty() || p.marker.is_some() {
        return None;
    }
    let text: String = p.runs.iter().map(|r| r.text.as_str()).collect();
    let cells: Vec<String> = text.split('\t').map(|c| c.trim().to_string()).collect();
    if cells.len() != p.tabs.len() + 1 {
        return None;
    }
    let mut starts = vec![p.first_x];
    starts.extend(&p.tabs);
    Some(Tabbed { starts, cells, bold: para_bold(p) })
}

/// Righe a pezzi staccati una sotto l'altra: diventano una tabella se sono almeno tre
/// e almeno due hanno lo stesso numero di colonne, il massimo.
fn columns_table(rows: &[Tabbed], page: usize) -> Option<SheetTable> {
    let width = rows.iter().map(|r| r.cells.len()).max()?;
    let full: Vec<&Tabbed> = rows.iter().filter(|r| r.cells.len() == width).collect();
    if rows.len() < 3 || full.len() < 2 || width < 2 {
        return None;
    }
    // Dove cominciano le colonne: la mediana degli inizi nelle righe complete.
    let anchors: Vec<f64> = (0..width).map(|k| median(full.iter().map(|r| r.starts[k]).collect())).collect();
    let mut out = Vec::new();
    for r in rows {
        let mut line: Vec<String> = vec![String::new(); width];
        if r.cells.len() == width {
            line = r.cells.clone();
        } else {
            // Ogni pezzo nella colonna più vicina, sempre a destra del pezzo prima.
            let mut next = 0;
            for (k, (x, text)) in r.starts.iter().zip(&r.cells).enumerate() {
                let left = r.cells.len() - k - 1;
                let last = width - 1 - left;
                let col = (next..=last.max(next))
                    .min_by(|a, b| (anchors[*a] - x).abs().partial_cmp(&(anchors[*b] - x).abs()).unwrap_or(std::cmp::Ordering::Equal))
                    .unwrap_or(next);
                line[col] = text.clone();
                next = col + 1;
            }
        }
        out.push(line.into_iter().map(|text| SheetCell { text, span: 1 }).collect());
    }
    let header = rows[0].bold && rows.get(1).is_some_and(|r| !r.bold);
    Some(SheetTable { pages: (page, page), rows: out, header, ruled: false })
}

fn ruled_table(t: &Table, page: usize) -> SheetTable {
    let rows: Vec<Vec<SheetCell>> = t
        .rows
        .iter()
        .map(|(_, cells)| {
            cells
                .iter()
                .map(|c| SheetCell { text: c.paras.iter().map(para_text).collect::<Vec<_>>().join("\n"), span: c.span })
                .collect()
        })
        .collect();
    let bold_top = t.rows.first().is_some_and(|(_, cells)| {
        let mut full = cells.iter().filter(|c| !c.paras.is_empty()).peekable();
        full.peek().is_some() && full.all(|c| c.paras.iter().all(para_bold))
    });
    let header = t.rows.len() > 1 && (t.header || bold_top);
    let first = t.from.unwrap_or(page);
    SheetTable { pages: (first, t.until.unwrap_or(first)), rows, header, ruled: true }
}

/// Le tabelle del PDF per il foglio di calcolo, nell'ordine del documento, e il testo che resta
/// fuori. `scan_text` dà il testo letto con l'OCR delle pagine scansionate (vuoto se non c'è).
pub fn sheet_contents(pages: &[Page], scan_text: impl Fn(usize) -> String) -> (Vec<SheetTable>, Vec<SheetText>) {
    let mut tables = Vec::new();
    let mut texts = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        let mut lines: Vec<String> = Vec::new();
        let mut run: Vec<Tabbed> = Vec::new();
        let mut run_lines: Vec<String> = Vec::new();
        let mut flush = |run: &mut Vec<Tabbed>, run_lines: &mut Vec<String>, lines: &mut Vec<String>, tables: &mut Vec<SheetTable>| {
            match columns_table(run, i) {
                Some(t) => tables.push(t),
                None => lines.append(run_lines),
            }
            run.clear();
            run_lines.clear();
        };
        match page {
            Page::Flow(flow) => {
                for seg in &flow.segments {
                    for col in &seg.columns {
                        for b in col {
                            match b {
                                Block::Para(p) => match tabbed(p) {
                                    Some(t) => {
                                        run_lines.push(t.cells.join(" "));
                                        run.push(t);
                                    }
                                    None => {
                                        flush(&mut run, &mut run_lines, &mut lines, &mut tables);
                                        let text = para_text(p);
                                        if !text.is_empty() {
                                            lines.push(text);
                                        }
                                    }
                                },
                                Block::Table(t) => {
                                    flush(&mut run, &mut run_lines, &mut lines, &mut tables);
                                    tables.push(ruled_table(t, i));
                                }
                                Block::Image { .. } => flush(&mut run, &mut run_lines, &mut lines, &mut tables),
                            }
                        }
                        flush(&mut run, &mut run_lines, &mut lines, &mut tables);
                    }
                }
            }
            Page::Form(form) => {
                for l in join_tabbed(&form.lines) {
                    let text = l.text();
                    if l.tabs.is_empty() {
                        flush(&mut run, &mut run_lines, &mut lines, &mut tables);
                        if !text.trim().is_empty() {
                            lines.push(text.trim().to_string());
                        }
                        continue;
                    }
                    let cells: Vec<String> = text.split('\t').map(|c| c.trim().to_string()).collect();
                    if cells.len() != l.tabs.len() + 1 {
                        continue;
                    }
                    let mut starts = vec![l.x];
                    starts.extend(&l.tabs);
                    run_lines.push(cells.join(" "));
                    run.push(Tabbed { starts, cells, bold: l.all_bold() });
                }
                flush(&mut run, &mut run_lines, &mut lines, &mut tables);
            }
            Page::Scan(_) => lines.extend(scan_text(i).lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from)),
            Page::Merged => {}
        }
        if !lines.is_empty() {
            texts.push(SheetText { page: i, lines });
        }
    }
    (tables, texts)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Il modo di prima: ogni riquadro confrontato con tutti i gruppi della lista.
    fn merge_by_scanning(items: &[([f64; 4], bool)]) -> Vec<Cluster> {
        let mut clusters: Vec<Cluster> = Vec::new();
        for (b, pic) in items {
            let mut cur = (b[0], b[1], b[2], b[3], 1usize, *pic);
            while let Some(i) = clusters.iter().position(|c| c.0 - 3.0 <= cur.2 && cur.0 - 3.0 <= c.2 && c.1 - 3.0 <= cur.3 && cur.1 - 3.0 <= c.3) {
                let c = clusters.swap_remove(i);
                cur = (cur.0.min(c.0), cur.1.min(c.1), cur.2.max(c.2), cur.3.max(c.3), cur.4 + c.4, cur.5 || c.5);
            }
            clusters.push(cur);
        }
        clusters
    }

    #[test]
    fn grouping_touching_shapes_with_cells_gives_the_same_groups_in_the_same_order() {
        // Numeri pseudo-casuali ripetibili.
        let mut seed = 12345u64;
        let mut next = move || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as f64 / (1u64 << 31) as f64
        };
        for (n, small) in [(50, 30.0), (2000, 4.0), (3000, 12.0), (500, 120.0)] {
            let mut items = Vec::new();
            for k in 0..n {
                let (x, y) = (next() * 700.0 - 50.0, next() * 900.0 - 30.0);
                let (w, h) = (next() * small, next() * small);
                // Ogni tanto uno grande, uno rovesciato o uno con NaN.
                let b = match k % 97 {
                    0 => [x, y, x + 400.0, y + 300.0],
                    1 => [x + w, y, x, y + h],
                    2 => [f64::NAN, y, x, y + h],
                    _ => [x, y, x + w, y + h],
                };
                items.push((b, k % 13 == 0));
            }
            let fast = merge_touching(items.iter().copied(), (595.0, 842.0));
            let slow = merge_by_scanning(&items);
            assert_eq!(format!("{fast:?}"), format!("{slow:?}"), "n = {n}");
        }
    }

    #[test]
    fn absurdly_wide_text_does_not_allocate_a_column_per_point() {
        let spans = vec![vec![(0.0, 10.0), (1.0e12, 1.0e12 + 5.0)]; 8];
        let heights = vec![(10.0, 10.0); 8];
        assert!(column_bands(&spans, &heights, 0.0, 1.0e12 + 5.0).is_empty());
        assert!(column_bands(&spans, &heights, 0.0, f64::INFINITY).is_empty());
    }

    #[test]
    fn monospaced_gaps_with_absurd_coordinates_stay_small() {
        let g = |text: &str, x: f64| Glyph {
            text: text.into(),
            x,
            y: 100.0,
            size: 10.0,
            adv: 6.0,
            stretch: 1,
            color: [0; 3],
            bold: false,
            italic: false,
            font: "Courier".into(),
            font_key: None,
        };
        let lines = group_lines(vec![g("a", 10.0), g("b", 16.0), g("c", 1.0e30)], 2.0);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].text().len() <= 1003, "{}", lines[0].text().len());
    }

    #[test]
    fn ocr_text_of_a_scan_keeps_its_paragraphs() {
        let pages = vec![Page::Scan(ScanPage { width: 595.0, height: 842.0, jpeg: vec![0xFF, 0xD8, 0xFF, 0xD9] })];
        let docx = docx(&pages, |_| "Fine del primo.\nInizio <del> secondo".to_string());
        let dir = std::env::temp_dir().join(format!("cedro-descr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("scan.docx");
        std::fs::write(&path, docx).unwrap();
        let document = crate::doctext::zip_entry_for_test(&path, "word/document.xml");
        assert!(document.contains("descr=\"Fine del primo.&#10;Inizio &lt;del&gt; secondo\""), "{document}");
        let _ = std::fs::remove_dir_all(dir);
    }
}
