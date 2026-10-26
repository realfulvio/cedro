//! Finestra «Come vuoi usare Cedro?»: due anteprime animate (Completa / Semplice), si sceglie qui (vedi `cedro::vista`).

use crate::guida_win::Gdi;
use cedro::vista::{self, Choice, Hit};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Mutex;
use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::Gdi::*,
    Win32::System::LibraryLoader::GetModuleHandleW,
    Win32::UI::HiDpi::GetDpiForSystem,
    Win32::UI::WindowsAndMessaging::*,
};

const TIMER: usize = 1;
static WINDOW: AtomicIsize = AtomicIsize::new(0);
static CHOSEN: Mutex<Choice> = Mutex::new(Choice::Simple);

#[link(name = "kernel32")]
extern "system" {
    fn GetTickCount64() -> u64;
}

fn px(v: i32, dpi: u32) -> i32 {
    (v as i64 * dpi as i64 / 96) as i32
}

/// `cedro.exe vista`: la finestra da sola (lanciata dall'installatore a fine installazione).
pub fn run_standalone() {
    crate::winfont::load();
    unsafe {
        if let Err(e) = create() {
            cedro::append_log(&format!("vista: {e}"));
            return;
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if WINDOW.load(Ordering::Relaxed) == 0 {
                break;
            }
        }
    }
}

unsafe fn create() -> Result<()> {
    let instance: HINSTANCE = GetModuleHandleW(None)?.into();
    let class = w!("CedroVista");
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
    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU;
    let mut rect = RECT { left: 0, top: 0, right: px(vista::W as i32, dpi), bottom: px(vista::H as i32, dpi) };
    let _ = AdjustWindowRect(&mut rect, style, false);
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    let x = (GetSystemMetrics(SM_CXSCREEN) - width) / 2;
    let y = (GetSystemMetrics(SM_CYSCREEN) - height) / 3;
    *CHOSEN.lock().unwrap() = vista::initial();
    let hwnd = CreateWindowExW(WINDOW_EX_STYLE::default(), class, w!("Cedro: scegli la vista"), style | WS_VISIBLE, x, y, width, height, None, None, Some(instance), None)?;
    WINDOW.store(hwnd.0 as isize, Ordering::Relaxed);
    SetTimer(Some(hwnd), TIMER, 33, None);
    let _ = SetForegroundWindow(hwnd);
    Ok(())
}

unsafe fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let screen = BeginPaint(hwnd, &mut ps);
    let d = GetDpiForSystem();
    let (w, h) = (px(vista::W as i32, d), px(vista::H as i32, d));
    let dc = CreateCompatibleDC(Some(screen));
    let bmp = CreateCompatibleBitmap(screen, w, h);
    let old = SelectObject(dc, bmp.into());
    vista::draw(&mut Gdi { dc, d: d as f32 / 96.0 }, *CHOSEN.lock().unwrap(), GetTickCount64());
    let _ = BitBlt(screen, 0, 0, w, h, Some(dc), 0, 0, SRCCOPY);
    SelectObject(dc, old);
    let _ = DeleteObject(bmp.into());
    let _ = DeleteDC(dc);
    let _ = EndPaint(hwnd, &ps);
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_TIMER => {
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let d = GetDpiForSystem() as f32 / 96.0;
            let (x, y) = ((lparam.0 & 0xffff) as i16 as f32 / d, ((lparam.0 >> 16) & 0xffff) as i16 as f32 / d);
            match vista::hit(x, y) {
                Some(Hit::Card(c)) => *CHOSEN.lock().unwrap() = c,
                Some(Hit::Done) => {
                    vista::save(*CHOSEN.lock().unwrap());
                    let _ = DestroyWindow(hwnd);
                }
                None => {}
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            match wparam.0 as u16 {
                0x1B => {
                    vista::mark_asked();
                    let _ = DestroyWindow(hwnd);
                }
                0x25 => *CHOSEN.lock().unwrap() = Choice::Complete,
                0x27 => *CHOSEN.lock().unwrap() = Choice::Simple,
                0x0D => {
                    vista::save(*CHOSEN.lock().unwrap());
                    let _ = DestroyWindow(hwnd);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            vista::mark_asked();
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            let _ = KillTimer(Some(hwnd), TIMER);
            WINDOW.store(0, Ordering::Relaxed);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
