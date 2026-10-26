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
    Win32::UI::Input::KeyboardAndMouse::EnableWindow,
    Win32::UI::Shell::*,
    Win32::UI::WindowsAndMessaging::*,
};

const ID_AUTO: i32 = 101;
const ID_NOTIFY: i32 = 102;
const ID_REVEAL: i32 = 103;
const ID_LOG: i32 = 104;
const ID_PROGRESS: i32 = 105;
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
const ID_NAV: i32 = 200;

/// Gli interruttori a levetta: disegnati da Cedro, con lo stato tenuto qui.
const TOGGLES: [i32; 10] = [ID_AUTO, ID_NOTIFY, ID_REVEAL, ID_PROGRESS, ID_SHIFT, ID_CTRL, ID_ALT, ID_DIALOGS, ID_UPDATE, 0];
/// I pulsanti disegnati da Cedro (Salva è quello blu).
const PUSHES: [i32; 6] = [ID_LOG, ID_BROWSE, ID_CHECK_NOW, ID_UNSKIP, 1 /* IDOK */, 2 /* IDCANCEL */];

fn is_toggle(id: i32) -> bool {
    id != 0 && TOGGLES.contains(&id)
}

// Colori del sito del Comune di Bondeno (COLORREF = 0x00BBGGRR).
const PRIMARY: COLORREF = COLORREF(0x008E5E00); // #005E8E
const HOVER: COLORREF = COLORREF(0x00CC6600); // #0066CC
const TEXT: COLORREF = COLORREF(0x00191919); // #191919
const GREY: COLORREF = COLORREF(0x00645A45); // #455A64
const WHITE: COLORREF = COLORREF(0x00FFFFFF);
const SIDE: COLORREF = COLORREF(0x00F7F5F3); // #F3F5F7
const LINE: COLORREF = COLORREF(0x00E5E5E5); // #E5E5E5
const POWERED_BY: &str = "Powered by Luca \"Realhcok\" A.";

const SECTIONS: [&str; 6] = ["Generale", "Ruota", "Salvataggio", "Qualità", "OCR", "Aggiornamenti"];
const DPI: [u32; 4] = [100, 144, 200, 300];
const MAX_PAGES: [u32; 4] = [50, 100, 300, 0];
const LANGS: [&str; 3] = ["auto", "it", "en"];

const WIDTH: i32 = 780;
const HEIGHT: i32 = 600;
const SIDE_W: i32 = 190;
const FOOT_H: i32 = 56;
const MARGIN: i32 = 28;

static WINDOW: AtomicIsize = AtomicIsize::new(0);
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
    !hwnd.0.is_null() && unsafe { IsDialogMessageW(hwnd, msg).as_bool() }
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
    unsafe {
        CreateFontW(
            -px(height, dpi), 0, 0, 0, weight, 0, 0, 0,
            DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY,
            0, crate::winfont::face(),
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

    for (i, name) in SECTIONS.iter().enumerate() {
        child(hwnd, b, name, owner, ID_NAV + i as i32, (8, 12 + 38 * i as i32, SIDE_W - 16, 36), dpi, normal, None);
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

    // Il piede: firma a sinistra, Annulla e Salva a destra.
    let foot = HEIGHT - FOOT_H;
    child(hwnd, s, POWERED_BY, SS_NOPREFIX, ID_POWERED, (18, foot + 18, 300, 20), dpi, small, None);
    child(hwnd, b, "Annulla", owner, IDCANCEL.0, (WIDTH - 18 - 88 - 8 - 88, foot + 12, 88, 32), dpi, normal, None);
    child(hwnd, b, "Salva", owner, IDOK.0, (WIDTH - 18 - 88, foot + 12, 88, 32), dpi, bold, None);
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
    set_check(hwnd, ID_SHIFT, s.key_shift);
    set_check(hwnd, ID_CTRL, s.key_ctrl);
    set_check(hwnd, ID_ALT, s.key_alt);
    set_check(hwnd, ID_DIALOGS, s.dialogs);
    set_check(hwnd, ID_UPDATE, s.update_check);
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
        gpu: base.gpu,
        avx512: base.avx512,
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
}

/// L'interruttore a levetta: pillola blu con il pallino a destra se acceso, contorno grigio se spento.
unsafe fn draw_toggle(dis: &DRAWITEMSTRUCT) {
    let id = dis.CtlID as i32;
    let on = STATE.with(|st| st.borrow().checks.get(&id).copied().unwrap_or(false));
    let hdc = dis.hDC;
    let r = dis.rcItem;
    let scale = (r.bottom - r.top) as f32 / 20.0;
    clear(dis, WHITE);
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
        WM_DRAWITEM => {
            let dis = &*(lparam.0 as *const DRAWITEMSTRUCT);
            let id = dis.CtlID as i32;
            if (ID_NAV..ID_NAV + SECTIONS.len() as i32).contains(&id) {
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
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
            let ctl = HWND(lparam.0 as *mut _);
            let hdc = HDC(wparam.0 as *mut _);
            let id = GetDlgCtrlID(ctl);
            let (fg, bg, slot) = match id {
                ID_POWERED => (GREY, SIDE, &BRUSH_SIDE),
                ID_DESC | ID_DEST_PATH => (GREY, WHITE, &BRUSH_WHITE),
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
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
