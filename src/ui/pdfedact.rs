//! I clic, il mouse sulla pagina e i tasti dell'editor dei PDF. Lo stato sta in `pdfeditor`, il disegno
//! in `views/pdfeditor`.

use std::sync::Arc;

use super::core::{Act, Field, Swatch, R};
use super::model::{Notice, PdfMode};
use super::pdfeditor::*;
use super::pdfedwork::{replace_in, Job};
use super::screen::{Key, Platform, Screen};
use crate::pdfmod::{find, Align, Color, Edit, FieldKind, FontChoice, MarkKind, Rect, ShapeKind};

/// I campi in cui si scrive dentro l'editor.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum EdField {
    /// Il testo in modifica dentro la pagina (paragrafo, testo nuovo o nota).
    Text,
    /// «Cerca nel PDF» in alto e il campo del pannello «Trova e sostituisci».
    TopFind,
    Find,
    Replace,
    /// «Cerca e oscura»: una parola a piacere.
    Free,
    /// Il testo della modifica scelta (nota, timbro, commento).
    SelText,
    /// Il testo di un timbro a piacere.
    StampText,
    /// Il campo del modulo in compilazione.
    FieldText,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Down,
    Move,
    Up,
}

/// Cosa fa un clic dentro l'editor.
#[derive(Clone, PartialEq, Debug)]
pub enum EdAct {
    /// Esce (verso la scheda PDF, o verso un altro modo del PDF); chiede se ci sono modifiche non salvate.
    Leave(Option<PdfMode>),
    LeaveMenu,
    LeaveSave,
    LeaveDiscard,
    LeaveStay,
    Tab(EdTab),
    Tool(Tool),
    Undo,
    Redo,
    Left(u8),
    /// La pagina `i` sullo schermo (si trascina).
    Page(usize),
    /// Il testo che si sta scrivendo dentro la pagina (un clic mette il cursore).
    TextBox,
    Thumb(usize),
    Prev,
    Next,
    ZoomIn,
    ZoomOut,
    ZoomFit,
    TwoPages(bool),
    Simple(bool),
    Ink(Color),
    Mark(Color),
    FillColor(Option<Color>),
    SizeSlider,
    WidthSlider,
    Align(Align),
    Bold,
    Italic,
    FontMenu,
    FontPick(usize),
    StampMenu,
    StampPick(usize),
    PickImage,
    FindRun,
    FindNext(i32),
    Case,
    Whole,
    ReplaceOne,
    ReplaceAll,
    FindHit(usize),
    Pattern(usize),
    MarkFree,
    ApplyRedact,
    ClearRedact,
    Flatten,
    Strip,
    Options,
    Save,
    OpenSaved,
    OpenFolder,
    Delete,
    CommentRow(usize),
    CommentDelete(usize),
    FieldPick(usize),
    /// «Tutte | Pagina N» nell'elenco dei commenti.
    OnlyPage(bool),
    /// La barra di scorrimento in orizzontale sotto la pagina.
    HBar,
    /// Porta alla pagina del campo (dall'elenco «Compila»).
    FieldGo(usize),
    FieldNext,
    FieldsClear,
    /// Dopo il salvataggio: torna ai comandi per modificare ancora.
    KeepEditing,
}

/// Quanto è grande un campo nuovo se si fa solo un clic (punti).
fn default_field(kind: FieldKind) -> (f32, f32) {
    match kind {
        FieldKind::Text | FieldKind::List => (160.0, 22.0),
        FieldKind::Check | FieldKind::Radio => (16.0, 16.0),
        FieldKind::Sign => (180.0, 48.0),
    }
}

fn field_kind(t: Tool) -> Option<FieldKind> {
    Some(match t {
        Tool::NewText => FieldKind::Text,
        Tool::NewCheck => FieldKind::Check,
        Tool::NewRadio => FieldKind::Radio,
        Tool::NewList => FieldKind::List,
        Tool::NewSign => FieldKind::Sign,
        _ => return None,
    })
}

impl PdfEd {
    pub fn field_text(&self, f: EdField) -> String {
        match f {
            EdField::Text => self.text.as_ref().map(|t| t.text.clone()).unwrap_or_default(),
            EdField::TopFind | EdField::Find => self.find.clone(),
            EdField::Replace => self.replace.clone(),
            EdField::Free => self.free.clone(),
            EdField::SelText => self.sel_text().unwrap_or_default(),
            EdField::StampText => self.stamp_text.clone(),
            EdField::FieldText => self.field.as_ref().map(|t| t.text.clone()).unwrap_or_default(),
        }
    }

    pub fn set_field_text(&mut self, f: EdField, text: String) {
        match f {
            EdField::Text => {
                if let Some(t) = self.text.as_mut() {
                    t.text = text;
                }
            }
            EdField::TopFind | EdField::Find => {
                self.find = text;
                self.found.clear();
                self.find_done = false;
            }
            EdField::Replace => self.replace = text,
            EdField::Free => self.free = text,
            EdField::SelText => self.set_sel_text(text),
            EdField::StampText => self.stamp_text = text,
            EdField::FieldText => {
                if let Some(t) = self.field.as_mut() {
                    t.text = if t.max_len > 0 { text.chars().take(t.max_len).collect() } else { text };
                }
            }
        }
    }
}

fn clamp_pt(ed: &PdfEd, page: usize, p: (f32, f32)) -> (f32, f32) {
    let (w, h) = ed.size_of(page);
    (p.0.clamp(0.0, w), p.1.clamp(0.0, h))
}

impl Screen {
    /// Il testo in modifica si chiude da solo quando il cursore lascia il suo campo (un clic altrove).
    pub fn ed_sync(&mut self) {
        let Some(ed) = self.app.ed.as_ref() else { return };
        if ed.text.is_some() && self.ui.edit != Some(Field::Ed(EdField::Text)) {
            self.ed_finish_text(true);
        }
        let Some(ed) = self.app.ed.as_ref() else { return };
        if ed.field.is_some() && self.ui.edit != Some(Field::Ed(EdField::FieldText)) {
            self.ed_finish_field(true);
        }
    }

    fn ed_begin_text(&mut self, te: TextEdit) {
        let Some(ed) = self.app.ed.as_mut() else { return };
        let n = te.text.chars().count();
        ed.text = Some(te);
        ed.sel = None;
        self.ui.edit = Some(Field::Ed(EdField::Text));
        self.ui.caret = n;
        self.ui.sel_all = false;
    }

    /// Chiude il testo in modifica: lo applica (`apply`) o lo butta.
    pub fn ed_finish_text(&mut self, apply: bool) {
        let Some(ed) = self.app.ed.as_mut() else { return };
        let Some(te) = ed.text.take() else { return };
        if self.ui.edit == Some(Field::Ed(EdField::Text)) {
            self.ui.edit = None;
        }
        if !apply {
            return;
        }
        let page = te.page;
        match te.target {
            Target::Existing(rect) => {
                if te.text != te.orig || te.ovr != Ovr::default() {
                    ed.push(Edit::EditText { page, rect, text: te.text, size: te.ovr.size, bold: te.ovr.bold, italic: te.ovr.italic, color: te.ovr.color, font: te.font, align: te.ovr.align });
                }
            }
            Target::New => {
                if !te.text.trim().is_empty() {
                    ed.push(Edit::AddText { page, x: te.rect.x, y: te.rect.y, w: te.rect.w, text: te.text, size: te.size, color: te.color, bold: te.bold, italic: te.italic, font: te.font, align: te.align });
                }
            }
            Target::Again(i) => {
                let mut v = ed.edits.clone();
                if i < v.len() {
                    if te.text.trim().is_empty() {
                        v.remove(i);
                    } else {
                        v[i] = Edit::AddText { page, x: te.rect.x, y: te.rect.y, w: te.rect.w, text: te.text, size: te.size, color: te.color, bold: te.bold, italic: te.italic, font: te.font, align: te.align };
                    }
                    ed.commit(v);
                }
            }
            Target::Note => {
                if !te.text.trim().is_empty() {
                    let c = ed.mark;
                    ed.push(Edit::Note { page, x: te.rect.x, y: te.rect.y, text: te.text, color: c });
                }
            }
            Target::NoteAgain(i) => {
                let mut v = ed.edits.clone();
                if let Some(Edit::Note { text, .. }) = v.get_mut(i) {
                    if te.text.trim().is_empty() {
                        v.remove(i);
                    } else {
                        *text = te.text;
                    }
                    ed.commit(v);
                }
            }
        }
    }

    fn ed_finish_field(&mut self, apply: bool) {
        let Some(ed) = self.app.ed.as_mut() else { return };
        let Some(fe) = ed.field.take() else { return };
        if self.ui.edit == Some(Field::Ed(EdField::FieldText)) {
            self.ui.edit = None;
        }
        if apply && fe.text != ed.field_value(&fe.name) {
            ed.set_field(&fe.name, fe.text);
        }
    }

    fn ed_notice(&mut self, text: &str, error: bool) {
        self.app.notice = Some(Notice { text: text.into(), error });
    }

    /// Un'immagine scelta: PNG o JPEG com'è, gli altri formati diventano PNG.
    fn ed_load_image(&mut self, path: &std::path::Path) {
        let Ok(bytes) = std::fs::read(path) else {
            self.ed_notice("Non riesco a leggere l'immagine.", true);
            return;
        };
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let dims = |b: &[u8]| image::ImageReader::new(std::io::Cursor::new(b)).with_guessed_format().ok().and_then(|r| r.into_dimensions().ok());
        let ready = if matches!(ext.as_str(), "png" | "jpg" | "jpeg") && dims(&bytes).is_some() {
            dims(&bytes).map(|(w, h)| (bytes, w, h))
        } else {
            image::load_from_memory(&bytes).ok().and_then(|img| {
                let (w, h) = (img.width(), img.height());
                let mut out = Vec::new();
                img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).ok()?;
                Some((out, w, h))
            })
        };
        match ready {
            Some((data, w, h)) => {
                if let Some(ed) = self.app.ed.as_mut() {
                    ed.image = Some((Arc::new(data), w, h));
                    ed.tool = Tool::Image;
                    ed.tab = EdTab::Shapes;
                }
            }
            None => self.ed_notice("L'immagine non si apre: scegli un PNG o un JPEG.", true),
        }
    }

    /// Esegue un clic dell'editor.
    pub fn ed_act(&mut self, a: EdAct, plat: &mut dyn Platform) {
        self.ed_sync();
        if self.app.ed.is_none() {
            return;
        }
        // Cambiare strumento o scheda lascia a metà il gesto del mouse.
        if let Some(ed) = self.app.ed.as_mut() {
            if !matches!(a, EdAct::Page(_) | EdAct::TextBox) {
                ed.drag = None;
            }
        }
        match a {
            EdAct::Leave(mode) => {
                let ed = self.app.ed.as_mut().expect("editor");
                ed.leave_lab = None;
                if ed.unsaved() > 0 {
                    ed.leave = Some(mode);
                } else {
                    self.app.ed_leave(mode);
                }
            }
            EdAct::LeaveMenu => {
                let mut items = vec![("Torna alla scheda PDF".to_string(), Act::Ed(EdAct::Leave(None)))];
                for (m, label) in [(PdfMode::Convert, "Converti"), (PdfMode::Merge, "Unisci"), (PdfMode::Split, "Dividi"), (PdfMode::Pages, "Pagine"), (PdfMode::Compress, "Comprimi")] {
                    items.push((label.to_string(), Act::Ed(EdAct::Leave(Some(m)))));
                }
                self.open_menu(items, None, &Act::Ed(EdAct::LeaveMenu));
            }
            EdAct::LeaveSave => {
                if let Some(mode) = self.app.ed.as_mut().and_then(|e| e.leave.take()) {
                    self.app.ed_save(Some(mode));
                }
            }
            EdAct::LeaveDiscard => {
                if let Some(mode) = self.app.ed.as_mut().and_then(|e| e.leave.take()) {
                    self.app.ed_leave(mode);
                    if std::mem::take(&mut self.app.close_wanted) {
                        plat.close();
                    }
                }
            }
            EdAct::LeaveStay => {
                if let Some(ed) = self.app.ed.as_mut() {
                    ed.leave = None;
                    ed.leave_lab = None;
                    ed.leave_close = false;
                }
            }
            EdAct::Tab(t) => {
                let ed = self.app.ed.as_mut().expect("editor");
                ed.tab = t;
                ed.tool = t.first_tool();
                ed.sel = None;
                ed.sel_file = None;
            }
            EdAct::Tool(t) => {
                let ed = self.app.ed.as_mut().expect("editor");
                if t != Tool::Select {
                    ed.tab = t.tab();
                }
                ed.tool = t;
                ed.sel = None;
                ed.sel_file = None;
                ed.applied = None;
                if t == Tool::Image {
                    self.ed_act(EdAct::PickImage, plat);
                }
            }
            EdAct::Undo => self.app.ed.as_mut().expect("editor").undo(),
            EdAct::Redo => self.app.ed.as_mut().expect("editor").redo(),
            EdAct::Left(n) => self.app.ed.as_mut().expect("editor").left = n,
            EdAct::Thumb(p) => self.app.ed.as_ref().expect("editor").goto(p, 0.0),
            EdAct::Prev | EdAct::Next => {
                let ed = self.app.ed.as_ref().expect("editor");
                let cur = ed.lay.borrow().cur;
                let to = if a == EdAct::Prev { cur.saturating_sub(1) } else { (cur + 1).min(ed.page_count().saturating_sub(1)) };
                ed.goto(to, 0.0);
            }
            EdAct::ZoomIn | EdAct::ZoomOut => {
                let ed = self.app.ed.as_mut().expect("editor");
                let base = if ed.zoom > 0.0 { ed.zoom } else { ed.lay.borrow().k.max(0.3) };
                ed.zoom = if a == EdAct::ZoomIn { (base * 1.2).min(6.0) } else { (base / 1.2).max(0.25) };
            }
            EdAct::ZoomFit => self.app.ed.as_mut().expect("editor").zoom = 0.0,
            EdAct::TwoPages(b) => self.app.ed.as_mut().expect("editor").two = b,
            EdAct::Simple(b) => self.app.ed.as_mut().expect("editor").complete = !b,
            EdAct::Ink(c) => {
                let ed = self.app.ed.as_mut().expect("editor");
                if let Some(t) = ed.text.as_mut() {
                    t.color = c;
                    t.ovr.color = Some(c);
                    if !matches!(t.target, Target::Existing(_)) {
                        ed.text_ink = c;
                    }
                } else if ed.sel.is_some() {
                    ed.recolor(c);
                } else if matches!(ed.tool, Tool::AddText | Tool::EditText) {
                    ed.text_ink = c;
                } else {
                    ed.ink = c;
                }
            }
            EdAct::Mark(c) => {
                let ed = self.app.ed.as_mut().expect("editor");
                ed.mark = c;
                if ed.sel.is_some() {
                    ed.recolor(c);
                }
            }
            EdAct::FillColor(c) => self.app.ed.as_mut().expect("editor").fill = c,
            EdAct::Align(al) => {
                let ed = self.app.ed.as_mut().expect("editor");
                if let Some(t) = ed.text.as_mut() {
                    t.align = al;
                    t.ovr.align = Some(al);
                }
                ed.align = al;
            }
            EdAct::Bold | EdAct::Italic => {
                let ed = self.app.ed.as_mut().expect("editor");
                if let Some(t) = ed.text.as_mut() {
                    if a == EdAct::Bold {
                        t.bold = !t.bold;
                        t.ovr.bold = Some(t.bold);
                    } else {
                        t.italic = !t.italic;
                        t.ovr.italic = Some(t.italic);
                    }
                } else if a == EdAct::Bold {
                    ed.bold = !ed.bold;
                } else {
                    ed.italic = !ed.italic;
                }
            }
            EdAct::FontMenu => {
                let ed = self.app.ed.as_ref().expect("editor");
                let cur = ed.text.as_ref().map(|t| &t.font).unwrap_or(&ed.font).clone();
                let mut items = vec![("Quello del file".to_string(), Act::Ed(EdAct::FontPick(0)))];
                for (i, f) in FONTS.iter().enumerate() {
                    items.push((f.to_string(), Act::Ed(EdAct::FontPick(i + 1))));
                }
                let sel = match cur {
                    FontChoice::Same => Some(0),
                    FontChoice::Family(n) => FONTS.iter().position(|f| *f == n).map(|i| i + 1),
                };
                self.open_menu(items, sel, &Act::Ed(EdAct::FontMenu));
            }
            EdAct::FontPick(i) => {
                let ed = self.app.ed.as_mut().expect("editor");
                let font = if i == 0 { FontChoice::Same } else { FontChoice::Family(FONTS[i - 1].to_string()) };
                if let Some(t) = ed.text.as_mut() {
                    t.font = font.clone();
                }
                ed.font = font;
            }
            EdAct::StampMenu => {
                let items = STAMPS.iter().enumerate().map(|(i, (l, c))| (l.to_string(), Act::Ed(EdAct::StampPick(i)), Swatch::Color((c[0], c[1], c[2])))).collect();
                let sel = self.app.ed.as_ref().map(|e| e.stamp);
                self.open_swatch_menu("Timbro", items, sel, &Act::Ed(EdAct::StampMenu));
            }
            EdAct::StampPick(i) => {
                let ed = self.app.ed.as_mut().expect("editor");
                ed.stamp = i.min(STAMPS.len() - 1);
                ed.stamp_text.clear();
                ed.tool = Tool::Stamp;
                ed.tab = EdTab::Comments;
            }
            EdAct::PickImage => {
                let files = plat.pick_files();
                if let Some(p) = files.into_iter().find(|p| p.is_file()) {
                    self.ed_load_image(&p);
                }
            }
            EdAct::FindRun => self.ed_find(),
            EdAct::FindNext(d) => {
                let ed = self.app.ed.as_mut().expect("editor");
                let n = ed.found.len();
                if n > 0 {
                    ed.find_cur = (ed.find_cur as i32 + d).rem_euclid(n as i32) as usize;
                    let (p, hit) = &ed.found[ed.find_cur];
                    ed.goto(*p, hit.rects.first().map_or(0.0, |r| r.y));
                }
            }
            EdAct::Case | EdAct::Whole => {
                let ed = self.app.ed.as_mut().expect("editor");
                if a == EdAct::Case {
                    ed.case = !ed.case;
                } else {
                    ed.whole = !ed.whole;
                }
                if !ed.find.is_empty() {
                    self.ed_find();
                }
            }
            EdAct::FindHit(i) => {
                let ed = self.app.ed.as_mut().expect("editor");
                if i < ed.found.len() {
                    ed.find_cur = i;
                    let (p, hit) = &ed.found[i];
                    ed.goto(*p, hit.rects.first().map_or(0.0, |r| r.y));
                }
            }
            EdAct::ReplaceOne => {
                let ed = self.app.ed.as_mut().expect("editor");
                let Some((page, hit)) = ed.found.get(ed.find_cur).cloned() else { return };
                let Some(rect) = hit.rects.first().copied() else { return };
                let Some(b) = ed.fresh_text(page).and_then(|t| t.blocks.iter().find(|b| b.editable() && b.rect.inflate(1.0).intersects(&rect))).cloned() else {
                    self.ed_notice("Questo paragrafo non si può riscrivere: il suo carattere non dice che lettera è ogni segno.", true);
                    return;
                };
                let (text, n) = replace_in(&b.text, &ed.find, &ed.replace, ed.case, ed.whole);
                if n == 0 {
                    return;
                }
                ed.push(Edit::EditText { page, rect: b.rect, text, size: None, bold: None, italic: None, color: None, font: FontChoice::Same, align: None });
            }
            EdAct::ReplaceAll => {
                let ed = self.app.ed.as_mut().expect("editor");
                if ed.find.trim().is_empty() {
                    return;
                }
                let id = ed.id();
                ed.find_id = id;
                ed.finding = true;
                let (snap, query, with, case, whole) = (ed.snap(), ed.find.clone(), ed.replace.clone(), ed.case, ed.whole);
                self.app.edw.other(Job::Replace { snap, id, query, with, case, whole });
            }
            EdAct::Pattern(i) => {
                let Some(p) = find::Pattern::ALL.get(i).copied() else { return };
                let ed = self.app.ed.as_mut().expect("editor");
                if ed.marked(p).is_some() {
                    // Già segnati: un secondo clic li toglie.
                    let marks: Vec<(usize, Rect)> = ed.pats.iter().find(|e| e.0 == p).map(|e| e.1.clone()).unwrap_or_default();
                    let v: Vec<Edit> = ed.edits.iter().filter(|e| !matches!(e, Edit::Redact { page, rect, applied: false } if marks.iter().any(|(p2, r2)| p2 == page && r2 == rect))).cloned().collect();
                    ed.commit(v);
                    ed.pats.retain(|e| e.0 != p);
                } else {
                    let id = ed.id();
                    ed.pat_id = id;
                    ed.pat_which = Some(Some(p));
                    let snap = ed.snap();
                    self.app.edw.other(Job::Patterns { snap, id, which: vec![p] });
                }
            }
            EdAct::MarkFree => {
                let ed = self.app.ed.as_mut().expect("editor");
                let q = ed.free.trim().to_string();
                if q.is_empty() {
                    return;
                }
                let id = ed.id();
                ed.pat_id = id;
                ed.pat_which = Some(None);
                let (snap, case) = (ed.snap(), ed.case);
                self.app.edw.other(Job::Find { snap, id, query: q, case, whole: false });
            }
            EdAct::ApplyRedact => {
                let ed = self.app.ed.as_mut().expect("editor");
                if ed.pending_redacts() == 0 {
                    return;
                }
                let v: Vec<Edit> = ed.edits.iter().map(|e| if let Edit::Redact { page, rect, applied: false } = e { Edit::Redact { page: *page, rect: *rect, applied: true } } else { e.clone() }).collect();
                ed.commit(v);
                ed.pats.clear();
                let snap = ed.snap();
                self.app.edw.other(Job::Report { snap });
            }
            EdAct::ClearRedact => {
                let ed = self.app.ed.as_mut().expect("editor");
                let v: Vec<Edit> = ed.edits.iter().filter(|e| !matches!(e, Edit::Redact { applied: false, .. })).cloned().collect();
                ed.commit(v);
                ed.pats.clear();
            }
            EdAct::Flatten => {
                let ed = self.app.ed.as_mut().expect("editor");
                ed.flatten = !ed.flatten;
            }
            EdAct::Strip => {
                let ed = self.app.ed.as_mut().expect("editor");
                ed.strip = !ed.strip;
            }
            EdAct::Options => {
                let ed = self.app.ed.as_mut().expect("editor");
                ed.options_open = !ed.options_open;
            }
            EdAct::Save => self.app.ed_save(None),
            EdAct::OpenSaved => {
                if let Some(p) = self.app.ed.as_ref().and_then(|e| e.saved.clone()) {
                    plat.open_path(&p);
                }
            }
            EdAct::OpenFolder => {
                if let Some(p) = self.app.ed.as_ref().and_then(|e| e.saved.clone()).and_then(|p| p.parent().map(|d| d.to_path_buf())) {
                    plat.open_path(&p);
                }
            }
            EdAct::Delete => self.app.ed.as_mut().expect("editor").delete_selected(),
            EdAct::CommentRow(i) => {
                let ed = self.app.ed.as_mut().expect("editor");
                let rows = ed.comments();
                if let Some(r) = rows.get(i) {
                    ed.sel = r.index;
                    ed.sel_file = r.from_file;
                    let y = r.index.and_then(|k| ed.edits.get(k)).and_then(|e| e.bounds()).or_else(|| ed.opened.as_ref().and_then(|o| o.comments.iter().find(|c| Some(c.id) == r.from_file)).map(|c| c.rect)).map_or(0.0, |b| b.y);
                    ed.goto(r.page, y);
                    ed.tool = Tool::Select;
                }
            }
            EdAct::CommentDelete(i) => {
                let ed = self.app.ed.as_mut().expect("editor");
                let rows = ed.comments();
                if let Some(r) = rows.get(i) {
                    ed.delete_comment(r);
                }
            }
            EdAct::FieldPick(i) => {
                let ed = self.app.ed.as_mut().expect("editor");
                if let Some(fi) = ed.list_for.take() {
                    let pick = ed.opened.as_ref().and_then(|o| o.fields.get(fi)).and_then(|f| f.options.get(i).map(|v| (f.name.clone(), v.clone())));
                    if let Some((name, value)) = pick {
                        ed.set_field(&name, value);
                    }
                }
            }
            EdAct::OnlyPage(b) => self.app.ed.as_mut().expect("editor").only_page = b,
            EdAct::FieldGo(i) => {
                let ed = self.app.ed.as_ref().expect("editor");
                if let Some((page, y)) = ed.opened.as_ref().and_then(|o| o.fields.get(i)).map(|f| (f.page, f.rect.y)) {
                    ed.goto(page, y);
                }
            }
            EdAct::FieldNext => {
                let ed = self.app.ed.as_ref().expect("editor");
                let cur = ed.lay.borrow().cur;
                let mut empty: Vec<(usize, f32)> = ed.opened.as_ref().map(|o| o.fields.iter().filter(|f| !f.pushbutton && !ed.field_filled(f)).map(|f| (f.page, f.rect.y)).collect()).unwrap_or_default();
                empty.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
                let target = empty.iter().find(|e| e.0 > cur || (e.0 == cur)).or(empty.first()).copied();
                match target {
                    Some((page, y)) => ed.goto(page, y),
                    None => self.ed_notice("Hai compilato tutti i campi.", false),
                }
            }
            EdAct::FieldsClear => {
                let ed = self.app.ed.as_mut().expect("editor");
                let v: Vec<Edit> = ed.edits.iter().filter(|e| !matches!(e, Edit::FieldValue { .. })).cloned().collect();
                if v.len() != ed.edits.len() {
                    ed.commit(v);
                }
            }
            EdAct::KeepEditing => self.app.ed.as_mut().expect("editor").saved = None,
            EdAct::Page(_) | EdAct::TextBox | EdAct::SizeSlider | EdAct::WidthSlider | EdAct::HBar => {}
        }
    }

    /// Una pagina di Cedro dalla barra di sinistra mentre l'editor è aperto: se ci sono modifiche non salvate
    /// si chiede prima. Vero se il clic è stato preso qui.
    pub fn ed_guard(&mut self, act: &Act) -> bool {
        let lab = match act {
            Act::Lab(l) | Act::LabGo(l, _) => *l,
            _ => return false,
        };
        let Some(ed) = self.app.ed.as_mut() else { return false };
        if lab == super::model::Lab::Pdf {
            ed.leave_lab = None;
            if ed.unsaved() > 0 {
                ed.leave = Some(None);
            } else {
                self.app.ed_leave(None);
            }
            return true;
        }
        ed.leave_lab = Some(lab);
        if ed.unsaved() > 0 {
            ed.leave = Some(None);
        } else {
            self.app.ed_leave(None);
        }
        true
    }

    /// Il pulsante «Modifica» della scheda PDF: apre il file scelto in coda (o il primo PDF), altrimenti chiede
    /// quale aprire.
    pub fn ed_start(&mut self, plat: &mut dyn Platform) {
        let app = &self.app;
        let vis = app.visible();
        let queued = app.selected.filter(|i| vis.contains(i)).or(vis.first().copied()).and_then(|i| app.files.get(i)).filter(|f| f.kind == crate::engine::Kind::Pdf).map(|f| f.path.clone());
        let path = match queued {
            Some(p) => Some(p),
            None => {
                let picked = plat.pick_files();
                let any = !picked.is_empty();
                let pdf = picked.into_iter().find(|p| p.is_file() && crate::engine::kind_of(p) == crate::engine::Kind::Pdf);
                if pdf.is_none() && any {
                    self.ed_notice("Scegli un PDF da modificare.", true);
                }
                pdf
            }
        };
        if let Some(p) = path {
            self.app.ed_open(&p);
        }
    }

    /// Un cursore o la pagina premuta e trascinata (gli `Slider` dell'editor).
    pub fn ed_drag(&mut self, a: &EdAct, r: R, x: f32, y: f32, ph: Phase) {
        match a {
            EdAct::Page(_) | EdAct::TextBox => self.ed_pointer(a.clone(), r, x, y, ph),
            EdAct::SizeSlider | EdAct::WidthSlider | EdAct::HBar if ph != Phase::Up => {
                let frac = if *a == EdAct::HBar { ((x - r.0) / r.2.max(1.0)).clamp(0.0, 1.0) } else { ((x - r.0 - 10.0) / (r.2 - 20.0).max(1.0)).clamp(0.0, 1.0) };
                self.ed_slide(a, frac);
            }
            _ => {}
        }
    }

    /// Cerca il testo scritto nel campo in tutto il file.
    fn ed_find(&mut self) {
        let Some(ed) = self.app.ed.as_mut() else { return };
        if ed.find.trim().is_empty() {
            ed.found.clear();
            ed.find_done = false;
            return;
        }
        let id = ed.id();
        ed.find_id = id;
        ed.finding = true;
        ed.find_done = false;
        ed.tab = EdTab::Text;
        ed.tool = Tool::Find;
        let (snap, query, case, whole) = (ed.snap(), ed.find.clone(), ed.case, ed.whole);
        self.app.edw.other(Job::Find { snap, id, query, case, whole });
    }

    /// Un cursore trascinato dell'editor: grandezza del testo o spessore del tratto.
    pub fn ed_slide(&mut self, a: &EdAct, frac: f32) {
        let Some(ed) = self.app.ed.as_mut() else { return };
        match a {
            EdAct::SizeSlider => {
                let v = (6.0 + frac * 90.0).round();
                if let Some(t) = ed.text.as_mut() {
                    t.size = v;
                    t.ovr.size = Some(v);
                    if !matches!(t.target, Target::Existing(_)) {
                        ed.size = v;
                    }
                } else {
                    ed.size = v;
                }
            }
            EdAct::WidthSlider => ed.width = (1.0 + frac * 15.0).round(),
            EdAct::HBar => {
                let max = ed.lay.borrow().max_x;
                self.ui.scroll.insert(super::core::Scroll::EdPageX, frac * max);
            }
            _ => {}
        }
    }

    /// Il mouse sulla pagina (o sul testo in modifica) dell'editor.
    pub fn ed_pointer(&mut self, a: EdAct, r: R, x: f32, y: f32, ph: Phase) {
        match a {
            EdAct::TextBox => self.ed_text_click(x, y),
            EdAct::Page(i) => {
                let Some(ed) = self.app.ed.as_ref() else { return };
                // Il rettangolo del clic è tagliato dalla zona che si vede: la pagina intera sta in `lay.pages`.
                let r = ed.lay.borrow().pages.iter().find(|(p, _)| *p == i).map_or(r, |(_, full)| *full);
                let (pw, phh) = ed.size_of(i);
                let pt = clamp_pt(ed, i, ((x - r.0) / r.2.max(1.0) * pw, (y - r.1) / r.3.max(1.0) * phh));
                match ph {
                    Phase::Down => self.ed_down(i, pt),
                    Phase::Move => self.ed_move(i, pt),
                    Phase::Up => self.ed_up(i, pt),
                }
            }
            _ => {}
        }
    }

    /// Il mouse passa sopra una pagina (senza premere): serve a evidenziare il paragrafo che si sceglierebbe.
    pub fn ed_hover(&mut self, over: Option<usize>, x: f32, y: f32) -> bool {
        let Some(ed) = self.app.ed.as_mut() else { return false };
        let new = over.and_then(|i| {
            let r = ed.lay.borrow().pages.iter().find(|(p, _)| *p == i).map(|(_, r)| *r)?;
            let (pw, ph) = ed.size_of(i);
            Some((i, (x - r.0) / r.2.max(1.0) * pw, (y - r.1) / r.3.max(1.0) * ph))
        });
        let changed = match (&ed.mouse, &new) {
            (None, None) => false,
            (Some(a), Some(b)) => (a.0, a.1.round(), a.2.round()) != (b.0, b.1.round(), b.2.round()),
            _ => true,
        };
        ed.mouse = new;
        changed
    }

    fn ed_text_click(&mut self, x: f32, y: f32) {
        let Some(ed) = self.app.ed.as_ref() else { return };
        let lay = ed.lay.borrow();
        let Some((r, lh, lines)) = lay.edit_box.as_ref() else { return };
        if lines.is_empty() {
            return;
        }
        let row = (((y - r.1) / lh.max(1.0)).floor().max(0.0) as usize).min(lines.len() - 1);
        let (start, xs) = &lines[row];
        let rel = x - r.0;
        let col = xs.iter().enumerate().min_by(|a, b| (a.1 - rel).abs().total_cmp(&(b.1 - rel).abs())).map_or(0, |c| c.0);
        let at = start + col.min(xs.len().saturating_sub(1));
        drop(lay);
        self.ui.caret = at;
        self.ui.sel_all = false;
        let on_field = self.app.ed.as_ref().is_some_and(|e| e.text.is_none() && e.field.is_some());
        self.ui.edit = Some(Field::Ed(if on_field { EdField::FieldText } else { EdField::Text }));
    }

    fn ed_down(&mut self, page: usize, p: (f32, f32)) {
        self.ed_sync();
        self.ui.edit = None;
        self.ed_sync();
        let Some(ed) = self.app.ed.as_mut() else { return };
        if ed.opened.is_none() {
            return;
        }
        ed.applied = None;
        match ed.tool {
            Tool::Select => {
                if let Some(i) = ed.edit_at(page, p.0, p.1) {
                    ed.sel = Some(i);
                    ed.sel_file = None;
                    ed.drag = Some(Drag::Move { index: i, page, from: p, orig: ed.edits[i].clone() });
                } else if let Some(id) = ed.comment_at(page, p.0, p.1) {
                    ed.sel = None;
                    ed.sel_file = Some(id);
                } else {
                    ed.sel = None;
                    ed.sel_file = None;
                }
            }
            Tool::EditText => {
                if let Some(i) = ed.edit_at(page, p.0, p.1).filter(|i| matches!(ed.edits[*i], Edit::AddText { .. } | Edit::Note { .. })) {
                    let e = ed.edits[i].clone();
                    match e {
                        Edit::AddText { x, y, w, text, size, color, bold, italic, font, align, .. } => {
                            let rect = e_bounds(&Edit::AddText { page, x, y, w, text: text.clone(), size, color, bold, italic, font: font.clone(), align });
                            self.ed_begin_text(TextEdit { page, target: Target::Again(i), rect, orig: text.clone(), text, size, color, bold, italic, align, font, font_name: String::new(), ovr: Ovr::default(), has: None });
                        }
                        Edit::Note { x, y, text, color, .. } => {
                            self.ed_begin_text(TextEdit { page, target: Target::NoteAgain(i), rect: Rect::new(x, y, 200.0, 60.0), orig: text.clone(), text, size: 12.0, color, bold: false, italic: false, align: Align::Left, font: FontChoice::Same, font_name: String::new(), ovr: Ovr::default(), has: None });
                        }
                        _ => {}
                    }
                    return;
                }
                if ed.fresh_text(page).is_none() {
                    return;
                }
                match ed.block_at(page, p.0, p.1).cloned() {
                    Some(b) if b.editable() => {
                        let te = TextEdit {
                            page,
                            target: Target::Existing(b.rect),
                            rect: b.rect,
                            orig: b.text.clone(),
                            text: b.text.clone(),
                            size: b.size,
                            color: b.color,
                            bold: b.bold,
                            italic: b.italic,
                            align: b.align,
                            font: FontChoice::Same,
                            font_name: b.font_name.clone(),
                            ovr: Ovr::default(),
                            has: Some(b.has.clone()),
                        };
                        self.ed_begin_text(te);
                    }
                    Some(b) => {
                        let why = match b.why {
                            crate::pdfmod::text::Why::Scan => "Questa pagina è una scansione: il testo si cerca e si evidenzia, ma non si riscrive.",
                            crate::pdfmod::text::Why::Rotated => "Il testo girato non si modifica ancora: si può cancellare con «Oscura».",
                            _ => "Il carattere di questo testo non dice che lettera è ogni segno: non lo posso riscrivere senza rovinarlo.",
                        };
                        self.ed_notice(why, true);
                    }
                    None => {}
                }
            }
            Tool::AddText | Tool::Stamp | Tool::Image | Tool::Rect | Tool::Ellipse | Tool::Area | Tool::NewText | Tool::NewCheck | Tool::NewRadio | Tool::NewList | Tool::NewSign => {
                ed.drag = Some(Drag::Rubber { page, from: p, to: p });
            }
            Tool::Line | Tool::Arrow => ed.drag = Some(Drag::Line { page, from: p, to: p }),
            Tool::Highlight | Tool::Underline | Tool::Strike => ed.drag = Some(Drag::Words { page, from: p, to: p }),
            Tool::Pen => ed.drag = Some(Drag::Pen { page, pts: vec![p] }),
            Tool::Note => {
                let rect = Rect::new(p.0, p.1, 200.0, 60.0);
                self.ed_begin_text(TextEdit { page, target: Target::Note, rect, orig: String::new(), text: String::new(), size: 12.0, color: [0x1D, 0x1D, 0x1F], bold: false, italic: false, align: Align::Left, font: FontChoice::Same, font_name: String::new(), ovr: Ovr::default(), has: None });
            }
            Tool::Fill => self.ed_fill_click(page, p),
            Tool::Find | Tool::SearchRedact | Tool::Hidden => {}
        }
    }

    fn ed_fill_click(&mut self, page: usize, p: (f32, f32)) {
        let Some(ed) = self.app.ed.as_mut() else { return };
        let Some(fi) = ed.field_at(page, p.0, p.1) else { return };
        let f = ed.opened.as_ref().expect("aperto").fields[fi].clone();
        if f.read_only {
            self.ed_notice("Questo campo è di sola lettura.", true);
            return;
        }
        match f.kind {
            FieldKind::Text => {
                let text = ed.field_value(&f.name);
                let n = text.chars().count();
                ed.field = Some(FieldEdit { name: f.name, rect: f.rect, page, multiline: f.multiline, max_len: f.max_len, text });
                self.ui.edit = Some(Field::Ed(EdField::FieldText));
                self.ui.caret = n;
                self.ui.sel_all = false;
            }
            FieldKind::Check => {
                let on = ed.field_value(&f.name);
                let on = on == "On" || on == f.export || (on != "Off" && !on.is_empty() && on != "0");
                ed.set_field(&f.name, if on { "Off".into() } else { "On".into() });
            }
            FieldKind::Radio => {
                let value = if f.export.is_empty() { "On".to_string() } else { f.export.clone() };
                ed.set_field(&f.name, value);
            }
            FieldKind::List => {
                if f.options.is_empty() {
                    return;
                }
                ed.list_for = Some(fi);
                let cur = ed.field_value(&f.name);
                let items: Vec<(String, Act)> = f.options.iter().enumerate().map(|(i, o)| (o.clone(), Act::Ed(EdAct::FieldPick(i)))).collect();
                let sel = f.options.iter().position(|o| *o == cur);
                let anchor = self.ed_page_point(page, f.rect.x, f.rect.bottom());
                self.ui.menu = Some(super::core::Menu { anchor: (anchor.0, anchor.1 - 4.0, f.rect.w * ed_k(&self.app), 4.0), items, selected: sel, title: None, swatches: Vec::new(), voci: Vec::new(), groups: Vec::new(), plain: false });
            }
            FieldKind::Sign => self.ed_notice("Il campo firma si firma poi in «Firma digitale».", false),
        }
    }

    /// Dove sta sullo schermo un punto della pagina.
    fn ed_page_point(&self, page: usize, x: f32, y: f32) -> (f32, f32) {
        let Some(ed) = self.app.ed.as_ref() else { return (0.0, 0.0) };
        let lay = ed.lay.borrow();
        match lay.pages.iter().find(|(p, _)| *p == page) {
            Some((_, r)) => (r.0 + x * lay.k, r.1 + y * lay.k),
            None => (lay.canvas.0, lay.canvas.1),
        }
    }

    fn ed_move(&mut self, page: usize, p: (f32, f32)) {
        let Some(ed) = self.app.ed.as_mut() else { return };
        match ed.drag.as_mut() {
            Some(Drag::Words { to, .. }) | Some(Drag::Rubber { to, .. }) | Some(Drag::Line { to, .. }) => *to = p,
            Some(Drag::Pen { pts, page: pg }) => {
                if *pg == page && pts.last().is_none_or(|l| (l.0 - p.0).hypot(l.1 - p.1) >= 1.0) {
                    pts.push(p);
                }
            }
            Some(Drag::Move { from, .. }) => {
                let _ = from;
                ed.mouse = Some((page, p.0, p.1));
            }
            None => {}
        }
    }

    fn ed_up(&mut self, page: usize, p: (f32, f32)) {
        let Some(ed) = self.app.ed.as_mut() else { return };
        let Some(drag) = ed.drag.take() else { return };
        match drag {
            Drag::Move { index, from, orig, .. } => {
                let (dx, dy) = (p.0 - from.0, p.1 - from.1);
                if dx.abs() + dy.abs() >= 2.0 && index < ed.edits.len() {
                    let mut e = orig;
                    translate(&mut e, dx, dy);
                    let mut v = ed.edits.clone();
                    v[index] = e;
                    ed.commit(v);
                }
            }
            Drag::Words { page, from, to } => {
                let rects = ed.words_between(page, from, to);
                if rects.is_empty() {
                    if (from.0 - to.0).abs() + (from.1 - to.1).abs() > 3.0 {
                        self.ed_notice("Trascina sopra il testo da segnare.", false);
                    }
                    return;
                }
                let (kind, color) = match ed.tool {
                    Tool::Underline => (MarkKind::Underline, if ed.mark == MARK_COLORS[0] { INK_COLORS[2] } else { ed.mark }),
                    Tool::Strike => (MarkKind::Strike, if ed.mark == MARK_COLORS[0] { INK_COLORS[2] } else { ed.mark }),
                    _ => (MarkKind::Highlight, ed.mark),
                };
                ed.push(Edit::Mark { page, kind, rects, color, note: String::new() });
            }
            Drag::Pen { page, pts } => {
                if pts.len() >= 2 {
                    let (color, width) = (ed.ink, ed.width.max(1.0));
                    ed.push(Edit::Pen { page, points: pts, color, width });
                }
            }
            Drag::Line { page, from, to } => {
                if (from.0 - to.0).hypot(from.1 - to.1) >= 4.0 {
                    let kind = if ed.tool == Tool::Arrow { ShapeKind::Arrow } else { ShapeKind::Line };
                    let (color, width) = (ed.ink, ed.width);
                    ed.push(Edit::Shape { page, kind, a: from, b: to, color, fill: None, width });
                }
            }
            Drag::Rubber { page, from, to } => self.ed_rubber(page, from, to),
        }
        let _ = page;
    }

    /// Un riquadro tirato sulla pagina: cosa diventa dipende dallo strumento.
    fn ed_rubber(&mut self, page: usize, from: (f32, f32), to: (f32, f32)) {
        let Some(ed) = self.app.ed.as_mut() else { return };
        let (pw, ph) = ed.size_of(page);
        let small = (from.0 - to.0).abs() < 4.0 && (from.1 - to.1).abs() < 4.0;
        let drawn = Rect::between(from, to);
        match ed.tool {
            Tool::Rect | Tool::Ellipse => {
                if !small {
                    let kind = if ed.tool == Tool::Rect { ShapeKind::Rect } else { ShapeKind::Ellipse };
                    let (color, fill, width) = (ed.ink, ed.fill, ed.width);
                    ed.push(Edit::Shape { page, kind, a: from, b: to, color, fill, width });
                }
            }
            Tool::Area => {
                if !small {
                    ed.push(Edit::Redact { page, rect: drawn, applied: false });
                }
            }
            Tool::Stamp => {
                let rect = if small { Rect::new(from.0, from.1, 150.0, 44.0) } else { drawn };
                let (label, color) = if ed.stamp_text.trim().is_empty() { (crate::i18n::tr_string(STAMPS[ed.stamp].0), STAMPS[ed.stamp].1) } else { (ed.stamp_text.trim().to_uppercase(), STAMPS[ed.stamp].1) };
                ed.push(Edit::Stamp { page, rect, label, color });
            }
            Tool::Image => {
                let Some((data, iw, ih)) = ed.image.clone() else {
                    self.ed_notice("Scegli prima l'immagine con il pulsante «Immagine».", false);
                    return;
                };
                let aspect = ih as f32 / iw.max(1) as f32;
                let rect = if small {
                    let w = (pw * 0.4).min(iw as f32).max(40.0);
                    Rect::new(from.0, from.1, w, w * aspect)
                } else {
                    // L'immagine segue la forma del riquadro: resta com'è, senza stirarsi.
                    let w = drawn.w.max(8.0);
                    Rect::new(drawn.x, drawn.y, w, w * aspect)
                };
                ed.push(Edit::Image { page, rect, data });
            }
            Tool::AddText => {
                let w = if small { (pw - from.0 - 12.0).clamp(60.0, 240.0) } else { drawn.w.max(40.0) };
                let (x, y) = if small { from } else { (drawn.x, drawn.y) };
                let rect = Rect::new(x, y, w, ed.size * 1.2);
                let te = TextEdit { page, target: Target::New, rect, orig: String::new(), text: String::new(), size: ed.size, color: ed.text_ink, bold: ed.bold, italic: ed.italic, align: ed.align, font: ed.font.clone(), font_name: String::new(), ovr: Ovr::default(), has: None };
                self.ed_begin_text(te);
            }
            t => {
                let Some(kind) = field_kind(t) else { return };
                let (dw, dh) = default_field(kind);
                let rect = if small { Rect::new(from.0, from.1, dw.min(pw - from.0), dh.min(ph - from.1)) } else { drawn };
                let name = ed.new_field_name(kind);
                let options = match kind {
                    FieldKind::List => vec!["Opzione 1".to_string(), "Opzione 2".to_string(), "Opzione 3".to_string()],
                    FieldKind::Radio => vec!["Sì".to_string()],
                    _ => Vec::new(),
                };
                ed.push(Edit::NewField { page, rect, kind, name, options });
            }
        }
    }

    /// I tasti dell'editor. Vero se li ha usati.
    pub fn ed_key(&mut self, key: Key) -> bool {
        let Some(ed) = self.app.ed.as_ref() else { return false };
        let editing = self.ui.edit;
        // Il testo che si scrive dentro la pagina.
        if editing == Some(Field::Ed(EdField::Text)) {
            match key {
                Key::Enter => {
                    // Invio va a capo; si finisce con un clic fuori o con Esc.
                    self.text_input("\n");
                    return true;
                }
                Key::Escape => {
                    self.ed_finish_text(false);
                    return true;
                }
                Key::Up | Key::Down => {
                    let lay = ed.lay.borrow();
                    if let Some((_, _, lines)) = lay.edit_box.as_ref() {
                        let caret = self.ui.caret;
                        let row = lines.iter().rposition(|(s, _)| *s <= caret).unwrap_or(0);
                        let (start, xs) = &lines[row];
                        let x = xs.get(caret - start).copied().unwrap_or(0.0);
                        let to = if key == Key::Up { row.checked_sub(1) } else { (row + 1 < lines.len()).then_some(row + 1) };
                        if let Some(t) = to {
                            let (s2, xs2) = &lines[t];
                            let col = xs2.iter().enumerate().min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs())).map_or(0, |c| c.0);
                            let at = s2 + col.min(xs2.len().saturating_sub(1));
                            drop(lay);
                            self.ui.caret = at;
                        }
                    }
                    return true;
                }
                Key::Undo | Key::Redo => return true,
                _ => return false,
            }
        }
        if editing == Some(Field::Ed(EdField::FieldText)) {
            match key {
                Key::Enter => {
                    if self.app.ed.as_ref().and_then(|e| e.field.as_ref()).is_some_and(|f| f.multiline) {
                        self.text_input("\n");
                    } else {
                        self.ed_finish_field(true);
                    }
                    return true;
                }
                Key::Escape => {
                    self.ed_finish_field(false);
                    return true;
                }
                _ => return false,
            }
        }
        if let Some(Field::Ed(f)) = editing {
            match (f, key) {
                (EdField::TopFind | EdField::Find, Key::Enter) => {
                    self.ui.edit = None;
                    self.ed_find();
                    return true;
                }
                (EdField::Free, Key::Enter) => {
                    self.ui.edit = None;
                    let mut plat = NoPlatform;
                    self.ed_act(EdAct::MarkFree, &mut plat);
                    return true;
                }
                (EdField::Replace | EdField::SelText | EdField::StampText, Key::Enter) => {
                    self.ui.edit = None;
                    return true;
                }
                _ => return false,
            }
        }
        if editing.is_some() {
            return false;
        }
        // Senza un campo in scrittura.
        match key {
            Key::Undo => {
                self.ed_sync();
                if let Some(ed) = self.app.ed.as_mut() {
                    ed.undo();
                }
                true
            }
            Key::Redo => {
                if let Some(ed) = self.app.ed.as_mut() {
                    ed.redo();
                }
                true
            }
            Key::Delete | Key::Backspace => {
                let ed = self.app.ed.as_mut().expect("editor");
                if ed.sel.is_some() || ed.sel_file.is_some() {
                    ed.delete_selected();
                    true
                } else {
                    false
                }
            }
            Key::Escape => {
                if self.ui.menu.is_some() {
                    return false;
                }
                let ed = self.app.ed.as_mut().expect("editor");
                if ed.leave.is_some() {
                    ed.leave = None;
                } else if ed.drag.take().is_some() {
                } else if ed.sel.is_some() || ed.sel_file.is_some() {
                    ed.sel = None;
                    ed.sel_file = None;
                } else if ed.tool != Tool::Select {
                    ed.tool = Tool::Select;
                } else {
                    return false;
                }
                true
            }
            Key::Up | Key::Down | Key::Home | Key::End => {
                let max = {
                    let lay = self.app.ed.as_ref().expect("editor").lay.borrow();
                    (lay.content_h - lay.canvas.3).max(0.0)
                };
                let cur = self.ui.scroll(super::core::Scroll::EdPage);
                let to = match key {
                    Key::Up => cur - 70.0,
                    Key::Down => cur + 70.0,
                    Key::Home => 0.0,
                    _ => max,
                };
                self.ui.scroll.insert(super::core::Scroll::EdPage, to.clamp(0.0, max));
                true
            }
            _ => false,
        }
    }
}

fn e_bounds(e: &Edit) -> Rect {
    e.bounds().unwrap_or_default()
}

fn ed_k(app: &super::model::App) -> f32 {
    app.ed.as_ref().map_or(1.0, |e| e.lay.borrow().k)
}

/// Una piattaforma che non fa niente: per i tasti che richiamano un clic senza averne una sotto mano.
struct NoPlatform;

impl Platform for NoPlatform {
    fn pick_files(&mut self) -> Vec<std::path::PathBuf> {
        Vec::new()
    }
    fn pick_folder(&mut self, _: &str) -> Option<std::path::PathBuf> {
        None
    }
    fn open_path(&mut self, _: &std::path::Path) {}
    fn copy_text(&mut self, _: &str) {}
    fn paste_text(&mut self) -> Option<String> {
        None
    }
    fn open_guide(&mut self) {}
    fn open_settings(&mut self) {}
    fn minimize(&mut self) {}
    fn toggle_maximize(&mut self) {}
    fn close(&mut self) {}
    fn open_full(&mut self) {}
}
