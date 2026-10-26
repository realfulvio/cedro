//! Il menu dell'icona con i colori e il carattere di Cedro: fondo bianco, voce sotto il mouse
//! grigio chiaro con gli angoli arrotondati, spunta blu. Le voci sono disegnate a mano (owner-draw).

use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::Gdi::*,
    Win32::UI::Controls::*,
    Win32::UI::HiDpi::GetDpiForSystem,
    Win32::UI::WindowsAndMessaging::*,
};

const WHITE: COLORREF = COLORREF(0x00FFFFFF);
const HOVER: COLORREF = COLORREF(0x00F7F5F3); // #F3F5F7
const LINE: COLORREF = COLORREF(0x00E5E5E5); // #E5E5E5
const TEXT: COLORREF = COLORREF(0x00191919);
const MUTED: COLORREF = COLORREF(0x00645A45); // #455A64
const PRIMARY: COLORREF = COLORREF(0x008E5E00); // #005E8E

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Header,
    Normal,
    Separator,
}

struct Entry {
    text: Vec<u16>,
    checked: bool,
    kind: Kind,
}

/// Un menu a comparsa con le voci disegnate da Cedro. Va tenuto vivo finché il menu è aperto.
pub struct Menu {
    pub handle: HMENU,
    entries: Vec<Box<Entry>>,
    background: HBRUSH,
}

fn px(value: i32) -> i32 {
    value * unsafe { GetDpiForSystem() } as i32 / 96
}

fn font(weight: i32, size: i32) -> HFONT {
    unsafe {
        CreateFontW(
            -px(size), 0, 0, 0, weight, 0, 0, 0,
            DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY,
            0, crate::winfont::face(),
        )
    }
}

impl Menu {
    pub fn new() -> Option<Menu> {
        unsafe {
            let handle = CreatePopupMenu().ok()?;
            let background = CreateSolidBrush(WHITE);
            let info = MENUINFO {
                cbSize: std::mem::size_of::<MENUINFO>() as u32,
                fMask: MIM_BACKGROUND,
                hbrBack: background,
                ..Default::default()
            };
            let _ = SetMenuInfo(handle, &info);
            Some(Menu { handle, entries: Vec::new(), background })
        }
    }

    fn push(&mut self, id: u32, text: &str, checked: bool, kind: Kind) {
        let entry = Box::new(Entry { text: text.encode_utf16().collect(), checked, kind });
        let data = &*entry as *const Entry as usize;
        unsafe {
            let _ = AppendMenuW(self.handle, MF_OWNERDRAW, id as usize, PCWSTR(data as *const u16));
        }
        self.entries.push(entry);
    }

    /// Una riga in alto, grigia e non cliccabile (per esempio la versione).
    pub fn header(&mut self, text: &str) {
        self.push(0, text, false, Kind::Header);
    }

    pub fn item(&mut self, id: u32, text: &str, checked: bool) {
        self.push(id, text, checked, Kind::Normal);
    }

    pub fn separator(&mut self) {
        self.push(0, "", false, Kind::Separator);
    }

    pub fn show(&self, owner: HWND, x: i32, y: i32) {
        unsafe {
            let _ = TrackPopupMenu(self.handle, TPM_RIGHTBUTTON, x, y, Some(0), owner, None);
        }
    }
}

impl Drop for Menu {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyMenu(self.handle);
            let _ = DeleteObject(self.background.into());
        }
    }
}

/// `WM_MEASUREITEM` di una voce del menu: dice quanto è larga e alta. Falso se non è una voce nostra.
pub fn measure(lparam: LPARAM) -> bool {
    unsafe {
        let mis = &mut *(lparam.0 as *mut MEASUREITEMSTRUCT);
        if mis.CtlType != ODT_MENU || mis.itemData == 0 {
            return false;
        }
        let entry = &*(mis.itemData as *const Entry);
        match entry.kind {
            Kind::Separator => {
                mis.itemWidth = 0;
                mis.itemHeight = px(9) as u32;
            }
            kind => {
                let dc = GetDC(None);
                let f = font(if kind == Kind::Header { 400 } else { 400 }, if kind == Kind::Header { 13 } else { 15 });
                let old = SelectObject(dc, f.into());
                let mut size = SIZE::default();
                let _ = GetTextExtentPoint32W(dc, &entry.text, &mut size);
                SelectObject(dc, old);
                let _ = DeleteObject(f.into());
                ReleaseDC(None, dc);
                mis.itemWidth = (size.cx + px(30 + 28)) as u32;
                mis.itemHeight = px(if kind == Kind::Header { 30 } else { 34 }) as u32;
            }
        }
        true
    }
}

/// `WM_DRAWITEM` di una voce del menu. Falso se non è una voce nostra.
pub fn draw(lparam: LPARAM) -> bool {
    unsafe {
        let dis = &*(lparam.0 as *const DRAWITEMSTRUCT);
        if dis.CtlType != ODT_MENU || dis.itemData == 0 {
            return false;
        }
        let entry = &*(dis.itemData as *const Entry);
        let hdc = dis.hDC;
        let r = dis.rcItem;
        let back = CreateSolidBrush(WHITE);
        FillRect(hdc, &r, back);
        let _ = DeleteObject(back.into());
        match entry.kind {
            Kind::Separator => {
                let mid = (r.top + r.bottom) / 2;
                let line = RECT { left: r.left + px(10), top: mid, right: r.right - px(10), bottom: mid + 1 };
                let brush = CreateSolidBrush(LINE);
                FillRect(hdc, &line, brush);
                let _ = DeleteObject(brush.into());
            }
            kind => {
                let selected = kind == Kind::Normal && dis.itemState.0 & ODS_SELECTED.0 != 0;
                if selected {
                    let inner = RECT { left: r.left + px(4), top: r.top + px(1), right: r.right - px(4), bottom: r.bottom - px(1) };
                    crate::wingfx::round_rect(hdc, &inner, px(4) as f32, Some(HOVER), None);
                }
                SetBkMode(hdc, TRANSPARENT);
                let f = font(400, if kind == Kind::Header { 13 } else { 15 });
                let old = SelectObject(hdc, f.into());
                SetTextColor(hdc, if kind == Kind::Header { MUTED } else { TEXT });
                let mut text = entry.text.clone();
                let mut area = RECT { left: r.left + px(30), ..r };
                DrawTextW(hdc, &mut text, &mut area, DT_SINGLELINE | DT_VCENTER | DT_LEFT | DT_NOPREFIX);
                if entry.checked {
                    SetTextColor(hdc, PRIMARY);
                    let mut tick: Vec<u16> = "✓".encode_utf16().collect();
                    let mut spot = RECT { left: r.left + px(12), ..r };
                    DrawTextW(hdc, &mut tick, &mut spot, DT_SINGLELINE | DT_VCENTER | DT_LEFT | DT_NOPREFIX);
                }
                SelectObject(hdc, old);
                let _ = DeleteObject(f.into());
            }
        }
        true
    }
}
