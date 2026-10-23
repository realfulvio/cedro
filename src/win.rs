use cedro::{
    wheelgeom_bounds, wheelgfx,
    append_log, kind_of, mode_from_vk, perform, read_quality, read_settings, same_kind_paths, slices, update, Kind, Mode,
    Settings,
    Slice,
};
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU32, AtomicU8, Ordering};
use std::sync::Mutex;
use windows::{
    core::{Interface, *},
    Win32::Foundation::*,
    Win32::Graphics::Gdi::*,
    Win32::System::Com::*,
    Win32::System::DataExchange::*,
    Win32::System::LibraryLoader::GetModuleHandleW,
    Win32::System::Memory::*,
    Win32::System::Ole::*,
    Win32::System::Registry::*,
    Win32::System::SystemServices::MODIFIERKEYS_FLAGS,
    Win32::System::Threading::CreateMutexW,
    Win32::System::Variant::VARIANT,
    Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationValuePattern, TreeScope_Descendants, UIA_ValuePatternId,
    },
    Win32::UI::HiDpi::*,
    Win32::UI::Input::KeyboardAndMouse::*,
    Win32::UI::Shell::*,
    Win32::UI::WindowsAndMessaging::*,
};

const WM_TRAY: u32 = WM_APP + 1;
const WM_APP_INPUT: u32 = WM_APP + 2;
const WM_APP_DROP: u32 = WM_APP + 3;
const WM_APP_RCLICK: u32 = WM_APP + 4;
const WM_APP_UPDATE: u32 = WM_APP + 5;
const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const WHEEL: i32 = 440;
const R0: f32 = 58.0;
const R1: f32 = 168.0;

static RDOWN: AtomicBool = AtomicBool::new(false);
static EAT_RIGHT: AtomicBool = AtomicBool::new(false);
static RMODE: AtomicU32 = AtomicU32::new(1);
static HMSG: AtomicIsize = AtomicIsize::new(0);
static CUR_X: AtomicI32 = AtomicI32::new(0);
static CUR_Y: AtomicI32 = AtomicI32::new(0);
// Bit 0 Shift, bit 1 Ctrl, bit 2 Alt: i tasti che aprono la ruota.
static KEYS: AtomicU8 = AtomicU8::new(0b111);
static UPDATE: Mutex<Option<UpdateNews>> = Mutex::new(None);

enum UpdateNews {
    Found { release: update::Release, manual: bool },
    Note(String),
}

struct Session {
    msg: HWND,
    wheel: HWND,
    anchor_x: i32,
    anchor_y: i32,
    mode: Mode,
    hover: i32,
    paths: Vec<PathBuf>,
    down: bool,
    down_x: i32,
    down_y: i32,
    dragging: bool,
    rdown: bool,
    rx: i32,
    ry: i32,
    rmoved: bool,
    chord: bool,
    armed: bool,
    quality: u8,
    icon: HICON,
    target: Option<IDropTarget>,
}

thread_local! {
    static SESSION: RefCell<Session> = RefCell::new(Session {
        msg: HWND::default(),
        wheel: HWND::default(),
        anchor_x: 0,
        anchor_y: 0,
        mode: Mode::Convert,
        hover: -1,
        paths: Vec::new(),
        down: false,
        down_x: 0,
        down_y: 0,
        dragging: false,
        rdown: false,
        rx: 0,
        ry: 0,
        rmoved: false,
        chord: false,
        armed: false,
        quality: 82,
        icon: HICON::default(),
        target: None,
    });
}

pub fn start() -> std::result::Result<(), String> {
    crate::winfont::load();
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        OleInitialize(None).map_err(|e| e.to_string())?;
        let _mutex = CreateMutexW(None, true, w!("Local\\CedroTray")).map_err(|e| e.to_string())?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return Ok(());
        }
        let _ = _mutex;

        let instance = GetModuleHandleW(None).map_err(|e| e.to_string())?;
        let class = w!("CedroMsg");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(msg_proc),
            hInstance: instance.into(),
            lpszClassName: class,
            ..Default::default()
        };
        if RegisterClassExW(&wc) == 0 {
            return Err("classe finestra non registrata".into());
        }
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            w!("Cedro"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .map_err(|e| e.to_string())?;
        HMSG.store(hwnd.0 as isize, Ordering::Relaxed);
        SESSION.with(|s| {
            let mut g = s.borrow_mut();
            g.msg = hwnd;
            g.quality = read_quality();
            g.icon = make_icon().unwrap_or(HICON::default());
        });
        add_tray(hwnd)?;
        let settings = read_settings();
        apply_settings(&settings);
        if settings.update_check {
            check_updates(false);
        }
        // Cedro resta aperto per giorni: ricontrolla ogni 24 ore se l'opzione è accesa.
        let _ = SetTimer(Some(hwnd), 10, 24 * 60 * 60 * 1000, None);
        let kb = SetWindowsHookExW(WH_KEYBOARD_LL, Some(kb_proc), None, 0).map_err(|e| e.to_string())?;
        let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0).map_err(|e| e.to_string())?;
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if crate::settings_win::handle_message(&msg) {
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = UnhookWindowsHookEx(kb);
        let _ = UnhookWindowsHookEx(mouse);
        remove_tray(hwnd);
    }
    Ok(())
}

unsafe extern "system" fn kb_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let down = wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN;
        let hwnd = HWND(HMSG.load(Ordering::Relaxed) as *mut _);
        if down {
            if let Some(mode) = mode_from_vk(info.vkCode).filter(|m| mode_enabled(*m)) {
                if RDOWN.load(Ordering::Relaxed) && !EAT_RIGHT.load(Ordering::Relaxed) {
                    let x = CUR_X.load(Ordering::Relaxed);
                    let y = CUR_Y.load(Ordering::Relaxed);
                    if file_surface(x, y) {
                        EAT_RIGHT.store(true, Ordering::Relaxed);
                        RMODE.store(mode_code(mode), Ordering::Relaxed);
                        let _ = PostMessageW(Some(hwnd), WM_APP_RCLICK, WPARAM(0), LPARAM(0));
                    }
                }
            }
            let _ = PostMessageW(Some(hwnd), WM_APP_INPUT, WPARAM(1), LPARAM(info.vkCode as isize));
        } else {
            let _ = PostMessageW(Some(hwnd), WM_APP_INPUT, WPARAM(2), LPARAM(info.vkCode as isize));
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        CUR_X.store(info.pt.x, Ordering::Relaxed);
        CUR_Y.store(info.pt.y, Ordering::Relaxed);
        let kind = wparam.0 as u32;
        let hwnd = HWND(HMSG.load(Ordering::Relaxed) as *mut _);
        if kind == WM_RBUTTONDOWN {
            RDOWN.store(true, Ordering::Relaxed);
            if let Some(mode) = held_mode() {
                if file_surface(info.pt.x, info.pt.y) {
                    EAT_RIGHT.store(true, Ordering::Relaxed);
                    RMODE.store(mode_code(mode), Ordering::Relaxed);
                    let _ = PostMessageW(Some(hwnd), WM_APP_RCLICK, WPARAM(0), LPARAM(0));
                    return LRESULT(1);
                }
            }
        }
        if kind == WM_RBUTTONUP {
            let eat = EAT_RIGHT.swap(false, Ordering::Relaxed);
            RDOWN.store(false, Ordering::Relaxed);
            let _ = PostMessageW(Some(hwnd), WM_APP_INPUT, WPARAM(3), LPARAM(kind as isize));
            if eat {
                return LRESULT(1);
            }
        }
        if kind == WM_LBUTTONDOWN || kind == WM_LBUTTONUP || kind == WM_MOUSEMOVE {
            let _ = PostMessageW(Some(hwnd), WM_APP_INPUT, WPARAM(3), LPARAM(kind as isize));
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn msg_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => {
            if lparam.0 as u32 == WM_RBUTTONUP || lparam.0 as u32 == WM_CONTEXTMENU {
                show_menu(hwnd);
            } else if lparam.0 as u32 == WM_LBUTTONDBLCLK {
                crate::settings_win::open();
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            match (wparam.0 & 0xffff) as u32 {
                1 => {
                    let _ = DestroyWindow(hwnd);
                }
                3 => check_updates(true),
                5 => crate::settings_win::open(),
                4 => {
                    let on = !autostart_enabled();
                    if let Err(e) = set_autostart(on) {
                        balloon(&format!("Avvio automatico non cambiato: {e}"));
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_APP_INPUT => {
            on_input(wparam.0, lparam.0);
            LRESULT(0)
        }
        WM_APP_DROP => {
            on_drop();
            LRESULT(0)
        }
        WM_APP_RCLICK => {
            on_rclick();
            LRESULT(0)
        }
        WM_APP_UPDATE => {
            on_update(hwnd);
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == 10 {
                if read_settings().update_check {
                    check_updates(false);
                }
                return LRESULT(0);
            }
            let pending = SESSION.with(|s| {
                let g = s.borrow();
                g.paths.is_empty() || !g.armed
            });
            if wparam.0 == 9 && pending {
                close_wheel();
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            remove_tray(hwnd);
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn on_input(kind: usize, extra: isize) {
    let x = CUR_X.load(Ordering::Relaxed);
    let y = CUR_Y.load(Ordering::Relaxed);
    if kind == 1 || kind == 2 {
        let vk = extra as u32;
        let down = kind == 1;
        SESSION.with(|s| {
            let mut g = s.borrow_mut();
            if vk == 0x1B && down {
                drop(g);
                close_wheel();
                return;
            }
            if down {
                if let Some(mode) = mode_from_vk(vk).filter(|m| mode_enabled(*m)) {
                    g.mode = mode;
                    let have = !g.wheel.0.is_null();
                    if g.dragging || have {
                        g.armed = true;
                        drop(g);
                        if !have {
                            open_wheel(x, y, mode, Vec::new(), false);
                        } else {
                            refresh_wheel();
                        }
                    }
                }
            }
        });
        return;
    }
    if kind != 3 {
        return;
    }
    let flag = extra as u32;
    let mut act = 0;
    SESSION.with(|s| {
        let mut g = s.borrow_mut();
        match flag {
            WM_LBUTTONDOWN if g.chord && !g.wheel.0.is_null() => {
                act = if g.hover >= 0 { 1 } else { 2 };
            }
            WM_LBUTTONDOWN => {
                g.down = true;
                g.dragging = false;
                g.down_x = x;
                g.down_y = y;
            }
            WM_MOUSEMOVE => {
                if g.rdown {
                    let dist = ((x - g.rx) as i64).pow(2) + ((y - g.ry) as i64).pow(2);
                    if dist > 64 {
                        g.rmoved = true;
                    }
                }
                if g.down {
                    let dist = ((x - g.down_x) as i64).pow(2) + ((y - g.down_y) as i64).pow(2);
                    if dist > 64 {
                        g.dragging = true;
                        if !g.armed {
                            if let Some(mode) = held_mode() {
                                g.mode = mode;
                                g.armed = true;
                                drop(g);
                                open_wheel(x, y, mode, Vec::new(), false);
                                return;
                            }
                        }
                    }
                }
                if !g.wheel.0.is_null() {
                    drop(g);
                    set_hover(x, y);
                }
            }
            WM_LBUTTONUP => {
                let chord = g.chord;
                g.down = false;
                g.dragging = false;
                let armed = g.armed && !chord;
                drop(g);
                if armed {
                    unsafe {
                        let hwnd = HWND(HMSG.load(Ordering::Relaxed) as *mut _);
                        let _ = SetTimer(Some(hwnd), 9, 180, None);
                    }
                }
            }
            WM_RBUTTONUP => {
                let commit = g.chord && g.rmoved && g.hover >= 0;
                g.rdown = false;
                g.rmoved = false;
                if commit {
                    act = 1;
                }
            }
            _ => {}
        }
    });
    if act == 1 {
        on_drop();
    } else if act == 2 {
        close_wheel();
    }
}

fn held_mode() -> Option<Mode> {
    unsafe {
        if GetAsyncKeyState(VK_SHIFT.0 as i32) < 0 && mode_enabled(Mode::Convert) {
            return Some(Mode::Convert);
        }
        if GetAsyncKeyState(VK_CONTROL.0 as i32) < 0 && mode_enabled(Mode::Tools) {
            return Some(Mode::Tools);
        }
        if GetAsyncKeyState(VK_MENU.0 as i32) < 0 && mode_enabled(Mode::Actions) {
            return Some(Mode::Actions);
        }
    }
    None
}

fn mode_enabled(mode: Mode) -> bool {
    let bit = match mode {
        Mode::Convert => 0b001,
        Mode::Tools => 0b010,
        Mode::Actions => 0b100,
    };
    KEYS.load(Ordering::Relaxed) & bit != 0
}

/// Le impostazioni salvate valgono subito, senza riavviare Cedro.
pub(crate) fn apply_settings(s: &Settings) {
    let mask = (s.key_shift as u8) | (s.key_ctrl as u8) << 1 | (s.key_alt as u8) << 2;
    KEYS.store(mask, Ordering::Relaxed);
    SESSION.with(|session| session.borrow_mut().quality = s.quality);
}

fn mode_code(mode: Mode) -> u32 {
    match mode {
        Mode::Convert => 1,
        Mode::Tools => 2,
        Mode::Actions => 3,
    }
}

fn mode_from_code(n: u32) -> Mode {
    match n {
        2 => Mode::Tools,
        3 => Mode::Actions,
        _ => Mode::Convert,
    }
}

fn on_rclick() {
    let x = CUR_X.load(Ordering::Relaxed);
    let y = CUR_Y.load(Ordering::Relaxed);
    let mode = held_mode().unwrap_or_else(|| mode_from_code(RMODE.load(Ordering::Relaxed)));
    let Some(path) = file_at(x, y) else {
        balloon("Nessun file sotto il cursore");
        return;
    };
    SESSION.with(|s| {
        let mut g = s.borrow_mut();
        g.rdown = true;
        g.rx = x;
        g.ry = y;
        g.rmoved = false;
        g.mode = mode;
    });
    open_wheel(x, y, mode, vec![path], true);
}

fn file_surface(x: i32, y: i32) -> bool {
    unsafe {
        let hwnd = WindowFromPoint(POINT { x, y });
        if hwnd.0.is_null() {
            return false;
        }
        let root = GetAncestor(hwnd, GA_ROOT);
        let root_class = window_class(if root.0.is_null() { hwnd } else { root });
        if matches!(root_class.as_str(), "Progman" | "WorkerW" | "CabinetWClass" | "ExploreWClass") {
            return true;
        }
        let mut h = hwnd;
        for _ in 0..16 {
            let class = window_class(h);
            if class == "SHELLDLL_DefView" || class == "SysListView32" {
                return true;
            }
            if (class == "DirectUIHWND" || class == "DUIViewWndClassName" || class == "Microsoft.UI.Content.DesktopChildSiteBridge")
                && (root_class == "#32770" || root_class == "CabinetWClass" || root_class == "ExploreWClass")
            {
                return true;
            }
            let parent = GetAncestor(h, GA_PARENT);
            if parent.0.is_null() || parent == h {
                break;
            }
            h = parent;
        }
        false
    }
}

fn on_desktop(x: i32, y: i32) -> bool {
    unsafe {
        let hwnd = WindowFromPoint(POINT { x, y });
        if hwnd.0.is_null() {
            return false;
        }
        let root = GetAncestor(hwnd, GA_ROOT);
        let class = window_class(if root.0.is_null() { hwnd } else { root });
        matches!(class.as_str(), "Progman" | "WorkerW")
    }
}

fn window_class(hwnd: HWND) -> String {
    unsafe {
        let mut buf = [0u16; 64];
        let n = GetClassNameW(hwnd, &mut buf);
        if n <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..n as usize])
    }
}

fn file_at(x: i32, y: i32) -> Option<PathBuf> {
    unsafe {
        let auto: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        let walker = auto.ControlViewWalker().ok()?;
        let mut el = auto.ElementFromPoint(POINT { x, y }).ok()?;
        let mut names = Vec::new();
        for _ in 0..12 {
            if let Ok(name) = el.CurrentName() {
                let text = name.to_string();
                if !text.is_empty() {
                    names.push(text);
                }
            }
            if let Ok(value) = el.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) {
                if let Ok(current) = value.CurrentValue() {
                    let text = current.to_string();
                    if !text.is_empty() {
                        names.push(text);
                    }
                }
            }
            match walker.GetParentElement(&el) {
                Ok(parent) => el = parent,
                Err(_) => break,
            }
        }
        if let Some(found) = resolve_names(x, y, &names) {
            return Some(found);
        }
        // Overlay trasparenti a schermo intero (NVIDIA, Xbox Game Bar...) rubano ElementFromPoint:
        // si ricomincia dalla finestra che Windows indica sotto il cursore, che li salta.
        let mut inner = Vec::new();
        names_from_window(&auto, WindowFromPoint(POINT { x, y }), x, y, &mut inner);
        resolve_names(x, y, &inner)
    }
}

// L'elemento più piccolo della finestra che contiene il punto: la voce del file sotto il cursore.
fn names_from_window(auto: &IUIAutomation, hwnd: HWND, x: i32, y: i32, names: &mut Vec<String>) {
    unsafe {
        if hwnd.0.is_null() {
            return;
        }
        let Ok(root) = auto.ElementFromHandle(hwnd) else { return };
        let Ok(cond) = auto.CreateTrueCondition() else { return };
        let Ok(all) = root.FindAll(TreeScope_Descendants, &cond) else { return };
        let mut best: Option<(i64, IUIAutomationElement)> = None;
        for i in 0..all.Length().unwrap_or(0).min(5000) {
            let Ok(el) = all.GetElement(i) else { continue };
            let Ok(r) = el.CurrentBoundingRectangle() else { continue };
            if x < r.left || x >= r.right || y < r.top || y >= r.bottom {
                continue;
            }
            let area = (r.right - r.left) as i64 * (r.bottom - r.top) as i64;
            if best.as_ref().map_or(true, |(a, _)| area < *a) {
                best = Some((area, el));
            }
        }
        if let Some((_, el)) = best {
            if let Ok(name) = el.CurrentName() {
                let text = name.to_string();
                if !text.is_empty() {
                    names.push(text);
                }
            }
        }
    }
}

fn resolve_names(x: i32, y: i32, names: &[String]) -> Option<PathBuf> {
    for name in names {
        let text = name.trim().trim_matches('"');
        let path = PathBuf::from(text);
        if path.is_file() {
            return Some(path);
        }
    }
    let mut dirs = Vec::new();
    dirs.extend(shell_dir(x, y));
    for name in names {
        if let Some(dir) = address_dir(name) {
            dirs.push(dir);
        }
    }
    if let Some(found) = first_match(&dirs, names) {
        return Some(found);
    }
    if !on_desktop(x, y) {
        return None;
    }
    let mut desks = Vec::new();
    if let Some(dir) = known_folder(&FOLDERID_Desktop) {
        desks.push(dir);
    }
    if let Some(dir) = known_folder(&FOLDERID_PublicDesktop) {
        desks.push(dir);
    }
    first_match(&desks, names)
}

// Le cartelle aperte nelle finestre di Esplora file sotto il cursore, schede comprese.
// IShellWindows funziona tra processi: Esplora file non gira dentro Cedro.
fn shell_dir(x: i32, y: i32) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    unsafe {
        let hwnd = WindowFromPoint(POINT { x, y });
        if hwnd.0.is_null() {
            return dirs;
        }
        let root = GetAncestor(hwnd, GA_ROOT);
        let root = if root.0.is_null() { hwnd } else { root };
        let Ok(windows): windows::core::Result<IShellWindows> = CoCreateInstance(&ShellWindows, None, CLSCTX_ALL) else {
            return dirs;
        };
        let count = windows.Count().unwrap_or(0);
        for i in 0..count {
            let Ok(item) = windows.Item(&VARIANT::from(i)) else { continue };
            let Ok(browser) = item.cast::<IWebBrowser2>() else { continue };
            if browser.HWND().map(|h| h.0 as isize).unwrap_or(0) != root.0 as isize {
                continue;
            }
            if let Ok(url) = browser.LocationURL() {
                if let Some(dir) = path_from_file_url(&url.to_string()) {
                    if dir.is_dir() && !dirs.contains(&dir) {
                        dirs.push(dir);
                    }
                }
            }
        }
    }
    dirs
}

fn path_from_file_url(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    let (host, path) = match rest.strip_prefix('/') {
        Some(local) => ("", local),
        None => rest.split_once('/').unwrap_or((rest, "")),
    };
    let mut bytes = Vec::with_capacity(path.len());
    let raw = path.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'%' && i + 2 < raw.len() {
            if let Some(v) = std::str::from_utf8(&raw[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                bytes.push(v);
                i += 3;
                continue;
            }
        }
        bytes.push(if raw[i] == b'/' { b'\\' } else { raw[i] });
        i += 1;
    }
    let text = String::from_utf8(bytes).ok()?;
    Some(PathBuf::from(if host.is_empty() { text } else { format!(r"\\{host}\{text}") }))
}

fn address_dir(name: &str) -> Option<PathBuf> {
    let text = name.trim();
    if let Some((_, rest)) = text.split_once(": ") {
        let path = PathBuf::from(rest.trim());
        if path.is_dir() {
            return Some(path);
        }
    }
    let path = PathBuf::from(text);
    if text.contains('\\') && path.is_dir() {
        return Some(path);
    }
    None
}

fn first_match(dirs: &[PathBuf], names: &[String]) -> Option<PathBuf> {
    if dirs.is_empty() {
        return None;
    }
    for name in names {
        let label = name.trim();
        if label.is_empty() || label.len() > 240 || label.contains('\\') || label.contains('/') || label.contains(':') {
            continue;
        }
        for dir in dirs {
            if let Some(path) = match_in_dir(dir, label) {
                return Some(path);
            }
        }
    }
    None
}

fn match_in_dir(dir: &std::path::Path, label: &str) -> Option<PathBuf> {
    let want = label.to_lowercase();
    let mut stem_hit = None;
    let mut stems = 0u32;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let fname = path.file_name()?.to_str()?.to_lowercase();
        if fname == want {
            return Some(path);
        }
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            if stem.to_lowercase() == want {
                stems += 1;
                stem_hit = Some(path);
            }
        }
    }
    if stems == 1 { stem_hit } else { None }
}

fn known_folder(id: &windows_core::GUID) -> Option<PathBuf> {
    unsafe {
        let p = SHGetKnownFolderPath(id, KNOWN_FOLDER_FLAG(0), None).ok()?;
        let text = p.to_string().ok()?;
        CoTaskMemFree(Some(p.0 as *const _));
        Some(PathBuf::from(text))
    }
}

fn open_wheel(x: i32, y: i32, mode: Mode, paths: Vec<PathBuf>, chord: bool) {
    unsafe {
        let instance = GetModuleHandleW(None).unwrap_or_default();
        let class = w!("CedroWheel");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wheel_proc),
            hInstance: instance.into(),
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);
        let (left, top) = clamp_origin(x - WHEEL / 2, y - WHEEL / 2);
        let hwnd = match CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
            class,
            w!("Cedro"),
            WS_POPUP,
            left,
            top,
            WHEEL,
            WHEEL,
            None,
            None,
            Some(instance.into()),
            None,
        ) {
            Ok(hwnd) => hwnd,
            Err(e) => {
                append_log(&format!("ruota non aperta: {e}"));
                return;
            }
        };
        let target: IDropTarget = CedroDrop { hwnd: hwnd.0 as isize }.into();
        SESSION.with(|s| {
            let mut g = s.borrow_mut();
            g.wheel = hwnd;
            g.anchor_x = left + WHEEL / 2;
            g.anchor_y = top + WHEEL / 2;
            g.mode = mode;
            g.hover = -1;
            g.paths = paths;
            g.chord = chord;
            g.armed = true;
            g.target = Some(target.clone());
        });
        if let Err(e) = RegisterDragDrop(hwnd, &target) {
            append_log(&format!("drop non registrato: {e}"));
        }
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        paint_wheel(hwnd);
    }
}

fn clamp_origin(x: i32, y: i32) -> (i32, i32) {
    unsafe {
        let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let vw = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let vh = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        let x = x.clamp(vx, vx + vw - WHEEL);
        let y = y.clamp(vy, vy + vh - WHEEL);
        (x, y)
    }
}

fn refresh_wheel() {
    SESSION.with(|s| {
        let hwnd = s.borrow().wheel;
        if hwnd.0.is_null() {
            return;
        }
        paint_wheel(hwnd);
    });
}

fn close_wheel() {
    let (hwnd, target_hwnd) = SESSION.with(|s| {
        let mut g = s.borrow_mut();
        let hwnd = g.wheel;
        g.wheel = HWND::default();
        g.armed = false;
        g.chord = false;
        g.rdown = false;
        g.rmoved = false;
        g.paths.clear();
        g.hover = -1;
        g.target = None;
        (hwnd, hwnd)
    });
    if target_hwnd.0.is_null() {
        return;
    }
    unsafe {
        let msg = HWND(HMSG.load(Ordering::Relaxed) as *mut _);
        let _ = KillTimer(Some(msg), 9);
        let _ = RevokeDragDrop(hwnd);
        let _ = DestroyWindow(hwnd);
    }
}

fn on_drop() {
    let job = SESSION.with(|s| {
        let g = s.borrow();
        (g.paths.clone(), g.hover, g.mode, g.quality)
    });
    let (all, hover, mode, quality) = job;
    close_wheel();
    if all.is_empty() || hover < 0 {
        return;
    }
    let kind = kind_of(&all[0]);
    let list = slices(kind, mode);
    let Some(slice) = list.get(hover as usize) else {
        return;
    };
    if slice.id == "none" {
        balloon("Niente da fare su questo file");
        return;
    }
    if slice.id == "path" {
        let text = all.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\r\n");
        copy_text(&text);
        balloon(&text);
        return;
    }
    let paths = if matches!(slice.id, "dup" | "zip") { all.clone() } else { same_kind_paths(&all) };
    let skipped = all.len().saturating_sub(paths.len());
    let mut notes = Vec::new();
    for path in &paths {
        match perform(path, slice.id, quality) {
            Ok(out) => notes.push(format!("{} · {}", out.path.file_name().and_then(|s| s.to_str()).unwrap_or("file"), out.message)),
            Err(e) => notes.push(e.0),
        }
    }
    if skipped > 0 {
        notes.push(format!("{skipped} file di altro tipo lasciati stare"));
    }
    let summary = if notes.len() == 1 {
        notes.remove(0)
    } else {
        format!("{} file\n{}", notes.len(), notes.join("\n"))
    };
    append_log(&summary);
    if read_settings().notify {
        balloon(&summary);
    }
}

fn current_slices() -> (Vec<Slice>, i32, Mode, String) {
    SESSION.with(|s| {
        let g = s.borrow();
        let kind = g.paths.first().map(|p| kind_of(p)).unwrap_or(Kind::Other);
        let label = g
            .paths
            .first()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("in attesa")
            .to_string();
        (slices(kind, g.mode).to_vec(), g.hover, g.mode, label)
    })
}

unsafe extern "system" fn wheel_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_PAINT {
        paint_wheel(hwnd);
        let _ = ValidateRect(Some(hwnd), None);
        return LRESULT(0);
    }
    if msg == WM_ERASEBKGND {
        return LRESULT(1);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

unsafe fn dib(dc: HDC) -> Option<(HBITMAP, *mut u8)> {
    let mut info = BITMAPINFO::default();
    info.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: WHEEL,
        biHeight: -WHEEL,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };
    let mut bits = std::ptr::null_mut();
    let bmp = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
    Some((bmp, bits as *mut u8))
}

/// Il testo si disegna in bianco su nero con GDI; la luminosità diventa l'alfa del colore voluto.
unsafe fn text_layer(mem: HDC, mask: *mut u8, px: &mut [u8], color: (u8, u8, u8), draw: impl FnOnce()) {
    let len = px.len();
    std::ptr::write_bytes(mask, 0, len);
    SetBkMode(mem, TRANSPARENT);
    SetTextColor(mem, COLORREF(0x00FFFFFF));
    draw();
    let mask = std::slice::from_raw_parts(mask, len);
    for (dst, src) in px.chunks_exact_mut(4).zip(mask.chunks_exact(4)) {
        let a = src[1] as u32;
        if a == 0 {
            continue;
        }
        let k = 255 - a;
        dst[0] = ((color.2 as u32 * a + dst[0] as u32 * k) / 255) as u8;
        dst[1] = ((color.1 as u32 * a + dst[1] as u32 * k) / 255) as u8;
        dst[2] = ((color.0 as u32 * a + dst[2] as u32 * k) / 255) as u8;
        dst[3] = (a + dst[3] as u32 * k / 255) as u8;
    }
}

fn paint_wheel(hwnd: HWND) {
    let (list, hover, mode, name) = current_slices();
    let mut px = wheelgfx::render(list.len(), hover);
    unsafe {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let (Some((mask_bmp, mask)), Some((out_bmp, out))) = (dib(screen), dib(screen)) else {
            let _ = DeleteDC(mem);
            ReleaseDC(None, screen);
            return;
        };
        let old = SelectObject(mem, mask_bmp.into());
        let c = WHEEL as f32 / 2.0;
        let n = list.len().max(1);
        let slice_px = if n > 6 { -16 } else if n > 4 { -17 } else { -19 };
        let radius = (R0 + R1) / 2.0;
        let labels = |on: bool| {
            let font = select_font(mem, slice_px, 700);
            for (i, slice) in list.iter().enumerate() {
                if (i as i32 == hover) != on {
                    continue;
                }
                let (a0, a1) = wheelgeom_bounds(i, n);
                let mid = (a0 + a1) / 2.0;
                text_centered(mem, (c + radius * mid.cos()) as i32, (c + radius * mid.sin()) as i32, slice.label);
            }
            restore_font(mem, font);
        };
        text_layer(mem, mask, &mut px, wheelgfx::TEXT, || labels(false));
        text_layer(mem, mask, &mut px, wheelgfx::TEXT_ON_HOVER, || labels(true));
        let title = match mode {
            Mode::Convert => "CONVERSIONE",
            Mode::Tools => "STRUMENTI",
            Mode::Actions => "AZIONI",
        };
        let center = if hover >= 0 {
            list.get(hover as usize).map(|s| s.label).unwrap_or("Annulla")
        } else {
            "Annulla"
        };
        let (cx, cy) = (WHEEL / 2, WHEEL / 2);
        let line = |dy: i32, size: i32, weight: i32, text: &str| {
            let font = select_font(mem, size, weight);
            text_centered(mem, cx, cy + dy, text);
            restore_font(mem, font);
        };
        text_layer(mem, mask, &mut px, wheelgfx::MUTED, || line(-20, -12, 700, title));
        text_layer(mem, mask, &mut px, wheelgfx::NAVY, || line(2, -18, 700, center));
        text_layer(mem, mask, &mut px, wheelgfx::MUTED, || line(24, -12, 400, &clip_name(&name)));
        std::ptr::copy_nonoverlapping(px.as_ptr(), out, px.len());
        SelectObject(mem, out_bmp.into());
        let size = SIZE { cx: WHEEL, cy: WHEEL };
        let origin = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: 1 };
        let _ = UpdateLayeredWindow(hwnd, Some(screen), None, Some(&size), Some(mem), Some(&origin), COLORREF(0), Some(&blend), ULW_ALPHA);
        SelectObject(mem, old);
        let _ = DeleteObject(mask_bmp.into());
        let _ = DeleteObject(out_bmp.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
    }
}

fn ui_font(height: i32, weight: i32) -> HFONT {
    unsafe {
        CreateFontW(
            height,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            ANTIALIASED_QUALITY,
            DEFAULT_PITCH.0 as u32 | FF_SWISS.0 as u32,
            crate::winfont::face(),
        )
    }
}

fn select_font(hdc: HDC, height: i32, weight: i32) -> HGDIOBJ {
    unsafe {
        let font = ui_font(height, weight);
        SelectObject(hdc, font.into())
    }
}

fn restore_font(hdc: HDC, previous: HGDIOBJ) {
    unsafe {
        let current = SelectObject(hdc, previous);
        let _ = DeleteObject(current);
    }
}

fn clip_name(name: &str) -> String {
    if name.chars().count() > 18 {
        format!("{}…", name.chars().take(16).collect::<String>())
    } else {
        name.to_string()
    }
}

fn text_at(hdc: HDC, x: i32, y: i32, text: &str) {
    let wide: Vec<u16> = text.encode_utf16().collect();
    unsafe {
        let _ = TextOutW(hdc, x, y, &wide);
    }
}

fn text_centered(hdc: HDC, cx: i32, cy: i32, text: &str) {
    let wide: Vec<u16> = text.encode_utf16().collect();
    if wide.is_empty() {
        return;
    }
    unsafe {
        let mut size = SIZE::default();
        let _ = GetTextExtentPoint32W(hdc, &wide, &mut size);
        text_at(hdc, cx - size.cx / 2, cy - size.cy / 2, text);
    }
}

fn hit(dx: f32, dy: f32, n: i32) -> i32 {
    if n <= 0 {
        return -1;
    }
    let dist = (dx * dx + dy * dy).sqrt();
    if dist < R0 || dist > R1 + 28.0 {
        return -1;
    }
    let mut from_top = dy.atan2(dx) + std::f32::consts::FRAC_PI_2;
    while from_top < 0.0 {
        from_top += std::f32::consts::TAU;
    }
    while from_top >= std::f32::consts::TAU {
        from_top -= std::f32::consts::TAU;
    }
    ((from_top / std::f32::consts::TAU) * n as f32) as i32
}

fn set_hover(x: i32, y: i32) {
    SESSION.with(|s| {
        let mut g = s.borrow_mut();
        if g.wheel.0.is_null() {
            return;
        }
        let n = {
            let kind = g.paths.first().map(|p| kind_of(p)).unwrap_or(Kind::Other);
            slices(kind, g.mode).len() as i32
        };
        let hover = hit((x - g.anchor_x) as f32, (y - g.anchor_y) as f32, n.max(1));
        if hover != g.hover {
            g.hover = hover;
            let hwnd = g.wheel;
            drop(g);
            paint_wheel(hwnd);
        }
    });
}

fn remember_paths(paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    SESSION.with(|s| {
        let mut g = s.borrow_mut();
        if g.paths.is_empty() {
            g.paths = paths;
            let hwnd = g.wheel;
            drop(g);
            paint_wheel(hwnd);
        }
    });
}

#[implement(IDropTarget)]
struct CedroDrop {
    hwnd: isize,
}

impl IDropTarget_Impl for CedroDrop_Impl {
    fn DragEnter(
        &self,
        pdataobj: Ref<IDataObject>,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> Result<()> {
        unsafe { *pdweffect = DROPEFFECT_COPY };
        if let Some(data) = pdataobj.as_ref() {
            if let Ok(paths) = paths_from(data) {
                remember_paths(paths);
            }
        }
        set_hover(pt.x, pt.y);
        Ok(())
    }

    fn DragOver(&self, _grfkeystate: MODIFIERKEYS_FLAGS, pt: &POINTL, pdweffect: *mut DROPEFFECT) -> Result<()> {
        unsafe { *pdweffect = DROPEFFECT_COPY };
        set_hover(pt.x, pt.y);
        Ok(())
    }

    fn DragLeave(&self) -> Result<()> {
        Ok(())
    }

    fn Drop(
        &self,
        pdataobj: Ref<IDataObject>,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> Result<()> {
        unsafe { *pdweffect = DROPEFFECT_COPY };
        if let Some(data) = pdataobj.as_ref() {
            if let Ok(paths) = paths_from(data) {
                remember_paths(paths);
            }
        }
        set_hover(pt.x, pt.y);
        let hwnd = HWND(HMSG.load(Ordering::Relaxed) as *mut _);
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_APP_DROP, WPARAM(0), LPARAM(self.hwnd));
        }
        Ok(())
    }
}

fn paths_from(data: &IDataObject) -> std::result::Result<Vec<PathBuf>, windows::core::Error> {
    unsafe {
        let format = FORMATETC {
            cfFormat: CF_HDROP.0,
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0 as u32,
        };
        let mut medium = data.GetData(&format)?;
        let hdrop = HDROP(medium.u.hGlobal.0);
        let count = DragQueryFileW(hdrop, 0xFFFF_FFFF, None);
        let mut paths = Vec::new();
        for i in 0..count {
            let len = DragQueryFileW(hdrop, i, None) as usize;
            let mut buf = vec![0u16; len + 1];
            DragQueryFileW(hdrop, i, Some(&mut buf));
            if let Some(0) = buf.last() {
                buf.pop();
            }
            paths.push(PathBuf::from(String::from_utf16_lossy(&buf)));
        }
        ReleaseStgMedium(&mut medium);
        Ok(paths)
    }
}

fn copy_text(text: &str) {
    unsafe {
        if OpenClipboard(None).is_err() {
            return;
        }
        let _ = EmptyClipboard();
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        wide.push(0);
        let bytes = wide.len() * 2;
        if let Ok(h) = GlobalAlloc(GMEM_MOVEABLE, bytes) {
            let p = GlobalLock(h);
            if !p.is_null() {
                std::ptr::copy_nonoverlapping(wide.as_ptr(), p as *mut u16, wide.len());
                let _ = GlobalUnlock(h);
                let _ = SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(h.0)));
            }
        }
        let _ = CloseClipboard();
    }
}

fn balloon(text: &str) {
    unsafe {
        let mut nid = NOTIFYICONDATAW::default();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = HWND(HMSG.load(Ordering::Relaxed) as *mut _);
        nid.uID = 1;
        nid.uFlags = NIF_INFO;
        nid.dwInfoFlags = NIIF_INFO;
        copy_wide(&mut nid.szInfoTitle, "Cedro");
        copy_wide(&mut nid.szInfo, &text.chars().take(180).collect::<String>());
        let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
}

fn add_tray(hwnd: HWND) -> std::result::Result<(), String> {
    unsafe {
        let icon = SESSION.with(|s| s.borrow().icon);
        let mut nid = NOTIFYICONDATAW::default();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY;
        nid.hIcon = icon;
        copy_wide(&mut nid.szTip, "Cedro");
        Shell_NotifyIconW(NIM_ADD, &nid).ok().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn remove_tray(hwnd: HWND) {
    unsafe {
        let mut nid = NOTIFYICONDATAW::default();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

fn show_menu(hwnd: HWND) {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        let version = HSTRING::from(format!("Cedro {}", update::CURRENT));
        let _ = AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, &version);
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, 5, w!("Impostazioni…"));
        let _ = AppendMenuW(menu, MF_STRING, 3, w!("Controlla aggiornamenti"));
        let check = if autostart_enabled() { MF_CHECKED } else { MF_UNCHECKED };
        let _ = AppendMenuW(menu, MF_STRING | check, 4, w!("Avvio automatico"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, 1, w!("Esci"));
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON, pt.x, pt.y, Some(0), hwnd, None);
        let _ = DestroyMenu(menu);
    }
}

/// All'avvio aspetta qualche secondo e tace se non c'è niente. Dal menu risponde sempre.
fn check_updates(manual: bool) {
    std::thread::spawn(move || {
        if !manual {
            std::thread::sleep(std::time::Duration::from_secs(8));
        }
        let news = match update::check() {
            Ok(Some(release)) if manual || !update::skipped(&release.version) => UpdateNews::Found { release, manual },
            Ok(_) if manual => UpdateNews::Note(format!("Hai già l'ultima versione ({}).", update::CURRENT)),
            Err(e) if manual => UpdateNews::Note(e),
            Err(e) => {
                append_log(&format!("aggiornamenti: {e}"));
                return;
            }
            Ok(_) => return,
        };
        post_update(news);
    });
}

fn post_update(news: UpdateNews) {
    if let Ok(mut slot) = UPDATE.lock() {
        *slot = Some(news);
    }
    unsafe {
        let hwnd = HWND(HMSG.load(Ordering::Relaxed) as *mut _);
        let _ = PostMessageW(Some(hwnd), WM_APP_UPDATE, WPARAM(0), LPARAM(0));
    }
}

fn on_update(hwnd: HWND) {
    let Some(news) = UPDATE.lock().ok().and_then(|mut slot| slot.take()) else { return };
    let (release, manual) = match news {
        UpdateNews::Note(text) => return balloon(&text),
        UpdateNews::Found { release, manual } => (release, manual),
    };
    let text = format!(
        "È disponibile Cedro {}. Hai la {}.\n\nVuoi aggiornare adesso?",
        release.version,
        update::CURRENT
    );
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let answer = unsafe {
        let _ = SetForegroundWindow(hwnd);
        MessageBoxW(Some(hwnd), PCWSTR(wide.as_ptr()), w!("Cedro"), MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2 | MB_SETFOREGROUND)
    };
    if answer != IDYES {
        if !manual {
            update::skip(&release.version);
        }
        return;
    }
    balloon(&format!("Scarico Cedro {}…", release.version));
    std::thread::spawn(move || {
        // L'installatore chiude Cedro da solo prima di copiare i file.
        if let Err(e) = crate::cli::install(&release) {
            append_log(&format!("aggiornamento: {e}"));
            post_update(UpdateNews::Note(format!("Aggiornamento non riuscito: {e}")));
        }
    });
}

pub(crate) fn autostart_enabled() -> bool {
    unsafe { RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, w!("Cedro"), RRF_RT_REG_SZ, None, None, None).is_ok() }
}

/// La stessa voce che scrive l'installatore quando si spunta l'avvio con Windows.
pub(crate) fn set_autostart(on: bool) -> std::result::Result<(), String> {
    unsafe {
        let result = if on {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let value: Vec<u16> =
                format!("\"{}\"", exe.display()).encode_utf16().chain(std::iter::once(0)).collect();
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                w!("Cedro"),
                REG_SZ.0,
                Some(value.as_ptr() as *const _),
                (value.len() * 2) as u32,
            )
        } else {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, w!("Cedro"))
        };
        result.ok().map_err(|e| e.to_string())
    }
}

fn copy_wide(dest: &mut [u16], text: &str) {
    dest.fill(0);
    for (i, unit) in text.encode_utf16().take(dest.len().saturating_sub(1)).enumerate() {
        dest[i] = unit;
    }
}

fn make_icon() -> windows::core::Result<HICON> {
    unsafe {
        let mut bits = vec![0u8; 32 * 32 * 4];
        for y in 0..32 {
            for x in 0..32 {
                let dx = x as f32 - 15.5;
                let dy = y as f32 - 15.5;
                let d = (dx * dx + dy * dy).sqrt();
                let i = (y * 32 + x) * 4;
                let (b, g, r) = if (7.0..11.5).contains(&d) {
                    (0x5A, 0xE1, 0xD5)
                } else {
                    (0x0F, 0x16, 0x14)
                };
                bits[i] = b;
                bits[i + 1] = g;
                bits[i + 2] = r;
                bits[i + 3] = 255;
            }
        }
        let color = CreateBitmap(32, 32, 1, 32, Some(bits.as_ptr() as _));
        let mask_bits = vec![0u8; 32 * 4];
        let mask = CreateBitmap(32, 32, 1, 1, Some(mask_bits.as_ptr() as _));
        let info = ICONINFO {
            fIcon: TRUE,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        CreateIconIndirect(&info)
    }
}
