//! La scheda «Aspetto» delle Impostazioni di Windows: una finestrella figlia che mostra la pagina
//! disegnata da `cedro::ui::aspetto` (la stessa di Linux) e le passa mouse, rotellina e frecce.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use cedro::ui::aspetto::Aspetto;
use cedro::ui::gfx::{Fonts, GlyphCache, Painter};
use cedro::ui::screen::Key;
use cedro::ui::theme::ThemeId;
use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::Gdi::*,
    Win32::System::LibraryLoader::GetModuleHandleW,
    Win32::UI::Input::KeyboardAndMouse::*,
    Win32::UI::WindowsAndMessaging::*,
};

const WM_MOUSELEAVE: u32 = 0x02A3;
const WM_PRONTA: u32 = WM_APP + 31;
const DLGC_WANTARROWS: u32 = 0x0001;

struct Host {
    page: RefCell<Aspetto>,
    cache: RefCell<GlyphCache>,
    dpi: u32,
    tracking: std::cell::Cell<bool>,
}

thread_local! {
    static HOSTS: RefCell<HashMap<isize, Rc<Host>>> = RefCell::new(HashMap::new());
}

fn host_of(hwnd: HWND) -> Option<Rc<Host>> {
    HOSTS.with(|h| h.borrow().get(&(hwnd.0 as isize)).cloned())
}

/// Crea la pagina dentro `parent`, nel rettangolo `r` (pixel: x, y, larghezza, altezza).
pub unsafe fn create(parent: HWND, id: i32, r: (i32, i32, i32, i32), dpi: u32, theme: ThemeId, simple: bool) -> HWND {
    let instance: HINSTANCE = GetModuleHandleW(None).unwrap_or_default().into();
    let class = w!("CedroAspetto");
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(proc),
        hInstance: instance,
        hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
        lpszClassName: class,
        ..Default::default()
    };
    RegisterClassExW(&wc);
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        class,
        w!(""),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        r.0,
        r.1,
        r.2,
        r.3,
        Some(parent),
        Some(HMENU(id as isize as *mut _)),
        Some(instance),
        None,
    )
    .unwrap_or_default();
    // Un'anteprima pronta (la prepara un altro thread) chiede il ridisegno con un messaggio.
    let raw = hwnd.0 as isize;
    let wake = Arc::new(move || {
        let _ = unsafe { PostMessageW(Some(HWND(raw as *mut _)), WM_PRONTA, WPARAM(0), LPARAM(0)) };
    });
    let mut page = Aspetto::new(theme, simple, wake);
    // Le Impostazioni di Windows sono nei colori del Comune: la pagina li segue.
    page.set_tema(&cedro::tema::BONDENO);
    let host = Rc::new(Host { page: RefCell::new(page), cache: RefCell::new(GlyphCache::new()), dpi, tracking: std::cell::Cell::new(false) });
    HOSTS.with(|h| h.borrow_mut().insert(raw, host));
    hwnd
}

/// Tema e modalità scelti.
pub fn value(hwnd: HWND) -> Option<(ThemeId, bool)> {
    host_of(hwnd).map(|h| {
        let p = h.page.borrow();
        (p.theme, p.simple)
    })
}

fn scale(h: &Host) -> f32 {
    h.dpi as f32 / 96.0
}

fn xy(lparam: LPARAM, h: &Host) -> (f32, f32) {
    let s = scale(h);
    ((lparam.0 & 0xffff) as i16 as f32 / s, ((lparam.0 >> 16) & 0xffff) as i16 as f32 / s)
}

unsafe fn paint(hwnd: HWND, h: &Host) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let (w, hh) = (rc.right.max(1), rc.bottom.max(1));
    let s = scale(h);
    let mut cache = h.cache.borrow_mut();
    let mut painter = Painter::new(w as u32, hh as u32, s, Fonts::shared(), &mut cache);
    h.page.borrow_mut().draw(&mut painter, w as f32 / s, hh as f32 / s);
    let data = painter.bgra();
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -hh,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    SetDIBitsToDevice(hdc, 0, 0, w as u32, hh as u32, 0, 0, 0, hh as u32, data.as_ptr() as *const _, &bmi, DIB_RGB_COLORS);
    let _ = EndPaint(hwnd, &ps);
}

fn redraw(hwnd: HWND) {
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let Some(host) = host_of(hwnd) else {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    };
    match msg {
        WM_PAINT => {
            paint(hwnd, &host);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_SIZE => {
            redraw(hwnd);
            LRESULT(0)
        }
        WM_PRONTA => {
            redraw(hwnd);
            LRESULT(0)
        }
        WM_GETDLGCODE => LRESULT(DLGC_WANTARROWS as isize),
        WM_MOUSEMOVE => {
            if !host.tracking.get() {
                let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                let _ = TrackMouseEvent(&mut t);
                host.tracking.set(true);
            }
            let (x, y) = xy(lparam, &host);
            if host.page.borrow_mut().pointer_move(x, y) {
                redraw(hwnd);
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            host.tracking.set(false);
            if host.page.borrow_mut().pointer_leave() {
                redraw(hwnd);
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            if (lparam.0 & 0xffff) as u32 == HTCLIENT {
                let hand = host.page.borrow().is_hand();
                if let Ok(cur) = LoadCursorW(None, if hand { IDC_HAND } else { IDC_ARROW }) {
                    SetCursor(Some(cur));
                }
                return LRESULT(1);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_LBUTTONDOWN => {
            let _ = SetFocus(Some(hwnd));
            let (x, y) = xy(lparam, &host);
            if host.page.borrow_mut().pointer_down(x, y) {
                redraw(hwnd);
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) & 0xffff) as i16 as f32 / 120.0;
            if host.page.borrow_mut().wheel(-delta * 70.0) {
                redraw(hwnd);
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let key = match VIRTUAL_KEY(wparam.0 as u16) {
                VK_LEFT => Some(Key::Left),
                VK_RIGHT => Some(Key::Right),
                VK_UP => Some(Key::Up),
                VK_DOWN => Some(Key::Down),
                _ => None,
            };
            if let Some(k) = key {
                if host.page.borrow_mut().key(k) {
                    redraw(hwnd);
                }
                return LRESULT(0);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_DESTROY => {
            HOSTS.with(|h| h.borrow_mut().remove(&(hwnd.0 as isize)));
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
