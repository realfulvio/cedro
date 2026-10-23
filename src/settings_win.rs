use cedro::{read_settings, write_settings, Settings};
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::Gdi::*,
    Win32::System::LibraryLoader::GetModuleHandleW,
    Win32::UI::Controls::*,
    Win32::UI::HiDpi::GetDpiForSystem,
    Win32::UI::WindowsAndMessaging::*,
};

const ID_AUTO: i32 = 101;
const ID_NOTIFY: i32 = 102;
const ID_UPDATE: i32 = 103;
const ID_SHIFT: i32 = 111;
const ID_CTRL: i32 = 112;
const ID_ALT: i32 = 113;
const ID_QUALITY: i32 = 121;
const ID_QUALITY_VALUE: i32 = 122;
const POWERED_BY: &str = "Powered by Luca \"Realhcok\" A.";

static WINDOW: AtomicIsize = AtomicIsize::new(0);

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
        hbrBackground: HBRUSH((COLOR_BTNFACE.0 + 1) as isize as *mut _),
        lpszClassName: class,
        ..Default::default()
    };
    RegisterClassExW(&wc);
    let dpi = GetDpiForSystem();
    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
    let mut rect = RECT { left: 0, top: 0, right: px(420, dpi), bottom: px(384, dpi) };
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
    let _ = SetForegroundWindow(hwnd);
    Ok(())
}

unsafe fn child(parent: HWND, instance: HINSTANCE, class: PCWSTR, text: &str, style: u32, id: i32, r: (i32, i32, i32, i32), dpi: u32, font: HFONT) -> HWND {
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
    hwnd
}

unsafe fn build(hwnd: HWND, instance: HINSTANCE, dpi: u32) {
    let font = CreateFontW(
        -px(15, dpi), 0, 0, 0, 400, 0, 0, 0,
        DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY,
        0, w!("Segoe UI"),
    );
    let tab = WS_TABSTOP.0;
    let check = tab | BS_AUTOCHECKBOX as u32;
    let group = BS_GROUPBOX as u32;
    let b = w!("BUTTON");
    child(hwnd, instance, b, "Generale", group, 0, (12, 8, 396, 102), dpi, font);
    child(hwnd, instance, b, "Avvia Cedro con Windows", check, ID_AUTO, (28, 30, 360, 22), dpi, font);
    child(hwnd, instance, b, "Avviso a fine conversione", check, ID_NOTIFY, (28, 54, 360, 22), dpi, font);
    child(hwnd, instance, b, "Cerca aggiornamenti (all'avvio e ogni giorno)", check, ID_UPDATE, (28, 78, 360, 22), dpi, font);
    child(hwnd, instance, b, "Tasti che aprono la ruota", group, 0, (12, 118, 396, 102), dpi, font);
    child(hwnd, instance, b, "Shift: conversioni", check, ID_SHIFT, (28, 140, 360, 22), dpi, font);
    child(hwnd, instance, b, "Ctrl: strumenti", check, ID_CTRL, (28, 164, 360, 22), dpi, font);
    child(hwnd, instance, b, "Alt: azioni (duplica, percorso, ZIP)", check, ID_ALT, (28, 188, 360, 22), dpi, font);
    child(hwnd, instance, b, "Qualità", group, 0, (12, 228, 396, 90), dpi, font);
    let s = w!("STATIC");
    child(hwnd, instance, s, "JPEG, WebP, MP3 e PDF in JPG. Più alta, file più grandi.", 0, 0, (28, 248, 370, 20), dpi, font);
    let bar = child(hwnd, instance, TRACKBAR_CLASSW, "", tab | TBS_HORZ as u32 | TBS_AUTOTICKS as u32, ID_QUALITY, (24, 270, 320, 36), dpi, font);
    SendMessageW(bar, TBM_SETRANGE, Some(WPARAM(1)), Some(LPARAM((40 | (95 << 16)) as isize)));
    SendMessageW(bar, TBM_SETTICFREQ, Some(WPARAM(11)), Some(LPARAM(0)));
    child(hwnd, instance, s, "82", 1 /* SS_CENTER */, ID_QUALITY_VALUE, (350, 278, 48, 20), dpi, font);
    child(hwnd, instance, s, POWERED_BY, 0, 0, (14, 354, 220, 20), dpi, font);
    child(hwnd, instance, b, "Salva", tab | BS_DEFPUSHBUTTON as u32, IDOK.0, (244, 348, 78, 28), dpi, font);
    child(hwnd, instance, b, "Annulla", tab, IDCANCEL.0, (330, 348, 78, 28), dpi, font);
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

unsafe fn fill(hwnd: HWND, s: &Settings, autostart: bool) {
    set_check(hwnd, ID_AUTO, autostart);
    set_check(hwnd, ID_NOTIFY, s.notify);
    set_check(hwnd, ID_UPDATE, s.update_check);
    set_check(hwnd, ID_SHIFT, s.key_shift);
    set_check(hwnd, ID_CTRL, s.key_ctrl);
    set_check(hwnd, ID_ALT, s.key_alt);
    SendMessageW(item(hwnd, ID_QUALITY), TBM_SETPOS, Some(WPARAM(1)), Some(LPARAM(s.quality as isize)));
    show_quality(hwnd);
}

unsafe fn quality(hwnd: HWND) -> u8 {
    (SendMessageW(item(hwnd, ID_QUALITY), WM_USER, Some(WPARAM(0)), Some(LPARAM(0))).0 as u8).clamp(40, 95)
}

unsafe fn show_quality(hwnd: HWND) {
    let _ = SetWindowTextW(item(hwnd, ID_QUALITY_VALUE), &HSTRING::from(quality(hwnd).to_string()));
}

unsafe fn save(hwnd: HWND) {
    let settings = Settings {
        quality: quality(hwnd),
        notify: is_checked(hwnd, ID_NOTIFY),
        update_check: is_checked(hwnd, ID_UPDATE),
        key_shift: is_checked(hwnd, ID_SHIFT),
        key_ctrl: is_checked(hwnd, ID_CTRL),
        key_alt: is_checked(hwnd, ID_ALT),
    };
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

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            match (wparam.0 & 0xffff) as i32 {
                x if x == IDOK.0 => save(hwnd),
                x if x == IDCANCEL.0 => {
                    let _ = DestroyWindow(hwnd);
                }
                _ => {}
            }
            LRESULT(0)
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
