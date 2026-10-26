//! La sottile barra di avanzamento in basso a destra, sopra l'orologio.
//!
//! Vive in un suo thread con il suo ciclo di messaggi: si muove anche quando chi converte
//! tiene occupato il proprio thread. Chi converte chiama solo `start`, `step` e `finish`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};

use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::Gdi::*,
    Win32::System::LibraryLoader::GetModuleHandleW,
    Win32::UI::HiDpi::GetDpiForSystem,
    Win32::UI::Shell::ShellExecuteW,
    Win32::UI::WindowsAndMessaging::*,
};

const PRIMARY: COLORREF = COLORREF(0x008E5E00); // #005E8E
const TRACK: COLORREF = COLORREF(0x00E8DDD0); // #d0dde8
const OK: COLORREF = COLORREF(0x00467A1D); // #1d7a46
const FAIL: COLORREF = COLORREF(0x001E26B3); // #b3261e
const TEXT: COLORREF = COLORREF(0x00191919);
const WHITE: COLORREF = COLORREF(0x00FFFFFF);
/// Il colore che il sistema rende trasparente: tutto ciò che non è etichetta o linea.
const KEY: COLORREF = COLORREF(0x00FF00FF);

const WIDTH: i32 = 250;
const HEIGHT: i32 = 32;
/// Quanto resta la linea verde dopo la fine.
const LINGER: Duration = Duration::from_millis(1200);
const TIMER: usize = 1;

struct Job {
    /// Etichetta corrente: «foto.jpg → WebP» oppure «3 di 8 · foto.jpg».
    label: String,
    done: usize,
    total: usize,
    started: Instant,
    /// A lavoro finito: quando, se è andato bene, e il messaggio finale.
    finished: Option<(Instant, bool)>,
    /// L'ultimo file creato: un clic sull'etichetta lo mostra in Esplora file.
    last: Option<PathBuf>,
}

static JOB: Mutex<Option<Job>> = Mutex::new(None);
static WINDOW: AtomicIsize = AtomicIsize::new(0);
static STARTED: Once = Once::new();

fn job() -> std::sync::MutexGuard<'static, Option<Job>> {
    JOB.lock().unwrap_or_else(|e| e.into_inner())
}

/// Parte un lavoro di `total` file. Con le Impostazioni che la spengono non mostra niente.
pub fn start(total: usize, label: &str) {
    if !cedro::read_settings().progress || total == 0 {
        return;
    }
    *job() = Some(Job { label: label.to_string(), done: 0, total, started: Instant::now(), finished: None, last: None });
    STARTED.call_once(|| {
        std::thread::spawn(|| unsafe { run() });
    });
    wake();
}

/// Il file `index` (da 0) sta per partire.
pub fn step(index: usize, label: &str) {
    if let Some(j) = job().as_mut() {
        j.done = index;
        j.label = label.to_string();
    }
    wake();
}

/// Un file è stato creato: lo ricorda per il clic sull'etichetta.
pub fn produced(path: &Path) {
    if let Some(j) = job().as_mut() {
        j.last = Some(path.to_path_buf());
    }
}

/// Fine del lavoro. Se è andato bene la linea diventa verde e sparisce da sola,
/// altrimenti diventa rossa e resta finché non la clicchi.
pub fn finish(ok: bool, label: &str) {
    if let Some(j) = job().as_mut() {
        j.done = j.total;
        j.label = label.to_string();
        j.finished = Some((Instant::now(), ok));
    }
    wake();
}

/// Il lavoro è stato annullato dall'utente: non è un errore né un successo, la linea sparisce.
pub fn dismiss() {
    if let Some(j) = job().as_mut() {
        j.finished = Some((Instant::now().checked_sub(LINGER * 2).unwrap_or_else(Instant::now), true));
    }
    wake();
}

fn wake() {
    let raw = WINDOW.load(Ordering::Relaxed);
    if raw != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(raw as *mut _)), WM_APP, WPARAM(0), LPARAM(0));
        }
    }
}

fn px(v: i32) -> i32 {
    v * unsafe { GetDpiForSystem() } as i32 / 96
}

unsafe fn run() {
    // ShellExecuteW (il clic che mostra il file) vuole COM in questo thread: senza, Esplora file si apre su Documenti.
    let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
    let Ok(module) = GetModuleHandleW(None) else { return };
    let instance: HINSTANCE = module.into();
    let class = w!("CedroProgress");
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(proc),
        hInstance: instance,
        hCursor: LoadCursorW(None, IDC_HAND).unwrap_or_default(),
        lpszClassName: class,
        ..Default::default()
    };
    RegisterClassExW(&wc);
    let ex = WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED;
    let Ok(hwnd) = CreateWindowExW(ex, class, w!("Cedro"), WS_POPUP, 0, 0, px(WIDTH), px(HEIGHT), None, None, Some(instance), None) else {
        return;
    };
    let _ = SetLayeredWindowAttributes(hwnd, KEY, 255, LWA_COLORKEY);
    WINDOW.store(hwnd.0 as isize, Ordering::Relaxed);
    SetTimer(Some(hwnd), TIMER, 33, None);
    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}

/// Mette la finestra nell'angolo in basso a destra dell'area di lavoro, sopra la barra delle applicazioni.
unsafe fn place(hwnd: HWND) {
    let mut area = RECT::default();
    let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut area as *mut _ as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
    let (w, h) = (px(WIDTH), px(HEIGHT));
    let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), area.right - w - px(12), area.bottom - h - px(8), w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
}

/// Cosa disegnare adesso, o `None` se la barra va nascosta.
struct Frame {
    label: String,
    color: COLORREF,
    label_color: COLORREF,
    /// Linea piena (a lavoro finito) invece del tratto che scorre.
    full: bool,
    /// Per più file: quanti fatti su quanti.
    ticks: Option<(usize, usize)>,
    /// A che punto è il tratto che scorre, da 0 a 1.
    phase: f32,
}

/// Il tratto che scorre: un terzo della larghezza, che entra da sinistra ed esce a destra.
fn sweep(phase: f32) -> (f32, f32) {
    let start = phase * 1.33 - 0.33;
    (start.max(0.0), (start + 0.33).min(1.0))
}

fn frame() -> Option<Frame> {
    let mut slot = job();
    let j = slot.as_ref()?;
    if let Some((at, ok)) = j.finished {
        if ok && at.elapsed() > LINGER {
            *slot = None;
            return None;
        }
        let color = if ok { OK } else { FAIL };
        return Some(Frame { label: j.label.clone(), color, label_color: color, full: true, ticks: None, phase: 0.0 });
    }
    // Il motore non dice quanto manca dentro un file: il file in corso ha un tratto che scorre.
    let phase = (j.started.elapsed().as_millis() % 1400) as f32 / 1400.0;
    let ticks = (j.total > 1).then_some((j.done, j.total));
    Some(Frame { label: j.label.clone(), color: PRIMARY, label_color: TEXT, full: false, ticks, phase })
}

unsafe fn fill(hdc: HDC, r: RECT, color: COLORREF) {
    let b = CreateSolidBrush(color);
    FillRect(hdc, &r, b);
    let _ = DeleteObject(b.into());
}

unsafe fn paint(hwnd: HWND, f: &Frame) {
    let mut ps = PAINTSTRUCT::default();
    let screen = BeginPaint(hwnd, &mut ps);
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    // Disegna fuori schermo e poi copia, così la linea non sfarfalla.
    let hdc = CreateCompatibleDC(Some(screen));
    let bmp = CreateCompatibleBitmap(screen, rc.right, rc.bottom);
    let old = SelectObject(hdc, bmp.into());
    fill(hdc, rc, KEY);

    // Etichetta: un riquadro bianco allineato a destra, largo quanto il testo.
    let font = CreateFontW(-px(12), 0, 0, 0, 600, 0, 0, 0, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY, 0, crate::winfont::face());
    let old_font = SelectObject(hdc, font.into());
    let mut text: Vec<u16> = f.label.encode_utf16().collect();
    let mut size = SIZE::default();
    let _ = GetTextExtentPoint32W(hdc, &text, &mut size);
    let label_w = (size.cx + px(16)).min(rc.right);
    let chip = RECT { left: rc.right - label_w, top: 0, right: rc.right, bottom: px(22) };
    fill(hdc, chip, WHITE);
    fill(hdc, RECT { bottom: chip.top + 1, ..chip }, TRACK);
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, f.label_color);
    let mut tr = RECT { left: chip.left + px(8), right: chip.right - px(8), ..chip };
    DrawTextW(hdc, &mut text, &mut tr, DT_SINGLELINE | DT_VCENTER | DT_RIGHT | DT_END_ELLIPSIS | DT_NOPREFIX);
    SelectObject(hdc, old_font);
    let _ = DeleteObject(font.into());

    // La linea: 3 pixel in fondo.
    let (top, bottom) = (rc.bottom - px(3), rc.bottom);
    let (a, b) = sweep(f.phase);
    match f.ticks {
        Some((done, total)) => {
            let gap = px(2);
            let seg = (rc.right - gap * (total as i32 - 1)) as f32 / total as f32;
            for i in 0..total {
                let left = (i as f32 * (seg + gap as f32)) as i32;
                let right = (left as f32 + seg) as i32;
                let r = RECT { left, top, right, bottom };
                fill(hdc, r, if i < done { f.color } else { TRACK });
                if i == done {
                    let w = (right - left) as f32;
                    fill(hdc, RECT { left: left + (a * w) as i32, right: left + (b * w) as i32, ..r }, f.color);
                }
            }
        }
        None => {
            fill(hdc, RECT { left: 0, top, right: rc.right, bottom }, TRACK);
            let (a, b) = if f.full { (0.0, 1.0) } else { (a, b) };
            let w = rc.right as f32;
            fill(hdc, RECT { left: (a * w) as i32, top, right: (b * w) as i32, bottom }, f.color);
        }
    }
    let _ = BitBlt(screen, 0, 0, rc.right, rc.bottom, Some(hdc), 0, 0, SRCCOPY);
    SelectObject(hdc, old);
    let _ = DeleteObject(bmp.into());
    let _ = DeleteDC(hdc);
    let _ = EndPaint(hwnd, &ps);
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER | WM_APP => {
            let visible = IsWindowVisible(hwnd).as_bool();
            match (frame().is_some(), visible) {
                (true, false) => place(hwnd),
                (false, true) => {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
                _ => {}
            }
            if visible {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_PAINT => {
            match frame() {
                Some(f) => paint(hwnd, &f),
                None => {
                    let mut ps = PAINTSTRUCT::default();
                    BeginPaint(hwnd, &mut ps);
                    let _ = EndPaint(hwnd, &ps);
                }
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_LBUTTONUP => {
            // Clic: mostra l'ultimo file creato e chiude la barra (anche quella rossa di un errore).
            let last = job().take().and_then(|j| j.last);
            let _ = ShowWindow(hwnd, SW_HIDE);
            if let Some(path) = last {
                let args = HSTRING::from(format!("/select,\"{}\"", path.display()));
                ShellExecuteW(None, w!("open"), w!("explorer.exe"), &args, None, SW_SHOWNORMAL);
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
