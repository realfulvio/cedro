//! Finestra «Guida» di Windows: mostra il disegno di `cedro::ui::guida` (lo stesso di Linux, nei colori e nel
//! carattere del tema scelto) e le passa tempo, mouse e tastiera. Qui c'è solo la finestra.

use cedro::ui::gfx::{Fonts, GlyphCache, Painter};
use cedro::ui::guida::{Guida, Out, H, W};
use cedro::ui::screen::Key;
use std::cell::RefCell;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::Gdi::*,
    Win32::System::LibraryLoader::GetModuleHandleW,
    Win32::UI::HiDpi::GetDpiForSystem,
    Win32::UI::Input::KeyboardAndMouse::*,
    Win32::UI::WindowsAndMessaging::*,
};

const TIMER: usize = 1;
const FRAME_MS: u32 = 33;
const WM_MOUSELEAVE: u32 = 0x02A3;

static WINDOW: AtomicIsize = AtomicIsize::new(0);

struct State {
    guida: Guida,
    cache: GlyphCache,
    tracking: bool,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn now() -> u64 {
    unsafe { GetTickCount64() }
}

#[link(name = "kernel32")]
extern "system" {
    fn GetTickCount64() -> u64;
}

fn px(v: f32, dpi: u32) -> i32 {
    (v * dpi as f32 / 96.0).round() as i32
}

fn scale() -> f32 {
    unsafe { GetDpiForSystem() as f32 / 96.0 }
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
            cedro::append_log(&format!("guida: {e}"));
        }
    }
}

/// `cedro.exe guida`: la finestra da sola, con il suo ciclo di messaggi (usata dall'installatore).
pub fn run_standalone() {
    crate::winfont::load();
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

unsafe fn create() -> Result<()> {
    // Le impostazioni dicono il tema (e accendono lo stemma del Comune se serve).
    let _ = cedro::settings::read_settings();
    let theme = cedro::edizione::tema();
    let instance: HINSTANCE = GetModuleHandleW(None)?.into();
    let class = w!("CedroGuida");
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(proc),
        hInstance: instance,
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        lpszClassName: class,
        ..Default::default()
    };
    RegisterClassExW(&wc);
    let dpi = GetDpiForSystem();
    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
    let mut rect = RECT { left: 0, top: 0, right: px(W, dpi), bottom: px(H, dpi) };
    let _ = AdjustWindowRect(&mut rect, style, false);
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    let x = (GetSystemMetrics(SM_CXSCREEN) - width) / 2;
    let y = (GetSystemMetrics(SM_CYSCREEN) - height) / 3;
    STATE.with(|s| *s.borrow_mut() = Some(State { guida: Guida::new(theme, now()), cache: GlyphCache::new(), tracking: false }));
    let hwnd = CreateWindowExW(WINDOW_EX_STYLE::default(), class, &HSTRING::from(cedro::i18n::tr("Guida di Cedro").as_ref()), style | WS_VISIBLE, x, y, width, height, None, None, Some(instance), None)?;
    WINDOW.store(hwnd.0 as isize, Ordering::Relaxed);
    SetTimer(Some(hwnd), TIMER, FRAME_MS, None);
    let _ = SetForegroundWindow(hwnd);
    Ok(())
}

unsafe fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let (w, h) = (rc.right.max(1), rc.bottom.max(1));
    STATE.with(|s| {
        if let Some(st) = s.borrow_mut().as_mut() {
            let theme = st.guida.theme;
            let k = w as f32 / W;
            let mut painter = Painter::new(w as u32, h as u32, k, Fonts::for_theme(theme), &mut st.cache);
            st.guida.draw(&mut painter, now());
            let data = painter.bgra();
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            SetDIBitsToDevice(hdc, 0, 0, w as u32, h as u32, 0, 0, 0, h as u32, data.as_ptr() as *const _, &bmi, DIB_RGB_COLORS);
        }
    });
    let _ = EndPaint(hwnd, &ps);
}

fn with<T>(f: impl FnOnce(&mut State) -> T) -> Option<T> {
    STATE.with(|s| s.borrow_mut().as_mut().map(f))
}

unsafe fn finish(hwnd: HWND, out: Out) {
    match out {
        Out::Close => {
            let _ = DestroyWindow(hwnd);
        }
        Out::Redraw => {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
        Out::None => {}
    }
}

fn xy(lparam: LPARAM) -> (f32, f32) {
    let d = scale();
    ((lparam.0 & 0xffff) as i16 as f32 / d, ((lparam.0 >> 16) & 0xffff) as i16 as f32 / d)
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_TIMER => {
            with(|st| st.guida.tick(now()));
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let (x, y) = xy(lparam);
            with(|st| {
                if !st.tracking {
                    let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                    let _ = TrackMouseEvent(&mut t);
                    st.tracking = true;
                }
                st.guida.pointer_move(x, y);
            });
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            with(|st| {
                st.tracking = false;
                st.guida.pointer_leave();
            });
            LRESULT(0)
        }
        WM_SETCURSOR => {
            if (lparam.0 & 0xffff) as u32 == HTCLIENT {
                let hand = with(|st| st.guida.is_hand()).unwrap_or(false);
                if let Ok(cur) = LoadCursorW(None, if hand { IDC_HAND } else { IDC_ARROW }) {
                    SetCursor(Some(cur));
                }
                return LRESULT(1);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = xy(lparam);
            let out = with(|st| st.guida.pointer_down(x, y, now())).unwrap_or(Out::None);
            finish(hwnd, out);
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let key = match VIRTUAL_KEY(wparam.0 as u16) {
                VK_ESCAPE => Some(Key::Escape),
                VK_RIGHT => Some(Key::Right),
                VK_LEFT => Some(Key::Left),
                VK_DOWN => Some(Key::Down),
                VK_UP => Some(Key::Up),
                VK_RETURN => Some(Key::Enter),
                VK_SPACE => Some(Key::Space),
                _ => None,
            };
            if let Some(k) = key {
                let out = with(|st| st.guida.key(k, now())).unwrap_or(Out::None);
                finish(hwnd, out);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let _ = KillTimer(Some(hwnd), TIMER);
            WINDOW.store(0, Ordering::Relaxed);
            STATE.with(|s| *s.borrow_mut() = None);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
