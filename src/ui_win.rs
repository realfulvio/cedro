//! La finestra di Windows per la nuova interfaccia (edizione standard). Fa solo il lavoro del
//! sistema: una finestra senza cornice di sistema (la barra del titolo la disegna `cedro::ui`),
//! copia l'immagine sullo schermo, passa mouse, tastiera e file trascinati, e offre a `Screen` i
//! dialoghi, gli appunti e i pulsanti della finestra. Tutto quello che si vede sta in `cedro::ui`.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Arc, Mutex};

use cedro::ui::screen::{Cursor, Key, Platform, Screen, Surface};
use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::Dwm::*,
    Win32::Graphics::Gdi::*,
    Win32::System::Com::*,
    Win32::System::DataExchange::*,
    Win32::System::LibraryLoader::GetModuleHandleW,
    Win32::System::Memory::*,
    Win32::System::Ole::*,
    Win32::System::SystemServices::MODIFIERKEYS_FLAGS,
    Win32::UI::HiDpi::*,
    Win32::UI::Input::KeyboardAndMouse::*,
    Win32::UI::Shell::Common::COMDLG_FILTERSPEC,
    Win32::UI::Shell::*,
    Win32::UI::WindowsAndMessaging::*,
};

const WM_MOUSELEAVE: u32 = 0x02A3;
const WM_WAKE: u32 = WM_APP + 21;
const WM_DRAG: u32 = WM_APP + 22;
const WM_DROPPED: u32 = WM_APP + 23;
const TIMER: usize = 77;

thread_local! {
    /// Un WM_WAKE è arrivato mentre lo schermo era occupato da una finestra di dialogo.
    static LATE_WAKE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
const ZOOM: f32 = cedro::ui::ZOOM;

static MAIN: AtomicIsize = AtomicIsize::new(0);
static POPUP: AtomicIsize = AtomicIsize::new(0);
static DROPPED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
/// Il formato da preselezionare quando i file arrivano dalla ruota.
static ACTION: Mutex<Option<&'static str>> = Mutex::new(None);

/// Una finestra: lo schermo comune e l'ultimo fotogramma (per ridisegnare mentre un dialogo è aperto).
struct Host {
    screen: RefCell<Screen>,
    frame: RefCell<Frame>,
    surface: Surface,
    tracking: std::cell::Cell<bool>,
    drop_target: RefCell<Option<IDropTarget>>,
}

#[derive(Default)]
struct Frame {
    w: i32,
    h: i32,
    bgra: Vec<u8>,
}

thread_local! {
    static HOSTS: RefCell<Vec<(isize, Rc<Host>)>> = const { RefCell::new(Vec::new()) };
}

fn host(hwnd: HWND) -> Option<Rc<Host>> {
    HOSTS.with(|h| h.borrow().iter().find(|(k, _)| *k == hwnd.0 as isize).map(|(_, v)| v.clone()))
}

fn dpi_of(hwnd: HWND) -> f32 {
    let d = unsafe { GetDpiForWindow(hwnd) };
    if d == 0 { 96.0 } else { d as f32 }
}

/// Pixel per unità di disegno.
fn scale_of(hwnd: HWND) -> f32 {
    dpi_of(hwnd) / 96.0 * ZOOM
}

struct Plat {
    hwnd: HWND,
}

impl Platform for Plat {
    fn pick_files(&mut self) -> Vec<PathBuf> {
        pick(self.hwnd, false, "Scegli i file")
    }

    fn pick_folder(&mut self, title: &str) -> Option<PathBuf> {
        pick(self.hwnd, true, title).into_iter().next()
    }

    fn open_path(&mut self, path: &Path) {
        unsafe {
            let p = HSTRING::from(path.as_os_str());
            // Un file si apre; con la cartella dentro che lo contiene si fa meglio con «seleziona».
            if path.is_file() {
                let args = HSTRING::from(format!("/select,\"{}\"", path.display()));
                let r = ShellExecuteW(Some(self.hwnd), w!("open"), w!("explorer.exe"), &args, PCWSTR::null(), SW_SHOWNORMAL);
                if r.0 as usize > 32 {
                    return;
                }
            }
            ShellExecuteW(Some(self.hwnd), w!("open"), &p, PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
        }
    }

    fn copy_text(&mut self, text: &str) {
        unsafe {
            if OpenClipboard(Some(self.hwnd)).is_err() {
                return;
            }
            let _ = EmptyClipboard();
            let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            if let Ok(mem) = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2) {
                let ptr = GlobalLock(mem) as *mut u16;
                if !ptr.is_null() {
                    std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
                    let _ = GlobalUnlock(mem);
                    // CF_UNICODETEXT = 13: la memoria passa al sistema.
                    let _ = SetClipboardData(13, Some(HANDLE(mem.0)));
                }
            }
            let _ = CloseClipboard();
        }
    }

    fn paste_text(&mut self) -> Option<String> {
        unsafe {
            if OpenClipboard(Some(self.hwnd)).is_err() {
                return None;
            }
            let mut out = None;
            if let Ok(h) = GetClipboardData(13) {
                let mem = HGLOBAL(h.0);
                let ptr = GlobalLock(mem) as *const u16;
                if !ptr.is_null() {
                    let mut len = 0;
                    while *ptr.add(len) != 0 {
                        len += 1;
                    }
                    out = Some(String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len)));
                    let _ = GlobalUnlock(mem);
                }
            }
            let _ = CloseClipboard();
            out
        }
    }

    fn open_guide(&mut self) {
        crate::guida_win::open();
    }

    fn open_settings(&mut self) {
        crate::settings_win::open();
    }

    fn minimize(&mut self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_MINIMIZE);
        }
    }

    fn toggle_maximize(&mut self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, if IsZoomed(self.hwnd).as_bool() { SW_RESTORE } else { SW_MAXIMIZE });
        }
    }

    fn close(&mut self) {
        unsafe {
            let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }

    fn open_full(&mut self) {
        unsafe {
            let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        open();
    }
}

fn pick(hwnd: HWND, folder: bool, title: &str) -> Vec<PathBuf> {
    let run = || -> Result<Vec<PathBuf>> {
        unsafe {
            let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
            let mut options = dialog.GetOptions()?;
            options |= FOS_FORCEFILESYSTEM;
            options |= if folder { FOS_PICKFOLDERS } else { FOS_ALLOWMULTISELECT };
            dialog.SetOptions(options)?;
            dialog.SetTitle(&HSTRING::from(title))?;
            if !folder {
                let all = HSTRING::from("Tutti i file");
                let star = HSTRING::from("*.*");
                let spec = [COMDLG_FILTERSPEC { pszName: PCWSTR(all.as_ptr()), pszSpec: PCWSTR(star.as_ptr()) }];
                dialog.SetFileTypes(&spec)?;
            }
            dialog.Show(Some(hwnd))?;
            let results = dialog.GetResults()?;
            let mut out = Vec::new();
            for i in 0..results.GetCount()? {
                let name = results.GetItemAt(i)?.GetDisplayName(SIGDN_FILESYSPATH)?;
                out.push(PathBuf::from(name.to_string()?));
                CoTaskMemFree(Some(name.0 as *const _));
            }
            Ok(out)
        }
    };
    run().unwrap_or_default()
}

#[implement(IDropTarget)]
struct Drop {
    hwnd: isize,
}

fn paths_from(data: &IDataObject) -> Vec<PathBuf> {
    unsafe {
        let format = FORMATETC { cfFormat: CF_HDROP.0, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0, lindex: -1, tymed: TYMED_HGLOBAL.0 as u32 };
        let Ok(mut medium) = data.GetData(&format) else { return Vec::new() };
        let hdrop = HDROP(medium.u.hGlobal.0);
        let count = DragQueryFileW(hdrop, 0xFFFF_FFFF, None);
        let mut paths = Vec::new();
        for i in 0..count {
            let len = DragQueryFileW(hdrop, i, None) as usize;
            let mut buf = vec![0u16; len + 1];
            DragQueryFileW(hdrop, i, Some(&mut buf));
            if buf.last() == Some(&0) {
                buf.pop();
            }
            paths.push(PathBuf::from(String::from_utf16_lossy(&buf)));
        }
        ReleaseStgMedium(&mut medium);
        paths
    }
}

#[allow(non_snake_case)]
impl IDropTarget_Impl for Drop_Impl {
    fn DragEnter(&self, _data: Ref<IDataObject>, _keys: MODIFIERKEYS_FLAGS, _pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        unsafe {
            *effect = DROPEFFECT_COPY;
            let _ = PostMessageW(Some(HWND(self.hwnd as *mut _)), WM_DRAG, WPARAM(1), LPARAM(0));
        }
        Ok(())
    }

    fn DragOver(&self, _keys: MODIFIERKEYS_FLAGS, _pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        unsafe { *effect = DROPEFFECT_COPY };
        Ok(())
    }

    fn DragLeave(&self) -> Result<()> {
        unsafe {
            let _ = PostMessageW(Some(HWND(self.hwnd as *mut _)), WM_DRAG, WPARAM(0), LPARAM(0));
        }
        Ok(())
    }

    fn Drop(&self, data: Ref<IDataObject>, _keys: MODIFIERKEYS_FLAGS, _pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        unsafe {
            *effect = DROPEFFECT_COPY;
            if let Some(d) = data.as_ref() {
                let paths = paths_from(d);
                DROPPED.lock().unwrap_or_else(|e| e.into_inner()).extend(paths);
            }
            let _ = PostMessageW(Some(HWND(self.hwnd as *mut _)), WM_DROPPED, WPARAM(0), LPARAM(0));
        }
        Ok(())
    }
}

/// Apre il pannello del tema scelto, oppure porta davanti quello già aperto.
pub fn open() {
    open_surface(Surface::Main);
}

/// Apre la finestra principale con questi file in coda e, se c'è, il formato già scelto (dalla ruota).
pub fn open_with(paths: Vec<PathBuf>, action: Option<&'static str>) {
    DROPPED.lock().unwrap_or_else(|e| e.into_inner()).extend(paths);
    *ACTION.lock().unwrap_or_else(|e| e.into_inner()) = action;
    open_surface(Surface::Main);
    unsafe {
        let main = HWND(MAIN.load(Ordering::Relaxed) as *mut _);
        if !main.0.is_null() {
            let _ = PostMessageW(Some(main), WM_DROPPED, WPARAM(0), LPARAM(0));
        }
    }
}

/// La finestrella compatta (dall'icona vicino all'orologio).
pub fn open_compact() {
    open_surface(Surface::Compact);
}

fn slot(surface: Surface) -> &'static AtomicIsize {
    match surface {
        Surface::Main => &MAIN,
        Surface::Compact => &POPUP,
    }
}

fn open_surface(surface: Surface) {
    unsafe {
        let existing = HWND(slot(surface).load(Ordering::Relaxed) as *mut _);
        if !existing.0.is_null() && IsWindow(Some(existing)).as_bool() {
            let _ = ShowWindow(existing, SW_RESTORE);
            let _ = SetForegroundWindow(existing);
            return;
        }
        if let Err(e) = create(surface) {
            cedro::append_log(&format!("interfaccia: {e}"));
        }
    }
}

unsafe fn create(surface: Surface) -> Result<()> {
    let instance: HINSTANCE = GetModuleHandleW(None)?.into();
    let class = w!("CedroUi");
    let icon = LoadIconW(Some(instance), PCWSTR(1usize as *const u16)).unwrap_or_default();
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_DBLCLKS,
        lpfnWndProc: Some(proc),
        hInstance: instance,
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        hIcon: icon,
        hIconSm: icon,
        lpszClassName: class,
        ..Default::default()
    };
    RegisterClassExW(&wc);
    let wake_target = Arc::new(AtomicIsize::new(0));
    let wt = wake_target.clone();
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        let h = wt.load(Ordering::Relaxed);
        if h != 0 {
            let _ = PostMessageW(Some(HWND(h as *mut _)), WM_WAKE, WPARAM(0), LPARAM(0));
        }
    });
    let screen = Screen::new(surface, wake);
    let (pw, ph) = screen.preferred_size();
    let sys_dpi = GetDpiForSystem() as f32;
    let k = sys_dpi / 96.0 * ZOOM;
    let mut work = RECT::default();
    let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut work as *mut _ as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
    let (aw, ah) = (work.right - work.left, work.bottom - work.top);
    let width = ((pw * k) as i32).min((aw as f32 * 0.94) as i32);
    let height = ((ph * k) as i32).min((ah as f32 * 0.94) as i32);
    let (x, y) = match surface {
        // La finestrella sta vicino all'orologio, in basso a destra.
        Surface::Compact => (work.right - width - 12, work.bottom - height - 12),
        Surface::Main => (work.left + (aw - width) / 2, work.top + (ah - height) / 3),
    };
    let style = WS_POPUP | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_SYSMENU | WS_CLIPCHILDREN;
    let ex = if surface == Surface::Compact { WS_EX_TOOLWINDOW } else { WINDOW_EX_STYLE(0) };
    let hwnd = CreateWindowExW(ex, class, w!("Cedro"), style, x, y, width, height, None, None, Some(instance), None)?;
    wake_target.store(hwnd.0 as isize, Ordering::Relaxed);
    let host = Rc::new(Host {
        screen: RefCell::new(screen),
        frame: RefCell::new(Frame::default()),
        surface,
        tracking: std::cell::Cell::new(false),
        drop_target: RefCell::new(None),
    });
    HOSTS.with(|h| h.borrow_mut().push((hwnd.0 as isize, host.clone())));
    slot(surface).store(hwnd.0 as isize, Ordering::Relaxed);
    // L'ombra e gli angoli arrotondati di Windows 11 anche senza cornice.
    let margins = windows::Win32::UI::Controls::MARGINS { cxLeftWidth: 0, cxRightWidth: 0, cyTopHeight: 0, cyBottomHeight: 1 };
    let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
    // Fa ricalcolare la parte non client: senza, resterebbe la cornice di sistema.
    let _ = SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
    let target: IDropTarget = Drop { hwnd: hwnd.0 as isize }.into();
    if RegisterDragDrop(hwnd, &target).is_ok() {
        *host.drop_target.borrow_mut() = Some(target);
    }
    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);
    let _ = SetTimer(Some(hwnd), TIMER, 33, None);
    Ok(())
}

/// Disegna il fotogramma e lo copia sulla finestra.
unsafe fn paint(hwnd: HWND, host: &Host) {
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
    if w <= 0 || h <= 0 {
        return;
    }
    let scale = scale_of(hwnd);
    if let Ok(mut screen) = host.screen.try_borrow_mut() {
        let painter = screen.render(w as u32, h as u32, scale);
        let mut f = host.frame.borrow_mut();
        f.w = w;
        f.h = h;
        f.bgra = painter.bgra();
    }
    let f = host.frame.borrow();
    if f.bgra.is_empty() || f.w != w || f.h != h {
        return;
    }
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: f.w,
            biHeight: -f.h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    SetDIBitsToDevice(hdc, 0, 0, f.w as u32, f.h as u32, 0, 0, 0, f.h as u32, f.bgra.as_ptr() as *const _, &bmi, DIB_RGB_COLORS);
    let _ = EndPaint(hwnd, &ps);
}

fn redraw(hwnd: HWND) {
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

fn xy(lparam: LPARAM, hwnd: HWND) -> (f32, f32) {
    let s = scale_of(hwnd);
    let x = (lparam.0 & 0xffff) as i16 as f32;
    let y = ((lparam.0 >> 16) & 0xffff) as i16 as f32;
    (x / s, y / s)
}

fn shift() -> bool {
    unsafe { GetKeyState(VK_SHIFT.0 as i32) < 0 }
}

fn ctrl() -> bool {
    unsafe { GetKeyState(VK_CONTROL.0 as i32) < 0 }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let Some(host) = host(hwnd) else {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    };
    let mut plat = Plat { hwnd };
    match msg {
        WM_NCCALCSIZE if wparam.0 != 0 => {
            // Tutta la finestra è disegno nostro. Da ingrandita la cornice invisibile esce dallo
            // schermo: si toglie.
            if IsZoomed(hwnd).as_bool() {
                let p = &mut *(lparam.0 as *mut NCCALCSIZE_PARAMS);
                let d = dpi_of(hwnd) as u32;
                let fx = GetSystemMetricsForDpi(SM_CXSIZEFRAME, d) + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, d);
                let fy = GetSystemMetricsForDpi(SM_CYSIZEFRAME, d) + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, d);
                p.rgrc[0].left += fx;
                p.rgrc[0].right -= fx;
                p.rgrc[0].top += fy;
                p.rgrc[0].bottom -= fy;
            }
            LRESULT(0)
        }
        WM_NCHITTEST => {
            let mut pt = POINT { x: (lparam.0 & 0xffff) as i16 as i32, y: ((lparam.0 >> 16) & 0xffff) as i16 as i32 };
            let mut wr = RECT::default();
            let _ = GetWindowRect(hwnd, &mut wr);
            let d = dpi_of(hwnd) as u32;
            let b = GetSystemMetricsForDpi(SM_CXSIZEFRAME, d) + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, d) - 2;
            let b = b.max(5);
            if !IsZoomed(hwnd).as_bool() {
                let (l, r, t, bt) = (pt.x - wr.left < b, wr.right - pt.x <= b, pt.y - wr.top < b, wr.bottom - pt.y <= b);
                let code = match (l, r, t, bt) {
                    (true, _, true, _) => Some(HTTOPLEFT),
                    (_, true, true, _) => Some(HTTOPRIGHT),
                    (true, _, _, true) => Some(HTBOTTOMLEFT),
                    (_, true, _, true) => Some(HTBOTTOMRIGHT),
                    (true, ..) => Some(HTLEFT),
                    (_, true, ..) => Some(HTRIGHT),
                    (_, _, true, _) => Some(HTTOP),
                    (_, _, _, true) => Some(HTBOTTOM),
                    _ => None,
                };
                if let Some(c) = code {
                    return LRESULT(c as isize);
                }
            }
            let _ = ScreenToClient(hwnd, &mut pt);
            let s = scale_of(hwnd);
            if let Ok(screen) = host.screen.try_borrow() {
                if screen.caption_at(pt.x as f32 / s, pt.y as f32 / s) {
                    return LRESULT(HTCAPTION as isize);
                }
            }
            LRESULT(HTCLIENT as isize)
        }
        WM_GETMINMAXINFO => {
            let info = &mut *(lparam.0 as *mut MINMAXINFO);
            if let Ok(screen) = host.screen.try_borrow() {
                let (mw, mh) = screen.min_size();
                let s = scale_of(hwnd);
                info.ptMinTrackSize.x = (mw * s) as i32;
                info.ptMinTrackSize.y = (mh * s) as i32;
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            paint(hwnd, &host);
            LRESULT(0)
        }
        WM_SIZE => {
            if let Ok(mut screen) = host.screen.try_borrow_mut() {
                screen.ui.maximized = IsZoomed(hwnd).as_bool();
            }
            redraw(hwnd);
            LRESULT(0)
        }
        WM_DPICHANGED => {
            let r = &*(lparam.0 as *const RECT);
            let _ = SetWindowPos(hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
            redraw(hwnd);
            LRESULT(0)
        }
        WM_ACTIVATE if (wparam.0 & 0xffff) != 0 => {
            // Si torna dalle Impostazioni: il tema o la modalità possono essere cambiati.
            if let Ok(mut s) = host.screen.try_borrow_mut() {
                s.reload_appearance();
            }
            redraw(hwnd);
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == TIMER => {
            // Un avviso arrivato mentre un selettore di file teneva occupato lo schermo.
            if LATE_WAKE.with(|w| w.get()) {
                if let Ok(mut s) = host.screen.try_borrow_mut() {
                    LATE_WAKE.with(|w| w.set(false));
                    s.pump();
                    drop(s);
                    redraw(hwnd);
                }
            }
            let animating = host.screen.try_borrow().map(|s| s.animating()).unwrap_or(false);
            if animating {
                redraw(hwnd);
            }
            LRESULT(0)
        }
        WM_WAKE => {
            match host.screen.try_borrow_mut() {
                Ok(mut s) => {
                    s.pump();
                }
                // Un selettore di file è aperto: si riprende appena finisce (vedi il timer).
                Err(_) => LATE_WAKE.with(|w| w.set(true)),
            }
            redraw(hwnd);
            LRESULT(0)
        }
        WM_DRAG => {
            if let Ok(mut s) = host.screen.try_borrow_mut() {
                s.drag_over(wparam.0 != 0);
            }
            redraw(hwnd);
            LRESULT(0)
        }
        WM_DROPPED => {
            let paths: Vec<PathBuf> = std::mem::take(&mut *DROPPED.lock().unwrap_or_else(|e| e.into_inner()));
            let action = ACTION.lock().unwrap_or_else(|e| e.into_inner()).take();
            if let Ok(mut s) = host.screen.try_borrow_mut() {
                s.drop_files(paths);
                if let Some(id) = action {
                    s.app.set_action(id);
                }
            }
            redraw(hwnd);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if !host.tracking.get() {
                let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                let _ = TrackMouseEvent(&mut t);
                host.tracking.set(true);
            }
            let (x, y) = xy(lparam, hwnd);
            if let Ok(mut s) = host.screen.try_borrow_mut() {
                if s.pointer_move(x, y) {
                    redraw(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            host.tracking.set(false);
            if let Ok(mut s) = host.screen.try_borrow_mut() {
                if s.pointer_leave() {
                    redraw(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            if (lparam.0 & 0xffff) as u32 == HTCLIENT {
                let mut pt = POINT::default();
                let _ = GetCursorPos(&mut pt);
                let _ = ScreenToClient(hwnd, &mut pt);
                let s = scale_of(hwnd);
                let c = host.screen.try_borrow().map(|sc| sc.cursor_at(pt.x as f32 / s, pt.y as f32 / s)).unwrap_or(Cursor::Arrow);
                let id = match c {
                    Cursor::Arrow => IDC_ARROW,
                    Cursor::Hand => IDC_HAND,
                    Cursor::Text => IDC_IBEAM,
                };
                if let Ok(cur) = LoadCursorW(None, id) {
                    SetCursor(Some(cur));
                }
                return LRESULT(1);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_LBUTTONDOWN => {
            let _ = SetFocus(Some(hwnd));
            SetCapture(hwnd);
            let (x, y) = xy(lparam, hwnd);
            if let Ok(mut s) = host.screen.try_borrow_mut() {
                s.pointer_down(x, y);
            }
            redraw(hwnd);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let _ = ReleaseCapture();
            let (x, y) = xy(lparam, hwnd);
            if let Ok(mut s) = host.screen.try_borrow_mut() {
                s.pointer_up(x, y, &mut plat);
            }
            redraw(hwnd);
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let mut pt = POINT { x: (lparam.0 & 0xffff) as i16 as i32, y: ((lparam.0 >> 16) & 0xffff) as i16 as i32 };
            let _ = ScreenToClient(hwnd, &mut pt);
            let s = scale_of(hwnd);
            let delta = ((wparam.0 >> 16) & 0xffff) as i16 as f32 / 120.0;
            if let Ok(mut sc) = host.screen.try_borrow_mut() {
                if sc.wheel(pt.x as f32 / s, pt.y as f32 / s, -delta * 70.0) {
                    redraw(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            let vk = VIRTUAL_KEY(wparam.0 as u16);
            let key = match vk {
                VK_TAB => Some(if shift() { Key::ShiftTab } else { Key::Tab }),
                VK_RETURN => Some(Key::Enter),
                VK_SPACE => Some(Key::Space),
                VK_ESCAPE => Some(Key::Escape),
                VK_BACK => Some(Key::Backspace),
                VK_DELETE => Some(Key::Delete),
                VK_LEFT => Some(Key::Left),
                VK_RIGHT => Some(Key::Right),
                VK_HOME => Some(Key::Home),
                VK_END => Some(Key::End),
                VK_UP => Some(Key::Up),
                VK_DOWN => Some(Key::Down),
                VK_V if ctrl() => Some(Key::Paste),
                VK_C if ctrl() => Some(Key::Copy),
                VK_A if ctrl() => Some(Key::SelectAll),
                _ => None,
            };
            if let Some(k) = key {
                if let Ok(mut s) = host.screen.try_borrow_mut() {
                    let used = if k == Key::Enter && s.ui.menu.is_some() { s.menu_enter(&mut plat) } else { s.key(k, &mut plat) };
                    if used {
                        redraw(hwnd);
                        return LRESULT(0);
                    }
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_CHAR => {
            let c = wparam.0 as u32;
            if c >= 32 && c != 127 && !ctrl() {
                // Le lettere fuori dal piano base arrivano come due unità UTF-16.
                thread_local! { static HIGH: std::cell::Cell<u16> = const { std::cell::Cell::new(0) }; }
                let unit = c as u16;
                let text = if (0xD800..0xDC00).contains(&unit) {
                    HIGH.with(|h| h.set(unit));
                    return LRESULT(0);
                } else if (0xDC00..0xE000).contains(&unit) {
                    let high = HIGH.with(|h| h.replace(0));
                    String::from_utf16_lossy(&[high, unit])
                } else {
                    String::from_utf16_lossy(&[unit])
                };
                if let Ok(mut s) = host.screen.try_borrow_mut() {
                    s.text_input(&text);
                }
                redraw(hwnd);
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            let busy = host.screen.try_borrow().map(|s| s.app.busy()).unwrap_or(false);
            if busy {
                MessageBoxW(Some(hwnd), w!("Una conversione è ancora in corso: aspetta che finisca."), w!("Cedro"), MB_OK | MB_ICONINFORMATION);
            } else {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let _ = RevokeDragDrop(hwnd);
            let _ = KillTimer(Some(hwnd), TIMER);
            slot(host.surface).store(0, Ordering::Relaxed);
            HOSTS.with(|h| h.borrow_mut().retain(|(k, _)| *k != hwnd.0 as isize));
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// `cedro.exe conversioni`: il pannello da solo, con il suo ciclo di messaggi.
pub fn run_standalone() {
    crate::winfont::load();
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = OleInitialize(None);
    }
    open();
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if MAIN.load(Ordering::Relaxed) == 0 && POPUP.load(Ordering::Relaxed) == 0 {
                break;
            }
        }
    }
}
