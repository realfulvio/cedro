//! Le pagine dell'editor dei PDF: la zona al centro che scorre, con la pagina vera (disegnata dal thread di
//! fondo), il testo che si sta scrivendo, il paragrafo sotto il mouse, quel che si trascina e quel che è scelto.
//! Scrive in `PdfEd::lay` dove stanno le pagine sullo schermo: i clic e i tasti lo leggono da lì.

use super::pdfeditor::{rgb, E, AMBER, BLUE, WHITE};
use crate::pdfmod::{Edit, Rect, ShapeKind};
use crate::ui::core::{inside, Act, Field, Frame, HitKind, Scroll, BOLD, R, SANS, SEMI};
use crate::ui::gfx::{Align as TA, Fit, Weight};
use crate::ui::model::App;
use crate::ui::pdfedact::{EdAct, EdField};
use crate::ui::pdfeditor::{Drag, PdfEd, Target, TextEdit, Tool, MARK_COLORS};
use crate::ui::theme::mix;

/// Lo spazio attorno alle pagine e tra una e l'altra.
const MARGIN: f32 = 24.0;
const GAP_X: f32 = 24.0;
const LABEL: f32 = 26.0;
const GAP_Y: f32 = 18.0;
const BANNER: f32 = 36.0;

fn clip(a: R, b: R) -> R {
    let (x0, y0) = (a.0.max(b.0), a.1.max(b.1));
    let (x1, y1) = ((a.0 + a.2).min(b.0 + b.2), (a.1 + a.3).min(b.1 + b.3));
    (x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
}

/// Il riquadro tratteggiato (quello che si sta tirando, la cosa scelta).
fn dashed(f: &mut Frame, r: R, c: (u8, u8, u8), lw: f32) {
    f.p.rrect_line(r.0, r.1, r.2, r.3, 2.0, c, lw, 1.0, Some((6.0, 4.0)));
}

pub(super) fn canvas(f: &mut Frame, app: &App, ed: &PdfEd, e: E, r: R) {
    f.p.fill(r.0, r.1, r.2, r.3, e.e2);
    // La riga sopra la pagina: cosa si sta facendo (scrivere dentro il file, l'oscuramento fatto…).
    let msg = (ed.text.is_some() || ed.field.is_some() || ed.applied.is_some()).then(|| ed.banner());
    let top = match &msg {
        Some(m) => {
            f.p.fill(r.0, r.1, r.2, BANNER, mix(e.e2, WHITE, 0.07));
            f.p.fill(r.0, r.1 + BANNER - 1.0, r.2, 1.0, mix(e.e2, WHITE, 0.09));
            f.p.circle(r.0 + 20.0, r.1 + BANNER / 2.0, 4.0, Some(e.acc), None);
            f.p.text(m, r.0 + 34.0, r.1, r.2 - 48.0, BANNER, 13.5, SEMI, e.ink, TA::Left);
            BANNER
        }
        None => 0.0,
    };
    let cv = (r.0, r.1 + top, r.2, r.3 - top);
    {
        let mut lay = ed.lay.borrow_mut();
        lay.canvas = cv;
        lay.pages.clear();
        lay.wants.clear();
        lay.texts.clear();
        lay.edit_box = None;
    }
    let n = ed.page_count();
    if ed.opened.is_none() || n == 0 {
        f.spinner(cv.0 + cv.2 / 2.0, cv.1 + cv.3 / 2.0, 18.0, e.acc);
        f.p.text("Apro il file…", cv.0, cv.1 + cv.3 / 2.0 + 32.0, cv.2, 24.0, 14.0, SEMI, e.muted, TA::Center);
        f.animating = true;
        return;
    }
    // La misura: larga quanto la zona (o lo zoom scelto).
    let cols = if ed.two && n > 1 { 2usize } else { 1 };
    let maxw = (0..n).map(|p| ed.size_of(p).0).fold(1.0f32, f32::max);
    let fit = ((cv.2 - 2.0 * MARGIN - GAP_X * (cols as f32 - 1.0)) / (cols as f32 * maxw)).clamp(0.15, 8.0);
    let k = if ed.zoom > 0.0 { ed.zoom } else { fit };
    // Le righe di pagine: dove sta ognuna nel contenuto (y dell'alto della pagina, senza lo scorrimento).
    let mut tops = vec![0.0f32; n];
    let mut rows: Vec<(usize, usize, f32, f32, f32)> = Vec::new();
    let mut y = 12.0;
    let mut p = 0;
    while p < n {
        let last = (p + cols).min(n);
        let row_w: f32 = (p..last).map(|i| ed.size_of(i).0 * k).sum::<f32>() + GAP_X * (last - p - 1) as f32;
        let row_h = (p..last).map(|i| ed.size_of(i).1 * k).fold(0.0f32, f32::max);
        for t in tops.iter_mut().take(last).skip(p) {
            *t = y + LABEL;
        }
        rows.push((p, last, row_w, y + LABEL, row_h));
        y += LABEL + row_h + GAP_Y;
        p = last;
    }
    let content_h = y + 16.0;
    let content_w = rows.iter().map(|r| r.2).fold(0.0f32, f32::max) + 2.0 * MARGIN;
    let max_x = (content_w - cv.2).max(0.0);
    let off = f.ui.scroll(Scroll::EdPage).clamp(0.0, (content_h - cv.3).max(0.0));
    let offx = f.ui.scroll(Scroll::EdPageX).clamp(0.0, max_x);
    let scale = f.p.scale;
    let center = cv.1 + cv.3 * 0.4;
    let mut cur = n - 1;
    f.p.push_clip(cv.0, cv.1, cv.2, cv.3);
    // Il testo si disegna alla misura della pagina: senza gli ingrandimenti dell'interfaccia.
    let keep = (f.p.larger, f.p.text_scale);
    let mut found_cur = false;
    let mut visible: Vec<(usize, R)> = Vec::new();
    for &(first, last, row_w, row_top, row_h) in &rows {
        let x0 = if max_x > 0.0 { cv.0 + MARGIN - offx + (content_w - 2.0 * MARGIN - row_w) / 2.0 } else { cv.0 + (cv.2 - row_w) / 2.0 };
        let mut x = x0;
        for pg in first..last {
            let (pw, ph) = ed.size_of(pg);
            let pr = (x, cv.1 + row_top - off, pw * k, ph * k);
            x += pw * k + GAP_X;
            if !found_cur && pr.1 + ph.max(row_h) * k > center {
                cur = pg;
                found_cur = true;
            }
            if pr.1 + pr.3 < cv.1 - 40.0 || pr.1 > cv.1 + cv.3 + 40.0 || pr.0 + pr.2 < cv.0 || pr.0 > cv.0 + cv.2 {
                continue;
            }
            visible.push((pg, pr));
        }
    }
    for (pg, pr) in &visible {
        let (pg, pr) = (*pg, *pr);
        // «Pagina 2» sopra la pagina.
        f.p.text(&format!("Pagina {}", pg + 1), pr.0, pr.1 - LABEL + 2.0, pr.2, 20.0, 13.0, SEMI, (0xCF, 0xC6, 0xDC), TA::Center);
        f.p.shadow(pr.0, pr.1, pr.2, pr.3, 2.0, 16.0, 0.6, 6.0);
        f.p.rrect(pr.0, pr.1, pr.2, pr.3, 1.0, Some(WHITE), None);
        let side = ((ed.size_of(pg).0.max(ed.size_of(pg).1) * k * scale).ceil() as u32).next_multiple_of(128).clamp(256, 4096);
        match ed.pages.get(&pg) {
            Some(pi) => {
                f.p.image(&pi.img, pr.0, pr.1, pr.2, pr.3, 0.0, Fit::Contain);
                if pi.rev != ed.rev || pi.side < side {
                    ed.lay.borrow_mut().wants.push((pg, side));
                }
            }
            None => {
                f.spinner(pr.0 + pr.2 / 2.0, pr.1 + pr.3 / 2.0, 14.0, (0x88, 0x88, 0x99));
                f.animating = true;
                ed.lay.borrow_mut().wants.push((pg, side));
            }
        }
        if matches!(ed.tool, Tool::EditText | Tool::Highlight | Tool::Underline | Tool::Strike | Tool::Find) {
            ed.lay.borrow_mut().texts.push(pg);
        }
        f.p.larger = false;
        f.p.text_scale = 1.0;
        overlays(f, ed, e, pg, pr, k);
        f.p.larger = keep.0;
        f.p.text_scale = keep.1;
        ed.lay.borrow_mut().pages.push((pg, pr));
        f.hit_kind(clip(pr, cv), Act::Ed(EdAct::Page(pg)), HitKind::Slider);
    }
    // Il testo che si sta scrivendo sta sopra a tutto e prende i clic (per mettere il cursore).
    if let Some(te) = ed.text.as_ref() {
        if let Some((_, pr)) = visible.iter().find(|(p, _)| *p == te.page) {
            f.p.larger = false;
            f.p.text_scale = 1.0;
            let b = text_edit(f, ed, e, te, *pr, k);
            f.p.larger = keep.0;
            f.p.text_scale = keep.1;
            if let Some((bx, lh, lines)) = b {
                let hit = clip(bx, cv);
                ed.lay.borrow_mut().edit_box = Some((bx, lh, lines));
                f.hit_kind(hit, Act::Ed(EdAct::TextBox), HitKind::Slider);
            }
        }
    } else if let Some(fe) = ed.field.as_ref() {
        if let Some((_, pr)) = visible.iter().find(|(p, _)| *p == fe.page) {
            f.p.larger = false;
            f.p.text_scale = 1.0;
            let b = field_edit(f, ed, fe, *pr, k);
            f.p.larger = keep.0;
            f.p.text_scale = keep.1;
            if let Some((bx, lh, lines)) = b {
                let hit = clip(bx, cv);
                ed.lay.borrow_mut().edit_box = Some((bx, lh, lines));
                f.hit_kind(hit, Act::Ed(EdAct::TextBox), HitKind::Slider);
            }
        }
    }
    f.p.pop_clip();
    {
        let mut lay = ed.lay.borrow_mut();
        lay.tops = tops;
        lay.content_h = content_h;
        lay.k = k;
        lay.cur = cur;
        lay.max_x = max_x;
    }
    f.scrollbar(cv, Scroll::EdPage, content_h);
    // La barra di scorrimento in orizzontale, quando la pagina è più larga della zona.
    if max_x > 0.0 {
        let bar = (cv.0, cv.1 + cv.3 - 14.0, cv.2, 14.0);
        f.scrolls.push((bar, Scroll::EdPageX, max_x));
        let tw = (cv.2 * cv.2 / content_w).max(40.0);
        let tx = cv.0 + (cv.2 - tw) * offx / max_x;
        let act = Act::Ed(EdAct::HBar);
        f.p.rrect_a(bar.0, bar.1 + 4.0, bar.2, 6.0, 3.0, WHITE, 0.07);
        let hot = f.hovered(&act) || f.ui.drag.as_ref().is_some_and(|d| d.0 == act);
        f.p.rrect(tx, bar.1 + 4.0, tw, 6.0, 3.0, Some(mix(e.e2, WHITE, if hot { 0.55 } else { 0.35 })), None);
        f.hit_kind(bar, act, HitKind::Slider);
    }
    // Gli avvisi («Non riesco a fare questa modifica», «Sostituite 3 occorrenze») in basso, sopra la pagina.
    crate::ui::views::notice(f, app, (cv.0 + 20.0, cv.1 + cv.3 - 62.0, (cv.2 - 40.0).min(720.0), 44.0), 14.0);
}

/// Quel che sta sopra una pagina: il paragrafo sotto il mouse, la cosa scelta, i risultati della ricerca, i campi
/// del modulo, il gesto in corso.
fn overlays(f: &mut Frame, ed: &PdfEd, e: E, pg: usize, pr: R, k: f32) {
    let rc = |r: &Rect| (pr.0 + r.x * k, pr.1 + r.y * k, r.w * k, r.h * k);
    let pt = |p: (f32, f32)| (pr.0 + p.0 * k, pr.1 + p.1 * k);
    // I risultati di «Trova».
    if ed.tool == Tool::Find {
        for (i, (p, hit)) in ed.found.iter().enumerate() {
            if *p != pg {
                continue;
            }
            for r in &hit.rects {
                let b = rc(&r.inflate(1.0));
                let cur = i == ed.find_cur;
                f.p.rrect_a(b.0, b.1, b.2, b.3, 2.0, if cur { (0xFF, 0x9F, 0x2E) } else { (0xFF, 0xE0, 0x4A) }, if cur { 0.55 } else { 0.4 });
                if cur {
                    f.p.rrect_line(b.0, b.1, b.2, b.3, 2.0, (0xE0, 0x7A, 0x00), 2.0, 1.0, None);
                }
            }
        }
    }
    // I campi del modulo si vedono quando si compila.
    if ed.tool == Tool::Fill {
        if let Some(o) = ed.opened.as_ref() {
            for fld in o.fields.iter().filter(|f| f.page == pg && !f.pushbutton) {
                let b = rc(&fld.rect);
                f.p.rrect_a(b.0, b.1, b.2, b.3, 2.0, BLUE, 0.12);
                f.p.rrect_line(b.0, b.1, b.2, b.3, 2.0, BLUE, 1.0, 0.7, None);
            }
        }
    }
    // Il paragrafo che si sceglierebbe con un clic.
    if ed.tool == Tool::EditText && ed.text.is_none() && ed.drag.is_none() {
        if let Some((mp, mx, my)) = ed.mouse {
            if mp == pg {
                if let Some(b) = ed.block_at(pg, mx, my) {
                    let r = rc(&b.rect.inflate(2.0));
                    if b.editable() {
                        f.p.rrect_a(r.0, r.1, r.2, r.3, 2.0, BLUE, 0.10);
                        f.p.rrect_line(r.0, r.1, r.2, r.3, 2.0, BLUE, 1.5, 0.9, None);
                    } else {
                        f.p.rrect_a(r.0, r.1, r.2, r.3, 2.0, AMBER, 0.10);
                        f.p.rrect_line(r.0, r.1, r.2, r.3, 2.0, AMBER, 1.5, 0.9, Some((5.0, 4.0)));
                    }
                }
            }
        }
    }
    // La cosa scelta (con «Seleziona» o dall'elenco dei commenti).
    if let Some(i) = ed.sel {
        if let Some(edit) = ed.edits.get(i) {
            if edit.page() == Some(pg) {
                if let Some(b) = edit.bounds() {
                    dashed(f, rc(&b.inflate(3.0)), BLUE, 1.5);
                }
            }
        }
    }
    if let (Some(id), Some(o)) = (ed.sel_file, ed.opened.as_ref()) {
        if let Some(c) = o.comments.iter().find(|c| c.id == id && c.page == pg) {
            dashed(f, rc(&c.rect.inflate(3.0)), BLUE, 1.5);
        }
    }
    // Il gesto in corso.
    match ed.drag.as_ref() {
        Some(Drag::Words { page, from, to }) if *page == pg => {
            let col = match ed.tool {
                Tool::Highlight => ed.mark,
                _ => if ed.mark == MARK_COLORS[0] { [0xD6, 0x32, 0x3C] } else { ed.mark },
            };
            for r in ed.words_between(pg, *from, *to) {
                let b = rc(&r);
                match ed.tool {
                    Tool::Underline => f.p.fill(b.0, b.1 + b.3 - 1.0, b.2, 1.5, rgb(col)),
                    Tool::Strike => f.p.fill(b.0, b.1 + b.3 * 0.55, b.2, 1.5, rgb(col)),
                    _ => f.p.rrect_a(b.0, b.1, b.2, b.3, 1.0, rgb(col), 0.45),
                }
            }
        }
        Some(Drag::Rubber { page, from, to }) if *page == pg => {
            let (a, b) = (pt(*from), pt(*to));
            let r = (a.0.min(b.0), a.1.min(b.1), (a.0 - b.0).abs(), (a.1 - b.1).abs());
            let c = if ed.tool == Tool::Area { (0xE0, 0x3C, 0x48) } else { e.acc };
            if ed.tool == Tool::Ellipse {
                f.p.circle(r.0 + r.2 / 2.0, r.1 + r.3 / 2.0, (r.2.min(r.3)) / 2.0, None, Some((rgb(ed.ink), ed.width.max(1.0) * k)));
            } else {
                f.p.rrect_a(r.0, r.1, r.2, r.3, 1.0, c, 0.14);
                dashed(f, r, c, 1.5);
            }
        }
        Some(Drag::Line { page, from, to }) if *page == pg => {
            let (a, b) = (pt(*from), pt(*to));
            let (c, w) = (rgb(ed.ink), (ed.width * k).max(1.0));
            f.p.line(a.0, a.1, b.0, b.1, c, w);
            if ed.tool == Tool::Arrow {
                arrow_head(f, a, b, c, w, k);
            }
        }
        Some(Drag::Pen { page, pts }) if *page == pg => {
            let c = rgb(ed.ink);
            let w = (ed.width * k).max(1.0);
            for s in pts.windows(2) {
                let (a, b) = (pt(s[0]), pt(s[1]));
                f.p.line(a.0, a.1, b.0, b.1, c, w);
            }
        }
        Some(Drag::Move { index, page, from, orig }) if *page == pg => {
            if let (Some((_, mx, my)), Some(b)) = (ed.mouse, orig.bounds()) {
                let _ = index;
                let (dx, dy) = (mx - from.0, my - from.1);
                dashed(f, rc(&Rect::new(b.x + dx, b.y + dy, b.w, b.h).inflate(3.0)), e.acc, 1.5);
            }
        }
        _ => {}
    }
    // Quel che si sta spostando disegna già il segno nel nuovo posto: il resto lo fa la pagina rifatta.
    let _ = ShapeKind::Rect;
    let _ = Edit::DeleteAnnot { page: 0, id: (0, 0) };
}

fn arrow_head(f: &mut Frame, a: (f32, f32), b: (f32, f32), c: (u8, u8, u8), w: f32, k: f32) {
    let ang = (b.1 - a.1).atan2(b.0 - a.0);
    let len = (10.0 * k).max(w * 3.0);
    for d in [0.5f32, -0.5] {
        let t = ang + std::f32::consts::PI + d;
        f.p.line(b.0, b.1, b.0 + len * t.cos(), b.1 + len * t.sin(), c, w);
    }
}

// ---------------------------------------------------------------------------------------------
// Il testo che si scrive dentro la pagina

/// Le righe di un testo a capo nella larghezza `w`: per ognuna dove comincia e dove finisce (in caratteri) e le x
/// dei confini tra i caratteri (da `lines[i].2[0]` a `[len]`).
fn wrap(f: &Frame, text: &str, size: f32, weight: Weight, w: f32) -> Vec<(usize, usize, Vec<f32>)> {
    let chars: Vec<char> = text.chars().collect();
    let width = |a: usize, b: usize| -> f32 {
        if a >= b {
            0.0
        } else {
            f.measure(&crate::i18n::lit(&chars[a..b].iter().collect::<String>()), size, weight)
        }
    };
    let mut out = Vec::new();
    let mut start = 0usize;
    loop {
        let nl = chars[start.min(chars.len())..].iter().position(|c| *c == '\n').map(|p| start + p);
        let end = nl.unwrap_or(chars.len());
        let mut ls = start;
        loop {
            let mut le = ls;
            let mut last_space: Option<usize> = None;
            while le < end {
                if chars[le] == ' ' {
                    last_space = Some(le);
                }
                if le > ls && width(ls, le + 1) > w {
                    if let Some(sp) = last_space.filter(|sp| *sp > ls) {
                        le = sp + 1;
                    }
                    break;
                }
                le += 1;
            }
            let xs: Vec<f32> = (0..=(le - ls)).map(|j| width(ls, ls + j)).collect();
            out.push((ls, le, xs));
            if le >= end {
                break;
            }
            ls = le;
        }
        match nl {
            Some(n) => start = n + 1,
            None => break,
        }
    }
    out
}

fn draw_lines(f: &mut Frame, lines: &mut [(usize, usize, Vec<f32>)], text: &str, x: f32, y: f32, w: f32, lh: f32, size: f32, weight: Weight, color: (u8, u8, u8), align: crate::pdfmod::Align) {
    let chars: Vec<char> = text.chars().collect();
    let (asc, desc) = f.p.fonts().extents(size * f.p.scale, weight);
    for (i, (s, en, xs)) in lines.iter_mut().enumerate() {
        let lw = xs.last().copied().unwrap_or(0.0);
        let off = match align {
            crate::pdfmod::Align::Left => 0.0,
            crate::pdfmod::Align::Center => ((w - lw) / 2.0).max(0.0),
            crate::pdfmod::Align::Right => (w - lw).max(0.0),
        };
        for v in xs.iter_mut() {
            *v += off;
        }
        let base = y + i as f32 * lh + (lh - (asc + desc) / f.p.scale) / 2.0 + asc / f.p.scale;
        let line: String = chars[*s..*en].iter().collect();
        f.p.text_at(&line, x + off, base, size, weight, color);
    }
}

/// Il caret e la selezione di tutto il testo.
fn caret(f: &mut Frame, lines: &[(usize, usize, Vec<f32>)], x: f32, y: f32, lh: f32, color: (u8, u8, u8)) {
    if f.ui.sel_all {
        for (i, (_, _, xs)) in lines.iter().enumerate() {
            let (a, b) = (xs.first().copied().unwrap_or(0.0), xs.last().copied().unwrap_or(0.0));
            f.p.rrect_a(x + a, y + i as f32 * lh, (b - a).max(4.0), lh, 1.0, BLUE, 0.35);
        }
    }
    f.animating = true;
    if (f.ms / 530).is_multiple_of(2) {
        let c = f.ui.caret;
        let row = lines.iter().rposition(|(s, _, _)| *s <= c).unwrap_or(0);
        let (s, _, xs) = &lines[row];
        let cx = xs.get(c - s).copied().unwrap_or(0.0);
        f.p.rrect_a(x + cx - 0.5, y + row as f32 * lh + lh * 0.1, 1.5, lh * 0.8, 0.0, color, 1.0);
    }
}

type Box3 = Option<(R, f32, Vec<(usize, Vec<f32>)>)>;

fn simple(lines: Vec<(usize, usize, Vec<f32>)>) -> Vec<(usize, Vec<f32>)> {
    lines.into_iter().map(|(s, _, xs)| (s, xs)).collect()
}

fn text_edit(f: &mut Frame, ed: &PdfEd, e: E, te: &TextEdit, pr: R, k: f32) -> Box3 {
    let _ = ed;
    let size = te.size * k;
    let lh = size * crate::pdfmod::newtext::LEADING;
    let weight = if te.bold { BOLD } else { SANS };
    let (x, y) = (pr.0 + te.rect.x * k, pr.1 + te.rect.y * k);
    let is_note = matches!(te.target, Target::Note | Target::NoteAgain(_));
    let w = if is_note { 220.0 } else { (te.rect.w * k).max(size * 2.0) };
    if is_note {
        // La nota: un fumetto giallo con il testo scuro.
        let mut lines = wrap(f, &te.text, 13.0, SANS, w - 20.0);
        let lh = 13.0 * 1.3;
        let h = (lines.len() as f32 * lh + 20.0).max(48.0);
        f.p.shadow(x, y, w, h, 8.0, 12.0, 0.4, 4.0);
        f.p.rrect(x, y, w, h, 8.0, Some((0xFF, 0xF1, 0xB8)), Some((BLUE, 2.0)));
        let ink = (0x4A, 0x3B, 0x00);
        draw_lines(f, &mut lines, &te.text, x + 10.0, y + 10.0, w - 20.0, lh, 13.0, SANS, ink, crate::pdfmod::Align::Left);
        if f.ui.edit == Some(Field::Ed(EdField::Text)) {
            caret(f, &lines, x + 10.0, y + 10.0, lh, ink);
        }
        if te.text.is_empty() && f.ui.edit != Some(Field::Ed(EdField::Text)) {
            f.p.text_at("Scrivi la nota…", x + 10.0, y + 28.0, 13.0, SANS, (0x9A, 0x86, 0x3A));
        }
        return Some(((x + 10.0, y + 10.0, w - 20.0, h - 20.0), lh, simple(lines)));
    }
    let mut lines = wrap(f, &te.text, size, weight, w);
    let h = (lines.len() as f32 * lh).max(te.rect.h * k);
    // Il testo di prima si copre: la pagina disegnata lo mostra ancora com'era.
    if matches!(te.target, Target::Existing(_) | Target::Again(_)) {
        f.p.rrect(x - 3.0, y - 3.0, w + 6.0, h + 6.0, 1.0, Some(WHITE), None);
    }
    f.p.rrect_a(x - 3.0, y - 3.0, w + 6.0, h + 6.0, 2.0, BLUE, 0.08);
    let color = rgb(te.color);
    draw_lines(f, &mut lines, &te.text, x, y, w, lh, size, weight, color, te.align);
    f.p.rrect_line(x - 3.0, y - 3.0, w + 6.0, h + 6.0, 2.0, BLUE, 2.0, 1.0, None);
    if f.ui.edit == Some(Field::Ed(EdField::Text)) {
        caret(f, &lines, x, y, lh, color);
    }
    let _ = e;
    Some(((x, y, w, h), lh, simple(lines)))
}

fn field_edit(f: &mut Frame, ed: &PdfEd, fe: &crate::ui::pdfeditor::FieldEdit, pr: R, k: f32) -> Box3 {
    let _ = ed;
    let (x, y, w, h) = (pr.0 + fe.rect.x * k, pr.1 + fe.rect.y * k, fe.rect.w * k, fe.rect.h * k);
    let pt = (fe.rect.h * 0.62).clamp(8.0, 14.0);
    let size = pt * k;
    let lh = size * 1.2;
    f.p.rrect(x - 1.0, y - 1.0, w + 2.0, h + 2.0, 1.0, Some(WHITE), None);
    let ink = (0x1D, 0x1D, 0x1F);
    let mut lines = if fe.multiline { wrap(f, &fe.text, size, SANS, w - 8.0) } else { wrap_single(f, &fe.text, size, SANS) };
    // Una riga sola che esce dal campo scorre per far vedere dove si scrive.
    let mut shift = 0.0;
    if !fe.multiline {
        let c = f.ui.caret.min(fe.text.chars().count());
        let cx = lines.first().and_then(|l| l.2.get(c)).copied().unwrap_or(0.0);
        if cx > w - 10.0 {
            shift = cx - (w - 10.0);
        }
    }
    let ty = if fe.multiline { y + 4.0 } else { y + (h - lh) / 2.0 };
    f.p.push_clip(x, y, w, h);
    draw_lines(f, &mut lines, &fe.text, x + 4.0 - shift, ty, w, lh, size, SANS, ink, crate::pdfmod::Align::Left);
    if f.ui.edit == Some(Field::Ed(EdField::FieldText)) {
        caret(f, &lines, x + 4.0 - shift, ty, lh, ink);
    }
    f.p.pop_clip();
    f.p.rrect_line(x - 1.0, y - 1.0, w + 2.0, h + 2.0, 1.0, BLUE, 2.0, 1.0, None);
    Some(((x + 4.0 - shift, ty, w, h), lh, simple(lines)))
}

/// Una riga sola, senza andare a capo.
fn wrap_single(f: &Frame, text: &str, size: f32, weight: Weight) -> Vec<(usize, usize, Vec<f32>)> {
    let chars: Vec<char> = text.chars().collect();
    let xs: Vec<f32> = (0..=chars.len()).map(|j| if j == 0 { 0.0 } else { f.measure(&crate::i18n::lit(&chars[..j].iter().collect::<String>()), size, weight) }).collect();
    vec![(0, chars.len(), xs)]
}

/// Dentro il riquadro? (per il passaggio del mouse sopra una pagina)
#[allow(dead_code)]
pub(super) fn over(r: R, x: f32, y: f32) -> bool {
    inside(r, x, y)
}
