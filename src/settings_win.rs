//! Finestra «Impostazioni»: a sinistra le sezioni, a destra le opzioni della sezione scelta.
//! Disegnata con i controlli di Windows, nei colori e nel font del Comune di Bondeno.

use cedro::{read_settings, update, write_settings, Dest, Settings};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::Dwm::*,
    Win32::Graphics::Gdi::*,
    Win32::System::Com::CoTaskMemFree,
    Win32::System::LibraryLoader::GetModuleHandleW,
    Win32::UI::Controls::*,
    Win32::UI::HiDpi::GetDpiForSystem,
    Win32::UI::Input::KeyboardAndMouse::{EnableWindow, GetFocus},
    Win32::UI::Shell::*,
    Win32::UI::WindowsAndMessaging::*,
};

const ID_AUTO: i32 = 101;
const ID_NOTIFY: i32 = 102;
const ID_REVEAL: i32 = 103;
const ID_LOG: i32 = 104;
const ID_PROGRESS: i32 = 105;
const ID_SIMPLE: i32 = 196;
const ID_SHIFT: i32 = 111;
const ID_CTRL: i32 = 112;
const ID_ALT: i32 = 113;
const ID_DIALOGS: i32 = 114;
const ID_QUALITY: i32 = 121;
const ID_QUALITY_VALUE: i32 = 122;
const ID_DPI: i32 = 123;
const ID_COMPRESS: i32 = 124;
const ID_DEST_SAME: i32 = 131;
const ID_DEST_SUB: i32 = 132;
const ID_DEST_FIXED: i32 = 133;
const ID_BROWSE: i32 = 134;
const ID_DEST_PATH: i32 = 135;
const ID_EXISTS: i32 = 136;
const ID_LANG: i32 = 141;
const ID_MAXPAGES: i32 = 142;
const ID_CHECK_NOW: i32 = 151;
const ID_UPDATE: i32 = 152;
const ID_UNSKIP: i32 = 153;
const ID_POWERED: i32 = 161;
const ID_DESC: i32 = 170;
const ID_TITLE: i32 = 171;
const ID_STATUS: i32 = 172;
const ID_GPU: i32 = 181;
const ID_AVX: i32 = 182;
const ID_COPY: i32 = 183;
const ID_COPIED: i32 = 184;
const ID_TAG_GPU: i32 = 185;
const ID_TAG_AVX: i32 = 186;
const ID_SEARCH: i32 = 187;
const ID_COUNT: i32 = 188;
const ID_FULL: i32 = 189;
const ID_LOGO: i32 = 197;
const ID_LOCKED: i32 = 198;
const ID_ABOUT_POWERED: i32 = 199;
/// Edizione Bondeno: stemma e «Comune di Bondeno» in cima alle sezioni, e il riquadro nella scheda Informazioni.
const ID_ENTE: i32 = 195;
const ID_ENTE_ABOUT: i32 = 196;
const ID_NAV: i32 = 200;
const ID_LIST: i32 = 220;
/// Le istruzioni del processore, una pillola ciascuna: da ID_CHIP in su.
const ID_CHIP: i32 = 230;

/// Gli interruttori a levetta: disegnati da Cedro, con lo stato tenuto qui.
const TOGGLES: [i32; 12] = [ID_SIMPLE, ID_AUTO, ID_NOTIFY, ID_REVEAL, ID_PROGRESS, ID_SHIFT, ID_CTRL, ID_ALT, ID_DIALOGS, ID_UPDATE, ID_GPU, ID_AVX];
/// I pulsanti disegnati da Cedro (Salva è quello blu).
const PUSHES: [i32; 8] = [ID_LOG, ID_BROWSE, ID_CHECK_NOW, ID_UNSKIP, ID_COPY, ID_FULL, 1 /* IDOK */, 2 /* IDCANCEL */];

fn is_toggle(id: i32) -> bool {
    id != 0 && TOGGLES.contains(&id)
}

// Colori del sito del Comune di Bondeno (COLORREF = 0x00BBGGRR).
const PRIMARY: COLORREF = COLORREF(0x008E5E00); // #005E8E
const HOVER: COLORREF = COLORREF(0x00CC6600); // #0066CC
const TEXT: COLORREF = COLORREF(0x00191919); // #191919
const GREY: COLORREF = COLORREF(0x00645A45); // #455A64
const OK: COLORREF = COLORREF(0x00467A1D); // #1d7a46
const OK_SOFT: COLORREF = COLORREF(0x00EAF3E3); // #E3F3EA
const OFF: COLORREF = COLORREF(0x00A3998A); // #8A99A3
const OFF_SOFT: COLORREF = COLORREF(0x00F3F1EE); // #EEF1F3
const WHITE: COLORREF = COLORREF(0x00FFFFFF);
const SIDE: COLORREF = COLORREF(0x00F7F5F3); // #F3F5F7
const LINE: COLORREF = COLORREF(0x00E5E5E5); // #E5E5E5
const ENTE_SOFT: COLORREF = COLORREF(0x00FBF7F2); // #F2F7FB, il fondo del riquadro dell'edizione Bondeno
const ENTE_LINE: COLORREF = COLORREF(0x00EEE3D4); // #D4E3EE

const SECTIONS: [&str; 8] = ["Generale", "Ruota", "Salvataggio", "Qualità", "OCR", "Aggiornamenti", "Sistema", "Informazioni"];
const DPI: [u32; 4] = [100, 144, 200, 300];
const MAX_PAGES: [u32; 4] = [50, 100, 300, 0];
const LANGS: [&str; 3] = ["auto", "it", "en"];

const WIDTH: i32 = 780;
const HEIGHT: i32 = 600;
const SIDE_W: i32 = 190;
const FOOT_H: i32 = 56;
const MARGIN: i32 = 28;

static WINDOW: AtomicIsize = AtomicIsize::new(0);
/// «Copia informazioni» è stato premuto: la nota accanto diventa verde.
static COPIED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static BRUSH_WHITE: AtomicIsize = AtomicIsize::new(0);
static BRUSH_SIDE: AtomicIsize = AtomicIsize::new(0);
static BRUSH_LINE: AtomicIsize = AtomicIsize::new(0);

struct State {
    sections: Vec<Vec<HWND>>,
    /// Le righe di separazione (y in pixel) di ogni sezione.
    lines: Vec<Vec<i32>>,
    checks: HashMap<i32, bool>,
    current: usize,
    font: HFONT,
    bold: HFONT,
    dest_dir: String,
    dpi: u32,
    /// I riquadri (pixel) disegnati dietro i campi di ogni sezione.
    frames: Vec<Vec<RECT>>,
    caption: HFONT,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State {
        sections: Vec::new(),
        lines: Vec::new(),
        checks: HashMap::new(),
        current: 0,
        font: HFONT::default(),
        bold: HFONT::default(),
        dest_dir: String::new(),
        dpi: 96,
        frames: Vec::new(),
        caption: HFONT::default(),
    });
}

fn brush(slot: &AtomicIsize, color: COLORREF) -> HBRUSH {
    let mut raw = slot.load(Ordering::Relaxed);
    if raw == 0 {
        raw = unsafe { CreateSolidBrush(color) }.0 as isize;
        slot.store(raw, Ordering::Relaxed);
    }
    HBRUSH(raw as *mut _)
}

fn px(value: i32, dpi: u32) -> i32 {
    value * dpi as i32 / 96
}

/// Apre la finestra, oppure porta davanti quella già aperta.
/// `cedro impostazioni`: solo la finestra, senza la ruota nel vassoio; il processo finisce quando la chiudi.
pub fn run_standalone() {
    crate::winfont::load();
    let _ = unsafe { windows::Win32::System::Ole::OleInitialize(None) };
    open();
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if !handle_message(&msg) {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if WINDOW.load(Ordering::Relaxed) == 0 {
                break;
            }
        }
    }
}

pub fn open() {
    unsafe {
        let existing = HWND(WINDOW.load(Ordering::Relaxed) as *mut _);
        if !existing.0.is_null() && IsWindow(Some(existing)).as_bool() {
            let _ = ShowWindow(existing, SW_RESTORE);
            let _ = SetForegroundWindow(existing);
            return;
        }
        if let Err(e) = create() {
            cedro::append_log(&format!("impostazioni: {e}"));
        }
    }
}

/// Dà alla finestra delle impostazioni i messaggi di tastiera (Tab, Invio, Esc).
pub fn handle_message(msg: &MSG) -> bool {
    let hwnd = HWND(WINDOW.load(Ordering::Relaxed) as *mut _);
    if hwnd.0.is_null() {
        return false;
    }
    unsafe {
        // Invio nella ricerca o nell'elenco dei crediti non vale «Salva»: va al campo.
        if msg.message == WM_KEYDOWN && msg.wParam.0 == 0x0D {
            let id = GetDlgCtrlID(GetFocus());
            if id == ID_SEARCH || id == ID_LIST {
                return false;
            }
        }
        IsDialogMessageW(hwnd, msg).as_bool()
    }
}

unsafe fn create() -> Result<()> {
    let icc = INITCOMMONCONTROLSEX { dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32, dwICC: ICC_BAR_CLASSES };
    let _ = InitCommonControlsEx(&icc);
    let instance: HINSTANCE = GetModuleHandleW(None)?.into();
    let class = w!("CedroSettings");
    // L'icona di Cedro (la prima risorsa dell'exe) nella barra del titolo.
    let icon = LoadIconW(Some(instance), PCWSTR(1usize as *const u16)).unwrap_or_default();
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(proc),
        hInstance: instance,
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        hIcon: icon,
        hIconSm: icon,
        hbrBackground: brush(&BRUSH_WHITE, WHITE),
        lpszClassName: class,
        ..Default::default()
    };
    RegisterClassExW(&wc);
    let dpi = GetDpiForSystem();
    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
    let mut rect = RECT { left: 0, top: 0, right: px(WIDTH, dpi), bottom: px(HEIGHT, dpi) };
    let _ = AdjustWindowRect(&mut rect, style, false);
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    let x = (GetSystemMetrics(SM_CXSCREEN) - width) / 2;
    let y = (GetSystemMetrics(SM_CYSCREEN) - height) / 3;
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        class,
        w!("Impostazioni di Cedro"),
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
    // Su Windows 11 la barra del titolo prende il blu di Cedro, con il testo bianco.
    for (attribute, color) in [(DWMWA_CAPTION_COLOR, PRIMARY), (DWMWA_BORDER_COLOR, PRIMARY), (DWMWA_TEXT_COLOR, WHITE)] {
        let _ = DwmSetWindowAttribute(hwnd, attribute, &color as *const COLORREF as *const _, 4);
    }
    build(hwnd, instance, dpi);
    fill(hwnd, &read_settings(), crate::win::autostart_enabled());
    show_section(hwnd, 0);
    SendMessageW(hwnd, DM_SETDEFID, Some(WPARAM(IDOK.0 as usize)), Some(LPARAM(0)));
    let _ = SetForegroundWindow(hwnd);
    Ok(())
}

fn font(height: i32, weight: i32, dpi: u32) -> HFONT {
    font_named(height, weight, dpi, crate::winfont::face())
}

fn font_named(height: i32, weight: i32, dpi: u32, face: PCWSTR) -> HFONT {
    unsafe {
        CreateFontW(
            -px(height, dpi), 0, 0, 0, weight, 0, 0, 0,
            DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY,
            0, face,
        )
    }
}

/// Un controllo figlio; `section` lo lega a una sezione (None: sempre visibile).
unsafe fn child(
    parent: HWND,
    class: PCWSTR,
    text: &str,
    style: u32,
    id: i32,
    r: (i32, i32, i32, i32),
    dpi: u32,
    font: HFONT,
    section: Option<usize>,
) -> HWND {
    let instance: HINSTANCE = GetModuleHandleW(None).unwrap_or_default().into();
    let text = HSTRING::from(text);
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        class,
        &text,
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(style),
        px(r.0, dpi),
        px(r.1, dpi),
        px(r.2, dpi),
        px(r.3, dpi),
        Some(parent),
        Some(HMENU(id as isize as *mut _)),
        Some(instance),
        None,
    )
    .unwrap_or_default();
    SendMessageW(hwnd, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
    if let Some(s) = section {
        STATE.with(|st| st.borrow_mut().sections[s].push(hwnd));
    }
    hwnd
}

/// Quante righe occupa `text` in `width` (unità a 96 dpi) con il carattere dato, e quanto è alta una riga.
unsafe fn measure(text: &str, font: HFONT, width: i32, dpi: u32) -> (i32, i32) {
    let dc = GetDC(None);
    let old = SelectObject(dc, font.into());
    let mut metrics = TEXTMETRICW::default();
    let _ = GetTextMetricsW(dc, &mut metrics);
    let line = metrics.tmHeight.max(1);
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    let mut rc = RECT { left: 0, top: 0, right: px(width, dpi), bottom: 0 };
    DrawTextW(dc, &mut wide, &mut rc, DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX);
    SelectObject(dc, old);
    ReleaseDC(None, dc);
    let lines = ((rc.bottom - rc.top + line - 1) / line).max(1);
    (lines, line * 96 / dpi as i32)
}

/// Dove sta una riga di impostazione una volta disposta.
struct Row {
    /// La y della riga dopo.
    next: i32,
    /// La x del controllo, a destra.
    ctl_x: i32,
    /// La y del centro della riga.
    mid: i32,
}

unsafe fn build(hwnd: HWND, _instance: HINSTANCE, dpi: u32) {
    let normal = font(14, 400, dpi);
    let bold = font(14, 700, dpi);
    let small = font(13, 400, dpi);
    let heading = font(22, 700, dpi);
    STATE.with(|st| {
        let mut st = st.borrow_mut();
        st.sections = vec![Vec::new(); SECTIONS.len()];
        st.lines = vec![Vec::new(); SECTIONS.len()];
        st.frames = vec![Vec::new(); SECTIONS.len()];
        st.font = normal;
        st.bold = bold;
        st.dpi = dpi;
    });
    let s = w!("STATIC");
    let b = w!("BUTTON");
    let tab = WS_TABSTOP.0;
    let owner = tab | BS_OWNERDRAW as u32;
    let combo_style = tab | CBS_DROPDOWNLIST as u32 | WS_VSCROLL.0;
    const SS_NOPREFIX: u32 = 0x80;

    // Nell'edizione Bondeno le sezioni scendono per lasciare posto allo stemma.
    let nav_top = if cedro::edizione::BONDENO {
        child(hwnd, s, "", crate::firma_win::SS_OWNERDRAW, ID_ENTE, (8, 10, SIDE_W - 16, 52), dpi, bold, None);
        72
    } else {
        12
    };
    for (i, name) in SECTIONS.iter().enumerate() {
        child(hwnd, b, name, owner, ID_NAV + i as i32, (8, nav_top + 38 * i as i32, SIDE_W - 16, 36), dpi, normal, None);
    }

    let x = SIDE_W + MARGIN; // sinistra del contenuto
    let cw = WIDTH - x - MARGIN; // larghezza del contenuto
    let top = 64;
    let heading_of = |sec: usize| {
        child(hwnd, s, SECTIONS[sec], SS_NOPREFIX, ID_TITLE, (x, 20, cw, 34), dpi, heading, Some(sec));
    };
    // Una riga: titolo in grassetto e descrizione a sinistra, controllo di `ctl_w` a destra.
    let row = |sec: usize, y: i32, title_text: &str, desc: &str, ctl_w: i32, min_h: i32| -> Row {
        let text_w = if ctl_w > 0 { cw - ctl_w - 24 } else { cw };
        let (lines, line_h) = measure(desc, small, text_w, dpi);
        let line_h = line_h.max(17);
        let height = (14 + 20 + 2 + lines * line_h + 14).max(min_h);
        if y > top {
            STATE.with(|st| st.borrow_mut().lines[sec].push(px(y, dpi)));
        }
        child(hwnd, s, title_text, SS_NOPREFIX, ID_TITLE, (x, y + 14, text_w, 21), dpi, bold, Some(sec));
        child(hwnd, s, desc, SS_NOPREFIX, ID_DESC, (x, y + 36, text_w, lines * line_h + 2), dpi, small, Some(sec));
        Row { next: y + height, ctl_x: x + cw - ctl_w, mid: y + height / 2 }
    };
    let toggle = |sec: usize, id: i32, r: &Row| {
        child(hwnd, b, "", owner, id, (r.ctl_x, r.mid - 10, 40, 20), dpi, normal, Some(sec));
    };
    let button = |sec: usize, id: i32, r: &Row, label: &str, w: i32| {
        child(hwnd, b, label, owner, id, (WIDTH - MARGIN - w, r.mid - 15, w, 30), dpi, normal, Some(sec));
    };
    let combo = |sec: usize, id: i32, r: &Row, items: &[&str]| {
        let c = child(hwnd, w!("COMBOBOX"), "", combo_style, id, (r.ctl_x, r.mid - 14, 190, 220), dpi, normal, Some(sec));
        for it in items {
            let h = HSTRING::from(*it);
            SendMessageW(c, CB_ADDSTRING, Some(WPARAM(0)), Some(LPARAM(h.as_ptr() as isize)));
        }
    };

    // Generale
    heading_of(0);
    let mut y = top;
    let r = row(0, y, "Modalità Semplice", "Mostra solo l'essenziale. Le altre opzioni restano a un clic, in «Altre opzioni». Vale dalla prossima apertura del pannello.", 40, 0);
    toggle(0, ID_SIMPLE, &r);
    y = r.next;
    let r = row(0, y, "Avvia Cedro con Windows", "La stessa voce del menu dell'icona e dell'installatore.", 40, 0);
    toggle(0, ID_AUTO, &r);
    y = r.next;
    let r = row(0, y, "Avviso a fine conversione", "Il fumetto vicino all'orologio con il nome del file creato.", 40, 0);
    toggle(0, ID_NOTIFY, &r);
    y = r.next;
    let r = row(0, y, "Mostra il file nuovo", "A lavoro finito apre la cartella in Esplora file e seleziona la copia.", 40, 0);
    toggle(0, ID_REVEAL, &r);
    y = r.next;
    let r = row(0, y, "Barra di avanzamento", "Una linea sottile in basso a destra mentre converte, verde a lavoro finito.", 40, 0);
    toggle(0, ID_PROGRESS, &r);
    y = r.next;
    let r = row(0, y, "Registro attività", "Il file log.txt e config.txt stanno in %APPDATA%\\Cedro.", 130, 0);
    button(0, ID_LOG, &r, "Apri cartella", 130);

    // Ruota
    heading_of(1);
    y = top;
    for (id, title, desc) in [
        (ID_SHIFT, "Shift apre le conversioni", "Tieni premuto Shift e fai clic destro sul file."),
        (ID_CTRL, "Ctrl apre gli strumenti", "Comprimi, ruota, OCR e le altre fette di Ctrl."),
        (ID_ALT, "Alt apre le azioni", "Duplica, percorso e ZIP. Spegnilo se Alt dà fastidio ad altri programmi."),
        (ID_DIALOGS, "Apri nelle finestre Apri/Salva", "Usa la ruota anche scegliendo un file in un programma."),
    ] {
        let r = row(1, y, title, desc, 40, 0);
        toggle(1, id, &r);
        y = r.next;
    }

    // Salvataggio
    heading_of(2);
    y = top;
    let r = row(2, y, "Dove mettere la copia", "L'originale resta sempre dov'è.", 320, 150);
    let radio = tab | BS_AUTORADIOBUTTON as u32;
    child(hwnd, b, "Nella stessa cartella del file", radio | WS_GROUP.0, ID_DEST_SAME, (r.ctl_x, y + 14, 320, 24), dpi, normal, Some(2));
    child(hwnd, b, "In una sottocartella «Cedro»", radio, ID_DEST_SUB, (r.ctl_x, y + 42, 320, 24), dpi, normal, Some(2));
    child(hwnd, b, "Sempre in questa cartella:", radio, ID_DEST_FIXED, (r.ctl_x, y + 70, 320, 24), dpi, normal, Some(2));
    child(hwnd, s, "", SS_NOPREFIX | 0x4000 /* SS_ENDELLIPSIS */, ID_DEST_PATH, (r.ctl_x + 22, y + 102, 190, 22), dpi, small, Some(2));
    child(hwnd, b, "Sfoglia…", owner, ID_BROWSE, (r.ctl_x + 220, y + 98, 100, 30), dpi, normal, Some(2));
    y = r.next;
    let r = row(2, y, "Se il file esiste già", "Aggiungi un numero: foto-2.jpg. Sostituisci riscrive il file.", 190, 0);
    combo(2, ID_EXISTS, &r, &["Aggiungi un numero", "Sostituisci"]);

    // Qualità
    heading_of(3);
    y = top;
    let r = row(3, y, "Qualità JPEG e WebP", "Anche per MP3 e PDF in JPG. Più alta, file più grandi.", 250, 0);
    let bar = child(hwnd, TRACKBAR_CLASSW, "", tab | TBS_HORZ as u32 | TBS_AUTOTICKS as u32, ID_QUALITY, (r.ctl_x - 6, r.mid - 18, 200, 36), dpi, normal, Some(3));
    SendMessageW(bar, TBM_SETRANGE, Some(WPARAM(1)), Some(LPARAM((40 | (95 << 16)) as isize)));
    SendMessageW(bar, TBM_SETTICFREQ, Some(WPARAM(11)), Some(LPARAM(0)));
    child(hwnd, s, "82", 2 /* SS_RIGHT */, ID_QUALITY_VALUE, (r.ctl_x + 206, r.mid - 11, 44, 22), dpi, bold, Some(3));
    y = r.next;
    let r = row(3, y, "PDF in JPG: risoluzione", "Più dpi, immagini più nitide e più grandi.", 190, 0);
    combo(3, ID_DPI, &r, &["100 dpi, leggero", "144 dpi, normale", "200 dpi", "300 dpi, stampa"]);
    y = r.next;
    let r = row(3, y, "Comprimi immagine", "Quanto riduce la fetta «Comprimi».", 190, 0);
    combo(3, ID_COMPRESS, &r, &["Poco", "Medio", "Molto"]);

    // OCR
    heading_of(4);
    y = top;
    let r = row(4, y, "Lingua del testo", "Automatica usa l'italiano di Windows. Se la lingua manca, quella dell'utente.", 190, 0);
    combo(4, ID_LANG, &r, &["Automatica", "Italiano", "Inglese"]);
    y = r.next;
    let r = row(4, y, "Italiano per l'OCR di Windows", "Senza, le lettere accentate escono peggio.", 190, 0);
    let installed = cedro::windows_italian_installed();
    child(hwnd, s, if installed { "Installato" } else { "Non installato" }, SS_NOPREFIX | 2, ID_STATUS, (r.ctl_x, r.mid - 11, 190, 22), dpi, bold, Some(4));
    y = r.next;
    let r = row(4, y, "Pagine al massimo per PDF", "Oltre il limite l'OCR salta le pagine e lo dice.", 190, 0);
    combo(4, ID_MAXPAGES, &r, &["50", "100", "300", "Tutte"]);

    // Aggiornamenti
    heading_of(5);
    y = top;
    let r = row(5, y, "Versione installata", &format!("Cedro {}", update::CURRENT), 130, 0);
    button(5, ID_CHECK_NOW, &r, "Controlla ora", 130);
    y = r.next;
    let r = row(5, y, "Cerca gli aggiornamenti da solo", "All'avvio e poi ogni 24 ore, in silenzio se non c'è niente.", 40, 0);
    toggle(5, ID_UPDATE, &r);
    y = r.next;
    let skipped = match update::skipped_version() {
        Some(v) => format!("Hai detto no alla {v}: all'avvio non la propone più."),
        None => "Nessuna versione rifiutata.".to_string(),
    };
    let r = row(5, y, "Versione saltata", &skipped, 170, 0);
    button(5, ID_UNSKIP, &r, "Proponila di nuovo", 170);

    build_system(hwnd, dpi, x, cw, normal, small, bold);
    build_about(hwnd, dpi, x, cw, normal, small, bold);

    // Il piede: firma a sinistra, Annulla e Salva a destra.
    let foot = HEIGHT - FOOT_H;
    let powered_w = if cedro::edizione::BONDENO { 440 } else { 300 };
    child(hwnd, s, "", crate::firma_win::SS_OWNERDRAW, ID_POWERED, (18, foot + 16, powered_w, 24), dpi, small, None);
    child(hwnd, b, "Annulla", owner, IDCANCEL.0, (WIDTH - 18 - 88 - 8 - 88, foot + 12, 88, 32), dpi, normal, None);
    child(hwnd, b, "Salva", owner, IDOK.0, (WIDTH - 18 - 88, foot + 12, 88, 32), dpi, bold, None);
}

/// Larghezza in pixel del dispositivo di `text` col carattere dato.
unsafe fn text_px(text: &str, font: HFONT) -> i32 {
    let dc = GetDC(None);
    let old = SelectObject(dc, font.into());
    let wide: Vec<u16> = text.encode_utf16().collect();
    let mut size = SIZE::default();
    let _ = GetTextExtentPoint32W(dc, &wide, &mut size);
    SelectObject(dc, old);
    ReleaseDC(None, dc);
    size.cx
}

/// Scheda «Sistema»: com'è fatto il PC, le istruzioni del processore e le due accelerazioni.
unsafe fn build_system(hwnd: HWND, dpi: u32, x: i32, cw: i32, normal: HFONT, small: HFONT, bold: HFONT) {
    const SEC: usize = 6;
    let s = w!("STATIC");
    let b = w!("BUTTON");
    let owner = WS_TABSTOP.0 | BS_OWNERDRAW as u32;
    const SS_NOPREFIX: u32 = 0x80;
    const SS_OWNERDRAW: u32 = 0xD;
    const SS_ENDELLIPSIS: u32 = 0x4000;
    let caption = font(12, 700, dpi);
    let heading = font(22, 700, dpi);
    STATE.with(|st| st.borrow_mut().caption = caption);
    let info = cedro::sysinfo::info();
    let hw = cedro::accel::hardware();

    child(hwnd, s, "Sistema", SS_NOPREFIX, ID_TITLE, (x, 20, cw, 34), dpi, heading, Some(SEC));
    let cap = |y: i32, text: &str| {
        child(hwnd, s, text, SS_NOPREFIX, ID_DESC, (x, y, cw, 18), dpi, caption, Some(SEC));
    };

    cap(62, "QUESTO PC");
    let mut y = 84;
    for (label, value, detail) in [
        ("Processore", info.cpu.as_str(), info.cpu_detail.as_str()),
        ("Memoria", info.ram.as_str(), ""),
        ("Scheda video", info.gpu.as_str(), info.gpu_detail.as_str()),
        ("Sistema", info.os.as_str(), ""),
    ] {
        child(hwnd, s, label, SS_NOPREFIX, ID_DESC, (x, y, 110, 20), dpi, normal, Some(SEC));
        child(hwnd, s, value, SS_NOPREFIX | SS_ENDELLIPSIS, ID_TITLE, (x + 126, y, cw - 126, 20), dpi, normal, Some(SEC));
        y += 20;
        if !detail.is_empty() {
            child(hwnd, s, detail, SS_NOPREFIX | SS_ENDELLIPSIS, ID_DESC, (x + 126, y - 1, cw - 126, 18), dpi, small, Some(SEC));
            y += 17;
        }
        y += 5;
    }

    cap(226, "ISTRUZIONI DEL PROCESSORE");
    let mono = font_named(12, 700, dpi, w!("Consolas"));
    let mut cx = 0;
    for (i, (name, _)) in info.isa.iter().enumerate() {
        let width = text_px(name, mono) * 96 / dpi as i32 + 36;
        if cx + width > cw {
            break;
        }
        child(hwnd, s, name, SS_OWNERDRAW | SS_NOPREFIX, ID_CHIP + i as i32, (x + cx, 248, width, 24), dpi, mono, Some(SEC));
        cx += width + 6;
    }

    cap(286, "ACCELERAZIONI");
    let tag_w = |text: &str| text_px(text, bold) * 96 / dpi as i32 + 18;
    let mut top = 308;
    for (id, tag_id, title, desc, tag) in [
        (
            ID_GPU,
            ID_TAG_GPU,
            "Accelerazione GPU",
            "Video in MP4 con la scheda video o la grafica integrata. Se la scheda rifiuta un video, Cedro lo rifà col processore.",
            match &hw.gpu {
                Some(found) => format!("Trovata: {found}"),
                None => "Non disponibile su questo PC".to_string(),
            },
        ),
        (
            ID_AVX,
            ID_TAG_AVX,
            format!("Accelerazione {}", cedro::accel::BOOST_NAME).as_str(),
            "Calcoli più veloci per OCR e trascrizione dell'audio.",
            if hw.avx512 { format!("Trovato: {}", cedro::accel::BOOST_FOUND) } else { format!("Il processore non ha {}", cedro::accel::BOOST_NAME) },
        ),
    ] {
        STATE.with(|st| st.borrow_mut().lines[SEC].push(px(top, dpi)));
        let text_w = cw - 40 - 24;
        let (lines, line_h) = measure(desc, small, text_w, dpi);
        let line_h = line_h.max(17);
        let available = if id == ID_GPU { hw.gpu.is_some() } else { hw.avx512 };
        let title_font = if available { bold } else { normal };
        child(hwnd, s, title, SS_NOPREFIX, if available { ID_TITLE } else { ID_LOCKED }, (x, top + 10, text_w, 21), dpi, title_font, Some(SEC));
        child(hwnd, s, desc, SS_NOPREFIX, ID_DESC, (x, top + 32, text_w, lines * line_h + 2), dpi, small, Some(SEC));
        let tag_y = top + 32 + lines * line_h + 4;
        child(hwnd, s, &tag, SS_OWNERDRAW | SS_NOPREFIX, tag_id, (x, tag_y, tag_w(&tag), 20), dpi, bold, Some(SEC));
        let height = 10 + 21 + 1 + lines * line_h + 4 + 20 + 8;
        let t = child(hwnd, b, "", owner, id, (x + cw - 40, top + height / 2 - 10, 40, 20), dpi, normal, Some(SEC));
        if !available {
            let _ = EnableWindow(t, false);
        }
        top += height;
    }
    STATE.with(|st| st.borrow_mut().lines[SEC].push(px(top, dpi)));

    child(hwnd, b, "Copia informazioni", owner, ID_COPY, (x, top + 12, 170, 30), dpi, normal, Some(SEC));
    child(hwnd, s, "Letto all'avvio di Cedro.", SS_NOPREFIX, ID_COPIED, (x + 182, top + 17, cw - 182, 20), dpi, small, Some(SEC));
    let _ = ID_COUNT;
}

/// Scheda «Informazioni»: chi siamo e, sotto, tutto il codice aperto che Cedro usa.
unsafe fn build_about(hwnd: HWND, dpi: u32, x: i32, cw: i32, normal: HFONT, small: HFONT, bold: HFONT) {
    const SEC: usize = 7;
    let s = w!("STATIC");
    let b = w!("BUTTON");
    let owner = WS_TABSTOP.0 | BS_OWNERDRAW as u32;
    const SS_NOPREFIX: u32 = 0x80;
    const SS_OWNERDRAW: u32 = 0xD;
    let big = font(18, 700, dpi);
    let caption = STATE.with(|st| st.borrow().caption);

    child(hwnd, s, "", SS_OWNERDRAW, ID_LOGO, (x, 20, 56, 56), dpi, normal, Some(SEC));
    child(hwnd, s, &format!("Cedro {}", update::CURRENT), SS_NOPREFIX, ID_TITLE, (x + 72, 20, cw - 72, 26), dpi, big, Some(SEC));
    child(hwnd, s, "Convertitore di file, con una ruota al cursore · Licenza MIT", SS_NOPREFIX, ID_DESC, (x + 72, 47, cw - 72, 18), dpi, small, Some(SEC));
    child(hwnd, s, "", SS_OWNERDRAW, ID_ABOUT_POWERED, (x + 72, 65, cw - 72, 18), dpi, small, Some(SEC));
    STATE.with(|st| st.borrow_mut().lines[SEC].push(px(92, dpi)));
    // Edizione Bondeno: il riquadro con lo stemma, e tutto il resto scende di `e` (la lista si accorcia).
    let e = if cedro::edizione::BONDENO {
        child(hwnd, s, "", SS_OWNERDRAW, ID_ENTE_ABOUT, (x, 104, cw, 80), dpi, small, Some(SEC));
        92
    } else {
        0
    };

    child(hwnd, s, "CODICE APERTO USATO DA CEDRO", SS_NOPREFIX, ID_DESC, (x, 104 + e, cw, 18), dpi, caption, Some(SEC));
    child(hwnd, s, "Cedro esiste grazie a queste persone e ai loro progetti. Grazie.", SS_NOPREFIX, ID_DESC, (x, 124 + e, cw, 20), dpi, small, Some(SEC));

    // La ricerca: il campo sta dentro un riquadro con l'angolo morbido.
    let frame = RECT { left: px(x, dpi), top: px(152 + e, dpi), right: px(x + cw - 130, dpi), bottom: px(152 + e + 30, dpi) };
    STATE.with(|st| st.borrow_mut().frames[SEC].push(frame));
    let edit = child(hwnd, w!("EDIT"), "", WS_TABSTOP.0 | 0x80 /* ES_AUTOHSCROLL */, ID_SEARCH, (x + 10, 158 + e, cw - 130 - 20, 20), dpi, normal, Some(SEC));
    let hint = HSTRING::from("Cerca un componente o una licenza");
    SendMessageW(edit, 0x1501 /* EM_SETCUEBANNER */, Some(WPARAM(1)), Some(LPARAM(hint.as_ptr() as isize)));
    child(hwnd, s, "", SS_NOPREFIX | 2 /* SS_RIGHT */, ID_COUNT, (x + cw - 120, 158 + e, 120, 20), dpi, small, Some(SEC));

    let list = crate::winlist::create(hwnd, ID_LIST, (px(x, dpi), px(194 + e, dpi), px(cw, dpi), px(284 - e, dpi)));
    STATE.with(|st| st.borrow_mut().sections[SEC].push(list));
    show_count(hwnd, crate::winlist::filter(list, ""));

    // Pulsante e nota sotto la lista, con la nota su due righe che restano dentro il pannello.
    child(hwnd, b, "Apri le licenze complete", owner, ID_FULL, (x, 488, 190, 30), dpi, normal, Some(SEC));
    let note = match cedro::credits::full_count() {
        Some(n) => format!("Tutti i {n} componenti, anche quelli minori, nel file «Licenze di terze parti»."),
        None => "Tutti i componenti, anche quelli minori, nel file «Licenze di terze parti».".to_string(),
    };
    child(hwnd, s, &note, SS_NOPREFIX, ID_DESC, (x + 202, 485, cw - 202, 40), dpi, small, Some(SEC));
    let _ = bold;
}

unsafe fn show_count(hwnd: HWND, n: usize) {
    let text = format!("{n} {}", if n == 1 { "componente" } else { "componenti" });
    let _ = SetWindowTextW(item(hwnd, ID_COUNT), &HSTRING::from(text));
}

/// Il file con tutte le licenze sta accanto a cedro.exe; se manca si apre la pagina del progetto.
fn open_full_licenses() {
    let beside = std::env::current_exe().ok().and_then(|exe| exe.parent().map(|d| d.join(cedro::credits::FULL_FILE)));
    let target = match beside {
        Some(path) if path.exists() => path.display().to_string(),
        _ => "https://github.com/realfulvio/cedro".to_string(),
    };
    unsafe {
        ShellExecuteW(None, w!("open"), &HSTRING::from(target), None, None, SW_SHOWNORMAL);
    }
}

unsafe fn item(hwnd: HWND, id: i32) -> HWND {
    GetDlgItem(Some(hwnd), id).unwrap_or_default()
}

unsafe fn set_check(hwnd: HWND, id: i32, on: bool) {
    if is_toggle(id) {
        STATE.with(|st| st.borrow_mut().checks.insert(id, on));
        let _ = InvalidateRect(Some(item(hwnd, id)), None, false);
        return;
    }
    let state = if on { BST_CHECKED } else { BST_UNCHECKED };
    SendMessageW(item(hwnd, id), BM_SETCHECK, Some(WPARAM(state.0 as usize)), Some(LPARAM(0)));
}

unsafe fn is_checked(hwnd: HWND, id: i32) -> bool {
    if is_toggle(id) {
        return STATE.with(|st| st.borrow().checks.get(&id).copied().unwrap_or(false));
    }
    SendMessageW(item(hwnd, id), BM_GETCHECK, Some(WPARAM(0)), Some(LPARAM(0))).0 == BST_CHECKED.0 as isize
}

unsafe fn set_sel(hwnd: HWND, id: i32, index: usize) {
    SendMessageW(item(hwnd, id), CB_SETCURSEL, Some(WPARAM(index)), Some(LPARAM(0)));
}

unsafe fn sel(hwnd: HWND, id: i32) -> usize {
    SendMessageW(item(hwnd, id), CB_GETCURSEL, Some(WPARAM(0)), Some(LPARAM(0))).0.max(0) as usize
}

fn short_path(path: &str) -> String {
    if path.is_empty() {
        "Nessuna cartella scelta".into()
    } else {
        path.to_string()
    }
}

unsafe fn fill(hwnd: HWND, s: &Settings, autostart: bool) {
    set_check(hwnd, ID_AUTO, autostart);
    set_check(hwnd, ID_NOTIFY, s.notify);
    set_check(hwnd, ID_REVEAL, s.reveal);
    set_check(hwnd, ID_PROGRESS, s.progress);
    set_check(hwnd, ID_SIMPLE, s.simple);
    set_check(hwnd, ID_SHIFT, s.key_shift);
    set_check(hwnd, ID_CTRL, s.key_ctrl);
    set_check(hwnd, ID_ALT, s.key_alt);
    set_check(hwnd, ID_DIALOGS, s.dialogs);
    set_check(hwnd, ID_UPDATE, s.update_check);
    let hw = cedro::accel::hardware();
    set_check(hwnd, ID_GPU, s.gpu && hw.gpu.is_some());
    set_check(hwnd, ID_AVX, s.avx512 && hw.avx512);
    set_check(
        hwnd,
        match s.dest {
            Dest::Same => ID_DEST_SAME,
            Dest::Sub => ID_DEST_SUB,
            Dest::Fixed => ID_DEST_FIXED,
        },
        true,
    );
    STATE.with(|st| st.borrow_mut().dest_dir = s.dest_dir.clone());
    let _ = SetWindowTextW(item(hwnd, ID_DEST_PATH), &HSTRING::from(short_path(&s.dest_dir)));
    set_sel(hwnd, ID_EXISTS, s.replace as usize);
    SendMessageW(item(hwnd, ID_QUALITY), TBM_SETPOS, Some(WPARAM(1)), Some(LPARAM(s.quality as isize)));
    show_quality(hwnd);
    set_sel(hwnd, ID_DPI, DPI.iter().position(|d| *d == s.pdf_dpi).unwrap_or(1));
    set_sel(hwnd, ID_COMPRESS, s.compress as usize);
    set_sel(hwnd, ID_LANG, LANGS.iter().position(|l| *l == s.ocr_lang).unwrap_or(0));
    set_sel(hwnd, ID_MAXPAGES, MAX_PAGES.iter().position(|m| *m == s.ocr_max).unwrap_or(1));
}

unsafe fn quality(hwnd: HWND) -> u8 {
    (SendMessageW(item(hwnd, ID_QUALITY), WM_USER, Some(WPARAM(0)), Some(LPARAM(0))).0 as u8).clamp(40, 95)
}

unsafe fn show_quality(hwnd: HWND) {
    let _ = SetWindowTextW(item(hwnd, ID_QUALITY_VALUE), &HSTRING::from(quality(hwnd).to_string()));
}

unsafe fn show_section(hwnd: HWND, index: usize) {
    STATE.with(|st| {
        let mut st = st.borrow_mut();
        st.current = index;
        for (i, list) in st.sections.iter().enumerate() {
            for c in list {
                let _ = ShowWindow(*c, if i == index { SW_SHOW } else { SW_HIDE });
            }
        }
    });
    for i in 0..SECTIONS.len() {
        let _ = InvalidateRect(Some(item(hwnd, ID_NAV + i as i32)), None, true);
    }
    let _ = InvalidateRect(Some(hwnd), None, false);
}

unsafe fn collect(hwnd: HWND) -> Settings {
    let dest = if is_checked(hwnd, ID_DEST_SUB) {
        Dest::Sub
    } else if is_checked(hwnd, ID_DEST_FIXED) {
        Dest::Fixed
    } else {
        Dest::Same
    };
    let base = read_settings();
    Settings {
        quality: quality(hwnd),
        notify: is_checked(hwnd, ID_NOTIFY),
        update_check: is_checked(hwnd, ID_UPDATE),
        key_shift: is_checked(hwnd, ID_SHIFT),
        key_ctrl: is_checked(hwnd, ID_CTRL),
        key_alt: is_checked(hwnd, ID_ALT),
        dialogs: is_checked(hwnd, ID_DIALOGS),
        reveal: is_checked(hwnd, ID_REVEAL),
        progress: is_checked(hwnd, ID_PROGRESS),
        simple: is_checked(hwnd, ID_SIMPLE),
        view_asked: base.view_asked,
        // Dove il PC non ha l'accelerazione l'interruttore è bloccato: resta la scelta di prima.
        gpu: if cedro::accel::hardware().gpu.is_some() { is_checked(hwnd, ID_GPU) } else { base.gpu },
        avx512: if cedro::accel::hardware().avx512 { is_checked(hwnd, ID_AVX) } else { base.avx512 },
        dest,
        dest_dir: STATE.with(|st| st.borrow().dest_dir.clone()),
        replace: sel(hwnd, ID_EXISTS) == 1,
        pdf_dpi: *DPI.get(sel(hwnd, ID_DPI)).unwrap_or(&base.pdf_dpi),
        compress: sel(hwnd, ID_COMPRESS).min(2) as u8,
        ocr_lang: LANGS.get(sel(hwnd, ID_LANG)).unwrap_or(&"auto").to_string(),
        ocr_max: *MAX_PAGES.get(sel(hwnd, ID_MAXPAGES)).unwrap_or(&base.ocr_max),
    }
}

unsafe fn save(hwnd: HWND) {
    let settings = collect(hwnd);
    if settings.dest == Dest::Fixed && settings.dest_dir.trim().is_empty() {
        MessageBoxW(Some(hwnd), w!("Scegli la cartella di destinazione con «Sfoglia…»."), w!("Cedro"), MB_OK | MB_ICONINFORMATION);
        show_section(hwnd, 2);
        return;
    }
    if let Err(e) = write_settings(&settings) {
        let text = HSTRING::from(format!("Impostazioni non salvate: {e}"));
        MessageBoxW(Some(hwnd), &text, w!("Cedro"), MB_OK | MB_ICONERROR);
        return;
    }
    crate::win::apply_settings(&settings);
    let want = is_checked(hwnd, ID_AUTO);
    if want != crate::win::autostart_enabled() {
        if let Err(e) = crate::win::set_autostart(want) {
            let text = HSTRING::from(format!("Avvio automatico non cambiato: {e}"));
            MessageBoxW(Some(hwnd), &text, w!("Cedro"), MB_OK | MB_ICONERROR);
        }
    }
    let _ = DestroyWindow(hwnd);
}

unsafe fn browse(hwnd: HWND) {
    let title = w!("Cartella dove salvare le copie");
    let info = BROWSEINFOW { hwndOwner: hwnd, lpszTitle: title, ulFlags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE, ..Default::default() };
    let pidl = SHBrowseForFolderW(&info);
    if pidl.is_null() {
        return;
    }
    let mut buf = [0u16; 260];
    if SHGetPathFromIDListW(pidl, &mut buf).as_bool() {
        let len = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        let path = String::from_utf16_lossy(&buf[..len]);
        let _ = SetWindowTextW(item(hwnd, ID_DEST_PATH), &HSTRING::from(short_path(&path)));
        STATE.with(|st| st.borrow_mut().dest_dir = path);
        set_check(hwnd, ID_DEST_SAME, false);
        set_check(hwnd, ID_DEST_SUB, false);
        set_check(hwnd, ID_DEST_FIXED, true);
    }
    CoTaskMemFree(Some(pidl as *const _));
}

fn open_config_dir() {
    let dir = cedro::config_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = HSTRING::from(dir.display().to_string());
    unsafe {
        ShellExecuteW(None, w!("open"), &path, None, None, SW_SHOWNORMAL);
    }
}

/// Colora lo sfondo dietro un controllo disegnato, prima delle forme morbide.
unsafe fn clear(dis: &DRAWITEMSTRUCT, color: COLORREF) {
    let brush = CreateSolidBrush(color);
    FillRect(dis.hDC, &dis.rcItem, brush);
    let _ = DeleteObject(brush.into());
}

unsafe fn put_text(hdc: HDC, text: &str, rc: &RECT, font: HFONT, color: COLORREF, format: DRAW_TEXT_FORMAT) {
    let old = SelectObject(hdc, font.into());
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, color);
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    let mut rect = *rc;
    DrawTextW(hdc, &mut wide, &mut rect, format | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);
    SelectObject(hdc, old);
}

unsafe fn draw_nav(dis: &DRAWITEMSTRUCT) {
    let index = (dis.CtlID as i32 - ID_NAV) as usize;
    let selected = STATE.with(|st| st.borrow().current == index);
    let hdc = dis.hDC;
    let r = dis.rcItem;
    let dpi = STATE.with(|st| st.borrow().dpi);
    clear(dis, SIDE);
    if selected {
        crate::wingfx::round_rect(hdc, &r, px(5, dpi) as f32, Some(WHITE), None);
        let bar = RECT { left: r.left, top: r.top + px(9, dpi), right: r.left + px(3, dpi), bottom: r.bottom - px(9, dpi) };
        crate::wingfx::round_rect(hdc, &bar, px(2, dpi) as f32, Some(PRIMARY), None);
    }
    let font = STATE.with(|st| if selected { st.borrow().bold } else { st.borrow().font });
    let text = RECT { left: r.left + px(14, dpi), ..r };
    put_text(hdc, SECTIONS[index], &text, font, TEXT, DT_LEFT);
    // Le due schede nuove della 0.15 portano l'etichetta «NUOVO» finché la versione non cambia.
    if index >= 6 && update::CURRENT.starts_with("0.15.") {
        let tag = STATE.with(|st| st.borrow().caption);
        let at = text.left + text_px(SECTIONS[index], font) + px(8, dpi);
        let area = RECT { left: at, ..text };
        put_text(hdc, "NUOVO", &area, tag, PRIMARY, DT_LEFT);
    }
}

/// Una pillola delle istruzioni del processore: verde se c'è, grigia e barrata se manca.
unsafe fn draw_chip(dis: &DRAWITEMSTRUCT) {
    let index = (dis.CtlID as i32 - ID_CHIP) as usize;
    let Some((name, yes)) = cedro::sysinfo::info().isa.get(index).copied() else { return };
    let hdc = dis.hDC;
    let r = dis.rcItem;
    let dpi = STATE.with(|st| st.borrow().dpi);
    clear(dis, WHITE);
    let (fill, fg) = if yes { (OK_SOFT, OK) } else { (OFF_SOFT, OFF) };
    crate::wingfx::round_rect(hdc, &r, (r.bottom - r.top) as f32 / 2.0, Some(fill), None);
    crate::wingfx::circle(hdc, (r.left + px(15, dpi)) as f32, ((r.top + r.bottom) / 2) as f32, px(4, dpi) as f32 - 0.5, fg);
    let face = HSTRING::from("Consolas");
    let f = CreateFontW(
        -px(12, dpi), 0, 0, 0, 700, 0, 0, if yes { 0 } else { 1 },
        DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY, 0, PCWSTR(face.as_ptr()),
    );
    let area = RECT { left: r.left + px(26, dpi), ..r };
    put_text(hdc, name, &area, f, fg, DT_LEFT);
    let _ = DeleteObject(f.into());
}

/// Il cartellino «Trovata: …» sotto un'accelerazione: verde se il PC la ha, grigio se no.
unsafe fn draw_tag(dis: &DRAWITEMSTRUCT) {
    let hw = cedro::accel::hardware();
    let yes = if dis.CtlID as i32 == ID_TAG_GPU { hw.gpu.is_some() } else { hw.avx512 };
    let hdc = dis.hDC;
    let r = dis.rcItem;
    let dpi = STATE.with(|st| st.borrow().dpi);
    clear(dis, WHITE);
    crate::wingfx::round_rect(hdc, &r, px(4, dpi) as f32, Some(if yes { OK_SOFT } else { OFF_SOFT }), None);
    let mut buf = [0u16; 120];
    let len = GetWindowTextW(dis.hwndItem, &mut buf) as usize;
    let label = String::from_utf16_lossy(&buf[..len]);
    let font = STATE.with(|st| st.borrow().bold);
    put_text(hdc, &label, &r, font, if yes { OK } else { OFF }, DT_CENTER);
}

/// L'icona di Cedro, grande, in cima alla scheda «Informazioni».
unsafe fn draw_logo(dis: &DRAWITEMSTRUCT) {
    clear(dis, WHITE);
    let r = dis.rcItem;
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    let instance: HINSTANCE = GetModuleHandleW(None).unwrap_or_default().into();
    if let Ok(icon) = LoadImageW(Some(instance), PCWSTR(1usize as *const u16), IMAGE_ICON, w, h, LR_DEFAULTCOLOR) {
        let icon = HICON(icon.0);
        let _ = DrawIconEx(dis.hDC, r.left, r.top, icon, w, h, 0, None, DI_NORMAL);
        let _ = DestroyIcon(icon);
    }
}

/// Edizione Bondeno, in cima alle sezioni: lo stemma e «Comune di / Bondeno», con una riga sotto.
unsafe fn draw_ente(dis: &DRAWITEMSTRUCT) {
    clear(dis, SIDE);
    let r = dis.rcItem;
    let dpi = STATE.with(|st| st.borrow().dpi);
    let side = px(38, dpi) as f32;
    let top = r.top as f32 + ((r.bottom - r.top - px(6, dpi)) as f32 - side) / 2.0;
    crate::wingfx::draw_stemma(dis.hDC, cedro::edizione::Stemma::Solo, (r.left + px(6, dpi)) as f32, top, side, side);
    let font = STATE.with(|st| st.borrow().bold);
    let text_left = r.left + px(52, dpi);
    let mid = (r.top + r.bottom - px(6, dpi)) / 2;
    put_text(dis.hDC, "Comune di", &RECT { left: text_left, top: mid - px(18, dpi), right: r.right, bottom: mid }, font, PRIMARY, DT_LEFT);
    put_text(dis.hDC, "Bondeno", &RECT { left: text_left, top: mid, right: r.right, bottom: mid + px(18, dpi) }, font, PRIMARY, DT_LEFT);
    let line = RECT { left: r.left + px(6, dpi), top: r.bottom - 1, right: r.right - px(6, dpi), bottom: r.bottom };
    FillRect(dis.hDC, &line, brush(&BRUSH_LINE, LINE));
}

/// Il riquadro della scheda Informazioni: stemma con la scritta e due righe sull'edizione.
unsafe fn draw_ente_about(dis: &DRAWITEMSTRUCT) {
    clear(dis, WHITE);
    let r = dis.rcItem;
    let dpi = STATE.with(|st| st.borrow().dpi);
    crate::wingfx::round_rect(dis.hDC, &r, px(6, dpi) as f32, Some(ENTE_SOFT), Some((ENTE_LINE, 1.0)));
    let side = (r.bottom - r.top - px(10, dpi)) as f32;
    crate::wingfx::draw_stemma(dis.hDC, cedro::edizione::Stemma::Scritta, (r.left + px(12, dpi)) as f32, (r.top + px(5, dpi)) as f32, side, side);
    let (bold, small) = STATE.with(|st| (st.borrow().bold, st.borrow().font));
    let left = r.left + px(12, dpi) + side as i32 + px(14, dpi);
    let right = r.right - px(12, dpi);
    let mid = (r.top + r.bottom) / 2;
    put_text(dis.hDC, "Edizione per il Comune di Bondeno", &RECT { left, top: mid - px(28, dpi), right, bottom: mid - px(6, dpi) }, bold, PRIMARY, DT_LEFT);
    let mut note = RECT { left, top: mid - px(4, dpi), right, bottom: r.bottom - px(2, dpi) };
    let old = SelectObject(dis.hDC, small.into());
    SetTextColor(dis.hDC, GREY);
    SetBkMode(dis.hDC, TRANSPARENT);
    let mut wide: Vec<u16> = "Stesse funzioni di Cedro, con lo stemma del Comune. Si aggiorna solo con i pacchetti di questa edizione."
        .encode_utf16()
        .collect();
    DrawTextW(dis.hDC, &mut wide, &mut note, DT_LEFT | DT_WORDBREAK | DT_NOPREFIX);
    SelectObject(dis.hDC, old);
}

/// L'interruttore a levetta: pillola blu con il pallino a destra se acceso, contorno grigio se spento.
unsafe fn draw_toggle(dis: &DRAWITEMSTRUCT) {
    let id = dis.CtlID as i32;
    let on = STATE.with(|st| st.borrow().checks.get(&id).copied().unwrap_or(false));
    let hdc = dis.hDC;
    let r = dis.rcItem;
    let scale = (r.bottom - r.top) as f32 / 20.0;
    clear(dis, WHITE);
    if dis.itemState.0 & ODS_DISABLED.0 != 0 {
        // Bloccato: il PC non ha questa accelerazione. Contorno chiaro, pallino a sinistra.
        let pale = COLORREF(0x00C9C2B8);
        crate::wingfx::round_rect(hdc, &r, 10.0 * scale, None, Some((pale, scale.max(1.0))));
        crate::wingfx::circle(hdc, r.left as f32 + 9.0 * scale, r.top as f32 + 10.0 * scale, 6.0 * scale, pale);
        return;
    }
    if on {
        crate::wingfx::round_rect(hdc, &r, 10.0 * scale, Some(PRIMARY), None);
        crate::wingfx::circle(hdc, r.left as f32 + 29.0 * scale, r.top as f32 + 10.0 * scale, 6.0 * scale, WHITE);
    } else {
        crate::wingfx::round_rect(hdc, &r, 10.0 * scale, None, Some((GREY, scale.max(1.0))));
        crate::wingfx::circle(hdc, r.left as f32 + 9.0 * scale, r.top as f32 + 10.0 * scale, 6.0 * scale, GREY);
    }
    if dis.itemState.0 & ODS_FOCUS.0 != 0 {
        let focus = RECT { left: r.left - 2, top: r.top - 2, right: r.right + 2, bottom: r.bottom + 2 };
        let _ = DrawFocusRect(hdc, &focus);
    }
}

/// Un pulsante: Salva è blu e pieno, gli altri hanno il fondo chiaro e il bordo sottile.
unsafe fn draw_push(dis: &DRAWITEMSTRUCT) {
    let id = dis.CtlID as i32;
    let primary = id == IDOK.0;
    let hdc = dis.hDC;
    let r = dis.rcItem;
    let dpi = STATE.with(|st| st.borrow().dpi);
    let pressed = dis.itemState.0 & ODS_SELECTED.0 != 0;
    clear(dis, if id == IDOK.0 || id == IDCANCEL.0 { SIDE } else { WHITE });
    let radius = px(5, dpi) as f32;
    let (font, color) = if primary {
        crate::wingfx::round_rect(hdc, &r, radius, Some(if pressed { HOVER } else { PRIMARY }), None);
        (STATE.with(|st| st.borrow().bold), WHITE)
    } else {
        let fill = if pressed { COLORREF(0x00FFEEE6) } else { SIDE }; // #E6EEFF
        crate::wingfx::round_rect(hdc, &r, radius, Some(if id == IDCANCEL.0 { WHITE } else { fill }), Some((LINE, 1.0)));
        (STATE.with(|st| st.borrow().font), TEXT)
    };
    let mut text = [0u16; 80];
    let len = GetWindowTextW(dis.hwndItem, &mut text) as usize;
    let label = String::from_utf16_lossy(&text[..len]);
    put_text(hdc, &label, &r, font, color, DT_CENTER);
    if dis.itemState.0 & ODS_FOCUS.0 != 0 {
        let focus = RECT { left: r.left + 3, top: r.top + 3, right: r.right - 3, bottom: r.bottom - 3 };
        let _ = DrawFocusRect(hdc, &focus);
    }
}

/// Il fondo della finestra: colonna delle sezioni, righe di separazione e piede.
unsafe fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let dpi = STATE.with(|st| st.borrow().dpi);
    let mut client = RECT::default();
    let _ = GetClientRect(hwnd, &mut client);
    FillRect(hdc, &client, brush(&BRUSH_WHITE, WHITE));
    let foot_y = px(HEIGHT - FOOT_H, dpi);
    let side = RECT { left: 0, top: 0, right: px(SIDE_W, dpi), bottom: foot_y };
    FillRect(hdc, &side, brush(&BRUSH_SIDE, SIDE));
    let edge = RECT { left: px(SIDE_W, dpi) - 1, top: 0, right: px(SIDE_W, dpi), bottom: foot_y };
    FillRect(hdc, &edge, brush(&BRUSH_LINE, LINE));
    let foot = RECT { left: 0, top: foot_y, right: client.right, bottom: client.bottom };
    FillRect(hdc, &foot, brush(&BRUSH_SIDE, SIDE));
    let top_line = RECT { left: 0, top: foot_y, right: client.right, bottom: foot_y + 1 };
    FillRect(hdc, &top_line, brush(&BRUSH_LINE, LINE));
    let lines = STATE.with(|st| {
        let st = st.borrow();
        st.lines.get(st.current).cloned().unwrap_or_default()
    });
    let frames = STATE.with(|st| {
        let st = st.borrow();
        st.frames.get(st.current).cloned().unwrap_or_default()
    });
    for f in frames {
        crate::wingfx::round_rect(hdc, &f, px(5, dpi) as f32, Some(WHITE), Some((LINE, 1.0)));
    }
    for y in lines {
        let line = RECT { left: px(SIDE_W + MARGIN, dpi), top: y, right: px(WIDTH - MARGIN, dpi), bottom: y + 1 };
        FillRect(hdc, &line, brush(&BRUSH_LINE, LINE));
    }
    let _ = EndPaint(hwnd, &ps);
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let id = (wparam.0 & 0xffff) as i32;
            match id {
                x if x == IDOK.0 => save(hwnd),
                x if x == IDCANCEL.0 => {
                    let _ = DestroyWindow(hwnd);
                }
                x if (ID_NAV..ID_NAV + SECTIONS.len() as i32).contains(&x) => show_section(hwnd, (x - ID_NAV) as usize),
                x if is_toggle(x) => {
                    let on = !is_checked(hwnd, x);
                    set_check(hwnd, x, on);
                }
                ID_SEARCH if (wparam.0 >> 16) as u32 == 0x0300 /* EN_CHANGE */ => {
                    let mut buf = [0u16; 120];
                    let len = GetWindowTextW(item(hwnd, ID_SEARCH), &mut buf) as usize;
                    let query = String::from_utf16_lossy(&buf[..len]);
                    let n = crate::winlist::filter(item(hwnd, ID_LIST), &query);
                    show_count(hwnd, n);
                }
                ID_COPY => {
                    crate::win::copy_text(&cedro::sysinfo::report());
                    COPIED.store(true, Ordering::Relaxed);
                    let _ = SetWindowTextW(item(hwnd, ID_COPIED), &HSTRING::from("Copiate: incollale in una segnalazione"));
                }
                ID_FULL => open_full_licenses(),
                ID_BROWSE => browse(hwnd),
                ID_LOG => open_config_dir(),
                ID_CHECK_NOW => crate::win::check_updates(true),
                ID_UNSKIP => {
                    update::unskip();
                    let _ = EnableWindow(item(hwnd, ID_UNSKIP), false);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_MEASUREITEM => LRESULT(crate::winlist::measure(lparam) as isize),
        WM_DRAWITEM => {
            if crate::winlist::draw(lparam) {
                return LRESULT(1);
            }
            let dis = &*(lparam.0 as *const DRAWITEMSTRUCT);
            let id = dis.CtlID as i32;
            if (ID_CHIP..ID_CHIP + 10).contains(&id) {
                draw_chip(dis);
            } else if id == ID_TAG_GPU || id == ID_TAG_AVX {
                draw_tag(dis);
            } else if id == ID_LOGO {
                draw_logo(dis);
            } else if id == ID_ENTE {
                draw_ente(dis);
            } else if id == ID_ENTE_ABOUT {
                draw_ente_about(dis);
            } else if id == ID_POWERED {
                crate::firma_win::draw(dis, SIDE, GREY, PRIMARY, false, "", true);
            } else if id == ID_ABOUT_POWERED {
                crate::firma_win::draw(dis, WHITE, GREY, PRIMARY, false, "  ·  github.com/realfulvio/cedro", false);
            } else if (ID_NAV..ID_NAV + SECTIONS.len() as i32).contains(&id) {
                draw_nav(dis);
            } else if is_toggle(id) {
                draw_toggle(dis);
            } else if PUSHES.contains(&id) {
                draw_push(dis);
            }
            LRESULT(1)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_CTLCOLOREDIT => {
            let hdc = HDC(wparam.0 as *mut _);
            SetTextColor(hdc, TEXT);
            SetBkColor(hdc, WHITE);
            LRESULT(brush(&BRUSH_WHITE, WHITE).0 as isize)
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
            let ctl = HWND(lparam.0 as *mut _);
            let hdc = HDC(wparam.0 as *mut _);
            let id = GetDlgCtrlID(ctl);
            let (fg, bg, slot) = match id {
                ID_POWERED => (GREY, SIDE, &BRUSH_SIDE),
                ID_DESC | ID_DEST_PATH | ID_COUNT => (GREY, WHITE, &BRUSH_WHITE),
                ID_LOCKED => (OFF, WHITE, &BRUSH_WHITE),
                ID_COPIED if COPIED.load(Ordering::Relaxed) => (OK, WHITE, &BRUSH_WHITE),
                ID_COPIED => (GREY, WHITE, &BRUSH_WHITE),
                _ => (TEXT, WHITE, &BRUSH_WHITE),
            };
            SetTextColor(hdc, fg);
            SetBkColor(hdc, bg);
            LRESULT(brush(slot, bg).0 as isize)
        }
        WM_HSCROLL => {
            show_quality(hwnd);
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            WINDOW.store(0, Ordering::Relaxed);
            COPIED.store(false, Ordering::Relaxed);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
