//! Il pannello Conversioni su Windows, con i colori del Comune di Bondeno.
//! Finestra Win32 scritta a mano come le Impostazioni: categorie a sinistra, coda di file con
//! trascinamento, formati e strumenti, qualità, cartella di destinazione. La conversione gira in un
//! thread a parte e riporta ogni file alla finestra con un messaggio.

use cedro::conversioni::{self, Control, OptionDef, Values, CATEGORIES, EXISTS_CHOICES};
use cedro::{Exists, ExistsAsk, ExistsChoice};
use cedro::{extension, kind_of, read_settings, Kind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Mutex;
#[path = "trascrizione_win.rs"]
mod trascrizione;

use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::Gdi::*,
    Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER},
    Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW},
    Win32::UI::Controls::*,
    Win32::UI::HiDpi::GetDpiForSystem,
    Win32::UI::Input::KeyboardAndMouse::EnableWindow,
    Win32::UI::Shell::*,
    Win32::UI::WindowsAndMessaging::*,
};

const ID_HEADER: i32 = 130;
const ID_POWERED: i32 = 131;
const ID_SIDE: i32 = 200;
const ID_TITLE: i32 = 201;
const ID_HINT: i32 = 202;
const ID_DROP: i32 = 203;
const ID_COUNT: i32 = 204;
const ID_QUEUE: i32 = 205;
const ID_CAP_FORMATS: i32 = 206;
const ID_CAP_TOOLS: i32 = 207;
const ID_CAP_QUALITY: i32 = 208;
const ID_QUALITY: i32 = 209;
const ID_QUALITY_VALUE: i32 = 210;
const ID_CAP_DEST: i32 = 212;
const ID_DEST: i32 = 213;
const ID_CHANGE: i32 = 214;
const ID_RESET: i32 = 215;
const ID_GO: i32 = 216;
const ID_FOOT: i32 = 217;
const ID_CAP_OPTIONS: i32 = 218;
const ID_EXISTS: i32 = 219;
const ID_CAP_EXISTS: i32 = 220;
const ID_FORMAT0: i32 = 1000;
const ID_TOOL0: i32 = 2000;
/// Le opzioni della voce scelta: il controllo, la sua etichetta e il valore scritto accanto a un cursore.
const OPT_CTL0: i32 = 4000;
const OPT_LABEL0: i32 = 4100;
const OPT_VALUE0: i32 = 4200;
const OPT_MAX: i32 = 12;

const WM_APP_RESULT: u32 = WM_APP + 10;
const WM_APP_DONE: u32 = WM_APP + 11;
const WM_APP_ASK: u32 = WM_APP + 12;

const TITLE: &str = "Conversioni di Cedro";
const POWERED_BY: &str = "Powered by Luca \"Realhcok\" A.";
const NEXT_TO_ORIGINAL: &str = "Accanto all'originale";

// Colori del sito del Comune di Bondeno (COLORREF = 0x00BBGGRR).
const PRIMARY: COLORREF = COLORREF(0x008E5E00); // #005E8E
const HOVER: COLORREF = COLORREF(0x00CC6600); // #0066CC
const TEXT: COLORREF = COLORREF(0x00191919);
const GREY: COLORREF = COLORREF(0x00645A45); // #455A64
const WHITE: COLORREF = COLORREF(0x00FFFFFF);
const SIDE: COLORREF = COLORREF(0x00F9F6F3); // #f3f6f9
const DROP_BG: COLORREF = COLORREF(0x00F8F0E3); // #e3f0f8
const LINE: COLORREF = COLORREF(0x00E5DED6); // #d6dee5
const OK: COLORREF = COLORREF(0x00467A1D); // #1d7a46
const FAIL: COLORREF = COLORREF(0x001E26B3); // #b3261e

static WINDOW: AtomicIsize = AtomicIsize::new(0);
static BRUSH_PRIMARY: AtomicIsize = AtomicIsize::new(0);
static BRUSH_WHITE: AtomicIsize = AtomicIsize::new(0);
static BRUSH_SIDE: AtomicIsize = AtomicIsize::new(0);
static BRUSH_DROP: AtomicIsize = AtomicIsize::new(0);
static FONT: AtomicIsize = AtomicIsize::new(0);
static FONT_BOLD: AtomicIsize = AtomicIsize::new(0);
static FONT_TITLE: AtomicIsize = AtomicIsize::new(0);
static FONT_SMALL: AtomicIsize = AtomicIsize::new(0);
static DPI: AtomicIsize = AtomicIsize::new(96);

#[derive(Clone)]
struct Entry {
    path: PathBuf,
    status: String,
    /// 0 in attesa, 1 fatto, 2 errore.
    state: u8,
}

struct State {
    files: Vec<Entry>,
    category: usize,
    action: String,
    dest: Option<PathBuf>,
    running: bool,
    /// I valori delle opzioni scelte (vedi `conversioni::option_defs`).
    values: Values,
}

static STATE: Mutex<State> = Mutex::new(State { files: Vec::new(), category: 0, action: String::new(), dest: None, running: false, values: Values::new() });
/// Risultati che il thread di conversione lascia alla finestra.
/// La domanda «il file esiste già» che il thread di conversione fa alla finestra, e dove aspetta la risposta.
static ASK: Mutex<Option<(PathBuf, std::sync::mpsc::Sender<ExistsChoice>)>> = Mutex::new(None);
static RESULTS: Mutex<Vec<(PathBuf, std::result::Result<String, String>)>> = Mutex::new(Vec::new());

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

fn current_files(s: &State) -> Vec<Entry> {
    let kind = CATEGORIES[s.category].kind;
    // La trascrizione prende anche l'audio dei video e i vocali (.opus, .amr…) che le altre voci non leggono.
    let transcribing = kind == Kind::Audio && trascrizione::CHOICES.lock().map(|c| c.on).unwrap_or(false);
    s.files.iter().filter(|e| if transcribing { trascrizione::takes(&e.path) } else { kind_of(&e.path) == kind }).cloned().collect()
}

fn brush(slot: &AtomicIsize, color: COLORREF) -> HBRUSH {
    let mut raw = slot.load(Ordering::Relaxed);
    if raw == 0 {
        raw = unsafe { CreateSolidBrush(color) }.0 as isize;
        slot.store(raw, Ordering::Relaxed);
    }
    HBRUSH(raw as *mut _)
}

fn font(slot: &AtomicIsize) -> HFONT {
    HFONT(slot.load(Ordering::Relaxed) as *mut _)
}

fn dpi() -> u32 {
    DPI.load(Ordering::Relaxed) as u32
}

fn px(value: i32) -> i32 {
    value * dpi() as i32 / 96
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// Apre il pannello, oppure porta davanti quello già aperto.
pub fn open() {
    unsafe {
        let existing = HWND(WINDOW.load(Ordering::Relaxed) as *mut _);
        if !existing.0.is_null() && IsWindow(Some(existing)).as_bool() {
            let _ = ShowWindow(existing, SW_RESTORE);
            let _ = SetForegroundWindow(existing);
            return;
        }
        if let Err(e) = create() {
            cedro::append_log(&format!("conversioni: {e}"));
        }
    }
}

unsafe fn make_font(size: i32, weight: i32) -> isize {
    CreateFontW(
        -px(size), 0, 0, 0, weight, 0, 0, 0,
        DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY,
        0, crate::winfont::face(),
    )
    .0 as isize
}

unsafe fn create() -> Result<()> {
    let icc = INITCOMMONCONTROLSEX { dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32, dwICC: ICC_BAR_CLASSES };
    let _ = InitCommonControlsEx(&icc);
    let instance: HINSTANCE = GetModuleHandleW(None)?.into();
    let class = w!("CedroConversioni");
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(proc),
        hInstance: instance,
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        hbrBackground: brush(&BRUSH_WHITE, WHITE),
        lpszClassName: class,
        ..Default::default()
    };
    RegisterClassExW(&wc);
    DPI.store(GetDpiForSystem() as isize, Ordering::Relaxed);
    FONT.store(make_font(15, 400), Ordering::Relaxed);
    FONT_BOLD.store(make_font(17, 700), Ordering::Relaxed);
    FONT_TITLE.store(make_font(24, 700), Ordering::Relaxed);
    FONT_SMALL.store(make_font(13, 700), Ordering::Relaxed);

    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
    let mut rect = RECT { left: 0, top: 0, right: px(900), bottom: px(636) };
    let _ = AdjustWindowRect(&mut rect, style, false);
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    let x = ((GetSystemMetrics(SM_CXSCREEN) - width) / 2).max(0);
    let y = ((GetSystemMetrics(SM_CYSCREEN) - height) / 3).max(0);
    {
        let mut s = state();
        s.running = false;
        s.category = 0;
        s.action.clear();
        s.values = Values::new();
    }
    let hwnd = CreateWindowExW(
        WS_EX_ACCEPTFILES,
        class,
        &HSTRING::from(TITLE),
        style | WS_VISIBLE,
        x,
        y,
        width,
        height,
        None,
        None,
        Some(instance),
        None,
    )?;
    WINDOW.store(hwnd.0 as isize, Ordering::Relaxed);
    DragAcceptFiles(hwnd, true);
    build(hwnd, instance);
    SendMessageW(item(hwnd, ID_SIDE), LB_SETCURSEL, Some(WPARAM(0)), Some(LPARAM(0)));
    render_category(hwnd);
    let _ = SetForegroundWindow(hwnd);
    Ok(())
}

unsafe fn child(parent: HWND, instance: HINSTANCE, class: PCWSTR, text: &str, style: u32, id: i32, r: (i32, i32, i32, i32), f: HFONT) -> HWND {
    let text = HSTRING::from(text);
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        class,
        &text,
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(style),
        px(r.0),
        px(r.1),
        px(r.2),
        px(r.3),
        Some(parent),
        Some(HMENU(id as isize as *mut _)),
        Some(instance),
        None,
    )
    .unwrap_or_default();
    SendMessageW(hwnd, WM_SETFONT, Some(WPARAM(f.0 as usize)), Some(LPARAM(1)));
    hwnd
}

unsafe fn build(hwnd: HWND, instance: HINSTANCE) {
    let (f, fb, ft, fs) = (font(&FONT), font(&FONT_BOLD), font(&FONT_TITLE), font(&FONT_SMALL));
    let s = w!("STATIC");
    let b = w!("BUTTON");
    let lb = w!("LISTBOX");
    let tab = WS_TABSTOP.0;
    // SS_CENTERIMAGE: il titolo sta al centro della barra, in altezza.
    child(hwnd, instance, s, "   Conversioni di Cedro", 0x200, ID_HEADER, (0, 0, 900, 40), fb);

    let side = child(
        hwnd, instance, lb, "",
        tab | LBS_NOTIFY as u32 | LBS_OWNERDRAWFIXED as u32 | LBS_NOINTEGRALHEIGHT as u32,
        ID_SIDE, (0, 40, 190, 600), f,
    );
    for cat in CATEGORIES {
        SendMessageW(side, LB_ADDSTRING, Some(WPARAM(0)), Some(LPARAM(HSTRING::from(cat.name).as_ptr() as isize)));
    }
    SendMessageW(side, LB_SETITEMHEIGHT, Some(WPARAM(0)), Some(LPARAM(px(42) as isize)));

    child(hwnd, instance, s, "", 0, ID_TITLE, (210, 44, 670, 32), ft);
    child(hwnd, instance, s, "", 0, ID_HINT, (210, 78, 670, 34), f);
    child(hwnd, instance, b, "Trascina qui i file o clicca per sceglierli", tab | BS_OWNERDRAW as u32, ID_DROP, (210, 114, 670, 40), fb);
    child(hwnd, instance, s, "", 0, ID_COUNT, (210, 160, 670, 16), fs);
    child(
        hwnd, instance, lb, "",
        tab | LBS_OWNERDRAWFIXED as u32 | LBS_NOINTEGRALHEIGHT as u32 | WS_VSCROLL.0 | WS_BORDER.0,
        ID_QUEUE, (210, 178, 670, 72), f,
    );
    SendMessageW(item(hwnd, ID_QUEUE), LB_SETITEMHEIGHT, Some(WPARAM(0)), Some(LPARAM(px(24) as isize)));
    child(hwnd, instance, s, "CONVERTI IN", 0, ID_CAP_FORMATS, (210, 256, 670, 16), fs);
    child(hwnd, instance, s, "STRUMENTI", 0, ID_CAP_TOOLS, (210, 310, 670, 16), fs);
    child(hwnd, instance, s, "OPZIONI", 0, ID_CAP_OPTIONS, (210, 366, 670, 16), fs);
    child(hwnd, instance, s, "QUALITÀ", 0, ID_CAP_QUALITY, (210, 490, 70, 18), fs);
    let bar = child(hwnd, instance, TRACKBAR_CLASSW, "", tab | TBS_HORZ as u32 | TBS_AUTOTICKS as u32, ID_QUALITY, (280, 482, 520, 30), f);
    SendMessageW(bar, TBM_SETRANGE, Some(WPARAM(1)), Some(LPARAM((40 | (95 << 16)) as isize)));
    SendMessageW(bar, TBM_SETTICFREQ, Some(WPARAM(11)), Some(LPARAM(0)));
    SendMessageW(bar, TBM_SETPOS, Some(WPARAM(1)), Some(LPARAM(read_settings().quality as isize)));
    child(hwnd, instance, s, "", 0, ID_QUALITY_VALUE, (810, 490, 50, 18), f);
    child(hwnd, instance, s, "DOVE SALVARE", 0, ID_CAP_DEST, (210, 520, 300, 16), fs);
    child(hwnd, instance, s, NEXT_TO_ORIGINAL, 0x200 | 0x8000 /* SS_CENTERIMAGE | SS_PATHELLIPSIS */, ID_DEST, (210, 538, 330, 28), f);
    child(hwnd, instance, b, "Cambia…", tab, ID_CHANGE, (548, 538, 80, 28), f);
    child(hwnd, instance, b, "Accanto all'originale", tab, ID_RESET, (634, 538, 150, 28), f);
    child(hwnd, instance, s, "Se il nome esiste già", 0x200, ID_CAP_EXISTS, (210, 572, 160, 26), f);
    let exists = child(hwnd, instance, w!("COMBOBOX"), "", tab | CBS_DROPDOWNLIST as u32, ID_EXISTS, (372, 572, 220, 200), f);
    for (label, _) in EXISTS_CHOICES {
        SendMessageW(exists, CB_ADDSTRING, Some(WPARAM(0)), Some(LPARAM(HSTRING::from(*label).as_ptr() as isize)));
    }
    SendMessageW(exists, CB_SETCURSEL, Some(WPARAM(0)), Some(LPARAM(0)));
    child(hwnd, instance, s, "Tutto sul tuo PC. L'originale non si tocca.", 0, ID_FOOT, (210, 600, 420, 18), f);
    child(hwnd, instance, s, POWERED_BY, 0, ID_POWERED, (210, 616, 420, 18), f);
    child(hwnd, instance, b, "Converti", tab | BS_OWNERDRAW as u32, ID_GO, (640, 590, 240, 38), fb);
    show_quality(hwnd);
    trascrizione::build(hwnd, instance);
}

unsafe fn item(hwnd: HWND, id: i32) -> HWND {
    GetDlgItem(Some(hwnd), id).unwrap_or_default()
}

unsafe fn set_text(hwnd: HWND, id: i32, text: &str) {
    let _ = SetWindowTextW(item(hwnd, id), &HSTRING::from(text));
}

unsafe fn quality(hwnd: HWND) -> u8 {
    (SendMessageW(item(hwnd, ID_QUALITY), WM_USER, Some(WPARAM(0)), Some(LPARAM(0))).0 as u8).clamp(40, 95)
}

unsafe fn show_quality(hwnd: HWND) {
    set_text(hwnd, ID_QUALITY_VALUE, &quality(hwnd).to_string());
}

/// Ricrea i pulsanti dei formati e degli strumenti e svuota o riempie la coda della categoria scelta.
unsafe fn render_category(hwnd: HWND) {
    let instance: HINSTANCE = GetModuleHandleW(None).unwrap_or_default().into();
    let cat = &CATEGORIES[state().category];
    set_text(hwnd, ID_TITLE, cat.name);
    for base in [ID_FORMAT0, ID_TOOL0] {
        for i in 0..24 {
            if let Ok(old) = GetDlgItem(Some(hwnd), base + i) {
                let _ = DestroyWindow(old);
            }
        }
    }
    let formats = conversioni::formats(cat.kind);
    let tools = conversioni::tools(cat.kind);
    let shift = trascrizione::shift();
    chips(hwnd, instance, formats, ID_FORMAT0, 274 + shift);
    chips(hwnd, instance, tools, ID_TOOL0, 328 + shift);
    // Audio ha le due schede in alto: la coda e i formati scendono per far loro posto.
    place(hwnd, ID_DROP, (210, 114 + shift, 670, 40));
    place(hwnd, ID_COUNT, (210, 160 + shift, 670, 16));
    place(hwnd, ID_QUEUE, (210, 178 + shift, 670, 72));
    place(hwnd, ID_CAP_FORMATS, (210, 256 + shift, 670, 16));
    place(hwnd, ID_CAP_TOOLS, (210, 310 + shift, 670, 16));
    // Senza strumenti le opzioni salgono al loro posto, senza lasciare un buco.
    place(hwnd, ID_CAP_OPTIONS, (210, options_caption_y(), 670, 16));
    state().action = formats.first().map(|s| s.id.to_string()).unwrap_or_default();
    refresh_options(hwnd);
    render_queue(hwnd);
}

fn defs_now() -> Vec<&'static OptionDef> {
    let s = state();
    conversioni::option_defs(CATEGORIES[s.category].kind, &s.action)
}

/// Ridisegna la zona Opzioni per la voce scelta: interruttori, elenchi, cursori, password.
/// I valori già scelti restano se la nuova voce li offre ancora.
unsafe fn refresh_options(hwnd: HWND) {
    let instance: HINSTANCE = GetModuleHandleW(None).unwrap_or_default().into();
    for i in 0..OPT_MAX {
        for base in [OPT_CTL0, OPT_LABEL0, OPT_VALUE0] {
            if let Ok(old) = GetDlgItem(Some(hwnd), base + i) {
                let _ = DestroyWindow(old);
            }
        }
    }
    show_convert(hwnd);
    if let Some(end) = trascrizione::layout(hwnd) {
        layout_bottom(hwnd, end);
        let _ = InvalidateRect(Some(hwnd), None, true);
        return;
    }
    let defs = defs_now();
    let _ = ShowWindow(item(hwnd, ID_CAP_OPTIONS), if defs.is_empty() { SW_HIDE } else { SW_SHOW });
    let (f, values) = (font(&FONT), state().values.clone());
    for (i, def) in defs.iter().enumerate().take(OPT_MAX as usize) {
        let (col, row) = ((i % 2) as i32, (i / 2) as i32);
        let (x, y) = (210 + col * 340, options_caption_y() + 18 + row * 32);
        let id = i as i32;
        let tab = WS_TABSTOP.0;
        match &def.control {
            Control::Switch => {
                let b = child(hwnd, instance, w!("BUTTON"), def.title, tab | BS_AUTOCHECKBOX as u32, OPT_CTL0 + id, (x, y, 330, 26), f);
                let on = if values.get(def) != 0 { BST_CHECKED } else { BST_UNCHECKED };
                SendMessageW(b, BM_SETCHECK, Some(WPARAM(on.0 as usize)), Some(LPARAM(0)));
            }
            Control::Choice(choices) => {
                child(hwnd, instance, w!("STATIC"), def.title, 0x200, OPT_LABEL0 + id, (x, y, 150, 26), f);
                let cb = child(hwnd, instance, w!("COMBOBOX"), "", tab | CBS_DROPDOWNLIST as u32 | WS_VSCROLL.0, OPT_CTL0 + id, (x + 150, y, 180, 220), f);
                for c in choices.iter() {
                    SendMessageW(cb, CB_ADDSTRING, Some(WPARAM(0)), Some(LPARAM(HSTRING::from(c.label).as_ptr() as isize)));
                }
                let at = choices.iter().position(|c| c.value == values.get(def)).unwrap_or(0);
                SendMessageW(cb, CB_SETCURSEL, Some(WPARAM(at)), Some(LPARAM(0)));
            }
            Control::Range { min, max, unit } => {
                child(hwnd, instance, w!("STATIC"), def.title, 0x200, OPT_LABEL0 + id, (x, y, 150, 26), f);
                let bar = child(hwnd, instance, TRACKBAR_CLASSW, "", tab | TBS_HORZ as u32 | TBS_NOTICKS as u32, OPT_CTL0 + id, (x + 146, y, 130, 28), f);
                SendMessageW(bar, TBM_SETRANGE, Some(WPARAM(1)), Some(LPARAM((*min as isize) | ((*max as isize) << 16))));
                SendMessageW(bar, TBM_SETPOS, Some(WPARAM(1)), Some(LPARAM(values.get(def) as isize)));
                child(hwnd, instance, w!("STATIC"), &format!("{}{unit}", values.get(def)), 0x200, OPT_VALUE0 + id, (x + 280, y, 50, 26), f);
            }
            Control::Text { hint } => {
                child(hwnd, instance, w!("STATIC"), def.title, 0x200, OPT_LABEL0 + id, (x, y, 150, 26), f);
                let e = child(hwnd, instance, w!("EDIT"), "", tab | ES_AUTOHSCROLL as u32 | WS_BORDER.0, OPT_CTL0 + id, (x + 150, y, 180, 24), f);
                let _ = SetWindowTextW(e, &HSTRING::from(values.text(def)));
                // Il suggerimento grigio nel campo vuoto (serve il controllo comune recente: se manca, non si vede e basta).
                SendMessageW(e, 0x1501 /* EM_SETCUEBANNER */, Some(WPARAM(1)), Some(LPARAM(HSTRING::from(*hint).as_ptr() as isize)));
            }
            Control::Password => {
                child(hwnd, instance, w!("STATIC"), def.title, 0x200, OPT_LABEL0 + id, (x, y, 150, 26), f);
                let e = child(hwnd, instance, w!("EDIT"), "", tab | ES_PASSWORD as u32 | ES_AUTOHSCROLL as u32 | WS_BORDER.0, OPT_CTL0 + id, (x + 150, y, 180, 24), f);
                let _ = SetWindowTextW(e, &HSTRING::from(values.text(def)));
            }
        }
    }
    let rows = (defs.len().min(OPT_MAX as usize) as i32 + 1) / 2;
    let end = if defs.is_empty() { options_caption_y() } else { options_caption_y() + 18 + rows * 32 };
    layout_bottom(hwnd, end + 10);
    let _ = InvalidateRect(Some(hwnd), None, true);
}

fn options_caption_y() -> i32 {
    let s = state();
    let y = if conversioni::tools(CATEGORIES[s.category].kind).is_empty() { 314 } else { 368 };
    drop(s);
    y + trascrizione::shift()
}

/// Mostra i pezzi di «Cambia formato» (formati, strumenti, qualità) o li nasconde se si vede la trascrizione,
/// e mette la descrizione giusta sotto il titolo.
unsafe fn show_convert(hwnd: HWND) {
    let on = !trascrizione::active();
    let kind = CATEGORIES[state().category].kind;
    let cmd = |show: bool| if show { SW_SHOW } else { SW_HIDE };
    let _ = ShowWindow(item(hwnd, ID_CAP_FORMATS), cmd(on));
    let _ = ShowWindow(item(hwnd, ID_CAP_TOOLS), cmd(on && !conversioni::tools(kind).is_empty()));
    if !on {
        let _ = ShowWindow(item(hwnd, ID_CAP_OPTIONS), SW_HIDE);
    }
    for base in [ID_FORMAT0, ID_TOOL0] {
        for i in 0..24 {
            if let Ok(c) = GetDlgItem(Some(hwnd), base + i) {
                let _ = ShowWindow(c, cmd(on));
            }
        }
    }
    for id in [ID_CAP_QUALITY, ID_QUALITY, ID_QUALITY_VALUE] {
        let _ = ShowWindow(item(hwnd, id), cmd(on));
    }
    let cat = &CATEGORIES[state().category];
    let hint = if on { format!("{} Accetta: {}.", cat.hint, cat.accepts) } else { trascrizione::HINT.to_string() };
    set_text(hwnd, ID_HINT, &hint);
}

unsafe fn place(hwnd: HWND, id: i32, r: (i32, i32, i32, i32)) {
    let _ = SetWindowPos(item(hwnd, id), None, px(r.0), px(r.1), px(r.2), px(r.3), SWP_NOZORDER | SWP_NOACTIVATE);
}

/// Mette qualità, destinazione e piede sotto le opzioni e adatta l'altezza della finestra,
/// così nessuna riga resta schiacciata qualunque sia il numero di opzioni.
unsafe fn layout_bottom(hwnd: HWND, top: i32) {
    place(hwnd, ID_CAP_QUALITY, (210, top + 8, 70, 18));
    place(hwnd, ID_QUALITY, (280, top, 520, 30));
    place(hwnd, ID_QUALITY_VALUE, (810, top + 8, 50, 18));
    // La trascrizione non ha la qualità: la destinazione sale al suo posto.
    let top = if trascrizione::active() { top - 42 } else { top };
    place(hwnd, ID_CAP_DEST, (210, top + 42, 300, 16));
    place(hwnd, ID_DEST, (210, top + 60, 330, 28));
    place(hwnd, ID_CHANGE, (548, top + 60, 80, 28));
    place(hwnd, ID_RESET, (634, top + 60, 150, 28));
    place(hwnd, ID_CAP_EXISTS, (210, top + 96, 160, 26));
    place(hwnd, ID_EXISTS, (372, top + 96, 220, 200));
    place(hwnd, ID_GO, (640, top + 140, 240, 38));
    place(hwnd, ID_FOOT, (210, top + 140, 420, 20));
    place(hwnd, ID_POWERED, (210, top + 160, 420, 20));
    let height = top + 196;
    place(hwnd, ID_SIDE, (0, 40, 190, height - 40));
    let style = WINDOW_STYLE(GetWindowLongW(hwnd, GWL_STYLE) as u32);
    let mut rect = RECT { left: 0, top: 0, right: px(900), bottom: px(height) };
    let _ = AdjustWindowRect(&mut rect, style, false);
    let _ = SetWindowPos(hwnd, None, 0, 0, rect.right - rect.left, rect.bottom - rect.top, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
    keep_on_screen(hwnd);
}

/// Se la finestra, cresciuta, esce in fondo dall'area di lavoro (sotto la barra delle applicazioni), sale quanto basta.
unsafe fn keep_on_screen(hwnd: HWND) {
    let mut win = RECT::default();
    let _ = GetWindowRect(hwnd, &mut win);
    let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    if !GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut info).as_bool() {
        return;
    }
    let work = info.rcWork;
    if win.bottom > work.bottom {
        let top = (win.top - (win.bottom - work.bottom)).max(work.top);
        let _ = SetWindowPos(hwnd, None, win.left, top, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

/// Un controllo delle opzioni è stato toccato: il suo valore passa ai valori scelti.
unsafe fn option_changed(hwnd: HWND, index: usize) {
    let defs = defs_now();
    let Some(def) = defs.get(index).copied() else { return };
    let ctl = item(hwnd, OPT_CTL0 + index as i32);
    let mut values = state().values.clone();
    match &def.control {
        Control::Switch => {
            let on = SendMessageW(ctl, BM_GETCHECK, Some(WPARAM(0)), Some(LPARAM(0))).0 == BST_CHECKED.0 as isize;
            values.set(def, on as u32);
        }
        Control::Choice(choices) => {
            let at = SendMessageW(ctl, CB_GETCURSEL, Some(WPARAM(0)), Some(LPARAM(0))).0;
            if let Some(c) = usize::try_from(at).ok().and_then(|i| choices.get(i)) {
                values.set(def, c.value);
            }
        }
        Control::Range { unit, .. } => {
            let at = SendMessageW(ctl, WM_USER, Some(WPARAM(0)), Some(LPARAM(0))).0 as u32;
            values.set(def, at);
            set_text(hwnd, OPT_VALUE0 + index as i32, &format!("{at}{unit}"));
        }
        Control::Text { .. } | Control::Password => {
            let mut buf = [0u16; 256];
            let n = GetWindowTextW(ctl, &mut buf) as usize;
            values.set_text(def, &String::from_utf16_lossy(&buf[..n]));
        }
    }
    state().values = values;
}

unsafe fn chips(hwnd: HWND, instance: HINSTANCE, items: &[cedro::Slice], base: i32, y: i32) {
    let mut x = 210;
    for (i, slice) in items.iter().enumerate() {
        let width = 28 + 9 * slice.label.chars().count() as i32;
        child(hwnd, instance, w!("BUTTON"), slice.label, WS_TABSTOP.0 | BS_OWNERDRAW as u32, base + i as i32, (x, y, width, 32), font(&FONT));
        x += width + 8;
    }
}

unsafe fn render_queue(hwnd: HWND) {
    let (files, running, action) = {
        let s = state();
        (current_files(&s), s.running, s.action.clone())
    };
    let list = item(hwnd, ID_QUEUE);
    SendMessageW(list, LB_RESETCONTENT, Some(WPARAM(0)), Some(LPARAM(0)));
    for e in &files {
        let name = HSTRING::from(e.path.file_name().and_then(|n| n.to_str()).unwrap_or(""));
        SendMessageW(list, LB_ADDSTRING, Some(WPARAM(0)), Some(LPARAM(name.as_ptr() as isize)));
    }
    set_text(
        hwnd,
        ID_COUNT,
        &match files.len() {
            0 => "CODA VUOTA".to_string(),
            1 => "CODA · 1 FILE".to_string(),
            n => format!("CODA · {n} FILE"),
        },
    );
    let kind = CATEGORIES[state().category].kind;
    let name = conversioni::formats(kind)
        .iter()
        .chain(conversioni::tools(kind).iter())
        .find(|s| s.id == action)
        .map(|s| s.label)
        .unwrap_or("");
    let n = files.len();
    let transcribing = trascrizione::active();
    let label = match (running, n) {
        _ if transcribing => trascrizione::go_label(running, n),
        (true, _) => "Conversione in corso…".to_string(),
        (_, 0) => "Converti".to_string(),
        (_, 1) => format!("Converti 1 file · {name}"),
        _ => format!("Converti {n} file · {name}"),
    };
    set_text(hwnd, ID_GO, &label);
    let _ = EnableWindow(item(hwnd, ID_GO), n > 0 && !running && (transcribing || !action.is_empty()));
    let _ = InvalidateRect(Some(hwnd), None, true);
}

unsafe fn redraw_chips(hwnd: HWND) {
    for base in [ID_FORMAT0, ID_TOOL0] {
        for i in 0..24 {
            if let Ok(c) = GetDlgItem(Some(hwnd), base + i) {
                let _ = InvalidateRect(Some(c), None, false);
            }
        }
    }
}

unsafe fn add_files(hwnd: HWND, paths: Vec<PathBuf>) {
    let mut first: Option<Kind> = None;
    let mut voice = false;
    let transcribing = trascrizione::active();
    {
        let mut s = state();
        for path in paths.into_iter().filter(|p| p.is_file()) {
            let only = cedro::trascrizione::only_transcribable(&path);
            if (kind_of(&path) == Kind::Other && !only) || s.files.iter().any(|e| e.path == path) {
                continue;
            }
            // Un vocale .opus o .amr va solo nella trascrizione; un video, con la trascrizione aperta, resta lì.
            if first.is_none() {
                voice = only || (transcribing && trascrizione::takes(&path));
            }
            first.get_or_insert(if only { Kind::Audio } else { kind_of(&path) });
            s.files.push(Entry { path, status: "In attesa".into(), state: 0 });
        }
    }
    if voice {
        trascrizione::set_active(true);
        if !transcribing {
            if let Some(i) = CATEGORIES.iter().position(|c| c.kind == Kind::Audio) {
                state().category = i;
                SendMessageW(item(hwnd, ID_SIDE), LB_SETCURSEL, Some(WPARAM(i)), Some(LPARAM(0)));
                render_category(hwnd);
                let _ = InvalidateRect(Some(item(hwnd, ID_SIDE)), None, true);
                return;
            }
        }
        render_queue(hwnd);
        return;
    }
    // Se i file appena aggiunti sono di un'altra categoria, si passa a quella: altrimenti restano invisibili.
    if let Some(kind) = first {
        if CATEGORIES[state().category].kind != kind {
            if let Some(i) = CATEGORIES.iter().position(|c| c.kind == kind) {
                state().category = i;
                SendMessageW(item(hwnd, ID_SIDE), LB_SETCURSEL, Some(WPARAM(i)), Some(LPARAM(0)));
                render_category(hwnd);
                let _ = InvalidateRect(Some(item(hwnd, ID_SIDE)), None, true);
                return;
            }
        }
    }
    render_queue(hwnd);
}

unsafe fn pick(hwnd: HWND, folder: bool) -> Vec<PathBuf> {
    let run = || -> Result<Vec<PathBuf>> {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
        let mut options = dialog.GetOptions()?;
        options |= FOS_FORCEFILESYSTEM;
        options |= if folder { FOS_PICKFOLDERS } else { FOS_ALLOWMULTISELECT };
        dialog.SetOptions(options)?;
        dialog.SetTitle(if folder { w!("Cartella di destinazione") } else { w!("Scegli i file da convertire") })?;
        dialog.Show(Some(hwnd))?;
        let results = dialog.GetResults()?;
        let mut out = Vec::new();
        for i in 0..results.GetCount()? {
            let name = results.GetItemAt(i)?.GetDisplayName(SIGDN_FILESYSPATH)?;
            out.push(PathBuf::from(name.to_string()?));
            CoTaskMemFree(Some(name.0 as *const _));
        }
        Ok(out)
    };
    run().unwrap_or_default()
}

unsafe fn start(hwnd: HWND) {
    if trascrizione::active() {
        let (files, dest, opts) = {
            let mut s = state();
            if s.running {
                return;
            }
            let files: Vec<PathBuf> = current_files(&s).into_iter().map(|e| e.path).collect();
            if files.is_empty() {
                return;
            }
            for e in s.files.iter_mut().filter(|e| files.contains(&e.path)) {
                e.status = "In attesa".into();
                e.state = 0;
            }
            s.running = true;
            (files, s.dest.clone(), conversioni::options_from(&s.values))
        };
        render_queue(hwnd);
        trascrizione::start(hwnd, files, dest, ask_exists(hwnd, opts));
        return;
    }
    let checked = {
        let s = state();
        conversioni::validate(&s.values, CATEGORIES[s.category].kind, &s.action)
    };
    if let Err(e) = checked {
        MessageBoxW(Some(hwnd), &HSTRING::from(e), w!("Cedro"), MB_OK | MB_ICONWARNING);
        return;
    }
    let (files, action, dest, opts) = {
        let mut s = state();
        if s.running {
            return;
        }
        let files: Vec<PathBuf> = current_files(&s).into_iter().map(|e| e.path).collect();
        if files.is_empty() {
            return;
        }
        for e in s.files.iter_mut().filter(|e| files.contains(&e.path)) {
            e.status = "In attesa".into();
            e.state = 0;
        }
        s.running = true;
        (files, s.action.clone(), s.dest.clone(), conversioni::options_from(&s.values))
    };
    let q = quality(hwnd);
    render_queue(hwnd);
    let raw = hwnd.0 as isize;
    let opts = ask_exists(hwnd, opts);
    let label = conversioni::action_label(CATEGORIES[state().category].kind, &action).to_string();
    std::thread::spawn(move || {
        let total = files.len();
        let name = |p: &std::path::Path| p.file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string();
        let step_label = |i: usize, p: &std::path::Path| {
            if total == 1 { format!("{} → {label}", name(p)) } else { format!("{} di {total} · {}", i + 1, name(p)) }
        };
        if let Some(first) = files.first() {
            crate::progress_win::start(total, &step_label(0, first));
        }
        let mut failed = None;
        for (i, path) in files.into_iter().enumerate() {
            crate::progress_win::step(i, &step_label(i, &path));
            let result = conversioni::convert_one_out(&path, &action, q, dest.as_deref(), &opts).map(|(out, message)| {
                crate::progress_win::produced(&out);
                message
            });
            if let Err(e) = &result {
                failed.get_or_insert_with(|| e.clone());
            }
            RESULTS.lock().unwrap_or_else(|e| e.into_inner()).push((path, result));
            unsafe {
                let _ = PostMessageW(Some(HWND(raw as *mut _)), WM_APP_RESULT, WPARAM(0), LPARAM(0));
            }
        }
        match failed {
            Some(e) => crate::progress_win::finish(false, &format!("Errore · {e}")),
            None => crate::progress_win::finish(true, &format!("✓ Fatto · {total} file in {label}")),
        }
        unsafe {
            let _ = PostMessageW(Some(HWND(raw as *mut _)), WM_APP_DONE, WPARAM(0), LPARAM(0));
        }
    });
}

/// Con «Chiedi» il thread di lavoro domanda alla finestra cosa fare di un file che esiste già, e aspetta.
fn ask_exists(hwnd: HWND, mut opts: cedro::Options) -> cedro::Options {
    let raw = hwnd.0 as isize;
    if opts.exists == Exists::Ask {
        opts.on_exists = Some(ExistsAsk::new(move |path| {
            let (tx, rx) = std::sync::mpsc::channel();
            *ASK.lock().unwrap_or_else(|e| e.into_inner()) = Some((path.to_path_buf(), tx));
            unsafe {
                let _ = PostMessageW(Some(HWND(raw as *mut _)), WM_APP_ASK, WPARAM(0), LPARAM(0));
            }
            rx.recv().unwrap_or(ExistsChoice::KeepBoth)
        }));
    }
    opts
}

unsafe fn take_results(hwnd: HWND) {
    let results: Vec<_> = std::mem::take(&mut *RESULTS.lock().unwrap_or_else(|e| e.into_inner()));
    if results.is_empty() {
        return;
    }
    {
        let mut s = state();
        for (path, result) in results {
            if let Some(e) = s.files.iter_mut().find(|e| e.path == path) {
                match result {
                    Ok(msg) => (e.status, e.state) = (format!("Fatto · {msg}"), 1),
                    Err(msg) => (e.status, e.state) = (format!("Errore · {msg}"), 2),
                }
            }
        }
    }
    let _ = InvalidateRect(Some(item(hwnd, ID_QUEUE)), None, true);
}

unsafe fn draw_text(hdc: HDC, text: &str, rc: &mut RECT, format: DRAW_TEXT_FORMAT) {
    let mut w = wide(text);
    DrawTextW(hdc, &mut w, rc, format);
}

unsafe fn draw_item(hwnd: HWND, di: &DRAWITEMSTRUCT) {
    let hdc = di.hDC;
    let mut rc = di.rcItem;
    let id = di.CtlID as i32;
    let selected = di.itemState.0 & ODS_SELECTED.0 != 0;
    SetBkMode(hdc, TRANSPARENT);
    if trascrizione::draw(di) {
        return;
    }
    match id {
        ID_SIDE => {
            if di.itemID == u32::MAX {
                return;
            }
            let on = state().category == di.itemID as usize;
            FillRect(hdc, &rc, brush(if on { &BRUSH_PRIMARY } else { &BRUSH_SIDE }, if on { PRIMARY } else { SIDE }));
            SelectObject(hdc, HGDIOBJ(font(if on { &FONT_BOLD } else { &FONT }).0));
            SetTextColor(hdc, if on { WHITE } else { TEXT });
            rc.left += px(16);
            draw_text(hdc, CATEGORIES[di.itemID as usize].name, &mut rc, DT_SINGLELINE | DT_VCENTER | DT_LEFT);
        }
        ID_QUEUE => {
            if di.itemID == u32::MAX {
                return;
            }
            FillRect(hdc, &rc, brush(&BRUSH_WHITE, WHITE));
            let entry = current_files(&state()).get(di.itemID as usize).cloned();
            let Some(entry) = entry else { return };
            SelectObject(hdc, HGDIOBJ(font(&FONT_SMALL).0));
            SetTextColor(hdc, GREY);
            let mut ext = RECT { left: rc.left + px(10), ..rc };
            draw_text(hdc, &extension(&entry.path).to_uppercase(), &mut ext, DT_SINGLELINE | DT_VCENTER | DT_LEFT);
            SelectObject(hdc, HGDIOBJ(font(&FONT).0));
            SetTextColor(hdc, match entry.state { 1 => OK, 2 => FAIL, _ => GREY });
            let mut status = RECT { right: rc.right - px(10), left: rc.left + (rc.right - rc.left) / 2, ..rc };
            draw_text(hdc, &entry.status, &mut status, DT_SINGLELINE | DT_VCENTER | DT_RIGHT | DT_END_ELLIPSIS);
            SetTextColor(hdc, TEXT);
            let mut name = RECT { left: rc.left + px(64), right: status.left - px(8), ..rc };
            let file = entry.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            draw_text(hdc, file, &mut name, DT_SINGLELINE | DT_VCENTER | DT_LEFT | DT_PATH_ELLIPSIS);
        }
        ID_DROP => {
            FillRect(hdc, &rc, brush(&BRUSH_DROP, DROP_BG));
            let pen = CreatePen(PS_DASH, 1, PRIMARY);
            let old_pen = SelectObject(hdc, HGDIOBJ(pen.0));
            let old_brush = SelectObject(hdc, GetStockObject(NULL_BRUSH));
            let _ = Rectangle(hdc, rc.left + 1, rc.top + 1, rc.right - 1, rc.bottom - 1);
            SelectObject(hdc, old_brush);
            SelectObject(hdc, old_pen);
            let _ = DeleteObject(HGDIOBJ(pen.0));
            SelectObject(hdc, HGDIOBJ(font(&FONT_BOLD).0));
            SetTextColor(hdc, PRIMARY);
            draw_text(hdc, "Trascina qui i file o clicca per sceglierli", &mut rc, DT_SINGLELINE | DT_VCENTER | DT_CENTER);
        }
        ID_GO => {
            let disabled = di.itemState.0 & ODS_DISABLED.0 != 0;
            let bg = if disabled { LINE } else if selected { HOVER } else { PRIMARY };
            let fill = CreateSolidBrush(bg);
            FillRect(hdc, &rc, fill);
            let _ = DeleteObject(HGDIOBJ(fill.0));
            SelectObject(hdc, HGDIOBJ(font(&FONT_BOLD).0));
            SetTextColor(hdc, if disabled { GREY } else { WHITE });
            let mut text = [0u16; 128];
            let n = GetWindowTextW(di.hwndItem, &mut text) as usize;
            draw_text(hdc, &String::from_utf16_lossy(&text[..n]), &mut rc, DT_SINGLELINE | DT_VCENTER | DT_CENTER | DT_END_ELLIPSIS);
        }
        _ => {
            let action = state().action.clone();
            let kind = CATEGORIES[state().category].kind;
            let (list, index) = if id >= ID_TOOL0 { (conversioni::tools(kind), (id - ID_TOOL0) as usize) } else { (conversioni::formats(kind), (id - ID_FORMAT0) as usize) };
            let Some(slice) = list.get(index) else { return };
            let (label, on) = (slice.label, slice.id == action);
            let (bg, fg) = if on { (PRIMARY, WHITE) } else { (WHITE, TEXT) };
            FillRect(hdc, &rc, brush(if on { &BRUSH_PRIMARY } else { &BRUSH_WHITE }, bg));
            let edge = CreateSolidBrush(if on { PRIMARY } else if selected { HOVER } else { LINE });
            FrameRect(hdc, &rc, edge);
            let _ = DeleteObject(HGDIOBJ(edge.0));
            SelectObject(hdc, HGDIOBJ(font(&FONT).0));
            SetTextColor(hdc, fg);
            draw_text(hdc, label, &mut rc, DT_SINGLELINE | DT_VCENTER | DT_CENTER);
        }
    }
    let _ = hwnd;
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let id = (wparam.0 & 0xffff) as i32;
            let code = ((wparam.0 >> 16) & 0xffff) as u32;
            if trascrizione::command(hwnd, id) {
                return LRESULT(0);
            }
            match id {
                ID_SIDE if code == LBN_SELCHANGE => {
                    let sel = SendMessageW(item(hwnd, ID_SIDE), LB_GETCURSEL, Some(WPARAM(0)), Some(LPARAM(0))).0;
                    if sel >= 0 {
                        state().category = sel as usize;
                        render_category(hwnd);
                        let _ = InvalidateRect(Some(item(hwnd, ID_SIDE)), None, true);
                    }
                }
                ID_DROP => {
                    let paths = pick(hwnd, false);
                    add_files(hwnd, paths);
                }
                ID_CHANGE => {
                    if let Some(path) = pick(hwnd, true).into_iter().next() {
                        set_text(hwnd, ID_DEST, &path.display().to_string());
                        state().dest = Some(path);
                    }
                }
                ID_RESET => {
                    set_text(hwnd, ID_DEST, NEXT_TO_ORIGINAL);
                    state().dest = None;
                }
                ID_GO => start(hwnd),
                ID_EXISTS if code == CBN_SELCHANGE => {
                    let at = SendMessageW(item(hwnd, ID_EXISTS), CB_GETCURSEL, Some(WPARAM(0)), Some(LPARAM(0))).0;
                    if let Some((_, choice)) = usize::try_from(at).ok().and_then(|i| EXISTS_CHOICES.get(i)) {
                        state().values.exists = *choice;
                    }
                }
                x if (OPT_CTL0..OPT_CTL0 + OPT_MAX).contains(&x) => {
                    // Interruttori e password si leggono al clic o a ogni lettera, gli elenchi alla scelta.
                    let notify_ok = matches!(code, BN_CLICKED | CBN_SELCHANGE | EN_CHANGE);
                    if notify_ok {
                        option_changed(hwnd, (x - OPT_CTL0) as usize);
                    }
                }
                x if (ID_FORMAT0..ID_FORMAT0 + 24).contains(&x) || (ID_TOOL0..ID_TOOL0 + 24).contains(&x) => {
                    let kind = CATEGORIES[state().category].kind;
                    let (list, index) = if x >= ID_TOOL0 { (conversioni::tools(kind), (x - ID_TOOL0) as usize) } else { (conversioni::formats(kind), (x - ID_FORMAT0) as usize) };
                    if let Some(slice) = list.get(index) {
                        state().action = slice.id.to_string();
                        refresh_options(hwnd);
                        redraw_chips(hwnd);
                        render_queue(hwnd);
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_DROPFILES => {
            let hdrop = HDROP(wparam.0 as *mut _);
            let count = DragQueryFileW(hdrop, u32::MAX, None);
            let mut paths = Vec::new();
            for i in 0..count {
                let len = DragQueryFileW(hdrop, i, None) as usize;
                let mut buf = vec![0u16; len + 1];
                let got = DragQueryFileW(hdrop, i, Some(&mut buf)) as usize;
                paths.push(PathBuf::from(String::from_utf16_lossy(&buf[..got])));
            }
            DragFinish(hdrop);
            add_files(hwnd, paths);
            LRESULT(0)
        }
        WM_DRAWITEM => {
            draw_item(hwnd, &*(lparam.0 as *const DRAWITEMSTRUCT));
            LRESULT(1)
        }
        WM_CTLCOLORLISTBOX if GetDlgCtrlID(HWND(lparam.0 as *mut _)) == ID_SIDE => LRESULT(brush(&BRUSH_SIDE, SIDE).0 as isize),
        WM_CTLCOLORSTATIC => {
            let ctl = HWND(lparam.0 as *mut _);
            let hdc = HDC(wparam.0 as *mut _);
            let (fg, bg, slot) = match GetDlgCtrlID(ctl) {
                ID_HEADER => (WHITE, PRIMARY, &BRUSH_PRIMARY),
                ID_TITLE => (PRIMARY, WHITE, &BRUSH_WHITE),
                trascrizione::ID_CAP_SAVEAS | trascrizione::ID_CAP_MODEL | trascrizione::ID_CAP_LOG | ID_POWERED | ID_CAP_EXISTS | ID_CAP_OPTIONS | ID_CAP_FORMATS | ID_CAP_TOOLS | ID_CAP_QUALITY | ID_CAP_DEST | ID_COUNT | ID_FOOT => (GREY, WHITE, &BRUSH_WHITE),
                _ => (TEXT, WHITE, &BRUSH_WHITE),
            };
            SetTextColor(hdc, fg);
            SetBkColor(hdc, bg);
            LRESULT(brush(slot, bg).0 as isize)
        }
        WM_HSCROLL => {
            let bar = HWND(lparam.0 as *mut _);
            let id = GetDlgCtrlID(bar);
            if (OPT_CTL0..OPT_CTL0 + OPT_MAX).contains(&id) {
                option_changed(hwnd, (id - OPT_CTL0) as usize);
            } else {
                show_quality(hwnd);
            }
            LRESULT(0)
        }
        trascrizione::WM_APP_LOG => {
            trascrizione::take_log(hwnd);
            LRESULT(0)
        }
        WM_APP_RESULT => {
            take_results(hwnd);
            LRESULT(0)
        }
        WM_APP_ASK => {
            let request = ASK.lock().unwrap_or_else(|e| e.into_inner()).take();
            if let Some((path, tx)) = request {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("il file");
                let text = HSTRING::from(format!("«{name}» esiste già in questa cartella.\n\nSì: lo sostituisco\nNo: tengo entrambi (aggiungo -2)\nAnnulla: salto questo file"));
                let answer = MessageBoxW(Some(hwnd), &text, w!("Cedro"), MB_YESNOCANCEL | MB_ICONQUESTION | MB_SETFOREGROUND);
                let _ = tx.send(match answer {
                    IDYES => ExistsChoice::Replace,
                    IDNO => ExistsChoice::KeepBoth,
                    _ => ExistsChoice::Skip,
                });
            }
            LRESULT(0)
        }
        WM_APP_DONE => {
            take_results(hwnd);
            state().running = false;
            render_queue(hwnd);
            LRESULT(0)
        }
        WM_CLOSE => {
            if state().running {
                MessageBoxW(Some(hwnd), w!("Una conversione è ancora in corso: aspetta che finisca."), w!("Cedro"), MB_OK | MB_ICONINFORMATION);
            } else {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            WINDOW.store(0, Ordering::Relaxed);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// `cedro.exe conversioni`: il pannello da solo, con il suo ciclo di messaggi (utile per provarlo).
pub fn run_standalone() {
    crate::winfont::load();
    let _ = unsafe { windows::Win32::System::Ole::OleInitialize(None) };
    open();
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            // Chiusa la finestra non arriva WM_QUIT: senza questo il processo resterebbe nascosto.
            if WINDOW.load(Ordering::Relaxed) == 0 {
                break;
            }
        }
    }
}
