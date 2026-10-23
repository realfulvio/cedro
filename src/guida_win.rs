//! Finestra "Guida": un'animazione che mostra cosa fanno le ruote con Shift, Ctrl e Alt.
//! Disegnata a mano con GDI (doppio buffer), senza altri file: la ruota è quella vera di `wheelgfx`.

use cedro::wheelgeom::{slice_bounds, R0, R1, WHEEL};
use cedro::wheelgfx;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::sync::Mutex;
use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::Gdi::*,
    Win32::System::LibraryLoader::GetModuleHandleW,
    Win32::UI::HiDpi::GetDpiForSystem,
    Win32::UI::WindowsAndMessaging::*,
};

// Palette del Comune di Bondeno (COLORREF = 0x00BBGGRR).
const PRIMARY: COLORREF = COLORREF(0x008E5E00); // #005E8E
const HOVER: COLORREF = COLORREF(0x00CC6600); // #0066CC
const TEXT: COLORREF = COLORREF(0x00191919);
const GREY: COLORREF = COLORREF(0x00645A45); // #455A64
const LINE: COLORREF = COLORREF(0x00E6DDD3);
const WHITE: COLORREF = COLORREF(0x00FFFFFF);
const STAGE: COLORREF = COLORREF(0x004A2200); // blu scuro dietro la ruota

const TIMER: usize = 1;
const FRAME_MS: u32 = 33;
const CYCLE_MS: u32 = 4800;
const W: i32 = 780;
const H: i32 = 440;
const STAGE_RECT: (i32, i32, i32, i32) = (400, 56, 364, 368); // x, y, larghezza, altezza
const WHEEL_SIZE: i32 = 330;

struct Mode {
    key: &'static str,
    title: &'static str,
    detail: &'static str,
    caption: &'static str,
    pick: usize,
    labels: &'static [&'static str],
}

const MODES: [Mode; 3] = [
    Mode { key: "Shift", title: "Converti", detail: "Cambia formato: JPEG, PDF, MP4…", caption: "CONVERSIONE", pick: 2, labels: &["PDF", "PNG", "JPEG", "WebP", "BMP", "GIF", "TIFF"] },
    Mode { key: "Ctrl", title: "Modifica", detail: "Comprimi, ruota, OCR, metadati…", caption: "STRUMENTI", pick: 5, labels: &["Comprimi", "Scala", "B/N", "Ruota", "Metadati", "OCR", "MAIUSC."] },
    Mode { key: "Alt", title: "Azioni sul file", detail: "Duplica, copia il percorso, ZIP", caption: "AZIONI", pick: 1, labels: &["Duplica", "Percorso", "ZIP"] },
];

static WINDOW: AtomicIsize = AtomicIsize::new(0);
static MODE: AtomicU32 = AtomicU32::new(0);
static START: AtomicU32 = AtomicU32::new(0);
/// Ruote già disegnate: (modo, fetta accesa o -1) → pixel BGRA premoltiplicati.
static CACHE: Mutex<Vec<((usize, i32), Vec<u8>)>> = Mutex::new(Vec::new());

fn px(v: i32, dpi: u32) -> i32 {
    v * dpi as i32 / 96
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
    let mut rect = RECT { left: 0, top: 0, right: px(W, dpi), bottom: px(H, dpi) };
    let _ = AdjustWindowRect(&mut rect, style, false);
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    let x = (GetSystemMetrics(SM_CXSCREEN) - width) / 2;
    let y = (GetSystemMetrics(SM_CYSCREEN) - height) / 3;
    let hwnd = CreateWindowExW(WINDOW_EX_STYLE::default(), class, w!("Guida di Cedro"), style | WS_VISIBLE, x, y, width, height, None, None, Some(instance), None)?;
    WINDOW.store(hwnd.0 as isize, Ordering::Relaxed);
    MODE.store(0, Ordering::Relaxed);
    START.store(GetTickCount(), Ordering::Relaxed);
    SetTimer(Some(hwnd), TIMER, FRAME_MS, None);
    let _ = SetForegroundWindow(hwnd);
    Ok(())
}

#[link(name = "kernel32")]
extern "system" {
    fn GetTickCount() -> u32;
}

fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: (f32, f32), b: (f32, f32), t: f32) -> (f32, f32) {
    let t = ease(t);
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

/// Stato dell'animazione al millisecondo `t` del ciclo: opacità della ruota, fetta accesa, cursore.
/// Coordinate del cursore in unità logiche, relative all'angolo alto a sinistra del palco.
fn scene(mode: usize, t: u32) -> (f32, bool, (f32, f32)) {
    let t = t as f32;
    let (sw, sh) = (STAGE_RECT.2 as f32, STAGE_RECT.3 as f32);
    let center = (sw / 2.0, sh / 2.0);
    let rest = (sw - 50.0, sh - 40.0);
    let on_file = (center.0 + 28.0, center.1 + 28.0);
    let m = &MODES[mode];
    let s = WHEEL_SIZE as f32 / WHEEL;
    let (a0, a1) = slice_bounds(m.pick, m.labels.len());
    let mid = (a0 + a1) / 2.0;
    let r = (R0 + R1) / 2.0 * s;
    let target = (center.0 + r * mid.cos(), center.1 + r * mid.sin());
    let alpha = if t < 1100.0 { 0.0 } else if t < 1450.0 { (t - 1100.0) / 350.0 } else if t < 3800.0 { 1.0 } else if t < 4150.0 { 1.0 - (t - 3800.0) / 350.0 } else { 0.0 };
    let cursor = if t < 1000.0 {
        lerp(rest, on_file, t / 1000.0)
    } else if t < 1900.0 {
        on_file
    } else if t < 2500.0 {
        lerp(on_file, target, (t - 1900.0) / 600.0)
    } else if t < 3800.0 {
        target
    } else {
        lerp(target, rest, (t - 3800.0) / 700.0)
    };
    (alpha, (2500.0..3800.0).contains(&t), cursor)
}

unsafe fn font(height: i32, weight: i32) -> HFONT {
    CreateFontW(-height, 0, 0, 0, weight, 0, 0, 0, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY, 0, crate::winfont::face())
}

unsafe fn fill(dc: HDC, r: RECT, color: COLORREF) {
    let b = CreateSolidBrush(color);
    FillRect(dc, &r, b);
    let _ = DeleteObject(b.into());
}

unsafe fn round(dc: HDC, r: RECT, radius: i32, fill_color: COLORREF, line: COLORREF) {
    let b = CreateSolidBrush(fill_color);
    let p = CreatePen(PS_SOLID, 1, line);
    let (ob, op) = (SelectObject(dc, b.into()), SelectObject(dc, p.into()));
    let _ = RoundRect(dc, r.left, r.top, r.right, r.bottom, radius, radius);
    SelectObject(dc, ob);
    SelectObject(dc, op);
    let _ = DeleteObject(b.into());
    let _ = DeleteObject(p.into());
}

unsafe fn text(dc: HDC, f: HFONT, color: COLORREF, s: &str, mut r: RECT, flags: DRAW_TEXT_FORMAT) {
    let old = SelectObject(dc, f.into());
    SetBkMode(dc, TRANSPARENT);
    SetTextColor(dc, color);
    let mut wide: Vec<u16> = s.encode_utf16().collect();
    DrawTextW(dc, &mut wide, &mut r, flags);
    SelectObject(dc, old);
}

fn rc(x: i32, y: i32, w: i32, h: i32, d: u32) -> RECT {
    RECT { left: px(x, d), top: px(y, d), right: px(x + w, d), bottom: px(y + h, d) }
}

fn wheel_pixels(mode: usize, hover: i32) -> Vec<u8> {
    let mut cache = CACHE.lock().unwrap();
    if let Some((_, v)) = cache.iter().find(|(k, _)| *k == (mode, hover)) {
        return v.clone();
    }
    let v = wheelgfx::render(MODES[mode].labels.len(), hover);
    cache.push(((mode, hover), v.clone()));
    v
}

unsafe fn draw_wheel(dc: HDC, mode: usize, hover: i32, alpha: f32, cx: i32, cy: i32, size: i32, d: u32) {
    let side = WHEEL as i32;
    let mut bi = BITMAPINFO::default();
    bi.bmiHeader = BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: side, biHeight: -side, biPlanes: 1, biBitCount: 32, biCompression: 0, ..Default::default() };
    let mut bits = std::ptr::null_mut();
    let Ok(bmp) = CreateDIBSection(Some(dc), &bi, DIB_RGB_COLORS, &mut bits, None, 0) else { return };
    let pixels = wheel_pixels(mode, hover);
    std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u8, pixels.len());
    let mem = CreateCompatibleDC(Some(dc));
    let old = SelectObject(mem, bmp.into());
    let blend = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: (alpha * 255.0) as u8, AlphaFormat: 1 };
    let _ = AlphaBlend(dc, cx - size / 2, cy - size / 2, size, size, mem, 0, 0, side, side, blend);
    SelectObject(mem, old);
    let _ = DeleteDC(mem);
    let _ = DeleteObject(bmp.into());
    // Le scritte vanno sopra, con la stessa dissolvenza: sotto il 60% si omettono (GDI non sfuma il testo).
    if alpha < 0.6 {
        return;
    }
    let m = &MODES[mode];
    let k = size as f32 / WHEEL;
    let n = m.labels.len();
    let f = font(px(if n > 6 { 13 } else { 15 }, d), 700);
    let r = (R0 + R1) / 2.0 * k;
    for (i, label) in m.labels.iter().enumerate() {
        let (a0, a1) = slice_bounds(i, n);
        let mid = (a0 + a1) / 2.0;
        let (x, y) = (cx + (r * mid.cos()) as i32, cy + (r * mid.sin()) as i32);
        let color = if i as i32 == hover { WHITE } else { TEXT };
        text(dc, f, color, label, RECT { left: x - 50, top: y - 12, right: x + 50, bottom: y + 12 }, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
    }
    let _ = DeleteObject(f.into());
    let small = font(px(9, d), 700);
    let big = font(px(14, d), 700);
    text(dc, small, GREY, m.caption, RECT { left: cx - 40, top: cy - 22, right: cx + 40, bottom: cy - 6 }, DT_CENTER | DT_SINGLELINE);
    let name = if hover >= 0 { m.labels[hover as usize] } else { "foto.png" };
    text(dc, big, COLORREF(0x004D1A00), name, RECT { left: cx - 40, top: cy - 4, right: cx + 40, bottom: cy + 18 }, DT_CENTER | DT_SINGLELINE);
    let _ = DeleteObject(small.into());
    let _ = DeleteObject(big.into());
}

unsafe fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let screen = BeginPaint(hwnd, &mut ps);
    let d = GetDpiForSystem();
    let (w, h) = (px(W, d), px(H, d));
    let dc = CreateCompatibleDC(Some(screen));
    let bmp = CreateCompatibleBitmap(screen, w, h);
    let old = SelectObject(dc, bmp.into());
    fill(dc, RECT { left: 0, top: 0, right: w, bottom: h }, WHITE);
    // barra del titolo
    fill(dc, rc(0, 0, W, 44, d), PRIMARY);
    let tf = font(px(19, d), 700);
    text(dc, tf, WHITE, "Guida di Cedro", rc(20, 0, 400, 44, d), DT_VCENTER | DT_SINGLELINE);
    let _ = DeleteObject(tf.into());

    let mode = MODE.load(Ordering::Relaxed) as usize % MODES.len();
    let t = GetTickCount().wrapping_sub(START.load(Ordering::Relaxed)) % CYCLE_MS;

    // colonna di sinistra
    let hf = font(px(26, d), 700);
    text(dc, hf, TEXT, "Tieni premuto un tasto,", rc(24, 62, 360, 34, d), DT_SINGLELINE);
    text(dc, hf, TEXT, "poi clic destro sul file", rc(24, 94, 360, 34, d), DT_SINGLELINE);
    let _ = DeleteObject(hf.into());
    let bf = font(px(15, d), 400);
    text(dc, bf, GREY, "Si apre la ruota. Clic sulla fetta: la copia nasce\naccanto all'originale, che resta dov'è.", rc(24, 136, 360, 50, d), DT_WORDBREAK);
    let kf = font(px(16, d), 700);
    let df = font(px(14, d), 400);
    for (i, m) in MODES.iter().enumerate() {
        let y = 204 + i as i32 * 72;
        let on = i == mode;
        round(dc, rc(24, y, 356, 60, d), px(10, d), WHITE, if on { PRIMARY } else { LINE });
        let cap = rc(36, y + 14, 74, 32, d);
        round(dc, if on { RECT { top: cap.top + px(3, d), ..cap } } else { cap }, px(8, d), if on { PRIMARY } else { LINE }, if on { PRIMARY } else { LINE });
        text(dc, kf, if on { WHITE } else { TEXT }, m.key, if on { RECT { top: cap.top + px(3, d), bottom: cap.bottom + px(3, d), ..cap } } else { cap }, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        text(dc, kf, TEXT, m.title, rc(126, y + 8, 240, 22, d), DT_SINGLELINE);
        text(dc, df, GREY, m.detail, rc(126, y + 31, 240, 22, d), DT_SINGLELINE);
    }
    let _ = DeleteObject(kf.into());
    let _ = DeleteObject(df.into());
    let _ = DeleteObject(bf.into());

    // palco
    let (sx, sy, sw, sh) = STAGE_RECT;
    round(dc, rc(sx, sy, sw, sh, d), px(12, d), STAGE, STAGE);
    let (ox, oy) = (px(sx, d), px(sy, d));
    let (cx, cy) = (ox + px(sw / 2, d), oy + px(sh / 2, d));
    // il file (sparisce sotto la ruota quando è aperta)
    let (alpha, lit, cursor) = scene(mode, t);
    let file = RECT { left: cx - px(46, d), top: cy - px(46, d), right: cx + px(46, d), bottom: cy + px(46, d) };
    round(dc, file, px(10, d), WHITE, WHITE);
    round(dc, RECT { left: file.left + px(12, d), top: file.top + px(10, d), right: file.right - px(12, d), bottom: file.top + px(52, d) }, px(6, d), COLORREF(0x00F2E3CF), COLORREF(0x00F2E3CF));
    let ff = font(px(12, d), 700);
    text(dc, ff, TEXT, "foto.png", RECT { left: file.left, top: file.top + px(58, d), right: file.right, bottom: file.bottom }, DT_CENTER | DT_SINGLELINE);
    let _ = DeleteObject(ff.into());
    if alpha > 0.0 {
        draw_wheel(dc, mode, if lit { MODES[mode].pick as i32 } else { -1 }, alpha, cx, cy, px(WHEEL_SIZE, d), d);
    }
    // cursore
    let (mx, my) = (ox + px(cursor.0 as i32, d), oy + px(cursor.1 as i32, d));
    let pts = [(0, 0), (0, 20), (5, 15), (9, 24), (13, 22), (9, 14), (16, 14)];
    let poly: Vec<POINT> = pts.iter().map(|(a, b)| POINT { x: mx + px(*a, d), y: my + px(*b, d) }).collect();
    let (b, p) = (CreateSolidBrush(WHITE), CreatePen(PS_SOLID, 1, COLORREF(0)));
    let (ob, op) = (SelectObject(dc, b.into()), SelectObject(dc, p.into()));
    let _ = Polygon(dc, &poly);
    SelectObject(dc, ob);
    SelectObject(dc, op);
    let _ = DeleteObject(b.into());
    let _ = DeleteObject(p.into());
    // tasto premuto accanto al cursore
    if t > 300 && t < 4150 {
        let kb = rc(sx + 14, sy + sh - 46, 76, 32, d);
        let kb = RECT { left: kb.left - px(sx, d) + ox, top: kb.top - px(sy, d) + oy, right: kb.right - px(sx, d) + ox, bottom: kb.bottom - px(sy, d) + oy };
        round(dc, kb, px(8, d), HOVER, HOVER);
        let f = font(px(15, d), 700);
        text(dc, f, WHITE, MODES[mode].key, kb, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        let _ = DeleteObject(f.into());
    }

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
            let t = GetTickCount().wrapping_sub(START.load(Ordering::Relaxed));
            if t >= CYCLE_MS {
                MODE.store((MODE.load(Ordering::Relaxed) + 1) % MODES.len() as u32, Ordering::Relaxed);
                START.fetch_add(CYCLE_MS, Ordering::Relaxed);
            }
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let d = GetDpiForSystem();
            let (x, y) = ((lparam.0 & 0xffff) as i32, ((lparam.0 >> 16) & 0xffff) as i32);
            for i in 0..MODES.len() {
                let r = rc(24, 204 + i as i32 * 72, 356, 60, d);
                if x >= r.left && x < r.right && y >= r.top && y < r.bottom {
                    MODE.store(i as u32, Ordering::Relaxed);
                    START.store(GetTickCount(), Ordering::Relaxed);
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
