//! Finestra «Guida» di Windows: mostra il disegno di `cedro::ui::guida` (lo stesso di Linux, nei colori e nel
//! carattere del tema scelto) e le passa tempo, mouse e tastiera. Qui c'è solo la finestra.

use cedro::guida::{Align, Canvas, Rgb};
use cedro::ui::gfx::{Fonts, GlyphCache, Painter};
use cedro::ui::guida::{Guida, Out, H, W};
use cedro::ui::screen::Key;
use cedro::wheelgeom::WHEEL;
use cedro::wheelgfx;
use std::cell::RefCell;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Mutex;
use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::Gdi::*,
    Win32::System::LibraryLoader::GetModuleHandleW,
    Win32::Graphics::Dwm::*,
    Win32::UI::HiDpi::{GetDpiForSystem, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2},
    Win32::UI::Input::KeyboardAndMouse::*,
    Win32::UI::WindowsAndMessaging::*,
};

const TIMER: usize = 1;
const FRAME_MS: u32 = 33;
const WM_MOUSELEAVE: u32 = 0x02A3;

static WINDOW: AtomicIsize = AtomicIsize::new(0);
/// Ruote già disegnate (per la finestra «Come vuoi usare Cedro?»): (fette, fetta accesa o -1) → pixel BGRA premoltiplicati.
static CACHE: Mutex<Vec<((usize, i32), Vec<u8>)>> = Mutex::new(Vec::new());

struct State {
    guida: Guida,
    /// Pixel per unità di disegno: la scala dello schermo, o meno se la finestra non ci starebbe.
    k: f32,
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

/// La scala con cui disegnare: quella del DPI, ridotta se la finestra non starebbe nell'area di lavoro.
unsafe fn fit_scale(style: WINDOW_STYLE) -> f32 {
    let dpi_scale = GetDpiForSystem() as f32 / 96.0;
    let mut work = RECT::default();
    let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut work as *mut RECT as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
    let mut frame = RECT::default();
    let _ = AdjustWindowRect(&mut frame, style, false);
    let avail_w = ((work.right - work.left) - (frame.right - frame.left)) as f32;
    let avail_h = ((work.bottom - work.top) - (frame.bottom - frame.top)) as f32;
    if avail_w <= 0.0 || avail_h <= 0.0 {
        return dpi_scale;
    }
    dpi_scale.min(avail_w / W).min(avail_h / H)
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
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    open();
    unsafe {
        // La finestra non si è aperta (già scritto nel registro): senza finestre il ciclo aspetterebbe per sempre, processo invisibile.
        if WINDOW.load(Ordering::Relaxed) == 0 {
            return;
        }
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
    // L'icona di Cedro (la prima risorsa dell'exe) nella barra del titolo.
    let icon = LoadIconW(Some(instance), PCWSTR(1usize as *const u16)).unwrap_or_default();
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        hIcon: icon,
        hIconSm: icon,
        lpfnWndProc: Some(proc),
        hInstance: instance,
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        lpszClassName: class,
        ..Default::default()
    };
    RegisterClassExW(&wc);
    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
    let k = fit_scale(style);
    let mut rect = RECT { left: 0, top: 0, right: (W * k).round() as i32, bottom: (H * k).round() as i32 };
    let _ = AdjustWindowRect(&mut rect, style, false);
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    let x = (GetSystemMetrics(SM_CXSCREEN) - width) / 2;
    let y = (GetSystemMetrics(SM_CYSCREEN) - height) / 3;
    STATE.with(|s| *s.borrow_mut() = Some(State { guida: Guida::new(theme, now()), k, cache: GlyphCache::new(), tracking: false }));
    let hwnd = CreateWindowExW(WINDOW_EX_STYLE::default(), class, &HSTRING::from(cedro::i18n::tr("Guida di Cedro").as_ref()), style | WS_VISIBLE, x, y, width, height, None, None, Some(instance), None)?;
    WINDOW.store(hwnd.0 as isize, Ordering::Relaxed);
    // Su Windows 11 la barra del titolo prende i colori del tema.
    let pal = theme.palette();
    for (attribute, color) in [(DWMWA_CAPTION_COLOR, pal.surface), (DWMWA_BORDER_COLOR, pal.border), (DWMWA_TEXT_COLOR, pal.text)] {
        let c = colorref(color);
        let _ = DwmSetWindowAttribute(hwnd, attribute, &c as *const COLORREF as *const _, 4);
    }
    SetTimer(Some(hwnd), TIMER, FRAME_MS, None);
    let _ = SetForegroundWindow(hwnd);
    Ok(())
}

fn colorref(c: Rgb) -> COLORREF {
    COLORREF(c.0 as u32 | (c.1 as u32) << 8 | (c.2 as u32) << 16)
}

/// Le primitive di `guida::Canvas` su un contesto GDI; `d` converte i pixel logici in pixel dello schermo.
pub(crate) struct Gdi {
    pub(crate) dc: HDC,
    pub(crate) d: f32,
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
        let s = &*cedro::i18n::tr(s);
        // DrawTextW con un testo vuoto va in crash dentro user32.
        if s.is_empty() {
            return;
        }
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
        let s = &*cedro::i18n::tr(s);
        if s.is_empty() {
            return 0.0;
        }
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

    fn stemma(&mut self, stemma: cedro::edizione::Stemma, x: f32, y: f32, w: f32, h: f32) {
        crate::wingfx::draw_stemma(self.dc, stemma, x * self.d, y * self.d, w * self.d, h * self.d);
    }
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
    let d = with(|st| st.k).unwrap_or(1.0);
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
