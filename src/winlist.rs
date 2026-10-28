//! L'elenco dei componenti open source nella scheda «Informazioni»: gruppi, una riga per componente con
//! autore, a cosa serve e licenza; un clic la apre e mostra il testo della licenza. Lista a scorrimento
//! con le righe disegnate da Cedro (owner-draw), nei colori del sito del Comune di Bondeno.

use cedro::credits::{self, Component};
use std::cell::RefCell;
use std::collections::HashSet;
use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::Gdi::*,
    Win32::System::LibraryLoader::GetModuleHandleW,
    Win32::UI::Controls::*,
    Win32::UI::HiDpi::GetDpiForSystem,
    Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass},
    Win32::UI::WindowsAndMessaging::*,
};

const WHITE: COLORREF = COLORREF(0x00FFFFFF);
const SIDE: COLORREF = COLORREF(0x00F7F5F3); // #F3F5F7
const LINE: COLORREF = COLORREF(0x00E5E5E5); // #E5E5E5
const TEXT: COLORREF = COLORREF(0x00191919);
const GREY: COLORREF = COLORREF(0x00645A45); // #455A64
const PRIMARY: COLORREF = COLORREF(0x008E5E00); // #005E8E
const SOFT: COLORREF = COLORREF(0x00FFEEE6); // #E6EEFF

const ITEM_H: i32 = 46;
const GROUP_H: i32 = 38;
const FIRST_GROUP_H: i32 = 26;

enum Row {
    Group(&'static str),
    Item(&'static Component, Option<String>),
}

struct State {
    id: i32,
    rows: Vec<Row>,
    open: HashSet<&'static str>,
    dpi: i32,
    /// Larghezza per andare a capo nel riquadro della licenza (pixel).
    wrap: i32,
    bold: HFONT,
    small: HFONT,
    mono: HFONT,
    mono_bold: HFONT,
    tiny: HFONT,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn px(value: i32, dpi: i32) -> i32 {
    value * dpi / 96
}

fn font(height: i32, weight: i32, dpi: i32, face: PCWSTR) -> HFONT {
    unsafe {
        CreateFontW(
            -px(height, dpi), 0, 0, 0, weight, 0, 0, 0,
            DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY,
            0, face,
        )
    }
}

/// Crea la lista dentro `parent`; `r` è (x, y, larghezza, altezza) in pixel del dispositivo.
pub fn create(parent: HWND, id: i32, r: (i32, i32, i32, i32)) -> HWND {
    unsafe {
        let dpi = GetDpiForSystem() as i32;
        let instance: HINSTANCE = GetModuleHandleW(None).unwrap_or_default().into();
        let style = WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_TABSTOP
            | WINDOW_STYLE((LBS_OWNERDRAWVARIABLE | LBS_NOINTEGRALHEIGHT | LBS_NOTIFY) as u32);
        let list = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("LISTBOX"),
            None,
            style,
            r.0, r.1, r.2, r.3,
            Some(parent),
            Some(HMENU(id as isize as *mut _)),
            Some(instance),
            None,
        )
        .unwrap_or_default();
        let face = cedro_face();
        let wrap = r.2 - GetSystemMetrics(SM_CXVSCROLL) - px(2 * 2 + 2 * 10, dpi);
        STATE.with(|s| {
            *s.borrow_mut() = Some(State {
                id,
                rows: Vec::new(),
                open: HashSet::new(),
                dpi,
                wrap,
                bold: font(14, 700, dpi, face),
                small: font(13, 400, dpi, face),
                mono: font(11, 400, dpi, w!("Consolas")),
                mono_bold: font(12, 700, dpi, w!("Consolas")),
                tiny: font(10, 700, dpi, face),
            });
        });
        let _ = SetWindowSubclass(list, Some(subclass), 1, 0);
        filter(list, "");
        list
    }
}

/// Elimina i caratteri della lista: da chiamare quando la finestra che la contiene è chiusa.
pub fn destroy() {
    if let Some(s) = STATE.with(|s| s.borrow_mut().take()) {
        for f in [s.bold, s.small, s.mono, s.mono_bold, s.tiny] {
            unsafe {
                let _ = DeleteObject(f.into());
            }
        }
    }
}

fn cedro_face() -> PCWSTR {
    // Lo stesso carattere delle altre finestre di Cedro.
    crate::winfont::face()
}

fn matches(item: &Component, query: &str) -> bool {
    query.is_empty()
        || [item.name, item.author, item.license, item.what].iter().any(|t| t.to_lowercase().contains(query))
}

/// Riempie la lista con i componenti che contengono `query` (nome, autore, licenza o uso). Restituisce quanti sono.
pub fn filter(list: HWND, query: &str) -> usize {
    let query = query.trim().to_lowercase();
    let mut count = 0;
    let rows: Vec<Row> = credits::visible()
        .into_iter()
        .flat_map(|(title, items)| {
            let found: Vec<&'static Component> = items.into_iter().filter(|i| matches(i, &query)).collect();
            count += found.len();
            let mut rows = Vec::new();
            if !found.is_empty() {
                rows.push(Row::Group(title));
                rows.extend(found.into_iter().map(|i| Row::Item(i, credits::version_text(i))));
            }
            rows
        })
        .collect();
    let total = rows.len();
    STATE.with(|s| {
        if let Some(st) = s.borrow_mut().as_mut() {
            st.rows = rows;
        }
    });
    unsafe {
        SendMessageW(list, LB_RESETCONTENT, None, None);
        for index in 0..total {
            SendMessageW(list, LB_ADDSTRING, Some(WPARAM(0)), Some(LPARAM(index as isize)));
        }
    }
    count
}

fn height_of(st: &State, index: usize, hdc: HDC) -> i32 {
    match st.rows.get(index) {
        Some(Row::Group(_)) => px(if index == 0 { FIRST_GROUP_H } else { GROUP_H }, st.dpi),
        Some(Row::Item(item, _)) if st.open.contains(item.name) => {
            let old = unsafe { SelectObject(hdc, st.mono.into()) };
            let mut text: Vec<u16> = credits::license_text(item.license).encode_utf16().collect();
            let mut rc = RECT { left: 0, top: 0, right: st.wrap, bottom: 0 };
            unsafe {
                DrawTextW(hdc, &mut text, &mut rc, DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX);
                SelectObject(hdc, old);
            }
            px(ITEM_H, st.dpi) + (rc.bottom - rc.top) + px(16 + 26 + 8, st.dpi)
        }
        _ => px(ITEM_H, st.dpi),
    }
}

/// `WM_MEASUREITEM` della lista. Falso se non è la nostra.
pub fn measure(lparam: LPARAM) -> bool {
    unsafe {
        let mis = &mut *(lparam.0 as *mut MEASUREITEMSTRUCT);
        if mis.CtlType != ODT_LISTBOX {
            return false;
        }
        STATE.with(|s| {
            let s = s.borrow();
            let Some(st) = s.as_ref() else { return false };
            if mis.CtlID as i32 != st.id {
                return false;
            }
            let dc = GetDC(None);
            mis.itemHeight = height_of(st, mis.itemData, dc).max(1) as u32;
            ReleaseDC(None, dc);
            true
        })
    }
}

unsafe fn text(hdc: HDC, s: &str, rc: &RECT, font: HFONT, color: COLORREF, format: DRAW_TEXT_FORMAT) {
    let old = SelectObject(hdc, font.into());
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, color);
    let mut wide: Vec<u16> = s.encode_utf16().collect();
    let mut rect = *rc;
    DrawTextW(hdc, &mut wide, &mut rect, format | DT_NOPREFIX);
    SelectObject(hdc, old);
}

unsafe fn text_width(hdc: HDC, s: &str, font: HFONT) -> i32 {
    let old = SelectObject(hdc, font.into());
    let wide: Vec<u16> = s.encode_utf16().collect();
    let mut size = SIZE::default();
    let _ = GetTextExtentPoint32W(hdc, &wide, &mut size);
    SelectObject(hdc, old);
    size.cx
}

/// `WM_DRAWITEM` della lista. Falso se non è la nostra.
pub fn draw(lparam: LPARAM) -> bool {
    unsafe {
        let dis = &*(lparam.0 as *const DRAWITEMSTRUCT);
        if dis.CtlType != ODT_LISTBOX {
            return false;
        }
        STATE.with(|s| {
            let s = s.borrow();
            let Some(st) = s.as_ref() else { return false };
            if dis.CtlID as i32 != st.id {
                return false;
            }
            let hdc = dis.hDC;
            let r = dis.rcItem;
            let selected = dis.itemState.0 & ODS_SELECTED.0 != 0;
            let back = CreateSolidBrush(WHITE);
            FillRect(hdc, &r, back);
            let _ = DeleteObject(back.into());
            let Some(row) = st.rows.get(dis.itemData) else { return true };
            let d = st.dpi;
            match row {
                Row::Group(title) => {
                    let area = RECT { left: r.left + px(2, d), top: r.top, right: r.right, bottom: r.bottom - px(4, d) };
                    text(hdc, &title.to_uppercase(), &area, st.bold, GREY, DT_LEFT | DT_BOTTOM | DT_SINGLELINE);
                }
                Row::Item(item, version) => {
                    if selected {
                        let rc = RECT { left: r.left, top: r.top + 1, right: r.right, bottom: r.top + px(ITEM_H, d) };
                        FillRect(hdc, &rc, brush(SIDE));
                    }
                    FillRect(hdc, &RECT { bottom: r.top + 1, ..r }, brush(LINE));
                    let head = r.top + px(ITEM_H, d);
                    // La licenza a destra, in una pillola chiara.
                    let pill_w = text_width(hdc, item.license, st.mono_bold) + px(14, d);
                    let pill = RECT {
                        left: r.right - px(30, d) - pill_w,
                        top: r.top + px(13, d),
                        right: r.right - px(30, d),
                        bottom: r.top + px(33, d),
                    };
                    crate::wingfx::round_rect(hdc, &pill, px(4, d) as f32, Some(SOFT), None);
                    text(hdc, item.license, &pill, st.mono_bold, PRIMARY, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
                    // Nome, e «in arrivo» se la funzione è ancora in lavorazione.
                    let name_rc = RECT { left: r.left + px(4, d), top: r.top + px(5, d), right: pill.left - px(10, d), bottom: r.top + px(25, d) };
                    text(hdc, item.name, &name_rc, st.bold, TEXT, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
                    let mut at = name_rc.left + text_width(hdc, item.name, st.bold) + px(8, d);
                    if item.soon {
                        let tag = RECT { left: at, ..name_rc };
                        text(hdc, "IN ARRIVO", &tag, st.tiny, PRIMARY, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
                        at += text_width(hdc, "IN ARRIVO", st.tiny) + px(8, d);
                    }
                    // La versione, in piccolo accanto al nome.
                    if let Some(v) = version {
                        let tag = RECT { left: at, ..name_rc };
                        text(hdc, v, &tag, st.small, GREY, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
                    }
                    let sub = RECT { top: r.top + px(24, d), bottom: head - px(3, d), ..name_rc };
                    let line = format!("{} · {}", item.author, item.what);
                    text(hdc, &line, &sub, st.small, GREY, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
                    chevron(hdc, r.right - px(14, d), r.top + px(ITEM_H / 2, d), px(4, d), st.open.contains(item.name));
                    if st.open.contains(item.name) {
                        let body = r.bottom - px(8, d) - px(26, d);
                        let boxr = RECT { left: r.left + px(2, d), top: head, right: r.right - px(2, d), bottom: body };
                        crate::wingfx::round_rect(hdc, &boxr, px(5, d) as f32, Some(SIDE), Some((LINE, 1.0)));
                        let inner = RECT { left: boxr.left + px(10, d), top: boxr.top + px(8, d), right: boxr.right - px(10, d), bottom: boxr.bottom - px(8, d) };
                        text(hdc, credits::license_text(item.license), &inner, st.mono, TEXT, DT_LEFT | DT_TOP | DT_WORDBREAK);
                        let url = RECT { left: boxr.left + px(2, d), top: body + px(4, d), right: boxr.right, bottom: body + px(24, d) };
                        text(hdc, item.url, &url, st.small, PRIMARY, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
                    }
                }
            }
            true
        })
    }
}

/// Una freccetta: verso destra se la riga è chiusa, in giù se è aperta.
unsafe fn chevron(hdc: HDC, x: i32, y: i32, size: i32, open: bool) {
    let pen = CreatePen(PS_SOLID, 2, GREY);
    let old = SelectObject(hdc, pen.into());
    let (a, b, c) = if open {
        ((x - size, y - size / 2), (x, y + size / 2), (x + size, y - size / 2))
    } else {
        ((x - size / 2, y - size), (x + size / 2, y), (x - size / 2, y + size))
    };
    let _ = MoveToEx(hdc, a.0, a.1, None);
    let _ = LineTo(hdc, b.0, b.1);
    let _ = LineTo(hdc, c.0, c.1);
    SelectObject(hdc, old);
    let _ = DeleteObject(pen.into());
}

fn brush(color: COLORREF) -> HBRUSH {
    // Pennelli di sistema solo per i colori usati qui: si creano una volta sola.
    use std::sync::atomic::{AtomicIsize, Ordering};
    static SIDE_B: AtomicIsize = AtomicIsize::new(0);
    static LINE_B: AtomicIsize = AtomicIsize::new(0);
    let slot = if color == SIDE { &SIDE_B } else { &LINE_B };
    let mut raw = slot.load(Ordering::Relaxed);
    if raw == 0 {
        raw = unsafe { CreateSolidBrush(color) }.0 as isize;
        slot.store(raw, Ordering::Relaxed);
    }
    HBRUSH(raw as *mut _)
}

/// Apre o chiude la riga `index` e ne rifà l'altezza.
fn toggle(list: HWND, index: usize) {
    let height = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let st = s.as_mut()?;
        let Some(Row::Item(item, _)) = st.rows.get(index) else { return None };
        if !st.open.remove(item.name) {
            st.open.insert(item.name);
        }
        let dc = unsafe { GetDC(None) };
        let h = height_of(st, index, dc);
        unsafe { ReleaseDC(None, dc) };
        Some(h)
    });
    if let Some(h) = height {
        unsafe {
            SendMessageW(list, LB_SETITEMHEIGHT, Some(WPARAM(index)), Some(LPARAM(h as isize)));
            let _ = InvalidateRect(Some(list), None, true);
        }
    }
}

/// Il clic su una riga la apre; Invio e Spazio fanno lo stesso da tastiera.
unsafe extern "system" fn subclass(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
    let result = DefSubclassProc(hwnd, msg, wparam, lparam);
    let pressed = match msg {
        WM_LBUTTONDOWN => true,
        WM_KEYDOWN => matches!(wparam.0 as u16, 0x0D | 0x20), // VK_RETURN, VK_SPACE
        _ => false,
    };
    if pressed {
        let current = SendMessageW(hwnd, LB_GETCURSEL, None, None).0;
        if current >= 0 {
            toggle(hwnd, current as usize);
        }
    }
    result
}
