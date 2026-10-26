//! Finestra «Impostazioni»: a sinistra le sezioni, a destra le opzioni della sezione scelta.
//! Disegnata con i controlli di Windows, nei colori e nel font del Comune di Bondeno.

use cedro::{read_settings, update, write_settings, Dest, Settings};
use std::cell::RefCell;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::{
    core::*,
    Win32::Foundation::*,
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
const ID_HEADER: i32 = 160;
const ID_POWERED: i32 = 161;
const ID_NAVBG: i32 = 162;
const ID_DESC: i32 = 170;
const ID_NAV: i32 = 200;

// Colori del sito del Comune di Bondeno (COLORREF = 0x00BBGGRR).
const PRIMARY: COLORREF = COLORREF(0x008E5E00); // #005E8E
const TEXT: COLORREF = COLORREF(0x00191919); // #191919
const GREY: COLORREF = COLORREF(0x00645A45); // #455A64
const WHITE: COLORREF = COLORREF(0x00FFFFFF);
const SIDE: COLORREF = COLORREF(0x00F7F5F3); // #F3F5F7
const POWERED_BY: &str = "Powered by Luca \"Realhcok\" A.";

const SECTIONS: [&str; 6] = ["Generale", "Ruota", "Salvataggio", "Qualità", "OCR", "Aggiornamenti"];
const DPI: [u32; 4] = [100, 144, 200, 300];
const MAX_PAGES: [u32; 4] = [50, 100, 300, 0];
const LANGS: [&str; 3] = ["auto", "it", "en"];

const WIDTH: i32 = 660;
const HEIGHT: i32 = 468;
const SIDE_W: i32 = 168;

static WINDOW: AtomicIsize = AtomicIsize::new(0);
static BRUSH_PRIMARY: AtomicIsize = AtomicIsize::new(0);
static BRUSH_WHITE: AtomicIsize = AtomicIsize::new(0);
static BRUSH_SIDE: AtomicIsize = AtomicIsize::new(0);

struct State {
    sections: Vec<Vec<HWND>>,
    current: usize,
    font: HFONT,
    bold: HFONT,
    dest_dir: String,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State {
        sections: Vec::new(),
        current: 0,
        font: HFONT::default(),
        bold: HFONT::default(),
        dest_dir: String::new(),
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
    let dpi = GetDpiForSystem();
    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
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
    build(hwnd, instance, dpi);
    fill(hwnd, &read_settings(), crate::win::autostart_enabled());
    show_section(hwnd, 0);
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

unsafe fn build(hwnd: HWND, _instance: HINSTANCE, dpi: u32) {
    let normal = font(15, 400, dpi);
    let bold = font(15, 700, dpi);
    let title = font(17, 700, dpi);
    STATE.with(|st| {
        let mut st = st.borrow_mut();
        st.sections = vec![Vec::new(); SECTIONS.len()];
        st.font = normal;
        st.bold = bold;
    });
    let s = w!("STATIC");
    let b = w!("BUTTON");
    let tab = WS_TABSTOP.0;
    let check = tab | (BS_AUTOCHECKBOX | BS_LEFTTEXT | BS_RIGHTBUTTON) as u32;
    let push = tab | BS_PUSHBUTTON as u32;
    let combo_style = tab | CBS_DROPDOWNLIST as u32 | WS_VSCROLL.0;

    child(hwnd, s, "   Impostazioni di Cedro", 0x200 /* SS_CENTERIMAGE */, ID_HEADER, (0, 0, WIDTH, 40), dpi, title, None);
    child(hwnd, s, "", 0, ID_NAVBG, (0, 40, SIDE_W, HEIGHT - 86), dpi, normal, None);
    for (i, name) in SECTIONS.iter().enumerate() {
        child(
            hwnd, b, name, tab | BS_OWNERDRAW as u32, ID_NAV + i as i32,
            (0, 52 + 40 * i as i32, SIDE_W, 38), dpi, normal, None,
        );
    }

    let x = SIDE_W + 20; // sinistra del contenuto
    let cx = WIDTH - 20 - 160; // sinistra del controllo, a destra
    let cw = 160;
    // Titolo e descrizione di una riga; ritorna la y della riga dopo.
    let row = |sec: usize, y: i32, title_text: &str, desc: &str| -> i32 {
        child(hwnd, s, title_text, 0, 0, (x, y, cx - x - 8, 20), dpi, bold, Some(sec));
        child(hwnd, s, desc, 0, ID_DESC, (x, y + 20, cx - x - 8, 42), dpi, normal, Some(sec));
        y + 62
    };
    let combo = |sec: usize, id: i32, y: i32, items: &[&str]| {
        let c = child(hwnd, w!("COMBOBOX"), "", combo_style, id, (cx, y, cw, 200), dpi, normal, Some(sec));
        for it in items {
            let h = HSTRING::from(*it);
            SendMessageW(c, CB_ADDSTRING, Some(WPARAM(0)), Some(LPARAM(h.as_ptr() as isize)));
        }
    };
    let toggle = |sec: usize, id: i32, y: i32, label: &str| {
        child(hwnd, b, label, check, id, (cx, y, cw, 22), dpi, normal, Some(sec));
    };
    let button = |sec: usize, id: i32, y: i32, label: &str| {
        child(hwnd, b, label, push, id, (cx, y, cw, 26), dpi, normal, Some(sec));
    };
    let top = 56;

    // Generale
    let mut y = top;
    toggle(0, ID_AUTO, y, "");
    y = row(0, y, "Avvia Cedro con Windows", "La stessa voce del menu dell'icona e dell'installatore.");
    toggle(0, ID_NOTIFY, y, "");
    y = row(0, y, "Avviso a fine conversione", "Il fumetto vicino all'orologio con il nome del file creato.");
    toggle(0, ID_REVEAL, y, "");
    y = row(0, y, "Mostra il file nuovo", "A lavoro finito apre la cartella in Esplora file e seleziona la copia.");
    button(0, ID_LOG, y, "Apri cartella");
    row(0, y, "Registro attività", "Il file log.txt e config.txt stanno in %APPDATA%\\Cedro.");

    // Ruota
    y = top;
    toggle(1, ID_SHIFT, y, "");
    y = row(1, y, "Shift apre le conversioni", "Tieni premuto Shift e fai clic destro sul file.");
    toggle(1, ID_CTRL, y, "");
    y = row(1, y, "Ctrl apre gli strumenti", "Comprimi, ruota, OCR e le altre fette di Ctrl.");
    toggle(1, ID_ALT, y, "");
    y = row(1, y, "Alt apre le azioni", "Duplica, percorso e ZIP. Spegnilo se Alt dà fastidio ad altri programmi.");
    toggle(1, ID_DIALOGS, y, "");
    row(1, y, "Apri nelle finestre Apri/Salva", "Usa la ruota anche scegliendo un file in un programma.");

    // Salvataggio
    y = top;
    child(hwnd, s, "Dove mettere la copia", 0, 0, (x, y, WIDTH - x - 20, 20), dpi, bold, Some(2));
    child(hwnd, s, "L'originale resta sempre dov'è.", 0, ID_DESC, (x, y + 20, WIDTH - x - 20, 20), dpi, normal, Some(2));
    let radio = tab | BS_AUTORADIOBUTTON as u32;
    child(hwnd, b, "Nella stessa cartella del file", radio | WS_GROUP.0, ID_DEST_SAME, (x, y + 50, 320, 22), dpi, normal, Some(2));
    child(hwnd, b, "In una sottocartella «Cedro»", radio, ID_DEST_SUB, (x, y + 76, 320, 22), dpi, normal, Some(2));
    child(hwnd, b, "Sempre in questa cartella:", radio, ID_DEST_FIXED, (x, y + 102, 320, 22), dpi, normal, Some(2));
    child(hwnd, s, "", 0, ID_DEST_PATH, (x + 22, y + 128, 260, 22), dpi, normal, Some(2));
    child(hwnd, b, "Sfoglia…", push, ID_BROWSE, (x + 290, y + 124, 100, 26), dpi, normal, Some(2));
    combo(2, ID_EXISTS, y + 176, &["Aggiungi un numero", "Sostituisci"]);
    row(2, y + 176, "Se il file esiste già", "Aggiungi un numero: foto-2.jpg. Sostituisci riscrive il file.");

    // Qualità
    y = top;
    child(hwnd, s, "Qualità JPEG e WebP", 0, 0, (x, y, WIDTH - x - 20, 20), dpi, bold, Some(3));
    child(hwnd, s, "Anche per MP3 e PDF in JPG. Più alta, file più grandi.", 0, ID_DESC, (x, y + 20, WIDTH - x - 20, 20), dpi, normal, Some(3));
    let bar = child(hwnd, TRACKBAR_CLASSW, "", tab | TBS_HORZ as u32 | TBS_AUTOTICKS as u32, ID_QUALITY, (x - 4, y + 44, 340, 36), dpi, normal, Some(3));
    SendMessageW(bar, TBM_SETRANGE, Some(WPARAM(1)), Some(LPARAM((40 | (95 << 16)) as isize)));
    SendMessageW(bar, TBM_SETTICFREQ, Some(WPARAM(11)), Some(LPARAM(0)));
    child(hwnd, s, "82", 1 /* SS_CENTER */, ID_QUALITY_VALUE, (x + 346, y + 52, 48, 20), dpi, bold, Some(3));
    combo(3, ID_DPI, y + 100, &["100 dpi, leggero", "144 dpi, normale", "200 dpi", "300 dpi, stampa"]);
    row(3, y + 100, "PDF in JPG: risoluzione", "Più dpi, immagini più nitide e più grandi.");
    combo(3, ID_COMPRESS, y + 162, &["Poco", "Medio", "Molto"]);
    row(3, y + 162, "Comprimi immagine", "Quanto riduce la fetta «Comprimi».");

    // OCR
    y = top;
    combo(4, ID_LANG, y, &["Automatica", "Italiano", "Inglese"]);
    y = row(4, y, "Lingua del testo", "Automatica usa l'italiano di Windows. Se la lingua manca, quella dell'utente.");
    child(hwnd, s, if cedro::windows_italian_installed() { "Installato" } else { "Non installato" }, 1, 0, (cx, y, cw, 22), dpi, bold, Some(4));
    y = row(4, y, "Italiano per l'OCR di Windows", "Senza, le lettere accentate escono peggio.");
    combo(4, ID_MAXPAGES, y, &["50", "100", "300", "Tutte"]);
    row(4, y, "Pagine al massimo per PDF", "Oltre il limite l'OCR salta le pagine e lo dice.");

    // Aggiornamenti
    y = top;
    button(5, ID_CHECK_NOW, y, "Controlla ora");
    y = row(5, y, "Versione installata", &format!("Cedro {}", update::CURRENT));
    toggle(5, ID_UPDATE, y, "");
    y = row(5, y, "Cerca gli aggiornamenti da solo", "All'avvio e poi ogni 24 ore, in silenzio se non c'è niente.");
    button(5, ID_UNSKIP, y, "Proponila di nuovo");
    let skipped = match update::skipped_version() {
        Some(v) => format!("Hai detto no alla {v}: all'avvio non la propone più."),
        None => "Nessuna versione rifiutata.".to_string(),
    };
    row(5, y, "Versione saltata", &skipped);

    child(hwnd, s, POWERED_BY, 0, ID_POWERED, (14, HEIGHT - 34, 240, 20), dpi, normal, None);
    child(hwnd, b, "Salva", tab | BS_DEFPUSHBUTTON as u32, IDOK.0, (WIDTH - 192, HEIGHT - 40, 84, 30), dpi, normal, None);
    child(hwnd, b, "Annulla", push, IDCANCEL.0, (WIDTH - 100, HEIGHT - 40, 84, 30), dpi, normal, None);
}

unsafe fn item(hwnd: HWND, id: i32) -> HWND {
    GetDlgItem(Some(hwnd), id).unwrap_or_default()
}

unsafe fn set_check(hwnd: HWND, id: i32, on: bool) {
    let state = if on { BST_CHECKED } else { BST_UNCHECKED };
    SendMessageW(item(hwnd, id), BM_SETCHECK, Some(WPARAM(state.0 as usize)), Some(LPARAM(0)));
}

unsafe fn is_checked(hwnd: HWND, id: i32) -> bool {
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

unsafe fn draw_nav(dis: &DRAWITEMSTRUCT) {
    let index = (dis.CtlID as i32 - ID_NAV) as usize;
    let selected = STATE.with(|st| st.borrow().current == index);
    let hdc = dis.hDC;
    let (bg, slot) = if selected { (WHITE, &BRUSH_WHITE) } else { (SIDE, &BRUSH_SIDE) };
    FillRect(hdc, &dis.rcItem, brush(slot, bg));
    if selected {
        let bar = RECT { left: 0, top: dis.rcItem.top + 6, right: 4, bottom: dis.rcItem.bottom - 6 };
        FillRect(hdc, &bar, brush(&BRUSH_PRIMARY, PRIMARY));
    }
    let font = STATE.with(|st| if selected { st.borrow().bold } else { st.borrow().font });
    let old = SelectObject(hdc, font.into());
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, if selected { PRIMARY } else { TEXT });
    let mut text: Vec<u16> = SECTIONS[index].encode_utf16().collect();
    let mut rect = RECT { left: dis.rcItem.left + 20, ..dis.rcItem };
    DrawTextW(hdc, &mut text, &mut rect, DT_SINGLELINE | DT_VCENTER | DT_LEFT);
    SelectObject(hdc, old);
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
            draw_nav(dis);
            LRESULT(1)
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
            let ctl = HWND(lparam.0 as *mut _);
            let hdc = HDC(wparam.0 as *mut _);
            let id = GetDlgCtrlID(ctl);
            let (fg, bg, slot) = match id {
                ID_HEADER => (WHITE, PRIMARY, &BRUSH_PRIMARY),
                ID_NAVBG => (TEXT, SIDE, &BRUSH_SIDE),
                ID_POWERED | ID_DESC | ID_DEST_PATH => (GREY, WHITE, &BRUSH_WHITE),
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
