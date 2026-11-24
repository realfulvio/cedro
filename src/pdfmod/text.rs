//! Il testo di una pagina: ogni carattere con la sua posizione (leggendo i flussi del contenuto con le
//! stesse regole del PDF), poi le righe, le parole e i paragrafi. Serve a scegliere cosa evidenziare, a
//! cercare, a modificare un paragrafo e a oscurare davvero: i caratteri si tolgono dal flusso, uno per uno.

use std::collections::HashMap;

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use super::fonts::{self, dict_of, num, FontInfo};
use super::geom::{apply as pt, mul, Geom, Mat, IDENT};
use super::{Color, Rect};

/// Dove sta un'operazione: nel contenuto della pagina o dentro un oggetto «Form» che la pagina disegna.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Cont {
    Page,
    Form(ObjectId),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// Un carattere della pagina, in punti della pagina vista.
#[derive(Clone, Debug)]
pub struct Glyph {
    pub ch: String,
    pub x0: f32,
    pub x1: f32,
    pub top: f32,
    pub bot: f32,
    /// La linea su cui sta (y).
    pub base: f32,
    /// Il corpo, come si vede.
    pub size: f32,
    pub font: usize,
    pub color: Color,
    pub cont: Cont,
    pub op: usize,
    pub gi: usize,
    pub horizontal: bool,
    /// Testo invisibile (lo strato dell'OCR sotto una scansione).
    pub invisible: bool,
}

impl Glyph {
    pub fn rect(&self) -> Rect {
        Rect::new(self.x0, self.top, self.x1 - self.x0, self.bot - self.top)
    }

    pub fn is_space(&self) -> bool {
        self.ch.chars().all(char::is_whitespace)
    }
}

/// Come è impostato un'operazione che mostra testo (serve per togliere i caratteri).
#[derive(Clone, Debug)]
pub struct ShowMeta {
    pub font: usize,
    pub fs: f32,
    pub tc: f32,
    pub tw: f32,
}

/// Un'immagine disegnata sulla pagina: i suoi quattro angoli (in punti della pagina vista).
#[derive(Clone, Debug)]
pub struct ImageUse {
    pub cont: Cont,
    pub op: usize,
    pub id: Option<ObjectId>,
    /// Angoli dell'immagine: in basso a sinistra, in basso a destra, in alto a destra, in alto a sinistra.
    pub quad: [(f32, f32); 4],
}

#[derive(Clone, Debug)]
pub struct Line {
    pub glyphs: Vec<usize>,
    pub text: String,
    pub x0: f32,
    pub x1: f32,
    pub top: f32,
    pub bot: f32,
    pub base: f32,
    pub size: f32,
    pub invisible: bool,
    /// Quanti caratteri all'inizio sono il segno di un elenco («•», «1.», «a)») con lo spazio dopo.
    pub marker: usize,
}

#[derive(Clone, Debug)]
pub struct Word {
    pub text: String,
    pub rect: Rect,
    pub line: usize,
    /// Primo e ultimo glifo (posti in `TextPage::glyphs`).
    pub first: usize,
    pub last: usize,
}

/// Perché un paragrafo non si può modificare.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Why {
    Ok,
    /// È lo strato di testo di una scansione: si cerca e si evidenzia, ma cambia la scrittura sopra.
    Scan,
    /// Il carattere del file non dice che lettera è ogni segno.
    Font,
    Rotated,
}

#[derive(Clone, Debug)]
pub struct TextBlock {
    pub rect: Rect,
    pub text: String,
    /// Per ogni carattere di `text`, il glifo da cui viene (spazi e a capo aggiunti: `None`).
    pub cmap: Vec<Option<usize>>,
    pub lines: Vec<usize>,
    /// I caratteri del paragrafo (senza il segno dell'elenco, che resta dov'è).
    pub glyphs: Vec<usize>,
    /// Il segno dell'elenco, se c'è («•», «1.»…).
    pub marker: String,
    pub size: f32,
    pub color: Color,
    pub font: usize,
    pub font_name: String,
    pub bold: bool,
    pub italic: bool,
    pub align: Align,
    pub line_h: f32,
    pub why: Why,
}

impl TextBlock {
    pub fn editable(&self) -> bool {
        self.why == Why::Ok
    }
}

#[derive(Clone, Debug, Default)]
pub struct TextPage {
    pub w: f32,
    pub h: f32,
    pub fonts: Vec<FontInfo>,
    pub glyphs: Vec<Glyph>,
    pub lines: Vec<Line>,
    pub words: Vec<Word>,
    pub blocks: Vec<TextBlock>,
    pub ops: HashMap<Cont, Vec<Operation>>,
    pub meta: HashMap<(Cont, usize), ShowMeta>,
    pub images: Vec<ImageUse>,
    /// I flussi non si sono letti del tutto: la pagina non si modifica.
    pub complete: bool,
}

#[derive(Clone)]
struct St {
    ctm: Mat,
    fill: Color,
    font: Option<usize>,
    fs: f32,
    tc: f32,
    tw: f32,
    th: f32,
    tl: f32,
    rise: f32,
    mode: i64,
    tm: Mat,
    tlm: Mat,
}

impl Default for St {
    fn default() -> St {
        St { ctm: IDENT, fill: [0, 0, 0], font: None, fs: 0.0, tc: 0.0, tw: 0.0, th: 1.0, tl: 0.0, rise: 0.0, mode: 0, tm: IDENT, tlm: IDENT }
    }
}

pub fn cmyk(c: f32, m: f32, y: f32, k: f32) -> Color {
    let f = |v: f32| ((1.0 - v.clamp(0.0, 1.0)) * (1.0 - k.clamp(0.0, 1.0)) * 255.0).round() as u8;
    [f(c), f(m), f(y)]
}

fn gray(v: f32) -> Color {
    let g = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    [g, g, g]
}

fn nums(ops: &[Object]) -> Vec<f32> {
    ops.iter().filter_map(num).collect()
}

/// Le risorse di una pagina, anche se le eredita dalle pagine sopra.
pub fn page_resources(doc: &Document, page: ObjectId) -> Dictionary {
    let mut node = Some(page);
    for _ in 0..32 {
        let Some(id) = node else { break };
        let Ok(d) = doc.get_dictionary(id) else { break };
        if let Ok(r) = d.get(b"Resources") {
            if let Some(r) = dict_of(doc, r) {
                return r.clone();
            }
        }
        node = d.get(b"Parent").ok().and_then(|p| p.as_reference().ok());
    }
    Dictionary::new()
}

pub fn decode_ops(bytes: &[u8]) -> Option<Vec<Operation>> {
    Content::decode(bytes).ok().map(|c| c.operations)
}

struct Reader<'a> {
    doc: &'a Document,
    geom: Geom,
    page: &'a mut TextPage,
    by_id: HashMap<ObjectId, usize>,
    by_name: HashMap<(Cont, Vec<u8>), usize>,
    budget: usize,
}

impl Reader<'_> {
    fn font_of(&mut self, res: &Dictionary, cont: Cont, name: &[u8]) -> Option<usize> {
        let fonts_dict = res.get(b"Font").ok().and_then(|o| dict_of(self.doc, o))?;
        let entry = fonts_dict.get(name).ok()?;
        if let Object::Reference(id) = entry {
            if let Some(&i) = self.by_id.get(id) {
                return Some(i);
            }
            let d = self.doc.get_dictionary(*id).ok()?;
            let info = fonts::load(self.doc, d, name, Some(*id));
            self.page.fonts.push(info);
            self.by_id.insert(*id, self.page.fonts.len() - 1);
            return Some(self.page.fonts.len() - 1);
        }
        if let Some(&i) = self.by_name.get(&(cont, name.to_vec())) {
            return Some(i);
        }
        let d = dict_of(self.doc, entry)?;
        let info = fonts::load(self.doc, d, name, None);
        self.page.fonts.push(info);
        self.by_name.insert((cont, name.to_vec()), self.page.fonts.len() - 1);
        Some(self.page.fonts.len() - 1)
    }

    fn walk(&mut self, cont: Cont, ops: &[Operation], res: &Dictionary, ctm0: Mat, depth: usize) {
        let mut st = St { ctm: ctm0, ..St::default() };
        let mut stack: Vec<St> = Vec::new();
        for (idx, op) in ops.iter().enumerate() {
            if self.budget == 0 {
                self.page.complete = false;
                return;
            }
            let a = &op.operands;
            let n = nums(a);
            match op.operator.as_str() {
                "q" => stack.push(st.clone()),
                "Q" => {
                    if let Some(s) = stack.pop() {
                        // Le matrici del testo non fanno parte dello stato che si salva.
                        let (tm, tlm) = (st.tm, st.tlm);
                        st = s;
                        st.tm = tm;
                        st.tlm = tlm;
                    }
                }
                "cm" if n.len() == 6 => st.ctm = mul(&[n[0], n[1], n[2], n[3], n[4], n[5]], &st.ctm),
                "BT" => {
                    st.tm = IDENT;
                    st.tlm = IDENT;
                }
                "Tf" if a.len() == 2 => {
                    st.font = a[0].as_name().ok().and_then(|nm| self.font_of(res, cont, nm));
                    st.fs = num(&a[1]).unwrap_or(0.0);
                }
                "Tc" if n.len() == 1 => st.tc = n[0],
                "Tw" if n.len() == 1 => st.tw = n[0],
                "Tz" if n.len() == 1 => st.th = n[0] / 100.0,
                "TL" if n.len() == 1 => st.tl = n[0],
                "Ts" if n.len() == 1 => st.rise = n[0],
                "Tr" if n.len() == 1 => st.mode = n[0] as i64,
                "Td" if n.len() == 2 => {
                    st.tlm = mul(&[1.0, 0.0, 0.0, 1.0, n[0], n[1]], &st.tlm);
                    st.tm = st.tlm;
                }
                "TD" if n.len() == 2 => {
                    st.tl = -n[1];
                    st.tlm = mul(&[1.0, 0.0, 0.0, 1.0, n[0], n[1]], &st.tlm);
                    st.tm = st.tlm;
                }
                "Tm" if n.len() == 6 => {
                    st.tm = [n[0], n[1], n[2], n[3], n[4], n[5]];
                    st.tlm = st.tm;
                }
                "T*" => {
                    st.tlm = mul(&[1.0, 0.0, 0.0, 1.0, 0.0, -st.tl], &st.tlm);
                    st.tm = st.tlm;
                }
                "g" if n.len() == 1 => st.fill = gray(n[0]),
                "rg" if n.len() == 3 => st.fill = [(n[0].clamp(0.0, 1.0) * 255.0).round() as u8, (n[1].clamp(0.0, 1.0) * 255.0).round() as u8, (n[2].clamp(0.0, 1.0) * 255.0).round() as u8],
                "k" if n.len() == 4 => st.fill = cmyk(n[0], n[1], n[2], n[3]),
                "sc" | "scn" if a.iter().all(|o| num(o).is_some()) => match n.len() {
                    1 => st.fill = gray(n[0]),
                    3 => st.fill = [(n[0].clamp(0.0, 1.0) * 255.0).round() as u8, (n[1].clamp(0.0, 1.0) * 255.0).round() as u8, (n[2].clamp(0.0, 1.0) * 255.0).round() as u8],
                    4 => st.fill = cmyk(n[0], n[1], n[2], n[3]),
                    _ => {}
                },
                "Tj" if a.len() == 1 => {
                    if let Ok(s) = a[0].as_str() {
                        self.show(cont, idx, &mut st, &[Item::Str(s.to_vec())]);
                    }
                }
                "'" if a.len() == 1 => {
                    st.tlm = mul(&[1.0, 0.0, 0.0, 1.0, 0.0, -st.tl], &st.tlm);
                    st.tm = st.tlm;
                    if let Ok(s) = a[0].as_str() {
                        self.show(cont, idx, &mut st, &[Item::Str(s.to_vec())]);
                    }
                }
                "\"" if a.len() == 3 => {
                    st.tw = num(&a[0]).unwrap_or(st.tw);
                    st.tc = num(&a[1]).unwrap_or(st.tc);
                    st.tlm = mul(&[1.0, 0.0, 0.0, 1.0, 0.0, -st.tl], &st.tlm);
                    st.tm = st.tlm;
                    if let Ok(s) = a[2].as_str() {
                        self.show(cont, idx, &mut st, &[Item::Str(s.to_vec())]);
                    }
                }
                "TJ" if a.len() == 1 => {
                    if let Ok(arr) = a[0].as_array() {
                        let items: Vec<Item> = arr
                            .iter()
                            .filter_map(|o| match o {
                                Object::String(s, _) => Some(Item::Str(s.clone())),
                                o => num(o).map(Item::Adj),
                            })
                            .collect();
                        self.show(cont, idx, &mut st, &items);
                    }
                }
                "Do" if a.len() == 1 => {
                    let Some(name) = a[0].as_name().ok() else { continue };
                    let Some(xobj) = res.get(b"XObject").ok().and_then(|o| dict_of(self.doc, o)) else { continue };
                    let Some(entry) = xobj.get(name).ok() else { continue };
                    let id = entry.as_reference().ok();
                    let Some(Object::Stream(s)) = fonts::deref(self.doc, entry) else { continue };
                    match s.dict.get(b"Subtype").ok().and_then(fonts::name_of).as_deref() {
                        Some("Image") => {
                            let corners = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)].map(|(x, y)| {
                                let (fx, fy) = pt(&st.ctm, x, y);
                                self.geom.to_view(fx, fy)
                            });
                            self.page.images.push(ImageUse { cont, op: idx, id, quad: corners });
                        }
                        Some("Form") if depth < 6 => {
                            let Some(fid) = id else { continue };
                            let m: Mat = s.dict.get(b"Matrix").ok().and_then(|o| o.as_array().ok()).map(|v| nums(v)).filter(|v| v.len() == 6).map(|v| [v[0], v[1], v[2], v[3], v[4], v[5]]).unwrap_or(IDENT);
                            let fres = s.dict.get(b"Resources").ok().and_then(|o| dict_of(self.doc, o)).cloned().unwrap_or_else(|| res.clone());
                            let Some(sub) = s.decompressed_content().ok().and_then(|b| decode_ops(&b)) else {
                                self.page.complete = false;
                                continue;
                            };
                            let fc = Cont::Form(fid);
                            let first_time = !self.page.ops.contains_key(&fc);
                            if first_time {
                                self.page.ops.insert(fc, sub.clone());
                            }
                            self.walk(fc, &sub, &fres, mul(&m, &st.ctm), depth + 1);
                        }
                        _ => {}
                    }
                }
                "BI" => {
                    // Un'immagine dentro il flusso: non ha nome; il suo posto è il quadrato dell'unità.
                    let corners = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)].map(|(x, y)| {
                        let (fx, fy) = pt(&st.ctm, x, y);
                        self.geom.to_view(fx, fy)
                    });
                    self.page.images.push(ImageUse { cont, op: idx, id: None, quad: corners });
                }
                _ => {}
            }
        }
    }

    fn show(&mut self, cont: Cont, op: usize, st: &mut St, items: &[Item]) {
        let Some(fi) = st.font else { return };
        let font = self.page.fonts[fi].clone();
        self.page.meta.insert((cont, op), ShowMeta { font: fi, fs: st.fs, tc: st.tc, tw: st.tw });
        let mut gi = 0;
        let (asc, desc) = (font.ascent / 1000.0, font.descent / 1000.0);
        for item in items {
            match item {
                Item::Adj(n) => {
                    let tx = -n / 1000.0 * st.fs * st.th;
                    st.tm = mul(&[1.0, 0.0, 0.0, 1.0, tx, 0.0], &st.tm);
                }
                Item::Str(bytes) => {
                    let len = font.code_len();
                    for chunk in bytes.chunks(len) {
                        if self.budget == 0 {
                            self.page.complete = false;
                            return;
                        }
                        self.budget -= 1;
                        let code = chunk.iter().fold(0u32, |a, b| (a << 8) | *b as u32);
                        let w0 = font.width(code) / 1000.0;
                        let trm = mul(&[st.fs * st.th, 0.0, 0.0, st.fs, 0.0, st.rise], &mul(&st.tm, &st.ctm));
                        let view = |u: f32, v: f32| {
                            let (fx, fy) = pt(&trm, u, v);
                            self.geom.to_view(fx, fy)
                        };
                        let (o, e, up) = (view(0.0, 0.0), view(1.0, 0.0), view(0.0, 1.0));
                        let corners = [view(0.0, desc), view(w0, desc), view(w0, asc), view(0.0, asc)];
                        let x0 = corners.iter().map(|p| p.0).fold(f32::MAX, f32::min);
                        let x1 = corners.iter().map(|p| p.0).fold(f32::MIN, f32::max);
                        let top = corners.iter().map(|p| p.1).fold(f32::MAX, f32::min);
                        let bot = corners.iter().map(|p| p.1).fold(f32::MIN, f32::max);
                        let (dx, dy) = (e.0 - o.0, e.1 - o.1);
                        let horizontal = dx > 0.0 && dy.abs() < 0.02 * dx.abs();
                        let size = ((up.0 - o.0).powi(2) + (up.1 - o.1).powi(2)).sqrt();
                        let ch = font.text_of(code);
                        self.page.glyphs.push(Glyph {
                            ch,
                            x0,
                            x1,
                            top,
                            bot,
                            base: o.1,
                            size,
                            font: fi,
                            color: st.fill,
                            cont,
                            op,
                            gi,
                            horizontal,
                            invisible: st.mode == 3,
                        });
                        gi += 1;
                        let space = len == 1 && code == 32;
                        let tx = (w0 * st.fs + st.tc + if space { st.tw } else { 0.0 }) * st.th;
                        st.tm = mul(&[1.0, 0.0, 0.0, 1.0, tx, 0.0], &st.tm);
                    }
                }
            }
        }
    }
}

enum Item {
    Str(Vec<u8>),
    Adj(f32),
}

impl TextPage {
    /// Legge il testo della pagina `page` (oggetto) del documento.
    pub fn read(doc: &Document, page: ObjectId) -> TextPage {
        let geom = Geom::of(doc, page);
        let mut tp = TextPage { w: geom.w, h: geom.h, complete: true, ..Default::default() };
        let bytes = doc.get_page_content(page);
        let ops = decode_ops(&bytes);
        let res = page_resources(doc, page);
        match ops {
            Some(ops) => {
                tp.ops.insert(Cont::Page, ops.clone());
                let mut r = Reader { doc, geom, page: &mut tp, by_id: HashMap::new(), by_name: HashMap::new(), budget: 400_000 };
                r.walk(Cont::Page, &ops, &res, IDENT, 0);
            }
            None => {
                tp.ops.insert(Cont::Page, Vec::new());
                tp.complete = false;
            }
        }
        tp.build();
        tp
    }

    fn build(&mut self) {
        // Righe: i caratteri orizzontali con la stessa linea, spezzate dove c'è un salto grande (colonne, celle).
        let mut order: Vec<usize> = (0..self.glyphs.len()).filter(|i| self.glyphs[*i].horizontal && !self.glyphs[*i].ch.is_empty()).collect();
        order.sort_by(|a, b| {
            let (ga, gb) = (&self.glyphs[*a], &self.glyphs[*b]);
            ga.base.partial_cmp(&gb.base).unwrap_or(std::cmp::Ordering::Equal).then(ga.x0.partial_cmp(&gb.x0).unwrap_or(std::cmp::Ordering::Equal))
        });
        let mut clusters: Vec<Vec<usize>> = Vec::new();
        for i in order {
            let g = &self.glyphs[i];
            match clusters.last_mut() {
                Some(c) => {
                    let first = &self.glyphs[c[0]];
                    if (g.base - first.base).abs() <= 0.3 * first.size.max(g.size).max(1.0) {
                        c.push(i);
                    } else {
                        clusters.push(vec![i]);
                    }
                }
                None => clusters.push(vec![i]),
            }
        }
        for mut c in clusters {
            c.sort_by(|a, b| self.glyphs[*a].x0.partial_cmp(&self.glyphs[*b].x0).unwrap_or(std::cmp::Ordering::Equal));
            let mut cur: Vec<usize> = Vec::new();
            for i in c {
                if let Some(&last) = cur.last() {
                    let (p, g) = (&self.glyphs[last], &self.glyphs[i]);
                    let gap = g.x0 - p.x1;
                    if gap > 2.4 * g.size.max(p.size).max(4.0) || gap < -2.0 * g.size.max(4.0) {
                        let line = std::mem::take(&mut cur);
                        self.push_line(line);
                    }
                }
                cur.push(i);
            }
            if !cur.is_empty() {
                self.push_line(cur);
            }
        }
        self.lines.sort_by(|a, b| a.base.partial_cmp(&b.base).unwrap_or(std::cmp::Ordering::Equal).then(a.x0.partial_cmp(&b.x0).unwrap_or(std::cmp::Ordering::Equal)));
        self.words();
        self.paragraphs();
    }

    fn push_line(&mut self, mut idx: Vec<usize>) {
        // Spazi finali e iniziali fuori dalla riga.
        while idx.first().is_some_and(|i| self.glyphs[*i].is_space()) {
            idx.remove(0);
        }
        while idx.last().is_some_and(|i| self.glyphs[*i].is_space()) {
            idx.pop();
        }
        if idx.is_empty() {
            return;
        }
        let mut text = String::new();
        let mut prev: Option<usize> = None;
        for &i in &idx {
            let g = &self.glyphs[i];
            if let Some(p) = prev {
                let pg = &self.glyphs[p];
                if !pg.is_space() && !g.is_space() && g.x0 - pg.x1 > 0.22 * g.size.max(1.0) {
                    text.push(' ');
                }
            }
            text.push_str(&g.ch);
            prev = Some(i);
        }
        let solid: Vec<&Glyph> = idx.iter().map(|i| &self.glyphs[*i]).filter(|g| !g.is_space()).collect();
        let basis: Vec<&Glyph> = if solid.is_empty() { idx.iter().map(|i| &self.glyphs[*i]).collect() } else { solid };
        let x0 = basis.iter().map(|g| g.x0).fold(f32::MAX, f32::min);
        let x1 = basis.iter().map(|g| g.x1).fold(f32::MIN, f32::max);
        let top = basis.iter().map(|g| g.top).fold(f32::MAX, f32::min);
        let bot = basis.iter().map(|g| g.bot).fold(f32::MIN, f32::max);
        let size = basis.iter().map(|g| g.size).fold(0.0, f32::max);
        let mut bases: Vec<f32> = basis.iter().map(|g| g.base).collect();
        bases.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let base = bases[bases.len() / 2];
        let invisible = basis.iter().all(|g| g.invisible);
        let marker = self.marker_len(&idx);
        self.lines.push(Line { glyphs: idx, text, x0, x1, top, bot, base, size, invisible, marker });
    }

    /// Quanti caratteri in testa alla riga sono il segno di un elenco (puntino, trattino, «1.», «a)»).
    fn marker_len(&self, idx: &[usize]) -> usize {
        let g = |k: usize| idx.get(k).map(|i| &self.glyphs[*i]);
        let first = match g(0) {
            Some(f) => f,
            None => return 0,
        };
        let size = first.size.max(1.0);
        let ch0 = first.ch.chars().next().unwrap_or(' ');
        // Quanti glifi formano il segno, poi lo spazio (un salto o un carattere spazio) prima del testo.
        let mut n = 0;
        let bullet = "•◦▪▫■□●○◆◇►▶➢➤✓✔–—-·∙‣⁃";
        if bullet.contains(ch0) || ('\u{E000}'..='\u{F8FF}').contains(&ch0) {
            n = 1;
        } else if ch0.is_ascii_digit() {
            let mut k = 0;
            while g(k).is_some_and(|x| x.ch.len() == 1 && x.ch.chars().all(|c| c.is_ascii_digit())) && k < 3 {
                k += 1;
            }
            if g(k).is_some_and(|x| x.ch == "." || x.ch == ")") {
                n = k + 1;
            }
        } else if ch0.is_ascii_alphabetic() && first.ch.chars().count() == 1 && g(1).is_some_and(|x| x.ch == ")") {
            n = 2;
        }
        if n == 0 {
            return 0;
        }
        // Dopo il segno deve esserci uno spazio o un salto, e poi del testo.
        let last = g(n - 1).map(|x| x.x1).unwrap_or(0.0);
        let mut k = n;
        let mut gap_ok = false;
        while let Some(x) = g(k) {
            if x.is_space() {
                gap_ok = true;
                k += 1;
            } else {
                if x.x0 - last > 0.22 * size {
                    gap_ok = true;
                }
                break;
            }
        }
        if !gap_ok || g(k).is_none() {
            return 0;
        }
        k
    }

    fn words(&mut self) {
        for (li, line) in self.lines.iter().enumerate() {
            let mut cur: Option<(usize, usize, String, Rect)> = None;
            let flush = |cur: &mut Option<(usize, usize, String, Rect)>, words: &mut Vec<Word>| {
                if let Some((f, l, t, r)) = cur.take() {
                    if !t.trim().is_empty() {
                        words.push(Word { text: t, rect: r, line: li, first: f, last: l });
                    }
                }
            };
            let mut prev: Option<usize> = None;
            for &i in &line.glyphs {
                let g = &self.glyphs[i];
                let gap_break = prev.is_some_and(|p| g.x0 - self.glyphs[p].x1 > 0.22 * g.size.max(1.0));
                if g.is_space() || gap_break {
                    flush(&mut cur, &mut self.words);
                }
                if !g.is_space() {
                    match &mut cur {
                        Some((_, l, t, r)) => {
                            *l = i;
                            t.push_str(&g.ch);
                            *r = r.union(&g.rect());
                        }
                        None => cur = Some((i, i, g.ch.clone(), g.rect())),
                    }
                }
                prev = Some(i);
            }
            flush(&mut cur, &mut self.words);
        }
    }

    fn paragraphs(&mut self) {
        // Ogni riga va con quella sopra se sta nello stesso passo, con lo stesso corpo, e comincia dove comincia.
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut open: Vec<usize> = Vec::new();
        for li in 0..self.lines.len() {
            let l = &self.lines[li];
            let mut joined = None;
            for &gi in open.iter().rev() {
                let last = *groups[gi].last().unwrap_or(&0);
                let p = &self.lines[last];
                let dy = l.base - p.base;
                let same_size = (l.size - p.size).abs() <= 0.15 * p.size.max(1.0);
                let first_line_of_group = groups[gi].len() == 1;
                let left = self.lines[groups[gi][0]].x0;
                let head = &self.lines[groups[gi][0]];
                let hang = head.marker > 0 && (l.x0 - self.body_x0(head)).abs() <= 0.9 * p.size.max(4.0);
                let near_left = hang || (head.marker == 0 && ((l.x0 - left).abs() <= 0.9 * p.size.max(4.0) || (first_line_of_group && (l.x0 - left).abs() <= 3.2 * p.size.max(4.0)) || (l.x0 - p.x0).abs() <= 0.9 * p.size.max(4.0)));
                let centered = ((l.x0 + l.x1) - (p.x0 + p.x1)).abs() <= 0.8 * p.size.max(4.0) && (l.x0 - p.x0).abs() > 0.9 * p.size.max(4.0);
                let same_font = self.glyphs[l.glyphs[0]].font == self.glyphs[p.glyphs[0]].font || (l.size - p.size).abs() < 0.5;
                if l.marker == 0 && dy > 0.7 * p.size && dy < 1.85 * p.size.max(l.size) && same_size && (near_left || centered) && same_font && l.invisible == p.invisible && !self.far_right_edge_break(&groups[gi], l) {
                    joined = Some(gi);
                    break;
                }
            }
            match joined {
                Some(gi) => groups[gi].push(li),
                None => {
                    groups.push(vec![li]);
                    open.push(groups.len() - 1);
                }
            }
            // Gruppi che non possono più ricevere righe (troppo in alto) si chiudono.
            let base = self.lines[li].base;
            open.retain(|&gi| {
                let last = self.lines[*groups[gi].last().unwrap_or(&0)].base;
                base - last < 2.2 * self.lines[*groups[gi].last().unwrap_or(&0)].size.max(8.0)
            });
        }
        for g in groups {
            self.make_block(&g);
        }
        self.blocks.sort_by(|a, b| a.rect.y.partial_cmp(&b.rect.y).unwrap_or(std::cmp::Ordering::Equal).then(a.rect.x.partial_cmp(&b.rect.x).unwrap_or(std::cmp::Ordering::Equal)));
    }

    /// Dove comincia il testo di una riga, dopo il segno dell'elenco.
    fn body_x0(&self, l: &Line) -> f32 {
        l.glyphs.get(l.marker).map(|i| self.glyphs[*i].x0).unwrap_or(l.x0)
    }

    /// Una riga che comincia molto più a destra dell'inizio del gruppo e dove il gruppo non ha altro: è un'altra cosa.
    fn far_right_edge_break(&self, group: &[usize], l: &Line) -> bool {
        let left = self.body_x0(&self.lines[group[0]]);
        let width = group.iter().map(|i| self.lines[*i].x1).fold(0.0, f32::max) - left;
        group.len() >= 2 && l.x0 > left + 0.6 * width.max(1.0)
    }

    fn make_block(&mut self, lines: &[usize]) {
        // I caratteri del corpo di ogni riga: la prima senza il segno dell'elenco.
        let body: Vec<Vec<usize>> = lines.iter().enumerate().map(|(k, li)| {
            let l = &self.lines[*li];
            if k == 0 { l.glyphs[l.marker.min(l.glyphs.len())..].to_vec() } else { l.glyphs.clone() }
        }).collect();
        if body.iter().all(|b| b.is_empty()) {
            return;
        }
        let marker: String = {
            let l = &self.lines[lines[0]];
            l.glyphs[..l.marker.min(l.glyphs.len())].iter().map(|i| self.glyphs[*i].ch.as_str()).collect::<String>().trim().to_string()
        };
        let solid = |b: &[usize]| -> Vec<usize> { b.iter().copied().filter(|i| !self.glyphs[*i].is_space()).collect() };
        let line_rects: Vec<Rect> = body
            .iter()
            .map(|b| {
                let s = solid(b);
                let basis: Vec<usize> = if s.is_empty() { b.clone() } else { s };
                basis.iter().map(|i| self.glyphs[*i].rect()).reduce(|a, c| a.union(&c)).unwrap_or_default()
            })
            .collect();
        let rect = line_rects.iter().copied().reduce(|a, b| a.union(&b)).unwrap_or_default();
        let width = rect.w.max(1.0);
        let size = lines.iter().map(|l| self.lines[*l].size).fold(0.0, f32::max);
        // Il carattere e il colore del paragrafo: quelli del carattere più usato.
        let mut counts: HashMap<(usize, Color), usize> = HashMap::new();
        for b in &body {
            for &g in b {
                if !self.glyphs[g].is_space() {
                    *counts.entry((self.glyphs[g].font, self.glyphs[g].color)).or_default() += 1;
                }
            }
        }
        let ((font, color), _) = counts.into_iter().max_by_key(|(_, n)| *n).unwrap_or(((0, [0, 0, 0]), 0));
        let fi = self.fonts.get(font).cloned().unwrap_or_default();
        let mut text = String::new();
        let mut cmap: Vec<Option<usize>> = Vec::new();
        for (k, b) in body.iter().enumerate() {
            let mut prev: Option<usize> = None;
            for &i in b {
                let g = &self.glyphs[i];
                if let Some(p) = prev {
                    let pg = &self.glyphs[p];
                    if !pg.is_space() && !g.is_space() && g.x0 - pg.x1 > 0.22 * g.size.max(1.0) {
                        text.push(' ');
                        cmap.push(None);
                    }
                }
                for c in g.ch.chars() {
                    text.push(c);
                    cmap.push(Some(i));
                }
                prev = Some(i);
            }
            if k + 1 < body.len() {
                let l = &line_rects[k];
                let wide = l.w >= 0.86 * width - 1.5 * size;
                let next_left = line_rects[k + 1].x;
                let hard = !wide && (next_left - l.x).abs() < 0.9 * size.max(4.0);
                if hard || !wide {
                    text.push('\n');
                    cmap.push(None);
                } else if text.ends_with('-') && self.glyphs[body[k + 1][0]].ch.chars().next().is_some_and(char::is_lowercase) {
                    // il trattino resta, le due metà si uniscono
                } else {
                    text.push(' ');
                    cmap.push(None);
                }
            }
        }
        let bases: Vec<f32> = lines.iter().map(|l| self.lines[*l].base).collect();
        let mut deltas: Vec<f32> = bases.windows(2).map(|w| w[1] - w[0]).collect();
        deltas.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let line_h = if deltas.is_empty() { size * 1.2 } else { deltas[deltas.len() / 2] };
        let align = if line_rects.len() < 2 {
            Align::Left
        } else if line_rects.iter().all(|l| (l.x - line_rects[0].x).abs() < 1.5) {
            Align::Left
        } else if line_rects.iter().all(|l| ((l.x + l.right()) - (line_rects[0].x + line_rects[0].right())).abs() < 3.0) {
            Align::Center
        } else if line_rects.iter().all(|l| (l.right() - line_rects[0].right()).abs() < 1.5) {
            Align::Right
        } else {
            Align::Left
        };
        let invisible = lines.iter().all(|l| self.lines[*l].invisible);
        let why = if invisible {
            Why::Scan
        } else if !fi.known || fi.vertical {
            Why::Font
        } else {
            Why::Ok
        };
        let glyphs: Vec<usize> = body.into_iter().flatten().collect();
        self.blocks.push(TextBlock {
            rect,
            text,
            cmap,
            lines: lines.to_vec(),
            glyphs,
            marker,
            size,
            color,
            font,
            font_name: fi.base.clone(),
            bold: fi.bold,
            italic: fi.italic,
            align,
            line_h: line_h.max(size * 0.9),
            why,
        });
    }

    /// Il paragrafo che contiene il punto.
    pub fn block_at(&self, x: f32, y: f32) -> Option<usize> {
        self.blocks.iter().position(|b| b.rect.inflate(2.0).contains(x, y))
    }

    /// Il testo della pagina (righe una sotto l'altra) con, per ogni carattere, il glifo da cui viene.
    pub fn flat(&self) -> (String, Vec<Option<usize>>) {
        let mut text = String::new();
        let mut map = Vec::new();
        for l in &self.lines {
            let mut prev: Option<usize> = None;
            for &i in &l.glyphs {
                let g = &self.glyphs[i];
                if let Some(p) = prev {
                    let pg = &self.glyphs[p];
                    if !pg.is_space() && !g.is_space() && g.x0 - pg.x1 > 0.22 * g.size.max(1.0) {
                        text.push(' ');
                        map.push(None);
                    }
                }
                for c in g.ch.chars() {
                    text.push(c);
                    map.push(Some(i));
                }
                prev = Some(i);
            }
            text.push('\n');
            map.push(None);
        }
        (text, map)
    }
}

/// Toglie dal contenuto i caratteri indicati (contenitore, operazione, posto nell'operazione): il resto del
/// testo non si muove. Dà il numero di caratteri tolti. Dopo questa chiamata i numeri delle operazioni
/// di `tp` non valgono più: si rilegge la pagina.
pub fn remove_glyphs(tp: &mut TextPage, remove: &std::collections::HashSet<(Cont, usize, usize)>) -> usize {
    let mut done = remove.len();
    let mut touched: Vec<(Cont, usize)> = remove.iter().map(|(c, o, _)| (*c, *o)).collect();
    touched.sort_by(|a, b| format!("{:?}", a.0).cmp(&format!("{:?}", b.0)).then(b.1.cmp(&a.1)));
    touched.dedup();
    for (cont, op) in touched {
        let Some(meta) = tp.meta.get(&(cont, op)).cloned() else {
            done = done.saturating_sub(1);
            continue;
        };
        let font = tp.fonts[meta.font].clone();
        let Some(ops) = tp.ops.get_mut(&cont) else { continue };
        let Some(o) = ops.get(op).cloned() else { continue };
        let (items, mut replacement): (Vec<Object>, Vec<Operation>) = match o.operator.as_str() {
            "Tj" => (o.operands.clone(), Vec::new()),
            "'" => (o.operands.clone(), vec![Operation::new("T*", vec![])]),
            "\"" if o.operands.len() == 3 => (
                vec![o.operands[2].clone()],
                vec![Operation::new("Tw", vec![o.operands[0].clone()]), Operation::new("Tc", vec![o.operands[1].clone()]), Operation::new("T*", vec![])],
            ),
            "TJ" => (o.operands.first().and_then(|a| a.as_array().ok()).cloned().unwrap_or_default(), Vec::new()),
            _ => continue,
        };
        let rebuilt = rebuild_tj(&items, remove, cont, op, &font, &meta);
        replacement.push(Operation::new("TJ", vec![Object::Array(rebuilt)]));
        ops.splice(op..=op, replacement);
    }
    done
}

/// L'array del TJ senza i caratteri tolti, con gli spostamenti che tengono ferme le lettere rimaste.
fn rebuild_tj(items: &[Object], remove: &std::collections::HashSet<(Cont, usize, usize)>, cont: Cont, op: usize, font: &FontInfo, meta: &ShowMeta) -> Vec<Object> {
    let mut out: Vec<Object> = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    let mut shift = 0.0f32;
    let mut gi = 0usize;
    let len = font.code_len();
    let fs = if meta.fs.abs() < 1e-3 { 1.0 } else { meta.fs };
    let push_shift = |out: &mut Vec<Object>, shift: &mut f32| {
        if *shift != 0.0 {
            // in un TJ un numero positivo sposta a sinistra: tolto un carattere largo w, si torna avanti di w
            out.push(Object::Real(-*shift));
            *shift = 0.0;
        }
    };
    for it in items {
        match it {
            Object::String(bytes, _) => {
                for chunk in bytes.chunks(len) {
                    if remove.contains(&(cont, op, gi)) {
                        let code = chunk.iter().fold(0u32, |a, b| (a << 8) | *b as u32);
                        let space = len == 1 && code == 32;
                        shift += font.width(code) + (meta.tc + if space { meta.tw } else { 0.0 }) * 1000.0 / fs;
                    } else {
                        if shift != 0.0 {
                            if !cur.is_empty() {
                                out.push(Object::String(std::mem::take(&mut cur), lopdf::StringFormat::Literal));
                            }
                            push_shift(&mut out, &mut shift);
                        }
                        cur.extend_from_slice(chunk);
                    }
                    gi += 1;
                }
            }
            other => {
                if !cur.is_empty() {
                    out.push(Object::String(std::mem::take(&mut cur), lopdf::StringFormat::Literal));
                }
                if shift != 0.0 {
                    push_shift(&mut out, &mut shift);
                }
                out.push(other.clone());
            }
        }
    }
    if !cur.is_empty() {
        out.push(Object::String(cur, lopdf::StringFormat::Literal));
    }
    if shift != 0.0 {
        push_shift(&mut out, &mut shift);
    }
    out
}

/// Scrive di nuovo i flussi del contenuto di una pagina (e degli oggetti che disegna) dopo una modifica.
pub fn write_back(doc: &mut Document, page: ObjectId, tp: &TextPage, original: &HashMap<Cont, Vec<Operation>>) -> Result<(), String> {
    for (cont, ops) in &tp.ops {
        if original.get(cont).is_some_and(|o| same_ops(o, ops)) {
            continue;
        }
        let data = Content { operations: ops.clone() }.encode().map_err(|e| format!("contenuto non scritto: {e}"))?;
        match cont {
            Cont::Page => {
                let mut s = Stream::new(Dictionary::new(), data);
                let _ = s.compress();
                let id = doc.add_object(Object::Stream(s));
                doc.get_dictionary_mut(page).map_err(|e| e.to_string())?.set("Contents", Object::Reference(id));
            }
            Cont::Form(id) => {
                if let Ok(Object::Stream(s)) = doc.get_object_mut(*id) {
                    s.dict.remove(b"Filter");
                    s.dict.remove(b"DecodeParms");
                    s.set_plain_content(data);
                    let _ = s.compress();
                }
            }
        }
    }
    Ok(())
}

fn same_ops(a: &[Operation], b: &[Operation]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.operator == y.operator && format!("{:?}", x.operands) == format!("{:?}", y.operands))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dump(name: &str, bytes: &[u8]) -> TextPage {
        let doc = Document::load_mem(bytes).unwrap();
        let page = *doc.get_pages().get(&1).unwrap();
        let tp = TextPage::read(&doc, page);
        println!("== {name}: {}x{}  glifi {} righe {} parole {} blocchi {} complete {}", tp.w, tp.h, tp.glyphs.len(), tp.lines.len(), tp.words.len(), tp.blocks.len(), tp.complete);
        for b in tp.blocks.iter().take(12) {
            println!("  [{:.0},{:.0} {:.0}x{:.0}] {:.1}pt {:?} {} {:?}: {:?}", b.rect.x, b.rect.y, b.rect.w, b.rect.h, b.size, b.align, b.font_name, b.why, b.text.chars().take(90).collect::<String>());
        }
        tp
    }

    #[test]
    fn reads_the_fixtures() {
        let c = dump("circolare", include_bytes!("../../tests/fixtures/circolare.pdf"));
        assert!(c.glyphs.len() > 50 && !c.blocks.is_empty());
        let k = dump("colonne", include_bytes!("../../tests/fixtures/colonne.pdf"));
        assert!(k.glyphs.len() > 50 && !k.blocks.is_empty());
    }
}
