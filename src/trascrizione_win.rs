//! La scheda «Trascrizione» della categoria Audio nel pannello Conversioni di Windows:
//! «Salva come», «Precisione» e la finestra «Cosa sta succedendo» a destra.
//! Vive dentro `conversioni_win` e ne usa i pezzi comuni (finestre figlie, colori, caratteri).

use super::*;
use cedro::trascrizione::{self, Event, Format, Line, Request, Tone, MODELS};

pub(super) const ID_TAB_CONVERT: i32 = 221;
pub(super) const ID_TAB_TRANSCRIBE: i32 = 222;
pub(super) const ID_CAP_SAVEAS: i32 = 223;
pub(super) const ID_CAP_MODEL: i32 = 224;
pub(super) const ID_CAP_LOG: i32 = 225;
pub(super) const ID_COPY: i32 = 226;
pub(super) const ID_LOG: i32 = 227;
pub(super) const ID_TFMT0: i32 = 3000;
pub(super) const ID_MODEL0: i32 = 3100;

pub(super) const WM_APP_LOG: u32 = WM_APP + 13;

const FORMATS: [(Format, &str); 2] = [(Format::Word, "Word"), (Format::Text, "Testo semplice")];

// La finestra dei messaggi: fondo scuro come un terminale, con i colori dell'anteprima.
const LOG_BG: COLORREF = COLORREF(0x00241B0D); // #0d1b24
const LOG_TEXT: COLORREF = COLORREF(0x00F0E3CF); // #cfe3f0
const LOG_KEY: COLORREF = COLORREF(0x00EEC47F); // #7fc4ee
const LOG_SPEECH: COLORREF = COLORREF(0x00A5DC7F); // #7fdca5
const LOG_NOTE: COLORREF = COLORREF(0x006DC3F0); // #f0c36d
const LOG_MUTED: COLORREF = COLORREF(0x00A3937D); // #7d93a3
const LOG_ERROR: COLORREF = COLORREF(0x008080FF); // #ff8080
const WARN: COLORREF = COLORREF(0x00005CA1); // #a15c00

static FONT_MONO: AtomicIsize = AtomicIsize::new(0);
/// Le righe che il thread della trascrizione lascia alla finestra.
static LOG_QUEUE: Mutex<Vec<Line>> = Mutex::new(Vec::new());

/// Le scelte della scheda, che restano finché il pannello è aperto.
pub(super) struct Choices {
    pub on: bool,
    pub format: Format,
    pub model: usize,
}

pub(super) static CHOICES: Mutex<Choices> = Mutex::new(Choices { on: false, format: Format::Word, model: trascrizione::DEFAULT_MODEL });

fn choices() -> std::sync::MutexGuard<'static, Choices> {
    CHOICES.lock().unwrap_or_else(|e| e.into_inner())
}

/// Vero se la categoria scelta è Audio.
pub(super) fn audio_category() -> bool {
    CATEGORIES[state().category].kind == Kind::Audio
}

/// Vero se si vede la scheda Trascrizione.
pub(super) fn active() -> bool {
    audio_category() && choices().on
}

pub(super) fn set_active(on: bool) {
    choices().on = on;
}

/// Quanto scende la parte alta del pannello per far posto alle due schede di Audio.
pub(super) fn shift() -> i32 {
    if audio_category() { 40 } else { 0 }
}

/// Crea i controlli della scheda (nascosti: li mostra `layout`).
pub(super) unsafe fn build(hwnd: HWND, instance: HINSTANCE) {
    let (f, fb, fs) = (font(&FONT), font(&FONT_BOLD), font(&FONT_SMALL));
    FONT_MONO.store(
        CreateFontW(-px(13), 0, 0, 0, 400, 0, 0, 0, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY, 0, w!("Consolas")).0 as isize,
        Ordering::Relaxed,
    );
    let b = w!("BUTTON");
    let s = w!("STATIC");
    let tab = WS_TABSTOP.0 | BS_OWNERDRAW as u32;
    child(hwnd, instance, b, "Cambia formato", tab, ID_TAB_CONVERT, (210, 114, 150, 32), fb);
    child(hwnd, instance, b, "Trascrizione", tab, ID_TAB_TRANSCRIBE, (364, 114, 150, 32), fb);
    child(hwnd, instance, s, "SALVA COME", 0, ID_CAP_SAVEAS, (210, 296, 320, 16), fs);
    for (i, (_, label)) in FORMATS.iter().enumerate() {
        child(hwnd, instance, b, label, tab, ID_TFMT0 + i as i32, (210, 314, 120, 32), f);
    }
    child(hwnd, instance, s, "PRECISIONE", 0, ID_CAP_MODEL, (210, 356, 320, 16), fs);
    for (i, m) in MODELS.iter().enumerate() {
        child(hwnd, instance, b, m.name, tab, ID_MODEL0 + i as i32, (210, 374, 320, 52), f);
    }
    child(hwnd, instance, s, "COSA STA SUCCEDENDO", 0, ID_CAP_LOG, (550, 296, 240, 16), fs);
    child(hwnd, instance, b, "Copia", WS_TABSTOP.0, ID_COPY, (810, 290, 70, 24), f);
    // Il RichEdit colora le righe come nell'anteprima; senza la DLL resta una casella semplice.
    let rich = LoadLibraryW(w!("Msftedit.dll")).is_ok();
    let class = if rich { w!("RICHEDIT50W") } else { w!("EDIT") };
    let style = WS_TABSTOP.0 | WS_VSCROLL.0 | ES_MULTILINE as u32 | ES_READONLY as u32 | ES_AUTOVSCROLL as u32;
    let log = child(hwnd, instance, class, "", style, ID_LOG, (550, 314, 330, 228), font(&FONT_MONO));
    if rich {
        SendMessageW(log, EM_SETBKGNDCOLOR, Some(WPARAM(0)), Some(LPARAM(LOG_BG.0 as isize)));
        set_format(log, LOG_TEXT, SCF_ALL);
        let margin = px(8) as isize;
        SendMessageW(log, EM_SETMARGINS, Some(WPARAM((EC_LEFTMARGIN | EC_RIGHTMARGIN) as usize)), Some(LPARAM(margin | (margin << 16))));
    }
    idle_text(hwnd);
}

const SCF_SELECTION: usize = 1;
const SCF_ALL: usize = 4;
const CFM_COLOR: u32 = 0x4000_0000;
const EM_SETBKGNDCOLOR: u32 = WM_USER + 67;
const EM_SETCHARFORMAT: u32 = WM_USER + 68;

/// CHARFORMATW, quanto basta per il colore.
#[repr(C)]
struct CharFormat {
    cb_size: u32,
    dw_mask: u32,
    dw_effects: u32,
    y_height: i32,
    y_offset: i32,
    cr_text_color: u32,
    b_char_set: u8,
    b_pitch_and_family: u8,
    sz_face_name: [u16; 32],
}

unsafe fn set_format(edit: HWND, color: COLORREF, scope: usize) {
    let mut cf = CharFormat {
        cb_size: std::mem::size_of::<CharFormat>() as u32,
        dw_mask: CFM_COLOR,
        dw_effects: 0,
        y_height: 0,
        y_offset: 0,
        cr_text_color: color.0,
        b_char_set: 0,
        b_pitch_and_family: 0,
        sz_face_name: [0; 32],
    };
    SendMessageW(edit, EM_SETCHARFORMAT, Some(WPARAM(scope)), Some(LPARAM(&mut cf as *mut _ as isize)));
}

/// Il testo della finestra quando non lavora: cosa succederà premendo il pulsante.
unsafe fn idle_text(hwnd: HWND) {
    let log = item(hwnd, ID_LOG);
    let _ = SetWindowTextW(log, w!(""));
    let ready = trascrizione::model_ready(choices().model);
    let model = &MODELS[choices().model];
    let first = if ready {
        format!("Il modello «{}» è già sul PC.", model.name)
    } else {
        format!("Alla prima trascrizione Cedro scarica il motore e il modello «{}» ({}).", model.name, trascrizione::size_text(model.megabytes))
    };
    append(hwnd, &Line { tone: Tone::Muted, key: String::new(), text: first }, false);
    append(hwnd, &Line { tone: Tone::Muted, key: String::new(), text: "Qui vedrai i passi e le frasi man mano che le scrive. A fine lavoro il motore si spegne e libera la memoria.".into() }, false);
}

/// Aggiunge una riga in fondo, con l'ora davanti, e scorre fino a lei.
unsafe fn append(hwnd: HWND, line: &Line, stamp: bool) {
    let log = item(hwnd, ID_LOG);
    let mut parts: Vec<(String, COLORREF)> = Vec::new();
    if stamp {
        let t = windows::Win32::System::SystemInformation::GetLocalTime();
        parts.push((format!("{:02}:{:02}:{:02} ", t.wHour, t.wMinute, t.wSecond), LOG_MUTED));
    }
    let (key_color, text_color) = match line.tone {
        Tone::Step => (LOG_KEY, LOG_TEXT),
        Tone::Speech => (LOG_SPEECH, LOG_TEXT),
        Tone::Note => (LOG_NOTE, LOG_TEXT),
        Tone::Error => (LOG_ERROR, LOG_ERROR),
        Tone::Muted => (LOG_MUTED, LOG_MUTED),
    };
    if !line.key.is_empty() {
        parts.push((format!("{} ", line.key), key_color));
    }
    parts.push((format!("{}\r\n", line.text), text_color));
    for (text, color) in parts {
        SendMessageW(log, EM_SETSEL, Some(WPARAM(usize::MAX)), Some(LPARAM(-1)));
        set_format(log, color, SCF_SELECTION);
        let wide = HSTRING::from(text);
        SendMessageW(log, EM_REPLACESEL, Some(WPARAM(0)), Some(LPARAM(wide.as_ptr() as isize)));
    }
    SendMessageW(log, WM_VSCROLL, Some(WPARAM(SB_BOTTOM.0 as usize)), Some(LPARAM(0)));
}

/// Le righe arrivate dal thread della trascrizione.
pub(super) unsafe fn take_log(hwnd: HWND) {
    let lines: Vec<Line> = std::mem::take(&mut *LOG_QUEUE.lock().unwrap_or_else(|e| e.into_inner()));
    for line in &lines {
        append(hwnd, line, true);
    }
}

/// Mostra o nasconde i pezzi delle due schede e mette al loro posto quelli della trascrizione.
/// Ritorna dove finisce la scheda (per la parte bassa), se la trascrizione è attiva.
pub(super) unsafe fn layout(hwnd: HWND) -> Option<i32> {
    let audio = audio_category();
    let on = active();
    for id in [ID_TAB_CONVERT, ID_TAB_TRANSCRIBE] {
        let _ = ShowWindow(item(hwnd, id), if audio { SW_SHOW } else { SW_HIDE });
        let _ = InvalidateRect(Some(item(hwnd, id)), None, true);
    }
    let mut ids = vec![ID_CAP_SAVEAS, ID_CAP_MODEL, ID_CAP_LOG, ID_COPY, ID_LOG];
    ids.extend((0..FORMATS.len() as i32).map(|i| ID_TFMT0 + i));
    ids.extend((0..MODELS.len() as i32).map(|i| ID_MODEL0 + i));
    for id in ids {
        let _ = ShowWindow(item(hwnd, id), if on { SW_SHOW } else { SW_HIDE });
    }
    if !on {
        return None;
    }
    let top = 256 + shift();
    place(hwnd, ID_CAP_SAVEAS, (210, top, 320, 16));
    let mut x = 210;
    for (i, (_, label)) in FORMATS.iter().enumerate() {
        let width = 28 + 9 * label.chars().count() as i32;
        place(hwnd, ID_TFMT0 + i as i32, (x, top + 18, width, 32));
        x += width + 8;
    }
    place(hwnd, ID_CAP_MODEL, (210, top + 62, 320, 16));
    for i in 0..MODELS.len() as i32 {
        place(hwnd, ID_MODEL0 + i, (210, top + 80 + i * 58, 320, 52));
    }
    let end = top + 80 + MODELS.len() as i32 * 58 - 6;
    place(hwnd, ID_CAP_LOG, (550, top, 240, 16));
    place(hwnd, ID_COPY, (810, top - 6, 70, 24));
    place(hwnd, ID_LOG, (550, top + 18, 330, end - top - 18));
    for id in (0..FORMATS.len() as i32).map(|i| ID_TFMT0 + i).chain((0..MODELS.len() as i32).map(|i| ID_MODEL0 + i)) {
        let _ = InvalidateRect(Some(item(hwnd, id)), None, true);
    }
    Some(end + 10)
}

pub(super) const HINT: &str = "Trasforma la voce in testo. Riconosce la lingua da sola, funziona sul tuo PC. Accetta: MP3, WAV, M4A, OGG, OPUS, FLAC, WMA, AMR e l'audio dei video.";

/// L'etichetta del pulsante in fondo.
pub(super) fn go_label(running: bool, n: usize) -> String {
    let what = FORMATS.iter().find(|(f, _)| *f == choices().format).map(|(f, _)| f.label()).unwrap_or("Word");
    match (running, n) {
        (true, _) => "Trascrizione in corso…".to_string(),
        (_, 0) => "Trascrivi".to_string(),
        (_, n) => format!("Trascrivi {n} file in {what}"),
    }
}

/// Un clic su una delle parti della scheda. Vero se era sua.
pub(super) unsafe fn command(hwnd: HWND, id: i32) -> bool {
    match id {
        ID_TAB_CONVERT | ID_TAB_TRANSCRIBE => {
            if state().running {
                return true;
            }
            set_active(id == ID_TAB_TRANSCRIBE);
            refresh_options(hwnd);
            render_queue(hwnd);
        }
        x if (ID_TFMT0..ID_TFMT0 + FORMATS.len() as i32).contains(&x) => {
            choices().format = FORMATS[(x - ID_TFMT0) as usize].0;
            let _ = layout(hwnd);
            render_queue(hwnd);
        }
        x if (ID_MODEL0..ID_MODEL0 + MODELS.len() as i32).contains(&x) => {
            choices().model = (x - ID_MODEL0) as usize;
            let _ = layout(hwnd);
            if !state().running {
                idle_text(hwnd);
            }
        }
        ID_COPY => copy_log(hwnd),
        _ => return false,
    }
    true
}

unsafe fn copy_log(hwnd: HWND) {
    use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    let log = item(hwnd, ID_LOG);
    let len = GetWindowTextLengthW(log) as usize;
    let mut buf = vec![0u16; len + 1];
    let n = GetWindowTextW(log, &mut buf) as usize;
    let text: Vec<u16> = buf[..n].iter().copied().chain(std::iter::once(0)).collect();
    if OpenClipboard(Some(hwnd)).is_err() {
        return;
    }
    let _ = EmptyClipboard();
    if let Ok(mem) = GlobalAlloc(GMEM_MOVEABLE, text.len() * 2) {
        let ptr = GlobalLock(mem) as *mut u16;
        if !ptr.is_null() {
            std::ptr::copy_nonoverlapping(text.as_ptr(), ptr, text.len());
            let _ = GlobalUnlock(mem);
            // CF_UNICODETEXT = 13: il sistema prende la memoria.
            let _ = SetClipboardData(13, Some(HANDLE(mem.0)));
        }
    }
    let _ = CloseClipboard();
}

/// Disegna le schede, i formati e le schede dei modelli. Vero se il controllo era suo.
pub(super) unsafe fn draw(di: &DRAWITEMSTRUCT) -> bool {
    let hdc = di.hDC;
    let mut rc = di.rcItem;
    let id = di.CtlID as i32;
    let pressed = di.itemState.0 & ODS_SELECTED.0 != 0;
    match id {
        ID_TAB_CONVERT | ID_TAB_TRANSCRIBE => {
            let on = (id == ID_TAB_TRANSCRIBE) == choices().on;
            FillRect(hdc, &rc, brush(&BRUSH_WHITE, WHITE));
            let line = CreateSolidBrush(if on { PRIMARY } else { LINE });
            let bar = RECT { top: rc.bottom - if on { px(3) } else { px(1) }, ..rc };
            FillRect(hdc, &bar, line);
            let _ = DeleteObject(HGDIOBJ(line.0));
            SelectObject(hdc, HGDIOBJ(font(&FONT_BOLD).0));
            SetTextColor(hdc, if on { PRIMARY } else if pressed { HOVER } else { GREY });
            let label = if id == ID_TAB_CONVERT { "Cambia formato" } else { "Trascrizione" };
            draw_text(hdc, label, &mut rc, DT_SINGLELINE | DT_VCENTER | DT_CENTER);
        }
        x if (ID_TFMT0..ID_TFMT0 + FORMATS.len() as i32).contains(&x) => {
            let (format, label) = FORMATS[(x - ID_TFMT0) as usize];
            let on = choices().format == format;
            FillRect(hdc, &rc, brush(if on { &BRUSH_PRIMARY } else { &BRUSH_WHITE }, if on { PRIMARY } else { WHITE }));
            let edge = CreateSolidBrush(if on { PRIMARY } else if pressed { HOVER } else { LINE });
            FrameRect(hdc, &rc, edge);
            let _ = DeleteObject(HGDIOBJ(edge.0));
            SelectObject(hdc, HGDIOBJ(font(&FONT).0));
            SetTextColor(hdc, if on { WHITE } else { TEXT });
            draw_text(hdc, label, &mut rc, DT_SINGLELINE | DT_VCENTER | DT_CENTER);
        }
        x if (ID_MODEL0..ID_MODEL0 + MODELS.len() as i32).contains(&x) => {
            let index = (x - ID_MODEL0) as usize;
            let model = &MODELS[index];
            let on = choices().model == index;
            FillRect(hdc, &rc, brush(if on { &BRUSH_DROP } else { &BRUSH_WHITE }, if on { DROP_BG } else { WHITE }));
            let edge = CreateSolidBrush(if on { PRIMARY } else if pressed { HOVER } else { LINE });
            FrameRect(hdc, &rc, edge);
            if on {
                let inner = RECT { left: rc.left + 1, top: rc.top + 1, right: rc.right - 1, bottom: rc.bottom - 1 };
                FrameRect(hdc, &inner, edge);
            }
            let _ = DeleteObject(HGDIOBJ(edge.0));
            let ready = trascrizione::model_ready(index);
            let mut top = RECT { left: rc.left + px(14), right: rc.right - px(12), top: rc.top + px(5), bottom: rc.top + px(27) };
            SelectObject(hdc, HGDIOBJ(font(&FONT_BOLD).0));
            SetTextColor(hdc, TEXT);
            draw_text(hdc, model.name, &mut top, DT_SINGLELINE | DT_VCENTER | DT_LEFT);
            SelectObject(hdc, HGDIOBJ(font(&FONT_SMALL).0));
            SetTextColor(hdc, if ready { OK } else { WARN });
            let status = if ready { "Sul PC".to_string() } else { "Si scarica la prima volta".to_string() };
            draw_text(hdc, &status, &mut top, DT_SINGLELINE | DT_VCENTER | DT_RIGHT);
            let mut bottom = RECT { left: rc.left + px(14), right: rc.right - px(12), top: rc.top + px(26), bottom: rc.bottom - px(5) };
            SelectObject(hdc, HGDIOBJ(font(&FONT).0));
            SetTextColor(hdc, GREY);
            draw_text(hdc, &format!("{} · {}", model.note, trascrizione::size_text(model.megabytes)), &mut bottom, DT_SINGLELINE | DT_VCENTER | DT_LEFT | DT_END_ELLIPSIS);
        }
        _ => return false,
    }
    true
}

/// Vero se la scheda prende questo file nella coda.
pub(super) fn takes(path: &std::path::Path) -> bool {
    trascrizione::transcribable(path)
}

/// Parte la trascrizione della coda in un thread. Il motore si accende ora e si spegne a fine coda.
pub(super) unsafe fn start(hwnd: HWND, files: Vec<PathBuf>, dest: Option<PathBuf>, opts: cedro::Options) {
    let settings = read_settings();
    let request = {
        let c = choices();
        Request { format: c.format, model: c.model, gpu: settings.gpu, avx512: settings.avx512 }
    };
    let log = item(hwnd, ID_LOG);
    let _ = SetWindowTextW(log, w!(""));
    let raw = hwnd.0 as isize;
    let format = request.format;
    std::thread::spawn(move || {
        let total = files.len();
        let name = |p: &std::path::Path| p.file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string();
        let label = |i: usize, p: &std::path::Path, pct: u8| {
            let head = if total == 1 { name(p) } else { format!("{} di {total} · {}", i + 1, name(p)) };
            if pct > 0 { format!("{head} · {pct}%") } else { format!("Trascrivo {head}") }
        };
        if let Some(first) = files.first() {
            crate::progress_win::start(total, &label(0, first, 0));
        }
        let failed = std::sync::Mutex::new(None::<String>);
        let post = |msg: u32| unsafe {
            let _ = PostMessageW(Some(HWND(raw as *mut _)), msg, WPARAM(0), LPARAM(0));
        };
        trascrizione::run(&files, &request, dest.as_deref(), &opts, &|event| match event {
            Event::Log(line) => {
                LOG_QUEUE.lock().unwrap_or_else(|e| e.into_inner()).push(line);
                post(WM_APP_LOG);
            }
            Event::Percent { index, percent } => {
                if let Some(p) = files.get(index) {
                    crate::progress_win::step(index, &label(index, p, percent));
                }
            }
            Event::Finished { path, result } => {
                let result = result.map(|(out, message)| {
                    crate::progress_win::produced(&out);
                    message
                });
                if let Err(e) = &result {
                    failed.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(|| e.clone());
                }
                RESULTS.lock().unwrap_or_else(|e| e.into_inner()).push((path, result));
                post(WM_APP_RESULT);
            }
        });
        match failed.into_inner().unwrap_or_else(|e| e.into_inner()) {
            Some(e) => crate::progress_win::finish(false, &format!("Errore · {e}")),
            None => crate::progress_win::finish(true, &format!("✓ Fatto · {total} file in {}", format.label())),
        }
        post(WM_APP_DONE);
    });
}
