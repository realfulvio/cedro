use cedro::{append_log, config_dir, kind_of, mode_from_vk, perform, read_quality, slices, Kind, Mode, Slice};
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI32, AtomicIsize, Ordering};
use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::Gdi::*,
    Win32::System::Com::*,
    Win32::System::DataExchange::*,
    Win32::System::LibraryLoader::GetModuleHandleW,
    Win32::System::Memory::*,
    Win32::System::Ole::*,
    Win32::System::SystemServices::MODIFIERKEYS_FLAGS,
    Win32::System::Threading::CreateMutexW,
    Win32::UI::HiDpi::*,
    Win32::UI::Input::KeyboardAndMouse::*,
    Win32::UI::Shell::*,
    Win32::UI::WindowsAndMessaging::*,
};

const WM_TRAY: u32 = WM_APP + 1;
const WM_APP_INPUT: u32 = WM_APP + 2;
const WM_APP_DROP: u32 = WM_APP + 3;
const WHEEL: i32 = 440;
const R0: f32 = 58.0;
const R1: f32 = 168.0;

static HMSG: AtomicIsize = AtomicIsize::new(0);
static CUR_X: AtomicI32 = AtomicI32::new(0);
static CUR_Y: AtomicI32 = AtomicI32::new(0);

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
        armed: false,
        quality: 82,
        icon: HICON::default(),
        target: None,
    });
}

pub fn start() -> std::result::Result<(), String> {
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
        let kb = SetWindowsHookExW(WH_KEYBOARD_LL, Some(kb_proc), None, 0).map_err(|e| e.to_string())?;
        let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0).map_err(|e| e.to_string())?;
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
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
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            match (wparam.0 & 0xffff) as u32 {
                1 => {
                    let _ = DestroyWindow(hwnd);
                }
                2 => open_config_dir(),
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
        WM_TIMER => {
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
                if let Some(mode) = mode_from_vk(vk) {
                    g.mode = mode;
                    if g.dragging {
                        g.armed = true;
                        let have = !g.wheel.0.is_null();
                        drop(g);
                        if !have {
                            open_wheel(x, y, mode);
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
    SESSION.with(|s| {
        let mut g = s.borrow_mut();
        match flag {
            WM_LBUTTONDOWN => {
                g.down = true;
                g.dragging = false;
                g.down_x = x;
                g.down_y = y;
            }
            WM_MOUSEMOVE if g.down => {
                let dist = ((x - g.down_x) as i64).pow(2) + ((y - g.down_y) as i64).pow(2);
                if dist > 64 {
                    g.dragging = true;
                    if !g.armed {
                        if let Some(mode) = held_mode() {
                            g.mode = mode;
                            g.armed = true;
                            drop(g);
                            open_wheel(x, y, mode);
                        }
                    }
                }
            }
            WM_LBUTTONUP => {
                g.down = false;
                g.dragging = false;
                let armed = g.armed;
                drop(g);
                if armed {
                    unsafe {
                        let hwnd = HWND(HMSG.load(Ordering::Relaxed) as *mut _);
                        let _ = SetTimer(Some(hwnd), 9, 180, None);
                    }
                }
            }
            _ => {}
        }
    });
}

fn held_mode() -> Option<Mode> {
    unsafe {
        if GetAsyncKeyState(VK_SHIFT.0 as i32) < 0 {
            return Some(Mode::Convert);
        }
        if GetAsyncKeyState(VK_CONTROL.0 as i32) < 0 {
            return Some(Mode::Tools);
        }
        if GetAsyncKeyState(VK_MENU.0 as i32) < 0 {
            return Some(Mode::Actions);
        }
    }
    None
}

fn open_wheel(x: i32, y: i32, mode: Mode) {
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
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0x00FF00FF), 0, LWA_COLORKEY);
        let target: IDropTarget = CedroDrop { hwnd: hwnd.0 as isize }.into();
        SESSION.with(|s| {
            let mut g = s.borrow_mut();
            g.wheel = hwnd;
            g.anchor_x = left + WHEEL / 2;
            g.anchor_y = top + WHEEL / 2;
            g.mode = mode;
            g.hover = -1;
            g.paths.clear();
            g.armed = true;
            g.target = Some(target.clone());
        });
        if let Err(e) = RegisterDragDrop(hwnd, &target) {
            append_log(&format!("drop non registrato: {e}"));
        }
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
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
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    });
}

fn close_wheel() {
    let (hwnd, target_hwnd) = SESSION.with(|s| {
        let mut g = s.borrow_mut();
        let hwnd = g.wheel;
        g.wheel = HWND::default();
        g.armed = false;
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
    let (paths, hover, mode, quality) = job;
    close_wheel();
    if paths.is_empty() || hover < 0 {
        return;
    }
    let kind = kind_of(&paths[0]);
    let list = slices(kind, mode);
    let Some(slice) = list.get(hover as usize) else {
        return;
    };
    if slice.id == "none" {
        balloon("Niente da fare su questo file");
        return;
    }
    if slice.id == "path" {
        let text = paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\r\n");
        copy_text(&text);
        balloon(&text);
        return;
    }
    let mut notes = Vec::new();
    for path in &paths {
        match perform(path, slice.id, quality) {
            Ok(out) => notes.push(format!("{} · {}", out.path.file_name().and_then(|s| s.to_str()).unwrap_or("file"), out.message)),
            Err(e) => notes.push(e.0),
        }
    }
    let summary = if notes.len() == 1 {
        notes.remove(0)
    } else {
        format!("{} file\n{}", notes.len(), notes.join("\n"))
    };
    append_log(&summary);
    balloon(&summary);
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
        return LRESULT(0);
    }
    if msg == WM_ERASEBKGND {
        return LRESULT(1);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn paint_wheel(hwnd: HWND) {
    let (list, hover, mode, name) = current_slices();
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);
        let rect = RECT { left: 0, top: 0, right: WHEEL, bottom: WHEEL };
        let back = CreateSolidBrush(COLORREF(0x00FF00FF));
        FillRect(hdc, &rect, back);
        let _ = DeleteObject(back.into());
        let cx = WHEEL / 2;
        let cy = WHEEL / 2;
        let n = list.len().max(1);
        for (i, slice) in list.iter().enumerate() {
            let sweep = std::f32::consts::TAU / n as f32;
            let a0 = -std::f32::consts::FRAC_PI_2 + i as f32 * sweep + 0.03;
            let a1 = a0 + sweep - 0.06;
            let color = if i as i32 == hover { COLORREF(0x005AE1D5) } else { COLORREF(0x001C2823) };
            let brush = CreateSolidBrush(color);
            let pen = CreatePen(PS_SOLID, 1, COLORREF(0x000F1614));
            let old_b = SelectObject(hdc, brush.into());
            let old_p = SelectObject(hdc, pen.into());
            let pts = slice_poly(cx, cy, a0, a1);
            let _ = Polygon(hdc, &pts);
            SelectObject(hdc, old_b);
            SelectObject(hdc, old_p);
            let _ = DeleteObject(brush.into());
            let _ = DeleteObject(pen.into());
            let mid = (a0 + a1) / 2.0;
            let rr = (R0 + R1) / 2.0;
            let tx = cx as f32 + rr * mid.cos();
            let ty = cy as f32 + rr * mid.sin();
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, if i as i32 == hover { COLORREF(0x000F1614) } else { COLORREF(0x00DCEAE7) });
            text_at(hdc, tx as i32 - 28, ty as i32 - 8, slice.label);
        }
        let hole = CreateSolidBrush(COLORREF(0x000F1614));
        let old = SelectObject(hdc, hole.into());
        let _ = Ellipse(hdc, cx - R0 as i32 + 6, cy - R0 as i32 + 6, cx + R0 as i32 - 6, cy + R0 as i32 - 6);
        SelectObject(hdc, old);
        let _ = DeleteObject(hole.into());
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, COLORREF(0x00DCEAE7));
        let title = match mode {
            Mode::Convert => "CONVERSIONE",
            Mode::Tools => "STRUMENTI",
            Mode::Actions => "AZIONI",
        };
        text_at(hdc, cx - 42, cy - 18, title);
        let center = if hover >= 0 { list.get(hover as usize).map(|s| s.label).unwrap_or("Annulla") } else { "Annulla" };
        text_at(hdc, cx - 36, cy + 2, center);
        text_at(hdc, cx - 50, cy + 20, &clip_name(&name));
        let _ = EndPaint(hwnd, &ps);
    }
}

fn clip_name(name: &str) -> String {
    if name.chars().count() > 18 {
        format!("{}…", name.chars().take(16).collect::<String>())
    } else {
        name.to_string()
    }
}

fn slice_poly(cx: i32, cy: i32, a0: f32, a1: f32) -> Vec<POINT> {
    let mut pts = Vec::new();
    let steps = 10;
    for i in 0..=steps {
        let t = a0 + (a1 - a0) * (i as f32 / steps as f32);
        pts.push(POINT {
            x: cx + (R1 * t.cos()) as i32,
            y: cy + (R1 * t.sin()) as i32,
        });
    }
    for i in (0..=steps).rev() {
        let t = a0 + (a1 - a0) * (i as f32 / steps as f32);
        pts.push(POINT {
            x: cx + (R0 * t.cos()) as i32,
            y: cy + (R0 * t.sin()) as i32,
        });
    }
    pts
}

fn text_at(hdc: HDC, x: i32, y: i32, text: &str) {
    let wide: Vec<u16> = text.encode_utf16().collect();
    unsafe {
        let _ = TextOutW(hdc, x, y, &wide);
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
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
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
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
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
        let _ = AppendMenuW(menu, MF_STRING, 2, w!("Apri cartella di Cedro"));
        let _ = AppendMenuW(menu, MF_STRING, 1, w!("Esci"));
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON, pt.x, pt.y, Some(0), hwnd, None);
        let _ = DestroyMenu(menu);
    }
}

fn open_config_dir() {
    let dir = config_dir();
    let _ = std::fs::create_dir_all(&dir);
    unsafe {
        let wide: Vec<u16> = dir.display().to_string().encode_utf16().chain(std::iter::once(0)).collect();
        let _ = ShellExecuteW(None, w!("open"), PCWSTR(wide.as_ptr()), None, None, SW_SHOWNORMAL);
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
