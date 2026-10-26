//! Finestra "Guida": sei passi animati (vedi `cedro::guida`, che sa cosa disegnare e come muoverlo).
//! Qui ci sono solo la finestra e le primitive GDI/GDI+ (doppio buffer); la ruota è quella vera di `wheelgfx`.

use cedro::guida::{self, Align, Canvas, Hit, Rgb, Tour};
use cedro::wheelgeom::WHEEL;
use cedro::wheelgfx;
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
const FRAME_MS: u32 = 33;

static WINDOW: AtomicIsize = AtomicIsize::new(0);
static TOUR: Mutex<Tour> = Mutex::new(Tour::new(0));
/// Ruote già disegnate: (fette, fetta accesa o -1) → pixel BGRA premoltiplicati.
static CACHE: Mutex<Vec<((usize, i32), Vec<u8>)>> = Mutex::new(Vec::new());

fn now() -> u64 {
    unsafe { GetTickCount64() }
}

fn px(v: i32, dpi: u32) -> i32 {
    v * dpi as i32 / 96
}

fn tour() -> Tour {
    *TOUR.lock().unwrap()
}

fn set_tour(t: Tour) {
    *TOUR.lock().unwrap() = t;
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
    let mut rect = RECT { left: 0, top: 0, right: px(guida::W as i32, dpi), bottom: px(guida::H as i32, dpi) };
    let _ = AdjustWindowRect(&mut rect, style, false);
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    let x = (GetSystemMetrics(SM_CXSCREEN) - width) / 2;
    let y = (GetSystemMetrics(SM_CYSCREEN) - height) / 3;
    let hwnd = CreateWindowExW(WINDOW_EX_STYLE::default(), class, w!("Guida di Cedro"), style | WS_VISIBLE, x, y, width, height, None, None, Some(instance), None)?;
    WINDOW.store(hwnd.0 as isize, Ordering::Relaxed);
    set_tour(Tour::new(now()));
    SetTimer(Some(hwnd), TIMER, FRAME_MS, None);
    let _ = SetForegroundWindow(hwnd);
    Ok(())
}

#[link(name = "kernel32")]
extern "system" {
    fn GetTickCount64() -> u64;
}

fn colorref(c: Rgb) -> COLORREF {
    COLORREF(c.0 as u32 | (c.1 as u32) << 8 | (c.2 as u32) << 16)
}

/// Le primitive di `guida::Canvas` su un contesto GDI; `d` converte i pixel logici in pixel dello schermo.
struct Gdi {
    dc: HDC,
    d: f32,
}

impl Gdi {
    fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> RECT {
        let d = self.d;
        RECT { left: (x * d).round() as i32, top: (y * d).round() as i32, right: ((x + w) * d).round() as i32, bottom: ((y + h) * d).round() as i32 }
    }

    unsafe fn font(&self, size: f32, bold: bool) -> HFONT {
        let height = -((size * self.d).round() as i32);
        CreateFontW(height, 0, 0, 0, if bold { 700 } else { 400 }, 0, 0, 0, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY, 0, crate::winfont::face())
    }
}

impl Canvas for Gdi {
    fn fill(&mut self, x: f32, y: f32, w: f32, h: f32, c: Rgb) {
        unsafe {
            let b = CreateSolidBrush(colorref(c));
            FillRect(self.dc, &self.rect(x, y, w, h), b);
            let _ = DeleteObject(b.into());
        }
    }

    fn round(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32, fill: Option<Rgb>, line: Option<Rgb>) {
        crate::wingfx::round_rect(self.dc, &self.rect(x, y, w, h), radius * self.d, fill.map(colorref), line.map(|c| (colorref(c), 1.0)));
    }

    fn text(&mut self, s: &str, x: f32, y: f32, w: f32, h: f32, size: f32, bold: bool, c: Rgb, align: Align, wrap: bool) {
        unsafe {
            let font = self.font(size, bold);
            let old = SelectObject(self.dc, font.into());
            SetBkMode(self.dc, TRANSPARENT);
            SetTextColor(self.dc, colorref(c));
            let mut wide: Vec<u16> = s.encode_utf16().collect();
            let mut r = self.rect(x, y, w, h);
            let horizontal = match align {
                Align::Left => DT_LEFT,
                Align::Center => DT_CENTER,
                Align::Right => DT_RIGHT,
            };
            let flags = if wrap { horizontal | DT_WORDBREAK | DT_NOPREFIX } else { horizontal | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX };
            DrawTextW(self.dc, &mut wide, &mut r, flags);
            SelectObject(self.dc, old);
            let _ = DeleteObject(font.into());
        }
    }

    fn text_width(&mut self, s: &str, size: f32, bold: bool) -> f32 {
        unsafe {
            let font = self.font(size, bold);
            let old = SelectObject(self.dc, font.into());
            let wide: Vec<u16> = s.encode_utf16().collect();
            let mut extent = SIZE::default();
            let _ = GetTextExtentPoint32W(self.dc, &wide, &mut extent);
            SelectObject(self.dc, old);
            let _ = DeleteObject(font.into());
            extent.cx as f32 / self.d
        }
    }

    fn poly(&mut self, pts: &[(f32, f32)], fill: Rgb, line: Rgb) {
        unsafe {
            let points: Vec<POINT> = pts.iter().map(|p| POINT { x: (p.0 * self.d).round() as i32, y: (p.1 * self.d).round() as i32 }).collect();
            let (b, p) = (CreateSolidBrush(colorref(fill)), CreatePen(PS_SOLID, 1, colorref(line)));
            let (ob, op) = (SelectObject(self.dc, b.into()), SelectObject(self.dc, p.into()));
            let _ = Polygon(self.dc, &points);
            SelectObject(self.dc, ob);
            SelectObject(self.dc, op);
            let _ = DeleteObject(b.into());
            let _ = DeleteObject(p.into());
        }
    }

    fn clip(&mut self, x: f32, y: f32, w: f32, h: f32) {
        unsafe {
            SaveDC(self.dc);
            let r = self.rect(x, y, w, h);
            IntersectClipRect(self.dc, r.left, r.top, r.right, r.bottom);
        }
    }

    fn unclip(&mut self) {
        unsafe {
            let _ = RestoreDC(self.dc, -1);
        }
    }

    fn wheel(&mut self, n: usize, hover: i32, cx: f32, cy: f32, size: f32, alpha: f32) {
        unsafe {
            let side = WHEEL as i32;
            let pixels = {
                let mut cache = CACHE.lock().unwrap();
                match cache.iter().find(|(k, _)| *k == (n, hover)) {
                    Some((_, v)) => v.clone(),
                    None => {
                        let v = wheelgfx::render(n, hover);
                        cache.push(((n, hover), v.clone()));
                        v
                    }
                }
            };
            let mut bi = BITMAPINFO::default();
            bi.bmiHeader = BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: side, biHeight: -side, biPlanes: 1, biBitCount: 32, biCompression: 0, ..Default::default() };
            let mut bits = std::ptr::null_mut();
            let Ok(bmp) = CreateDIBSection(Some(self.dc), &bi, DIB_RGB_COLORS, &mut bits, None, 0) else { return };
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u8, pixels.len());
            let mem = CreateCompatibleDC(Some(self.dc));
            let old = SelectObject(mem, bmp.into());
            let blend = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: (alpha.clamp(0.0, 1.0) * 255.0) as u8, AlphaFormat: 1 };
            let target = ((size * self.d).round() as i32).max(1);
            let _ = AlphaBlend(self.dc, (cx * self.d).round() as i32 - target / 2, (cy * self.d).round() as i32 - target / 2, target, target, mem, 0, 0, side, side, blend);
            SelectObject(mem, old);
            let _ = DeleteDC(mem);
            let _ = DeleteObject(bmp.into());
        }
    }

    fn logo(&mut self, left: f32, baseline: f32, scale: f32, ink: Rgb, accent: Rgb, real: Rgb) {
        crate::wingfx::draw_logo(self.dc, left * self.d, baseline * self.d, scale * self.d, true, colorref(ink), colorref(accent), colorref(real));
    }
}

unsafe fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let screen = BeginPaint(hwnd, &mut ps);
    let d = GetDpiForSystem();
    let (w, h) = (px(guida::W as i32, d), px(guida::H as i32, d));
    let dc = CreateCompatibleDC(Some(screen));
    let bmp = CreateCompatibleBitmap(screen, w, h);
    let old = SelectObject(dc, bmp.into());
    guida::draw(&mut Gdi { dc, d: d as f32 / 96.0 }, &tour(), now());
    let _ = BitBlt(screen, 0, 0, w, h, Some(dc), 0, 0, SRCCOPY);
    SelectObject(dc, old);
    let _ = DeleteObject(bmp.into());
    let _ = DeleteDC(dc);
    let _ = EndPaint(hwnd, &ps);
}

/// Esegue il comando e dice se la finestra va chiusa.
fn command(hit: Hit) -> bool {
    let mut t = tour();
    let at = now();
    match hit {
        Hit::Page(i) => t.goto(i, at),
        Hit::Prev => t.goto(t.page + guida::PAGE_COUNT - 1, at),
        Hit::Next if t.is_last() => return true,
        Hit::Next => t.goto(t.page + 1, at),
        Hit::Row(i) => t.show_row(i, at),
    }
    set_tour(t);
    false
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_TIMER => {
            let mut t = tour();
            t.tick(now());
            set_tour(t);
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let d = GetDpiForSystem() as f32 / 96.0;
            let (x, y) = ((lparam.0 & 0xffff) as i16 as f32 / d, ((lparam.0 >> 16) & 0xffff) as i16 as f32 / d);
            if let Some(hit) = guida::hit(x, y, tour().page) {
                if command(hit) {
                    let _ = DestroyWindow(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let hit = match wparam.0 as u16 {
                0x1B => {
                    let _ = DestroyWindow(hwnd);
                    return LRESULT(0);
                }
                0x27 => Some(Hit::Next), // freccia destra
                0x25 if tour().page > 0 => Some(Hit::Prev), // freccia sinistra
                _ => None,
            };
            if let Some(hit) = hit {
                if command(hit) {
                    let _ = DestroyWindow(hwnd);
                }
            }
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
