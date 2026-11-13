//! Mette in pagina un documento di testo come fa Word: righe spezzate sugli spazi con tabulazioni,
//! allineamenti e interlinee; paragrafi tenuti insieme, vedove e orfane; tabelle con le righe che
//! passano di pagina e le intestazioni ripetute; intestazioni e piè di pagina (prima pagina, pari),
//! note a piè di pagina e di chiusura, colonne, immagini e caselle di testo con il testo attorno.

use std::collections::{HashMap, VecDeque};

use super::documento::*;
use super::font::{FontId, Fonti};
use super::pagina::{Dash, Link, Mark, Op, Pagina, Rgb, Stampato, Target};

/// Segnaposto del numero di pagina e del totale nel corpo: si scrivono alla fine.
const PAGE_MARK: &str = "\u{E000}";
const TOTAL_MARK: &str = "\u{E001}";

// ---------------------------------------------------------------- pezzi di riga

#[derive(Clone, Copy, Debug, PartialEq)]
enum K {
    Word,
    Space,
    Tab,
    ACapo,
    Pagina,
    Colonna,
    Img(usize, f32, f32),
}

#[derive(Clone, Debug)]
struct Frag {
    k: K,
    text: String,
    font: FontId,
    size: f32,
    rise: f32,
    asc: f32,
    desc: f32,
    w: f32,
    /// Il carattere com'è nel documento (colore, sottolineature…).
    style: usize,
    link: Option<Target>,
    /// Dopo questo pezzo la riga si può spezzare (trattino, ideogramma).
    brk_after: bool,
    note: Option<usize>,
    /// 1 numero di pagina, 2 totale (nel corpo: segnaposto).
    field: u8,
    /// Dopo c'è un trattino facoltativo: se la riga va a capo qui, il trattino si vede.
    shy: bool,
}

struct Prep<'a> {
    par: &'a Paragrafo,
    frags: Vec<Frag>,
    label: Vec<Frag>,
    styles: Vec<Font>,
    end_asc: f32,
    end_desc: f32,
    /// Larghezza della cornice in cui sta il paragrafo.
    w: f32,
    before: f32,
    after: f32,
    /// Bordo sopra e sotto: il paragrafo è il primo / l'ultimo del gruppo con gli stessi bordi.
    open_top: bool,
    open_bottom: bool,
}

#[derive(Clone, Debug, PartialEq)]
enum End {
    Soft,
    Hard,
    Last,
    Pagina,
    Colonna,
}

struct Line {
    items: Vec<(Frag, f32)>,
    leaders: Vec<(f32, f32, char, usize)>,
    asc: f32,
    desc: f32,
    /// Le misure del solo testo (senza immagini), per l'interlinea alla Writer.
    t_asc: f32,
    t_desc: f32,
    end: usize,
    kind: End,
}

// ---------------------------------------------------------------- disegni e unità

#[derive(Clone, Debug, Default)]
struct Draw {
    ops: Vec<Op>,
    links: Vec<Link>,
    marks: Vec<Mark>,
    anchors: Vec<(String, f32)>,
}

impl Draw {
    fn add(&mut self, o: &Draw, dx: f32, dy: f32) {
        self.ops.extend(o.ops.iter().map(|op| shift(op, dx, dy)));
        self.links.extend(o.links.iter().map(|l| Link { x: l.x + dx, y: l.y + dy, ..l.clone() }));
        self.marks.extend(o.marks.iter().map(|m| Mark { y: m.y + dy, ..m.clone() }));
        self.anchors.extend(o.anchors.iter().map(|(n, y)| (n.clone(), y + dy)));
    }
}

fn shift(op: &Op, dx: f32, dy: f32) -> Op {
    match op {
        Op::Text { x, y, font, size, color, text } => Op::Text { x: x + dx, y: y + dy, font: *font, size: *size, color: *color, text: text.clone() },
        Op::Rect { x, y, w, h, color } => Op::Rect { x: x + dx, y: y + dy, w: *w, h: *h, color: *color },
        Op::Line { x1, y1, x2, y2, width, color, dash } => Op::Line { x1: x1 + dx, y1: y1 + dy, x2: x2 + dx, y2: y2 + dy, width: *width, color: *color, dash: *dash },
        Op::Image { index, x, y, w, h } => Op::Image { index: *index, x: x + dx, y: y + dy, w: *w, h: *h },
        Op::Clip { x, y, w, h } => Op::Clip { x: x + dx, y: y + dy, w: *w, h: *h },
        Op::Unclip => Op::Unclip,
    }
}

#[derive(Clone, Debug)]
struct Slice {
    y0: f32,
    y1: f32,
    draw: Draw,
}

#[derive(Clone, Debug)]
struct CellDraw {
    x: f32,
    w: f32,
    ml: f32,
    mt: f32,
    mb: f32,
    fill: Option<Rgb>,
    borders: [Option<Linea>; 4],
    valign: u8,
    slices: Vec<Slice>,
    content_h: f32,
    /// Continua la cella unita di sopra: niente contenuto e niente bordo sopra.
    cont: bool,
    /// Prosegue nella riga sotto: niente bordo sotto.
    open_bottom: bool,
    /// Cella unita in verticale: l'altezza di tutte le sue righe.
    group_h: Option<f32>,
}

#[derive(Clone, Debug)]
struct RowData {
    cells: Vec<CellDraw>,
    h: f32,
    exact: bool,
}

#[derive(Clone, Debug)]
enum Body {
    Ops(Draw),
    Row(RowData),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Brk {
    /// Salto di pagina scritto nel testo: sempre.
    Pagina,
    /// «Interruzione di pagina prima»: solo se la pagina ha già qualcosa.
    PaginaSePiena,
    Colonna,
}

#[derive(Clone, Debug)]
struct Unit<'a> {
    h: f32,
    before: f32,
    after: f32,
    keep: bool,
    brk: Option<Brk>,
    body: Body,
    notes: Vec<usize>,
    floats: Vec<&'a Mobile>,
    /// Una riga di paragrafo: quale e da quale pezzo (per rifarla attorno a un'immagine).
    line: Option<(usize, usize, bool)>,
    /// Le righe d'intestazione della tabella, da ripetere in cima alla pagina nuova.
    head: Option<std::rc::Rc<Vec<Unit<'a>>>>,
    split: bool,
}

impl<'a> Unit<'a> {
    fn ops(draw: Draw, h: f32) -> Unit<'a> {
        Unit { h, before: 0.0, after: 0.0, keep: false, brk: None, body: Body::Ops(draw), notes: Vec::new(), floats: Vec::new(), line: None, head: None, split: false }
    }
}

// ---------------------------------------------------------------- pagine

#[derive(Clone, Copy, Debug)]
struct Zone {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    sides: bool,
}

struct PageInfo {
    p: Pagina,
    sez: usize,
    num: u32,
    first_of_section: bool,
    notes: Vec<usize>,
    notes_h: f32,
    body_bottom: f32,
    back: Vec<Op>,
    zones: Vec<Zone>,
}

#[derive(Clone, Copy)]
struct Geo {
    w: f32,
    h: f32,
    ml: f32,
    mr: f32,
    top: f32,
    bottom: f32,
}

pub struct Impaginatore<'a> {
    doc: &'a Documento,
    pub fonts: Fonti,
    preps: Vec<Prep<'a>>,
    deftab: f32,
    /// I campi pagina e totale: nel corpo segnaposto, nelle intestazioni i valori veri.
    fields: Option<(String, String)>,
    pages: Vec<PageInfo>,
    note_cache: HashMap<usize, (f32, Draw)>,
    // stato della paginazione
    y: f32,
    cols_top: f32,
    col: usize,
    ncols: usize,
    col_w: f32,
    col_gap: f32,
    geo: Geo,
    at_top: bool,
    natural: bool,
    prev_keep: bool,
    sez: usize,
    /// Fondo delle colonne bilanciate (fine di una sezione continua).
    balance_bottom: Option<f32>,
    /// Dentro una cella di tabella (quante tabelle una nell'altra).
    in_cell: usize,
    /// Lo spazio dopo l'ultimo paragrafo messo (si confronta con lo spazio prima del seguente).
    pending: f32,
}

const NOTE_SEP: f32 = 14.0;

impl<'a> Impaginatore<'a> {
    pub fn new(doc: &'a Documento, fonts: Fonti) -> Impaginatore<'a> {
        Impaginatore {
            doc,
            fonts,
            preps: Vec::new(),
            deftab: if doc.tab_predefinita > 1.0 { doc.tab_predefinita } else { 36.0 },
            fields: None,
            pages: Vec::new(),
            note_cache: HashMap::new(),
            y: 0.0,
            cols_top: 0.0,
            col: 0,
            ncols: 1,
            col_w: 0.0,
            col_gap: 0.0,
            geo: Geo { w: 595.0, h: 842.0, ml: 56.0, mr: 56.0, top: 56.0, bottom: 786.0 },
            at_top: true,
            natural: false,
            prev_keep: false,
            sez: 0,
            balance_bottom: None,
            in_cell: 0,
            pending: 0.0,
        }
    }

    // ------------------------------------------------------------ preparazione dei paragrafi

    fn face_metrics(&self, id: FontId, size: f32) -> (f32, f32) {
        let f = self.fonts.face(id);
        ((f.ascent + f.leading) * size / 1000.0, (f.descent + f.gap) * size / 1000.0)
    }

    fn font_id(&mut self, f: &Font) -> FontId {
        let family = if f.family.is_empty() { "Calibri" } else { &f.family };
        self.fonts.get(family, f.bold, f.italic)
    }

    /// Spezza il testo di un pezzo nei suoi frammenti (parole e spazi).
    fn push_text(&mut self, out: &mut Vec<Frag>, text: &str, font: &Font, style: usize, link: &Option<Target>, note: Option<usize>, field: u8) {
        let id = self.font_id(font);
        let base = font.size.max(1.0);
        let (mut size, rise) = match font.vert {
            1 => (base * 0.65, base * 0.33),
            -1 => (base * 0.65, -base * 0.14),
            _ => (base, 0.0),
        };
        let text: String = if font.caps { text.to_uppercase() } else { text.to_string() };
        // Maiuscoletto: le minuscole diventano maiuscole più piccole.
        let mut segs: Vec<(String, f32)> = Vec::new();
        if font.small_caps && !font.caps {
            for c in text.chars() {
                let small = c.is_lowercase();
                let s = if small { size * 0.8 } else { size };
                let up: String = if small { c.to_uppercase().collect() } else { c.to_string() };
                match segs.last_mut() {
                    Some((t, z)) if (*z - s).abs() < 0.01 => t.push_str(&up),
                    _ => segs.push((up, s)),
                }
            }
        } else {
            segs.push((text, size));
        }
        for (seg, s) in segs {
            size = s;
            let (a, d) = self.face_metrics(id, size);
            let mut word = String::new();
            let flush = |me: &mut Self, out: &mut Vec<Frag>, word: &mut String, brk: bool| {
                if word.is_empty() {
                    return;
                }
                let w = me.fonts.width(id, word, size) + font.spacing * word.chars().count() as f32;
                out.push(Frag { k: K::Word, text: std::mem::take(word), font: id, size, rise, asc: a + rise, desc: d - rise, w, style, link: link.clone(), brk_after: brk, note, field, shy: false });
            };
            for c in seg.chars() {
                match c {
                    ' ' | '\u{3000}' | '\u{200B}' => {
                        flush(self, out, &mut word, false);
                        let t = if c == '\u{200B}' { String::new() } else { c.to_string() };
                        let w = if t.is_empty() { 0.0 } else { self.fonts.width(id, &t, size) + font.spacing };
                        out.push(Frag { k: K::Space, text: t, font: id, size, rise, asc: a + rise, desc: d - rise, w, style, link: link.clone(), brk_after: true, note: None, field: 0, shy: false });
                    }
                    '\t' => {
                        flush(self, out, &mut word, false);
                        out.push(Frag { k: K::Tab, text: String::new(), font: id, size, rise: 0.0, asc: a, desc: d, w: 0.0, style, link: None, brk_after: true, note: None, field: 0, shy: false });
                    }
                    '\n' | '\u{2028}' => {
                        flush(self, out, &mut word, false);
                        out.push(Frag { k: K::ACapo, text: String::new(), font: id, size, rise: 0.0, asc: a, desc: d, w: 0.0, style, link: None, brk_after: true, note: None, field: 0, shy: false });
                    }
                    '\u{AD}' => {
                        flush(self, out, &mut word, true);
                        if let Some(f) = out.last_mut() {
                            f.shy = f.k == K::Word;
                        }
                    }
                    '\r' => {}
                    '-' | '\u{2010}' | '\u{2013}' | '\u{2014}' | '/' if !word.is_empty() && c != '/' => {
                        word.push(c);
                        flush(self, out, &mut word, true);
                    }
                    c if is_cjk(c) => {
                        flush(self, out, &mut word, false);
                        word.push(c);
                        flush(self, out, &mut word, true);
                    }
                    c => word.push(c),
                }
            }
            flush(self, out, &mut word, false);
        }
    }

    fn prepare(&mut self, par: &'a Paragrafo, w: f32) -> usize {
        let mut frags = Vec::new();
        let mut styles: Vec<Font> = Vec::new();
        for r in &par.runs {
            let style = match styles.iter().position(|s| *s == r.font) {
                Some(i) => i,
                None => {
                    styles.push(r.font.clone());
                    styles.len() - 1
                }
            };
            let simple = |k: K, me: &mut Self, frags: &mut Vec<Frag>| {
                let id = me.font_id(&r.font);
                let (a, d) = me.face_metrics(id, r.font.size.max(1.0));
                frags.push(Frag { k, text: String::new(), font: id, size: r.font.size, rise: 0.0, asc: a, desc: d, w: 0.0, style, link: None, brk_after: true, note: None, field: 0, shy: false });
            };
            match &r.pezzo {
                Pezzo::Testo(t) => self.push_text(&mut frags, t, &r.font, style, &r.link, None, 0),
                Pezzo::Tab => simple(K::Tab, self, &mut frags),
                Pezzo::ACapo => simple(K::ACapo, self, &mut frags),
                Pezzo::SaltoPagina => simple(K::Pagina, self, &mut frags),
                Pezzo::SaltoColonna => simple(K::Colonna, self, &mut frags),
                Pezzo::Immagine { img, w: iw, h: ih } => {
                    let id = self.font_id(&r.font);
                    frags.push(Frag { k: K::Img(*img, *iw, *ih), text: String::new(), font: id, size: r.font.size, rise: 0.0, asc: *ih, desc: 0.0, w: *iw, style, link: r.link.clone(), brk_after: true, note: None, field: 0, shy: false });
                }
                Pezzo::Nota(i) => {
                    let n = self.doc.note.get(*i).map(|n| n.0.clone()).unwrap_or_default();
                    self.push_text(&mut frags, &n, &r.font, style, &r.link, Some(*i), 0);
                }
                Pezzo::NotaFine(i) => {
                    let n = self.doc.note_fine.get(*i).map(|n| n.0.clone()).unwrap_or_default();
                    self.push_text(&mut frags, &n, &r.font, style, &r.link, None, 0);
                }
                Pezzo::NumPagina | Pezzo::NumPagine => {
                    let total = matches!(r.pezzo, Pezzo::NumPagine);
                    let t = match &self.fields {
                        Some((p, n)) => {
                            if total {
                                n.clone()
                            } else {
                                p.clone()
                            }
                        }
                        None => "99".to_string(),
                    };
                    let field = if self.fields.is_some() { 0 } else if total { 2 } else { 1 };
                    self.push_text(&mut frags, &t, &r.font, style, &r.link, None, field);
                }
            }
        }
        let mut label = Vec::new();
        if let Some((t, f)) = &par.etichetta {
            styles.push(f.clone());
            let st = styles.len() - 1;
            self.push_text(&mut label, t, f, st, &None, None, 0);
            for l in &mut label {
                if l.k == K::Space {
                    l.k = K::Word;
                }
            }
        }
        let end_id = self.font_id(&par.font_fine);
        let (end_asc, end_desc) = self.face_metrics(end_id, par.font_fine.size.max(1.0));
        self.preps.push(Prep { par, frags, label, styles, end_asc, end_desc, w, before: par.prima, after: par.dopo, open_top: true, open_bottom: true });
        self.preps.len() - 1
    }

    // ------------------------------------------------------------ righe

    fn next_stop(&self, par: &Paragrafo, x: f32, first: bool) -> (f32, TipoTab, Option<char>) {
        let mut best: Option<(f32, TipoTab, Option<char>)> = par.tabs.iter().find(|t| t.pos > x + 0.5).map(|t| (t.pos, t.tipo, t.leader));
        if first && par.prima_riga < 0.0 && x < par.rientro_sx - 0.5 && best.is_none_or(|b| par.rientro_sx < b.0) {
            best = Some((par.rientro_sx, TipoTab::Sinistra, None));
        }
        if best.is_none() {
            let last = par.tabs.last().map_or(0.0, |t| t.pos).max(x);
            let d = self.deftab;
            let n = ((last + 0.5) / d).floor() + 1.0;
            best = Some((n * d, TipoTab::Sinistra, None));
        }
        best.unwrap()
    }

    /// Una riga del paragrafo da `start`, tra `lim_l` e `lim_r` della cornice.
    fn break_line(&mut self, pi: usize, start: usize, lim_l: f32, lim_r: f32, first: bool) -> Line {
        let par = self.preps[pi].par;
        let w = self.preps[pi].w;
        let mut items: Vec<(Frag, f32)> = Vec::new();
        let mut leaders = Vec::new();
        let right = (w - par.rientro_dx).min(lim_r);
        let ind = par.rientro_sx + if first { par.prima_riga } else { 0.0 };
        let mut x = ind.max(lim_l);
        if first && !self.preps[pi].label.is_empty() {
            for l in self.preps[pi].label.clone() {
                let lw = l.w;
                items.push((l, x));
                x += lw;
            }
            if par.etichetta_tab {
                let (stop, _, _) = self.next_stop(par, x, true);
                x = if stop > right && x < right { x + 6.0 } else { stop.max(x) };
            } else if let Some((l, _)) = items.last() {
                x += self.fonts.width(l.font, " ", l.size);
            }
        }
        let content_start = items.len();
        // Segmento dopo una tabulazione a destra, al centro o decimale.
        struct Seg {
            kind: TipoTab,
            pos: f32,
            leader: Option<char>,
            from: usize,
            x0: f32,
            style: usize,
        }
        let mut seg: Option<Seg> = None;
        let close = |seg: &mut Option<Seg>, items: &mut Vec<(Frag, f32)>, x: &mut f32, leaders: &mut Vec<(f32, f32, char, usize)>| {
            let Some(s) = seg.take() else { return };
            let total = *x - s.x0;
            let lead = match s.kind {
                TipoTab::Destra => total,
                TipoTab::Centro => total / 2.0,
                TipoTab::Decimale => {
                    let mut acc = 0.0;
                    let mut found = None;
                    for (f, _) in &items[s.from..] {
                        if let Some(p) = f.text.rfind([',', '.']) {
                            found = Some(acc + f.w * (f.text[..p].chars().count() as f32 / f.text.chars().count().max(1) as f32));
                        }
                        acc += f.w;
                    }
                    found.unwrap_or(total)
                }
                TipoTab::Sinistra => 0.0,
            };
            let start = (s.pos - lead).max(s.x0);
            let dx = start - s.x0;
            for it in &mut items[s.from..] {
                it.1 += dx;
            }
            if let Some(c) = s.leader {
                leaders.push((s.x0, start, c, s.style));
            }
            *x += dx;
        };
        let mut i = start;
        let mut last_break: Option<(usize, usize)> = None;
        let kind;
        loop {
            if i >= self.preps[pi].frags.len() {
                kind = End::Last;
                break;
            }
            let fr = self.preps[pi].frags[i].clone();
            match fr.k {
                K::ACapo => {
                    i += 1;
                    kind = End::Hard;
                    break;
                }
                K::Pagina | K::Colonna => {
                    i += 1;
                    kind = if fr.k == K::Pagina { End::Pagina } else { End::Colonna };
                    break;
                }
                K::Tab => {
                    close(&mut seg, &mut items, &mut x, &mut leaders);
                    let (stop, tipo, leader) = self.next_stop(par, x, first);
                    if stop > right + 0.5 && tipo == TipoTab::Sinistra && items.len() > content_start {
                        // La tabulazione oltre il margine: la riga va a capo.
                        if stop > w {
                            i += 1;
                            kind = End::Soft;
                            break;
                        }
                    }
                    match tipo {
                        TipoTab::Sinistra => {
                            if let Some(c) = leader {
                                leaders.push((x, stop, c, fr.style));
                            }
                            let mut f = fr.clone();
                            f.w = (stop - x).max(0.0);
                            items.push((f, x));
                            x = stop.max(x);
                        }
                        _ => {
                            let mut f = fr.clone();
                            f.w = 0.0;
                            items.push((f, x));
                            seg = Some(Seg { kind: tipo, pos: stop, leader, from: items.len(), x0: x, style: fr.style });
                        }
                    }
                    i += 1;
                    last_break = Some((items.len(), i));
                }
                K::Space => {
                    items.push((fr.clone(), x));
                    x += fr.w;
                    i += 1;
                    last_break = Some((items.len(), i));
                }
                K::Word | K::Img(..) => {
                    let has = items.len() > content_start;
                    if x + fr.w > right + 0.01 && seg.is_none() {
                        if let Some((n, j)) = last_break.filter(|(n, _)| *n > content_start) {
                            items.truncate(n);
                            // A capo su un trattino facoltativo: il trattino si scrive.
                            if let Some((last, lx)) = items.last().cloned() {
                                if last.shy {
                                    let mut h = last.clone();
                                    h.text = "-".into();
                                    h.note = None;
                                    h.w = self.fonts.width(h.font, "-", h.size);
                                    items.push((h, lx + last.w));
                                    x = lx + last.w + self.fonts.width(last.font, "-", last.size);
                                }
                            }
                            i = j;
                            kind = End::Soft;
                            break;
                        }
                        if has {
                            kind = End::Soft;
                            break;
                        }
                        // Una parola più lunga della riga: si spezza dove finisce lo spazio.
                        if fr.k == K::Word && fr.text.chars().count() > 1 {
                            let mut cut = 0;
                            let mut acc = 0.0;
                            for (bi, c) in fr.text.char_indices() {
                                let cw = self.fonts.width(fr.font, &c.to_string(), fr.size) + self.preps[pi].styles[fr.style].spacing;
                                if acc + cw > right - x && bi > 0 {
                                    break;
                                }
                                acc += cw;
                                cut = bi + c.len_utf8();
                            }
                            if cut < fr.text.len() {
                                let mut a = fr.clone();
                                let mut b = fr.clone();
                                a.text = fr.text[..cut].to_string();
                                b.text = fr.text[cut..].to_string();
                                a.w = acc;
                                b.w = (fr.w - acc).max(0.0);
                                a.brk_after = true;
                                // Due pezzi: la riga dopo riparte dal secondo (l'indice avanza).
                                self.preps[pi].frags[i] = a.clone();
                                self.preps[pi].frags.insert(i + 1, b);
                                items.push((a, x));
                                x += acc;
                                i += 1;
                                kind = End::Soft;
                                break;
                            }
                        }
                    }
                    items.push((fr.clone(), x));
                    x += fr.w;
                    i += 1;
                    if fr.brk_after {
                        last_break = Some((items.len(), i));
                    }
                }
            }
        }
        close(&mut seg, &mut items, &mut x, &mut leaders);
        // Misure della riga.
        let mut asc: f32 = 0.0;
        let mut desc: f32 = 0.0;
        let (mut t_asc, mut t_desc): (f32, f32) = (0.0, 0.0);
        for (f, _) in &items {
            if f.k == K::Tab && f.w == 0.0 {
                continue;
            }
            asc = asc.max(f.asc);
            desc = desc.max(f.desc);
            if !matches!(f.k, K::Img(..)) {
                t_asc = t_asc.max(f.asc);
                t_desc = t_desc.max(f.desc);
            }
        }
        let p = &self.preps[pi];
        if items.is_empty() || kind == End::Last {
            asc = asc.max(p.end_asc);
            desc = desc.max(p.end_desc);
        }
        if t_asc + t_desc <= 0.0 {
            (t_asc, t_desc) = (p.end_asc, p.end_desc);
        }
        // Allineamento: gli spazi in fondo non contano.
        let mut used_end = x;
        for (f, fx) in items.iter().rev() {
            if f.k == K::Space {
                used_end = *fx;
            } else {
                break;
            }
        }
        let after_tab = items.iter().rposition(|(f, _)| f.k == K::Tab).map_or(content_start, |t| t + 1);
        let free = right - used_end;
        match par.allinea {
            Allinea::Centro if free > 0.0 => items.iter_mut().for_each(|it| it.1 += free / 2.0),
            Allinea::Destra if free > 0.0 => items.iter_mut().for_each(|it| it.1 += free),
            Allinea::Giustifica if free > 0.0 && matches!(kind, End::Soft | End::Hard) => {
                let last_word = items.iter().rposition(|(f, _)| f.k != K::Space).unwrap_or(0);
                let spaces = items[after_tab..].iter().enumerate().filter(|(j, (f, _))| f.k == K::Space && after_tab + j < last_word).count();
                if spaces > 0 {
                    let add = free / spaces as f32;
                    let mut acc = 0.0;
                    for (j, it) in items.iter_mut().enumerate() {
                        it.1 += acc;
                        if j >= after_tab && j < last_word && it.0.k == K::Space {
                            it.0.w += add;
                            acc += add;
                        }
                    }
                }
            }
            _ => {}
        }
        Line { items, leaders, asc, desc, t_asc, t_desc, end: i, kind }
    }

    /// L'altezza della riga e la posizione della linea di base dentro di lei.
    fn line_box(par: &Paragrafo, l: &Line) -> (f32, f32) {
        let natural = l.asc + l.desc;
        match par.interlinea {
            // Lo spazio in più va sotto il testo; con meno di una riga si taglia sopra.
            // Writer: il multiplo vale per il testo, un'immagine più alta conta per quel che è.
            Interlinea::Multipla(m) if par.interlinea_testo && l.asc + l.desc > l.t_asc + l.t_desc => {
                let h = ((l.t_asc + l.t_desc) * m.max(0.06)).max(natural);
                (h, l.asc)
            }
            Interlinea::Multipla(m) => {
                let h = natural * m.max(0.06);
                (h, if h >= natural { l.asc } else { h - l.desc })
            }
            // Interlinea esatta: la linea di base a quattro quinti della riga, qualunque sia il carattere.
            Interlinea::Esatta(v) => (v, v * 0.8),
            Interlinea::Minima(v) => {
                let h = natural.max(v);
                (h, h - l.desc)
            }
        }
    }

    /// Disegna una riga di paragrafo: sfondo, bordi, testo, sottolineature, link.
    fn line_unit(&mut self, pi: usize, l: &Line, first: bool, start: usize) -> Unit<'a> {
        let par = self.preps[pi].par;
        let styles = self.preps[pi].styles.clone();
        let (open_top, open_bottom, pw, p_before, p_after) = {
            let p = &self.preps[pi];
            (p.open_top, p.open_bottom, p.w, p.before, p.after)
        };
        let last = l.kind == End::Last;
        let (lh, base) = Self::line_box(par, l);
        let bt = if first && open_top { par.bordi[0].map(|b| b.width + b.space) } else { None };
        let bb = if last && open_bottom { par.bordi[1].map(|b| b.width + b.space) } else { None };
        let top_pad = bt.unwrap_or(0.0);
        let h = top_pad + lh + bb.unwrap_or(0.0);
        let mut d = Draw::default();
        let x_l = par.rientro_sx.min(par.rientro_sx + par.prima_riga.min(0.0)) - par.bordi[2].map_or(0.0, |b| b.space + b.width / 2.0);
        let x_r = pw - par.rientro_dx + par.bordi[3].map_or(0.0, |b| b.space + b.width / 2.0);
        if let Some(c) = par.sfondo {
            d.ops.push(Op::Rect { x: x_l, y: 0.0, w: x_r - x_l, h, color: c });
        }
        if let Some(b) = par.bordi[0].filter(|_| first && open_top) {
            line_op(&mut d, x_l, b.width / 2.0, x_r, b.width / 2.0, &b);
        }
        if let Some(b) = par.bordi[1].filter(|_| last && open_bottom) {
            line_op(&mut d, x_l, h - b.width / 2.0, x_r, h - b.width / 2.0, &b);
        }
        if let Some(b) = par.bordi[2] {
            line_op(&mut d, x_l, 0.0, x_l, h, &b);
        }
        if let Some(b) = par.bordi[3] {
            line_op(&mut d, x_r, 0.0, x_r, h, &b);
        }
        let by = top_pad + base;
        let mut notes = Vec::new();
        // L'ultimo pezzo che non è uno spazio: le sottolineature non continuano sugli spazi in fondo.
        let last_vis = l.items.iter().rposition(|(f, _)| f.k != K::Space).unwrap_or(0);
        for (j, (f, fx)) in l.items.iter().enumerate() {
            let st = &styles[f.style];
            let y = by - f.rise;
            if let Some(n) = f.note {
                notes.push(n);
            }
            if let Some(hc) = st.highlight {
                if f.k != K::Tab || f.w > 0.0 {
                    d.ops.push(Op::Rect { x: *fx, y: by - f.asc + f.rise.max(0.0) * 0.0, w: f.w, h: f.asc + f.desc, color: hc });
                }
            }
            match f.k {
                K::Word | K::Space if !f.text.is_empty() && f.k == K::Word => {
                    let text = match f.field {
                        1 => PAGE_MARK.to_string(),
                        2 => TOTAL_MARK.to_string(),
                        _ => f.text.clone(),
                    };
                    if st.spacing.abs() > 0.01 && f.field == 0 {
                        let mut cx = *fx;
                        for c in text.chars() {
                            let s = c.to_string();
                            let cw = self.fonts.width(f.font, &s, f.size);
                            for (fid, piece) in self.fonts.runs(f.font, &s) {
                                d.ops.push(Op::Text { x: cx, y, font: fid, size: f.size, color: st.color, text: piece });
                            }
                            cx += cw + st.spacing;
                        }
                    } else {
                        let mut cx = *fx;
                        for (fid, piece) in self.fonts.runs(f.font, &text) {
                            let pw = if f.field != 0 { 0.0 } else { self.fonts.face(fid).width(&piece, f.size) };
                            d.ops.push(Op::Text { x: cx, y, font: fid, size: f.size, color: st.color, text: piece });
                            cx += pw;
                        }
                    }
                }
                K::Img(idx, iw, ih) => {
                    if idx != usize::MAX {
                        d.ops.push(Op::Image { index: idx, x: *fx, y: by - ih, w: iw, h: ih });
                    }
                }
                _ => {}
            }
            let decorate = j <= last_vis && !matches!(f.k, K::Img(..)) && (f.k != K::Tab || f.w > 0.0);
            if decorate && (st.underline || st.double_underline) {
                let t = (st.size * 0.06).max(0.4);
                let uy = by + st.size * 0.12;
                d.ops.push(Op::Line { x1: *fx, y1: uy, x2: fx + f.w, y2: uy, width: t, color: st.color, dash: Dash::Solid });
                if st.double_underline {
                    d.ops.push(Op::Line { x1: *fx, y1: uy + t * 2.5, x2: fx + f.w, y2: uy + t * 2.5, width: t, color: st.color, dash: Dash::Solid });
                }
            }
            if decorate && st.strike {
                let t = (st.size * 0.06).max(0.4);
                let sy = y - f.size * 0.28;
                d.ops.push(Op::Line { x1: *fx, y1: sy, x2: fx + f.w, y2: sy, width: t, color: st.color, dash: Dash::Solid });
            }
            if let Some(t) = &f.link {
                if f.w > 0.0 {
                    match d.links.last_mut() {
                        Some(prev) if same_target(&prev.target, t) && (prev.x + prev.w - fx).abs() < 1.0 && (prev.y - (by - f.asc)).abs() < 0.5 => prev.w = fx + f.w - prev.x,
                        _ => d.links.push(Link { x: *fx, y: by - f.asc, w: f.w, h: f.asc + f.desc, target: t.clone() }),
                    }
                }
            }
        }
        for (x0, x1, c, st) in &l.leaders {
            let style = &styles[*st];
            let id = self.font_id(style);
            let size = style.size.max(1.0);
            let cw = self.fonts.width(id, &c.to_string(), size).max(0.5);
            let pad = cw * 0.5;
            let n = ((x1 - x0 - pad * 2.0) / cw).floor();
            if n >= 1.0 {
                let s: String = std::iter::repeat_n(*c, n as usize).collect();
                d.ops.push(Op::Text { x: x1 - pad - n * cw, y: by, font: id, size, color: style.color, text: s });
            }
        }
        if first {
            if let Some(level) = par.titolo {
                let t = par.testo().split_whitespace().collect::<Vec<_>>().join(" ");
                if !t.is_empty() {
                    d.marks.push(Mark { level, title: t, y: 0.0 });
                }
            }
            for s in &par.segnalibri {
                d.anchors.push((s.clone(), 0.0));
            }
        }
        let mut u = Unit::ops(d, h);
        u.notes = notes;
        u.before = if first { p_before } else { 0.0 };
        u.after = if last { p_after } else { 0.0 };
        u.line = Some((pi, start, first));
        u
    }

    /// Tutte le righe di un paragrafo in una cornice larga `w`.
    fn par_units(&mut self, pi: usize) -> Vec<Unit<'a>> {
        let par = self.preps[pi].par;
        let w = self.preps[pi].w;
        let mut out: Vec<Unit<'a>> = Vec::new();
        let mut start = 0;
        let mut first = true;
        let mut brk = if par.salto_prima { Some(Brk::PaginaSePiena) } else { None };
        loop {
            let l = self.break_line(pi, start, f32::MIN, w * 2.0 + 10_000.0, first);
            let mut u = self.line_unit(pi, &l, first, start);
            u.brk = brk.take();
            brk = match l.kind {
                End::Pagina => Some(Brk::Pagina),
                End::Colonna => Some(Brk::Colonna),
                _ => None,
            };
            let done = l.kind == End::Last || l.end >= self.preps[pi].frags.len() && l.kind == End::Soft;
            out.push(u);
            first = false;
            if l.end == start && !matches!(l.kind, End::Hard | End::Pagina | End::Colonna) {
                break;
            }
            start = l.end;
            if done {
                // Una riga che finisce con un salto lascia comunque la riga del segno di paragrafo.
                if l.kind != End::Last {
                    let l2 = self.break_line(pi, start, f32::MIN, w * 2.0 + 10_000.0, false);
                    let mut u2 = self.line_unit(pi, &l2, false, start);
                    u2.brk = brk.take();
                    out.push(u2);
                }
                break;
            }
            if start >= self.preps[pi].frags.len() && l.kind != End::Hard && l.kind != End::Pagina && l.kind != End::Colonna {
                break;
            }
        }
        // Vedove e orfane, righe insieme, insieme al seguente.
        let n = out.len();
        if par.righe_insieme {
            for u in out.iter_mut().take(n.saturating_sub(1)) {
                u.keep = true;
            }
        } else if par.vedove && n >= 2 {
            out[0].keep = true;
            out[n - 2].keep = true;
        }
        if par.insieme_al_seguente {
            if let Some(u) = out.last_mut() {
                u.keep = true;
            }
        }
        out
    }

    // ------------------------------------------------------------ blocchi

    fn blocks_units(&mut self, blocks: &'a [Blocco], w: f32, out: &mut Vec<Unit<'a>>) {
        let mut floats: Vec<&'a Mobile> = Vec::new();
        let pars: Vec<Option<&Paragrafo>> = blocks.iter().map(|b| if let Blocco::Par(p) = b { Some(p) } else { None }).collect();
        let next_par = |i: usize| -> Option<&Paragrafo> {
            for p in pars.iter().skip(i + 1) {
                match p {
                    Some(p) => return Some(p),
                    None => {
                        if !matches!(blocks.get(i + 1), Some(Blocco::Mobile(_))) {
                            return None;
                        }
                    }
                }
            }
            None
        };
        let mut prev_par: Option<&Paragrafo> = None;
        for (i, b) in blocks.iter().enumerate() {
            match b {
                Blocco::Mobile(m) => floats.push(m),
                Blocco::Par(p) => {
                    let pi = self.prepare(p, w);
                    let next = next_par(i);
                    // Spaziatura contestuale: niente spazio tra paragrafi dello stesso stile.
                    if p.contestuale && prev_par.is_some_and(|q| q.stile == p.stile) {
                        self.preps[pi].before = 0.0;
                    }
                    if p.contestuale && next.is_some_and(|q| q.stile == p.stile) {
                        self.preps[pi].after = 0.0;
                    }
                    // Spaziatura automatica: tra due paragrafi conta la più grande; nelle celle
                    // sparisce sopra il primo e sotto l'ultimo.
                    if p.auto_prima && prev_par.is_some_and(|q| q.auto_dopo) {
                        self.preps[pi].before = 0.0;
                    }
                    if self.in_cell > 0 && p.auto_prima && prev_par.is_none() && i == 0 {
                        self.preps[pi].before = 0.0;
                    }
                    if self.in_cell > 0 && p.auto_dopo && next.is_none() && i + 1 == blocks.len() {
                        self.preps[pi].after = 0.0;
                    }
                    // Paragrafi con gli stessi bordi: un riquadro solo.
                    let same = |q: &Paragrafo| q.bordi == p.bordi && q.rientro_sx == p.rientro_sx && q.rientro_dx == p.rientro_dx && p.bordi.iter().any(|b| b.is_some());
                    if prev_par.is_some_and(same) {
                        self.preps[pi].open_top = false;
                    }
                    if next.is_some_and(same) {
                        self.preps[pi].open_bottom = false;
                    }
                    let mut us = self.par_units(pi);
                    if let Some(u) = us.first_mut() {
                        u.floats = std::mem::take(&mut floats);
                    }
                    out.extend(us);
                    prev_par = Some(p);
                }
                Blocco::Tab(t) => {
                    self.table_units(t, w, out);
                    if let Some(u) = out.last_mut() {
                        if !floats.is_empty() {
                            u.floats.append(&mut floats);
                        }
                    }
                    prev_par = None;
                }
            }
        }
        if !floats.is_empty() {
            let mut u = Unit::ops(Draw::default(), 0.0);
            u.floats = floats;
            out.push(u);
        }
    }

    /// Le unità messe una sotto l'altra, come in una cella o in una casella di testo.
    fn stack(&mut self, units: Vec<Unit<'a>>, w: f32) -> (Vec<Slice>, f32) {
        let mut y = 0.0;
        let mut pend: f32 = 0.0;
        let mut out = Vec::new();
        for u in units {
            // Tra due paragrafi vale lo spazio più grande tra «dopo» e «prima».
            y += u.before.max(pend);
            let mut d = Draw::default();
            for m in &u.floats {
                let (fd, mw, mh) = self.float_draw(m);
                let (fx, fy) = local_float(m, mw, mh, w, y);
                d.add(&fd, fx, fy - y);
            }
            match &u.body {
                Body::Ops(o) => d.add(o, 0.0, 0.0),
                Body::Row(r) => d.add(&render_row(r), 0.0, 0.0),
            }
            let mut dd = Draw::default();
            dd.add(&d, 0.0, y);
            out.push(Slice { y0: y, y1: y + u.h, draw: dd });
            y += u.h;
            pend = u.after;
        }
        (out, y + pend)
    }

    fn table_units(&mut self, t: &'a Tabella, w: f32, out: &mut Vec<Unit<'a>>) {
        let ncols = t.griglia.len().max(1);
        let mut grid = t.griglia.clone();
        grid.resize(ncols, 72.0);
        if let Some(f) = t.relativa {
            // Griglia in proporzione: la tabella prende la sua parte dello spazio tra i margini.
            let avail = (w - t.rientro.max(0.0)).max(10.0) * f;
            let sum: f32 = grid.iter().sum::<f32>().max(0.001);
            for g in grid.iter_mut() {
                *g = *g / sum * avail;
            }
        }
        let total: f32 = grid.iter().sum();
        let first_ml = t.righe.first().and_then(|r| r.celle.iter().find(|c| c.span > 0)).map_or(5.4, |c| c.margini[0]);
        let x0 = match t.allinea {
            1 => (w - total) / 2.0,
            2 => w - total,
            _ => t.rientro - first_ml,
        };
        let mut xs = vec![x0];
        for g in &grid {
            xs.push(xs.last().unwrap() + g);
        }
        // Celle disposte sulla griglia.
        struct Cell<'b> {
            c: &'b Cella,
            col: usize,
        }
        let rows: Vec<Vec<Cell>> = t
            .righe
            .iter()
            .map(|r| {
                let mut col = 0;
                r.celle
                    .iter()
                    .map(|c| {
                        let cell = Cell { c, col };
                        col += c.span.max(1);
                        cell
                    })
                    .collect()
            })
            .collect();
        let mut data: Vec<RowData> = Vec::new();
        for (ri, r) in rows.iter().enumerate() {
            let mut cells = Vec::new();
            let mut h: f32 = 0.0;
            for cell in r {
                let c = cell.c;
                let span = c.span.max(1);
                let col = cell.col.min(ncols);
                let end = (col + span).min(ncols);
                let x = xs[col];
                let cw = xs[end.max(col)] - x;
                let [ml, mr, mt, mb] = c.margini;
                let cont = c.vmerge == Some(false);
                let (slices, content_h) = if cont {
                    (Vec::new(), 0.0)
                } else {
                    let mut units = Vec::new();
                    self.in_cell += 1;
                    self.blocks_units(&c.blocchi, (cw - ml - mr).max(4.0), &mut units);
                    self.in_cell -= 1;
                    self.stack(units, (cw - ml - mr).max(4.0))
                };
                let open_bottom = rows.get(ri + 1).is_some_and(|nr| nr.iter().any(|n| n.col == cell.col && n.c.vmerge == Some(false)));
                // I bordi occupano spazio: il testo comincia sotto quello di sopra.
                let mt = mt + if cont { 0.0 } else { c.bordi[0].map_or(0.0, |b| b.width) };
                let mb = mb + if ri + 1 == rows.len() { c.bordi[1].map_or(0.0, |b| b.width) } else { 0.0 };
                if c.vmerge != Some(true) || !open_bottom {
                    if !cont {
                        h = h.max(content_h + mt + mb);
                    }
                }
                cells.push(CellDraw { x, w: cw, ml, mt, mb, fill: c.sfondo, borders: c.bordi, valign: c.valign, slices, content_h, cont, open_bottom, group_h: None });
            }
            let exact = r.first().is_some() && t.righe[ri].altezza < 0.0;
            let want = t.righe[ri].altezza.abs();
            let h = if exact { want } else { h.max(want).max(1.0) };
            data.push(RowData { cells, h, exact });
        }
        // Celle unite in verticale: la riga in fondo al gruppo cresce se il contenuto non ci sta.
        for ri in 0..data.len() {
            for ci in 0..data[ri].cells.len() {
                let c = &data[ri].cells[ci];
                if c.cont || !c.open_bottom {
                    continue;
                }
                let x = c.x;
                let need = c.content_h + c.mt + c.mb;
                let mut last = ri;
                while last + 1 < data.len() && data[last + 1].cells.iter().any(|n| (n.x - x).abs() < 0.5 && n.cont) {
                    last += 1;
                    if !data[last].cells.iter().any(|n| (n.x - x).abs() < 0.5 && n.open_bottom) {
                        break;
                    }
                }
                let sum: f32 = data[ri..=last].iter().map(|r| r.h).sum();
                if need > sum && !data[last].exact {
                    data[last].h += need - sum;
                }
                let total: f32 = data[ri..=last].iter().map(|r| r.h).sum();
                data[ri].cells[ci].group_h = Some(total);
            }
        }
        let heads = t.righe.iter().take_while(|r| r.intestazione).count();
        let head_units: Vec<Unit<'a>> = data[..heads.min(data.len())]
            .iter()
            .map(|r| {
                let mut u = Unit::ops(Draw::default(), r.h);
                u.body = Body::Row(r.clone());
                u
            })
            .collect();
        let head = if heads > 0 && heads < data.len() { Some(std::rc::Rc::new(head_units)) } else { None };
        let in_group: Vec<bool> = data.iter().map(|r| r.cells.iter().any(|c| c.open_bottom || c.cont)).collect();
        for (ri, r) in data.into_iter().enumerate() {
            let mut u = Unit::ops(Draw::default(), r.h);
            u.split = !t.righe[ri].non_dividere && !r.exact && !in_group[ri];
            u.body = Body::Row(r);
            if ri < heads {
                u.keep = true;
            } else {
                u.head = head.clone();
            }
            out.push(u);
        }
    }

    /// Il contenuto di un oggetto mobile, con l'origine nel suo angolo in alto a sinistra, e la sua misura.
    fn float_draw(&mut self, m: &'a Mobile) -> (Draw, f32, f32) {
        let mut d = Draw::default();
        let (mut w, mut h) = (m.w, m.h.abs());
        match &m.contenuto {
            MobileContenuto::Immagine(i) => d.ops.push(Op::Image { index: *i, x: 0.0, y: 0.0, w: m.w, h: m.h }),
            MobileContenuto::Testo(blocks, fill, line) => {
                if let Some(c) = fill {
                    d.ops.push(Op::Rect { x: 0.0, y: 0.0, w: m.w, h: m.h, color: *c });
                }
                let iw = (m.w - 14.4).max(4.0);
                let mut units = Vec::new();
                self.blocks_units(blocks, iw, &mut units);
                let (slices, _) = self.stack(units, iw);
                d.ops.push(Op::Clip { x: 0.0, y: 0.0, w: m.w, h: m.h });
                for s in &slices {
                    d.add(&s.draw, 7.2, 3.6);
                }
                d.ops.push(Op::Unclip);
                if let Some(l) = line {
                    let hw = l.width / 2.0;
                    for (a, b, c, e) in [(hw, hw, m.w - hw, hw), (hw, m.h - hw, m.w - hw, m.h - hw), (hw, hw, hw, m.h - hw), (m.w - hw, hw, m.w - hw, m.h - hw)] {
                        line_op(&mut d, a, b, c, e, l);
                    }
                }
            }
            MobileContenuto::Cornice(blocks) => {
                let mut fw = if m.w > 0.0 { m.w } else { self.col_w.max(20.0) };
                let mut units = Vec::new();
                self.blocks_units(blocks, fw, &mut units);
                let (mut slices, mut ch) = self.stack(units, fw);
                if m.w <= 0.0 {
                    // Larga quanto il testo: si misura e si rifà a quella larghezza.
                    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
                    for s in &slices {
                        for op in &s.draw.ops {
                            if let Op::Text { x, font, size, text, .. } = op {
                                lo = lo.min(*x);
                                hi = hi.max(x + self.fonts.face(*font).width(text, *size));
                            }
                        }
                    }
                    if hi > lo {
                        fw = (hi - lo + 0.5).min(fw);
                        let mut units = Vec::new();
                        self.blocks_units(blocks, fw, &mut units);
                        (slices, ch) = self.stack(units, fw);
                    }
                }
                w = fw;
                h = if m.h < 0.0 { -m.h } else { ch.max(m.h) };
                if m.h < 0.0 {
                    d.ops.push(Op::Clip { x: 0.0, y: 0.0, w, h });
                }
                for s in &slices {
                    d.add(&s.draw, 0.0, 0.0);
                }
                if m.h < 0.0 {
                    d.ops.push(Op::Unclip);
                }
            }
        }
        (d, w, h)
    }

    // ------------------------------------------------------------ note

    fn note_draw(&mut self, i: usize, w: f32) -> (f32, Draw) {
        if let Some(v) = self.note_cache.get(&i) {
            return v.clone();
        }
        let doc = self.doc;
        let mut units = Vec::new();
        if let Some((_, blocks)) = doc.note.get(i) {
            self.blocks_units(blocks, w, &mut units);
        }
        let (slices, h) = self.stack(units, w);
        let mut d = Draw::default();
        for s in &slices {
            d.add(&s.draw, 0.0, 0.0);
        }
        self.note_cache.insert(i, (h, d.clone()));
        (h, d)
    }

    // ------------------------------------------------------------ pagine

    fn header_height(&mut self, blocks: &'a Option<Vec<Blocco>>, w: f32) -> f32 {
        let Some(b) = blocks else { return 0.0 };
        let saved = self.fields.replace(("99".into(), "99".into()));
        let mut units = Vec::new();
        self.blocks_units(b, w, &mut units);
        self.fields = saved;
        let (_, h) = self.stack(units, w);
        h
    }

    fn variant(s: &'a Sezione, first: bool, even: bool, pari_dispari: bool, head: bool) -> &'a Option<Vec<Blocco>> {
        let i = if head { &s.testa } else { &s.piede };
        if first && s.prima_diversa {
            &i.prima
        } else if even && pari_dispari {
            &i.pari
        } else {
            &i.normale
        }
    }

    fn new_page(&mut self, natural: bool) {
        let doc = self.doc;
        let s = &doc.sezioni[self.sez];
        let first = !self.pages.iter().any(|p| p.sez == self.sez);
        let num = match (first, s.primo_numero) {
            (true, Some(n)) => n,
            _ => self.pages.last().map_or(1, |p| p.num + 1),
        };
        let tw = s.w - s.margini[0] - s.margini[1];
        let hh = self.header_height(Self::variant(s, first, num % 2 == 0, doc.pari_dispari, true), tw);
        let fh = self.header_height(Self::variant(s, first, num % 2 == 0, doc.pari_dispari, false), tw);
        let top = s.margini[2].max(s.dist_testa + hh);
        let bottom = s.h - s.margini[3].max(s.dist_piede + fh);
        self.geo = Geo { w: s.w, h: s.h, ml: s.margini[0], mr: s.margini[1], top, bottom };
        self.pages.push(PageInfo { p: Pagina::new(s.w, s.h), sez: self.sez, num, first_of_section: first, notes: Vec::new(), notes_h: 0.0, body_bottom: bottom, back: Vec::new(), zones: Vec::new() });
        self.y = top;
        self.cols_top = top;
        self.col = 0;
        self.pending = 0.0;
        self.at_top = true;
        self.natural = natural;
        self.balance_bottom = None;
    }

    fn next_column(&mut self, natural: bool) {
        if self.col + 1 < self.ncols {
            self.col += 1;
            self.y = self.cols_top;
            self.pending = 0.0;
            self.at_top = true;
            self.natural = natural;
        } else {
            self.new_page(natural);
        }
    }

    fn col_x(&self) -> f32 {
        self.geo.ml + self.col as f32 * (self.col_w + self.col_gap)
    }

    fn bottom(&self) -> f32 {
        let p = self.pages.last().unwrap();
        let b = self.geo.bottom - if p.notes_h > 0.0 { p.notes_h + NOTE_SEP } else { 0.0 };
        match self.balance_bottom {
            Some(bb) => bb.min(b),
            None => b,
        }
    }

    fn draw_on_page(&mut self, d: &Draw, dx: f32, dy: f32, back: bool) {
        let p = self.pages.last_mut().unwrap();
        if back {
            p.back.extend(d.ops.iter().map(|o| shift(o, dx, dy)));
        } else {
            p.p.ops.extend(d.ops.iter().map(|o| shift(o, dx, dy)));
        }
        p.p.links.extend(d.links.iter().map(|l| Link { x: l.x + dx, y: l.y + dy, ..l.clone() }));
        p.p.marks.extend(d.marks.iter().map(|m| Mark { y: m.y + dy, ..m.clone() }));
        p.p.anchors.extend(d.anchors.iter().map(|(n, y)| (n.clone(), y + dy)));
    }

    /// Posizione di un oggetto mobile sulla pagina (il paragrafo comincia a `par_y`).
    fn float_pos(&self, m: &Mobile, mw: f32, mh: f32, par_y: f32) -> (f32, f32) {
        let g = self.geo;
        let cx = self.col_x();
        let hx = |b: Base| -> (f32, f32) {
            match b {
                Base::Pagina => (0.0, g.w),
                Base::Margine => (g.ml, g.w - g.mr),
                Base::Colonna | Base::Paragrafo => (cx, cx + self.col_w),
            }
        };
        let x = match m.x {
            Riferimento::Spostato(b, o) => hx(b).0 + o,
            Riferimento::Allineato(b, a) => {
                let (l, r) = hx(b);
                match a {
                    1 => (l + r - mw) / 2.0,
                    2 => r - mw,
                    _ => l,
                }
            }
        };
        let vy = |b: Base| -> (f32, f32) {
            match b {
                Base::Pagina => (0.0, g.h),
                Base::Margine => (g.top, g.bottom),
                Base::Colonna | Base::Paragrafo => (par_y, par_y),
            }
        };
        let y = match m.y {
            Riferimento::Spostato(b, o) => vy(b).0 + o,
            Riferimento::Allineato(b, a) => {
                let (t, bt) = vy(b);
                match a {
                    1 => (t + bt - mh) / 2.0,
                    2 => bt - mh,
                    _ => t,
                }
            }
        };
        (x, y.min(g.h - mh).max(0.0))
    }

    fn place_float(&mut self, m: &'a Mobile, par_y: f32) {
        let (d, mw, mh) = self.float_draw(m);
        let (x, y) = self.float_pos(m, mw, mh, par_y);
        self.draw_on_page(&d, x, y, m.dietro);
        if m.avvolge != Avvolge::Nessuno {
            let gap = if matches!(m.contenuto, MobileContenuto::Cornice(_)) { 0.0 } else { 9.0 };
            self.pages.last_mut().unwrap().zones.push(Zone { x0: x - gap, y0: y, x1: x + mw + gap, y1: y + mh, sides: m.avvolge == Avvolge::Lati });
        }
    }

    /// I limiti a sinistra e a destra (nella colonna) per una riga tra `y0` e `y1`.
    fn limits(&self, y0: f32, y1: f32) -> (f32, f32, Option<f32>) {
        let cx = self.col_x();
        let (mut l, mut r) = (f32::MIN, f32::MAX);
        let mut below: Option<f32> = None;
        for z in &self.pages.last().unwrap().zones {
            if z.y1 <= y0 || z.y0 >= y1 || z.x1 <= cx || z.x0 >= cx + self.col_w {
                continue;
            }
            if !z.sides {
                below = Some(below.map_or(z.y1, |b: f32| b.max(z.y1)));
                continue;
            }
            let left_room = z.x0 - cx;
            let right_room = cx + self.col_w - z.x1;
            if right_room >= left_room {
                l = l.max(z.x1 - cx);
            } else {
                r = r.min(z.x0 - cx);
            }
        }
        if r - l.max(0.0) < 36.0 && (l > f32::MIN || r < f32::MAX) {
            let lowest = self.pages.last().unwrap().zones.iter().filter(|z| !(z.y1 <= y0 || z.y0 >= y1)).map(|z| z.y1).fold(y0, f32::max);
            below = Some(lowest);
        }
        (l, r, below)
    }

    fn chain_height(&self, u: &Unit, rest: &VecDeque<Unit<'a>>) -> f32 {
        let mut h = u.before + u.h;
        if !u.keep {
            return h;
        }
        let mut prev_after = u.after;
        for n in rest {
            if n.brk.is_some() {
                break;
            }
            h += prev_after + n.before + n.h;
            prev_after = n.after;
            if !n.keep {
                break;
            }
        }
        h
    }

    fn notes_need(&mut self, u: &Unit) -> f32 {
        let mut h = 0.0;
        let w = self.col_w;
        for n in &u.notes {
            if !self.pages.last().unwrap().notes.contains(n) {
                h += self.note_draw(*n, w).0;
            }
        }
        h
    }

    fn place(&mut self, mut u: Unit<'a>, rest: &mut VecDeque<Unit<'a>>) {
        match u.brk {
            Some(Brk::Pagina) => self.new_page(false),
            Some(Brk::PaginaSePiena) if !(self.at_top && self.col == 0) || self.pages.last().is_some_and(|p| !p.p.ops.is_empty() && !self.at_top) => self.new_page(false),
            Some(Brk::Colonna) => self.next_column(false),
            _ => {}
        }
        // Una catena di righe da tenere insieme che non ci sta: si comincia nella colonna dopo.
        if !self.prev_keep && u.keep && !self.at_top {
            let chain = self.chain_height(&u, rest);
            let full = self.bottom() - self.cols_top;
            if self.y + chain > self.bottom() + 0.01 && chain <= full {
                self.next_column(true);
            }
        }
        let mut guard = 0;
        loop {
            guard += 1;
            let before = if self.at_top && self.natural { 0.0 } else { u.before.max(self.pending) };
            let notes = if u.notes.is_empty() { 0.0 } else { self.notes_need(&u) + if self.pages.last().unwrap().notes_h == 0.0 { NOTE_SEP } else { 0.0 } };
            let limit = self.bottom() - notes;
            let need = before + u.h;
            if self.y + need > limit + 0.01 && !(self.at_top && guard > 1) && guard < 4 {
                // Una riga di tabella si divide tra le pagine.
                if u.split {
                    if let Body::Row(r) = &u.body {
                        if let Some((a, b)) = split_row(r, limit - self.y - before) {
                            let mut first = u.clone();
                            first.h = a.h;
                            first.body = Body::Row(a);
                            first.floats.clear();
                            let mut second = u.clone();
                            second.h = b.h;
                            second.body = Body::Row(b);
                            second.floats.clear();
                            second.brk = None;
                            second.before = 0.0;
                            let floats = std::mem::take(&mut u.floats);
                            first.floats = floats;
                            first.split = false;
                            self.put(first, before);
                            self.next_column(true);
                            self.repeat_head(&second);
                            u = second;
                            continue;
                        }
                    }
                }
                if !self.at_top {
                    self.next_column(true);
                    self.repeat_head(&u);
                    continue;
                }
            }
            // Oggetti mobili ancorati qui.
            let floats = std::mem::take(&mut u.floats);
            for m in floats {
                self.place_float(m, self.y + before);
            }
            // Il testo attorno agli oggetti: la riga si rifà con i limiti della pagina.
            if let Some((pi, start, first)) = u.line {
                let y0 = self.y + before;
                let (l, r, below) = self.limits(y0, y0 + u.h);
                if let Some(b) = below {
                    if b > self.y && b < self.bottom() {
                        self.y = b;
                        continue;
                    }
                }
                if l > f32::MIN || r < f32::MAX {
                    // Le righe seguenti dello stesso paragrafo si rifanno una alla volta.
                    while rest.front().is_some_and(|n| n.line.is_some_and(|x| x.0 == pi)) {
                        rest.pop_front();
                    }
                    self.rewrap(pi, start, first, u.brk.is_some(), before);
                    self.prev_keep = false;
                    return;
                }
            }
            self.put(u, before);
            return;
        }
    }

    fn repeat_head(&mut self, u: &Unit<'a>) {
        if let Some(h) = &u.head {
            for hu in h.iter() {
                let mut c = hu.clone();
                c.keep = false;
                self.put(c, 0.0);
            }
        }
    }

    /// Le righe di un paragrafo una alla volta, con i limiti degli oggetti sulla pagina.
    fn rewrap(&mut self, pi: usize, mut start: usize, mut first: bool, _brk: bool, mut before: f32) {
        let par = self.preps[pi].par;
        for _ in 0..10_000 {
            let est = par.font_fine.size * 1.2;
            let y0 = self.y + before;
            let (l, r, below) = self.limits(y0, y0 + est);
            if let Some(b) = below {
                if b > self.y && b + est < self.bottom() {
                    self.y = b;
                    continue;
                }
            }
            let l = self.break_line(pi, start, l.max(f32::MIN), r, first);
            let mut u = self.line_unit(pi, &l, first, start);
            u.line = None;
            if !first {
                u.before = 0.0;
            }
            let end = l.end;
            let kind = l.kind.clone();
            if self.y + before + u.h > self.bottom() + 0.01 && !self.at_top {
                self.next_column(true);
                before = 0.0;
                continue;
            }
            self.put(u, before);
            before = 0.0;
            first = false;
            match kind {
                End::Last => return,
                End::Pagina => self.new_page(false),
                End::Colonna => self.next_column(false),
                _ => {}
            }
            if end == start && kind == End::Soft {
                return;
            }
            start = end;
        }
    }

    fn put(&mut self, u: Unit<'a>, before: f32) {
        let x = self.col_x();
        let y = self.y + before;
        for m in &u.floats {
            self.place_float(m, y);
        }
        match &u.body {
            Body::Ops(d) => self.draw_on_page(d, x, y, false),
            Body::Row(r) => {
                let d = render_row(r);
                self.draw_on_page(&d, x, y, false);
            }
        }
        let w = self.col_w;
        for n in &u.notes {
            if !self.pages.last().unwrap().notes.contains(n) {
                let h = self.note_draw(*n, w).0;
                let p = self.pages.last_mut().unwrap();
                p.notes.push(*n);
                p.notes_h += h;
            }
        }
        self.y = y + u.h;
        self.pending = u.after;
        self.at_top = false;
        self.prev_keep = u.keep;
    }

    /// Altezza delle colonne di una sezione continua che finisce: le colonne si pareggiano.
    fn balanced(&self, units: &[Unit<'a>], avail: f32) -> Option<f32> {
        if self.ncols < 2 || units.is_empty() || units.iter().any(|u| u.brk.is_some()) {
            return None;
        }
        let total: f32 = units.iter().map(|u| u.before + u.h + u.after).sum();
        if total > avail * self.ncols as f32 {
            return None;
        }
        let fits = |hh: f32| -> bool {
            let mut col = 0;
            let mut y = 0.0;
            for u in units {
                let need = u.before + u.h;
                if y + need > hh + 0.01 && y > 0.0 {
                    col += 1;
                    y = 0.0;
                    if col >= self.ncols {
                        return false;
                    }
                }
                y += need + u.after;
            }
            true
        };
        let mut lo = total / self.ncols as f32;
        let mut hi = avail;
        if !fits(hi) {
            return None;
        }
        for _ in 0..30 {
            let mid = (lo + hi) / 2.0;
            if fits(mid) {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        Some(hi)
    }

    // ------------------------------------------------------------ tutto il documento

    pub fn impagina(mut self, title: &str) -> Stampato {
        let doc = self.doc;
        let nsez = doc.sezioni.len();
        for si in 0..nsez {
            let s = &doc.sezioni[si];
            let prev_cols = self.ncols;
            self.sez = si;
            self.ncols = s.colonne.max(1) as usize;
            self.col_gap = s.spazio_colonne;
            let tw = s.w - s.margini[0] - s.margini[1];
            self.col_w = ((tw - self.col_gap * (self.ncols as f32 - 1.0)) / self.ncols as f32).max(20.0);
            let continuous = si > 0 && !s.pagina_nuova && !self.pages.is_empty() && (doc.sezioni[si - 1].w - s.w).abs() < 1.0;
            if !continuous {
                if self.pages.is_empty() || !(self.at_top && self.col == 0 && self.pages.last().is_some_and(|p| p.p.ops.is_empty())) {
                    self.new_page(false);
                } else {
                    // La pagina vuota prende la sezione nuova.
                    self.pages.pop();
                    self.new_page(false);
                }
                let want_odd = match s.parita {
                    1 => Some(true),
                    2 => Some(false),
                    _ => None,
                };
                if let Some(odd) = want_odd {
                    let n = self.pages.last().unwrap().num;
                    if (n % 2 == 1) != odd && self.pages.len() > 1 {
                        self.new_page(false);
                    }
                }
            } else {
                // Sezione continua: le colonne nuove cominciano dove finisce il testo.
                if prev_cols > 1 || self.ncols > 1 {
                    let lowest = self.y;
                    self.cols_top = lowest;
                    self.col = 0;
                }
                self.balance_bottom = None;
            }
            let mut units = Vec::new();
            self.blocks_units(&s.blocchi, self.col_w, &mut units);
            if si + 1 == nsez && !doc.note_fine.is_empty() {
                let mut d = Draw::default();
                d.ops.push(Op::Line { x1: 0.0, y1: 7.0, x2: 144.0, y2: 7.0, width: 0.5, color: (0, 0, 0), dash: Dash::Solid });
                units.push(Unit::ops(d, NOTE_SEP));
                for (_, blocks) in &doc.note_fine {
                    self.blocks_units(blocks, self.col_w, &mut units);
                }
            }
            // Colonne pareggiate prima di una sezione continua.
            let next_continuous = doc.sezioni.get(si + 1).is_some_and(|n| !n.pagina_nuova);
            if next_continuous && self.ncols > 1 {
                let avail = self.bottom() - self.cols_top;
                self.balance_bottom = self.balanced(&units, avail).map(|h| self.cols_top + h);
            }
            let mut q: VecDeque<Unit<'a>> = units.into();
            while let Some(u) = q.pop_front() {
                self.place(u, &mut q);
            }
            if self.balance_bottom.take().is_some() {
                // Il testo dopo riparte sotto la colonna più lunga.
                self.y = self.y.max(self.cols_top);
                let lowest = self.pages.last().unwrap().p.ops.iter().filter_map(|o| match o {
                    Op::Text { y, .. } => Some(*y),
                    _ => None,
                });
                let _ = lowest;
            }
            if self.ncols > 1 && next_continuous {
                // In fondo alle colonne pareggiate.
                self.y = self.balance_end();
                self.col = 0;
            }
        }
        if self.pages.is_empty() {
            self.new_page(false);
        }
        self.finish(title)
    }

    fn balance_end(&self) -> f32 {
        let p = self.pages.last().unwrap();
        let mut y = self.cols_top;
        for o in &p.p.ops {
            let b = match o {
                Op::Text { y, size, .. } => y + size * 0.3,
                Op::Rect { y, h, .. } => y + h,
                Op::Image { y, h, .. } => y + h,
                Op::Line { y1, y2, .. } => y1.max(*y2),
                _ => continue,
            };
            if b > y && b < self.geo.bottom {
                y = b;
            }
        }
        y + 6.0
    }

    fn finish(mut self, title: &str) -> Stampato {
        let doc = self.doc;
        let total = self.pages.len();
        let total_s = total.to_string();
        for i in 0..total {
            let (sez, num, first, notes, body_bottom, notes_h) = {
                let p = &self.pages[i];
                (p.sez, p.num, p.first_of_section, p.notes.clone(), p.body_bottom, p.notes_h)
            };
            let s = &doc.sezioni[sez];
            let tw = s.w - s.margini[0] - s.margini[1];
            let num_s = num.to_string();
            // Il segnaposto del numero di pagina nel corpo.
            for op in &mut self.pages[i].p.ops {
                if let Op::Text { text, .. } = op {
                    if text.contains(PAGE_MARK) || text.contains(TOTAL_MARK) {
                        *text = text.replace(PAGE_MARK, &num_s).replace(TOTAL_MARK, &total_s);
                    }
                }
            }
            // Intestazione e piè di pagina con i numeri veri.
            self.fields = Some((num_s.clone(), total_s.clone()));
            self.geo = Geo { w: s.w, h: s.h, ml: s.margini[0], mr: s.margini[1], top: s.margini[2], bottom: body_bottom };
            self.col = 0;
            self.col_w = tw;
            let mut hf_ops: Vec<Op> = Vec::new();
            let mut hf_back: Vec<Op> = Vec::new();
            for head in [true, false] {
                let Some(blocks) = Self::variant(s, first, num % 2 == 0, doc.pari_dispari, head) else { continue };
                let mut units = Vec::new();
                self.blocks_units(blocks, tw, &mut units);
                let total_h: f32 = units.iter().fold((0.0f32, 0.0f32), |(y, pend), u| (y + u.before.max(pend) + u.h, u.after)).0;
                let mut y = if head { s.dist_testa } else { (s.h - s.dist_piede - total_h).min(s.h - s.dist_piede - 1.0) };
                let mut pend: f32 = 0.0;
                for u in units {
                    y += u.before.max(pend);
                    for m in &u.floats {
                        let (d, mw, mh) = self.float_draw(m);
                        let (fx, fy) = self.float_pos(m, mw, mh, y);
                        let target = if m.dietro { &mut hf_back } else { &mut hf_ops };
                        target.extend(d.ops.iter().map(|o| shift(o, fx, fy)));
                    }
                    let d = match &u.body {
                        Body::Ops(d) => d.clone(),
                        Body::Row(r) => render_row(r),
                    };
                    hf_ops.extend(d.ops.iter().map(|o| shift(o, s.margini[0], y)));
                    let page = &mut self.pages[i].p;
                    page.links.extend(d.links.iter().map(|l| Link { x: l.x + s.margini[0], y: l.y + y, ..l.clone() }));
                    y += u.h;
                    pend = u.after;
                }
            }
            // Note a piè di pagina, sopra il margine sotto.
            if !notes.is_empty() {
                let mut y = body_bottom - notes_h - NOTE_SEP;
                let x = s.margini[0];
                hf_ops.push(Op::Line { x1: x, y1: y + 7.0, x2: x + 144.0, y2: y + 7.0, width: 0.5, color: (0, 0, 0), dash: Dash::Solid });
                y += NOTE_SEP;
                for n in notes {
                    let (h, d) = self.note_draw(n, tw);
                    hf_ops.extend(d.ops.iter().map(|o| shift(o, x, y)));
                    let page = &mut self.pages[i].p;
                    page.links.extend(d.links.iter().map(|l| Link { x: l.x + x, y: l.y + y, ..l.clone() }));
                    page.anchors.extend(d.anchors.iter().map(|(a, ay)| (a.clone(), ay + y)));
                    y += h;
                }
            }
            let p = &mut self.pages[i];
            let mut ops = std::mem::take(&mut p.back);
            ops.append(&mut hf_back);
            ops.append(&mut hf_ops);
            ops.append(&mut p.p.ops);
            p.p.ops = ops;
        }
        let images = doc.immagini.clone();
        Stampato { pages: self.pages.into_iter().map(|p| p.p).collect(), images, fonts: self.fonts, title: if doc.titolo.is_empty() { title.to_string() } else { doc.titolo.clone() } }
    }
}

fn same_target(a: &Target, b: &Target) -> bool {
    match (a, b) {
        (Target::Uri(x), Target::Uri(y)) => x == y,
        (Target::Anchor(x), Target::Anchor(y)) => x == y,
        _ => false,
    }
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF)
}

fn line_op(d: &mut Draw, x1: f32, y1: f32, x2: f32, y2: f32, b: &Linea) {
    if b.width <= 0.0 {
        return;
    }
    if b.double {
        let t = (b.width / 3.0).max(0.25);
        let (ox, oy) = if (y1 - y2).abs() < 0.01 { (0.0, t) } else { (t, 0.0) };
        d.ops.push(Op::Line { x1: x1 - ox, y1: y1 - oy, x2: x2 - ox, y2: y2 - oy, width: t, color: b.color, dash: b.dash });
        d.ops.push(Op::Line { x1: x1 + ox, y1: y1 + oy, x2: x2 + ox, y2: y2 + oy, width: t, color: b.color, dash: b.dash });
    } else {
        d.ops.push(Op::Line { x1, y1, x2, y2, width: b.width, color: b.color, dash: b.dash });
    }
}

/// Un oggetto mobile dentro una cella o una casella: posizione rispetto al contenuto.
fn local_float(m: &Mobile, mw: f32, _mh: f32, w: f32, par_y: f32) -> (f32, f32) {
    let x = match m.x {
        Riferimento::Spostato(_, o) => o,
        Riferimento::Allineato(_, a) => match a {
            1 => (w - mw) / 2.0,
            2 => w - mw,
            _ => 0.0,
        },
    };
    let y = match m.y {
        Riferimento::Spostato(_, o) => par_y + o,
        Riferimento::Allineato(..) => par_y,
    };
    (x, y.max(0.0))
}

fn render_row(r: &RowData) -> Draw {
    let mut d = Draw::default();
    for c in &r.cells {
        let hh = c.group_h.unwrap_or(r.h);
        if c.cont {
            if let Some(f) = c.fill {
                d.ops.push(Op::Rect { x: c.x, y: 0.0, w: c.w, h: r.h, color: f });
            }
        } else {
            if let Some(f) = c.fill {
                d.ops.push(Op::Rect { x: c.x, y: 0.0, w: c.w, h: hh, color: f });
            }
            let avail = hh - c.mt - c.mb;
            let dy = c.mt
                + match c.valign {
                    1 => ((avail - c.content_h) / 2.0).max(0.0),
                    2 => (avail - c.content_h).max(0.0),
                    _ => 0.0,
                };
            if r.exact {
                d.ops.push(Op::Clip { x: c.x, y: 0.0, w: c.w, h: hh });
            }
            for s in &c.slices {
                d.add(&s.draw, c.x + c.ml, dy);
            }
            if r.exact {
                d.ops.push(Op::Unclip);
            }
        }
    }
    // I bordi sopra a tutto.
    for c in &r.cells {
        let h = if c.cont { r.h } else { r.h.max(0.0) };
        if let Some(b) = c.borders[0].filter(|_| !c.cont) {
            line_op(&mut d, c.x, 0.0, c.x + c.w, 0.0, &b);
        }
        if let Some(b) = c.borders[1].filter(|_| !c.open_bottom) {
            line_op(&mut d, c.x, h, c.x + c.w, h, &b);
        }
        if let Some(b) = c.borders[2] {
            line_op(&mut d, c.x, 0.0, c.x, h, &b);
        }
        if let Some(b) = c.borders[3] {
            line_op(&mut d, c.x + c.w, 0.0, c.x + c.w, h, &b);
        }
    }
    d
}

/// Divide una riga di tabella: la prima parte alta al più `avail`.
fn split_row(r: &RowData, avail: f32) -> Option<(RowData, RowData)> {
    if avail < 8.0 {
        return None;
    }
    let mut a = r.clone();
    let mut b = r.clone();
    let mut any = false;
    let mut ha: f32 = 0.0;
    let mut hb: f32 = 0.0;
    for (i, c) in r.cells.iter().enumerate() {
        let room = avail - c.mt - c.mb;
        let k = c.slices.iter().take_while(|s| s.y1 <= room + 0.01).count();
        if k > 0 {
            any = true;
        }
        let cut = c.slices.get(k).map_or(c.content_h, |s| s.y0);
        let first: Vec<Slice> = c.slices[..k].to_vec();
        let rest: Vec<Slice> = c.slices[k..]
            .iter()
            .map(|s| {
                let mut d = Draw::default();
                d.add(&s.draw, 0.0, -cut);
                Slice { y0: s.y0 - cut, y1: s.y1 - cut, draw: d }
            })
            .collect();
        let used = first.last().map_or(0.0, |s| s.y1);
        a.cells[i].slices = first;
        a.cells[i].content_h = used;
        a.cells[i].valign = 0;
        a.cells[i].group_h = None;
        b.cells[i].content_h = (c.content_h - cut).max(0.0);
        b.cells[i].slices = rest;
        b.cells[i].group_h = None;
        if !c.cont {
            ha = ha.max(used + c.mt + c.mb);
            hb = hb.max(b.cells[i].content_h + c.mt + c.mb);
        }
    }
    if !any || r.cells.iter().all(|c| c.slices.iter().all(|s| s.y1 <= avail - c.mt - c.mb + 0.01)) {
        return None;
    }
    a.h = avail.max(ha);
    b.h = hb.max(1.0);
    Some((a, b))
}

/// Il documento messo in pagina.
pub fn impagina(doc: &Documento, fonts: Fonti, title: &str) -> Stampato {
    let mut doc_fix;
    let doc = if doc.sezioni.is_empty() {
        doc_fix = doc.clone();
        doc_fix.sezioni.push(Sezione::default());
        &doc_fix
    } else {
        doc
    };
    Impaginatore::new(doc, fonts).impagina(title)
}
