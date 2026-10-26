//! La scheda YouTube del pannello Conversioni su Windows: link, Video o Solo audio, un pulsante.
//! Le altre opzioni e il registro stanno in due riquadri chiusi. Sta dentro la finestra del pannello
//! (modulo figlio di `conversioni_win`) e ne nasconde i controlli finché è aperta.

use super::*;
use cedro::youtube::{self, Event, Request};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub const WM_APP_YT: u32 = WM_APP + 20;

const Y_TITLE: i32 = 5000;
const Y_HINT: i32 = 5001;
const Y_EDIT: i32 = 5002;
const Y_ADD: i32 = 5003;
const Y_COUNT: i32 = 5004;
const Y_QUEUE: i32 = 5005;
const Y_CAP_WHAT: i32 = 5006;
const Y_VIDEO: i32 = 5007;
const Y_AUDIO: i32 = 5008;
const Y_MORE: i32 = 5009;
const Y_LOGT: i32 = 5010;
const Y_LOG: i32 = 5011;
const Y_NOTE: i32 = 5012;
const Y_CAP_DEST: i32 = 5013;
const Y_DEST: i32 = 5014;
const Y_CHANGE: i32 = 5015;
const Y_GO: i32 = 5016;
const Y_BAR: i32 = 5017;
const Y_POWERED: i32 = 5018;
const Y_COPY: i32 = 5019;
const Y_L_QUALITY: i32 = 5020;
const Y_QUALITY: i32 = 5021;
const Y_L_FORMAT: i32 = 5022;
const Y_FORMAT: i32 = 5023;
const Y_L_SUBS: i32 = 5024;
const Y_SUBS: i32 = 5025;
const Y_THUMB: i32 = 5030;
const Y_CHAP: i32 = 5031;
const Y_SPLIT: i32 = 5032;
const Y_SPON: i32 = 5033;
const Y_NORM: i32 = 5034;
const Y_PLAY: i32 = 5035;
const Y_SKIP: i32 = 5036;

/// Le voci di «Altre opzioni», dall'alto: (id, vale per Video, vale per Solo audio).
const OPTION_IDS: &[(i32, bool, bool)] = &[
    (Y_QUALITY, true, true),
    (Y_FORMAT, true, true),
    (Y_SUBS, true, false),
    (Y_THUMB, true, true),
    (Y_CHAP, true, true),
    (Y_SPLIT, true, true),
    (Y_SPON, true, true),
    (Y_NORM, false, true),
    (Y_PLAY, true, true),
    (Y_SKIP, true, true),
];
/// Le etichette che stanno a sinistra dei due elenchi.
const OPTION_LABELS: &[(i32, i32)] = &[(Y_L_QUALITY, Y_QUALITY), (Y_L_FORMAT, Y_FORMAT), (Y_L_SUBS, Y_SUBS)];
const FIXED_IDS: &[i32] = &[Y_TITLE, Y_HINT, Y_EDIT, Y_ADD, Y_COUNT, Y_QUEUE, Y_CAP_WHAT, Y_VIDEO, Y_AUDIO, Y_MORE, Y_LOGT, Y_NOTE, Y_CAP_DEST, Y_DEST, Y_CHANGE, Y_GO, Y_POWERED];
/// I controlli della scheda «file» che si nascondono mentre c'è YouTube.
const LEGACY_IDS: &[i32] = &[
    ID_TITLE, ID_HINT, ID_DROP, ID_COUNT, ID_QUEUE, ID_CAP_FORMATS, ID_CAP_TOOLS, ID_CAP_QUALITY, ID_QUALITY, ID_QUALITY_VALUE, ID_CAP_DEST, ID_DEST, ID_CHANGE, ID_RESET, ID_GO,
    ID_FOOT, ID_POWERED, ID_CAP_OPTIONS, ID_EXISTS, ID_CAP_EXISTS,
];

const VIDEO_QUALITY: &[(&str, u32)] = &[("Migliore", 0), ("Fino a 1080p · consigliato", 1080), ("720p · file leggero", 720), ("480p · file piccolo", 480)];
const AUDIO_QUALITY: &[(&str, u32)] = &[("Migliore", 0), ("Buona · consigliata", 1), ("Leggera", 2)];
const VIDEO_FORMATS: &[&str] = &["mp4", "mkv", "webm"];
const AUDIO_FORMATS: &[&str] = &["mp3", "m4a", "opus", "wav", "flac"];
const SUBTITLES: &[(&str, u32)] = &[("Nessuno", 0), ("Italiano", 1), ("Italiano e inglese", 2), ("Tutti", 3)];

const BASE_HEIGHT: i32 = 636;
/// L'altezza della nota sotto i riquadri: due righe di testo.
const NOTE_H: i32 = 46;
const LEFT: i32 = 210;
const WIDTH: i32 = 670;

struct Row {
    url: String,
    title: String,
    detail: String,
    status: String,
    /// 0 in attesa o in corso, 1 fatto, 2 errore.
    state: u8,
}

enum Msg {
    Event(Event),
    Finished(std::result::Result<Vec<PathBuf>, String>),
}

struct Page {
    shown: bool,
    rows: Vec<Row>,
    audio: bool,
    more: bool,
    log_open: bool,
    dest: PathBuf,
    running: bool,
    progress: f32,
    note: String,
    log: String,
    cancel: Arc<AtomicBool>,
    /// Posizioni scelte negli elenchi di Video e di Solo audio.
    quality_sel: [usize; 2],
    format_sel: [usize; 2],
}

static PAGE: Mutex<Option<Page>> = Mutex::new(None);
static EVENTS: Mutex<Vec<Msg>> = Mutex::new(Vec::new());

fn page() -> std::sync::MutexGuard<'static, Option<Page>> {
    PAGE.lock().unwrap_or_else(|e| e.into_inner())
}

fn with<R>(f: impl FnOnce(&mut Page) -> R) -> Option<R> {
    page().as_mut().map(f)
}

fn downloads_dir() -> PathBuf {
    std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join("Downloads")).unwrap_or_else(|| PathBuf::from("."))
}

pub fn active() -> bool {
    with(|p| p.shown).unwrap_or(false)
}

pub fn busy() -> bool {
    with(|p| p.running).unwrap_or(false)
}

pub fn owns(id: i32) -> bool {
    (5000..5100).contains(&id)
}

unsafe fn ctl(hwnd: HWND, id: i32) -> HWND {
    item(hwnd, id)
}

unsafe fn put(hwnd: HWND, id: i32, x: i32, y: i32, w: i32, h: i32) {
    let c = ctl(hwnd, id);
    let _ = MoveWindow(c, px(x), px(y), px(w), px(h), true);
}

unsafe fn show_id(hwnd: HWND, id: i32, on: bool) {
    let _ = ShowWindow(ctl(hwnd, id), if on { SW_SHOW } else { SW_HIDE });
}

unsafe fn text_of(hwnd: HWND, id: i32) -> String {
    let mut buf = [0u16; 4096];
    let n = GetWindowTextW(ctl(hwnd, id), &mut buf) as usize;
    String::from_utf16_lossy(&buf[..n])
}

unsafe fn checked(hwnd: HWND, id: i32) -> bool {
    SendMessageW(ctl(hwnd, id), BM_GETCHECK, Some(WPARAM(0)), Some(LPARAM(0))).0 == 1
}

unsafe fn set_checked(hwnd: HWND, id: i32, on: bool) {
    SendMessageW(ctl(hwnd, id), BM_SETCHECK, Some(WPARAM(on as usize)), Some(LPARAM(0)));
}

unsafe fn combo_set(hwnd: HWND, id: i32, items: &[String], selected: usize) {
    let c = ctl(hwnd, id);
    SendMessageW(c, CB_RESETCONTENT, Some(WPARAM(0)), Some(LPARAM(0)));
    for s in items {
        SendMessageW(c, CB_ADDSTRING, Some(WPARAM(0)), Some(LPARAM(HSTRING::from(s.as_str()).as_ptr() as isize)));
    }
    SendMessageW(c, CB_SETCURSEL, Some(WPARAM(selected)), Some(LPARAM(0)));
}

unsafe fn combo_get(hwnd: HWND, id: i32) -> usize {
    SendMessageW(ctl(hwnd, id), CB_GETCURSEL, Some(WPARAM(0)), Some(LPARAM(0))).0.max(0) as usize
}

/// Crea tutti i controlli della scheda, nascosti. Si chiama una volta, alla creazione del pannello.
pub unsafe fn build(hwnd: HWND, instance: HINSTANCE) {
    *page() = Some(Page {
        shown: false,
        rows: Vec::new(),
        audio: false,
        more: false,
        log_open: false,
        dest: downloads_dir(),
        running: false,
        progress: 0.0,
        note: String::new(),
        log: String::new(),
        cancel: Arc::new(AtomicBool::new(false)),
        quality_sel: [1, 1],
        format_sel: [0, 0],
    });
    let (f, fb, ft, fs) = (font(&FONT), font(&FONT_BOLD), font(&FONT_TITLE), font(&FONT_SMALL));
    let s = w!("STATIC");
    let b = w!("BUTTON");
    let tab = WS_TABSTOP.0;
    let hidden = |id: i32| {
        let _ = ShowWindow(GetDlgItem(Some(hwnd), id).unwrap_or_default(), SW_HIDE);
    };
    child(hwnd, instance, s, "YouTube", 0, Y_TITLE, (LEFT, 44, WIDTH, 32), ft);
    child(hwnd, instance, s, "Incolla un link e scarica il video oppure solo l'audio.", 0, Y_HINT, (LEFT, 78, WIDTH, 22), f);
    let edit = child(hwnd, instance, w!("EDIT"), "", tab | WS_BORDER.0 | 0x80 /* ES_AUTOHSCROLL */, Y_EDIT, (LEFT, 104, 560, 32), f);
    SendMessageW(edit, EM_SETCUEBANNER, Some(WPARAM(1)), Some(LPARAM(w!("Incolla qui il link di YouTube").as_ptr() as isize)));
    child(hwnd, instance, b, "Aggiungi", tab | BS_OWNERDRAW as u32, Y_ADD, (LEFT + 568, 104, 102, 32), fb);
    child(hwnd, instance, s, "", 0, Y_COUNT, (LEFT, 146, WIDTH, 16), fs);
    child(hwnd, instance, w!("LISTBOX"), "", tab | LBS_OWNERDRAWFIXED as u32 | LBS_NOINTEGRALHEIGHT as u32 | WS_VSCROLL.0 | WS_BORDER.0, Y_QUEUE, (LEFT, 164, WIDTH, 120), f);
    SendMessageW(ctl(hwnd, Y_QUEUE), LB_SETITEMHEIGHT, Some(WPARAM(0)), Some(LPARAM(px(42) as isize)));
    child(hwnd, instance, s, "COSA VUOI SCARICARE", 0, Y_CAP_WHAT, (LEFT, 290, WIDTH, 16), fs);
    child(hwnd, instance, b, "", tab | BS_OWNERDRAW as u32, Y_VIDEO, (LEFT, 308, 330, 58), fb);
    child(hwnd, instance, b, "", tab | BS_OWNERDRAW as u32, Y_AUDIO, (LEFT + 340, 308, 330, 58), fb);
    child(hwnd, instance, b, "Altre opzioni", tab | BS_OWNERDRAW as u32, Y_MORE, (LEFT, 374, WIDTH, 28), fb);
    child(hwnd, instance, b, "Cosa sta succedendo", tab | BS_OWNERDRAW as u32, Y_LOGT, (LEFT, 404, WIDTH, 28), fb);
    child(hwnd, instance, b, "Copia", tab, Y_COPY, (LEFT + WIDTH - 70, 408, 70, 24), f);
    trascrizione::make_log(hwnd, instance, Y_LOG, (LEFT, 434, WIDTH, 120));
    child(hwnd, instance, s, "", 0, Y_NOTE, (LEFT, 440, WIDTH, 36), f);
    child(hwnd, instance, s, "DOVE SALVARE", 0, Y_CAP_DEST, (LEFT, 482, 300, 16), fs);
    child(hwnd, instance, s, "", 0x200 | 0x8000 /* SS_CENTERIMAGE | SS_PATHELLIPSIS */, Y_DEST, (LEFT, 500, 330, 34), f);
    child(hwnd, instance, b, "Cambia…", tab, Y_CHANGE, (LEFT + 340, 500, 80, 34), f);
    child(hwnd, instance, b, "Scarica video", tab | BS_OWNERDRAW as u32, Y_GO, (LEFT + 430, 500, 240, 38), fb);
    child(hwnd, instance, s, "", 0xD /* SS_OWNERDRAW */, Y_BAR, (LEFT, 633, WIDTH, 3), f);
    child(hwnd, instance, s, POWERED_BY, 0, Y_POWERED, (LEFT, 560, 420, 18), f);
    for (label_id, combo_id) in OPTION_LABELS {
        let text = match *combo_id {
            Y_QUALITY => "Qualità",
            Y_FORMAT => "Salva come",
            _ => "Sottotitoli",
        };
        child(hwnd, instance, s, text, 0x200, *label_id, (LEFT, 0, 320, 26), f);
        child(hwnd, instance, w!("COMBOBOX"), "", tab | CBS_DROPDOWNLIST as u32, *combo_id, (LEFT + 330, 0, 300, 220), f);
    }
    let checks: &[(i32, &str, bool)] = &[
        (Y_THUMB, "Copertina nel file", true),
        (Y_CHAP, "Capitoli e descrizione nel file", true),
        (Y_SPLIT, "Dividi per capitoli", false),
        (Y_SPON, "Salta le parti sponsorizzate", false),
        (Y_NORM, "Livella il volume", false),
        (Y_PLAY, "Playlist: scarica tutta", true),
        (Y_SKIP, "Salta i già scaricati", true),
    ];
    for (id, text, on) in checks {
        child(hwnd, instance, b, text, tab | BS_AUTOCHECKBOX as u32, *id, (LEFT, 0, WIDTH, 26), f);
        set_checked(hwnd, *id, *on);
    }
    for id in FIXED_IDS.iter().chain([Y_LOG, Y_COPY, Y_BAR].iter()).copied().chain(OPTION_IDS.iter().map(|o| o.0)).chain(OPTION_LABELS.iter().map(|o| o.0)) {
        hidden(id);
    }
    refresh_choices(hwnd);
}

/// Le scelte che cambiano tra Video e Solo audio.
unsafe fn refresh_choices(hwnd: HWND) {
    let (audio, q, f) = with(|p| (p.audio, p.quality_sel, p.format_sel)).unwrap_or((false, [1, 1], [0, 0]));
    let i = audio as usize;
    let quality: Vec<String> = if audio { AUDIO_QUALITY.iter().map(|x| x.0.to_string()).collect() } else { VIDEO_QUALITY.iter().map(|x| x.0.to_string()).collect() };
    let formats: Vec<String> = if audio { AUDIO_FORMATS } else { VIDEO_FORMATS }.iter().map(|x| x.to_uppercase()).collect();
    combo_set(hwnd, Y_QUALITY, &quality, q[i]);
    combo_set(hwnd, Y_FORMAT, &formats, f[i]);
    let subs: Vec<String> = SUBTITLES.iter().map(|x| x.0.to_string()).collect();
    if SendMessageW(ctl(hwnd, Y_SUBS), CB_GETCOUNT, Some(WPARAM(0)), Some(LPARAM(0))).0 <= 0 {
        combo_set(hwnd, Y_SUBS, &subs, 0);
    }
}

/// Le misure verticali della scheda: (inizio della coda, spazio sotto la coda, altezza di «Altre opzioni», altezza del registro).
fn metrics(audio: bool, more: bool, log_open: bool) -> (i32, i32, i32, i32) {
    let count = OPTION_IDS.iter().filter(|(_, v, a)| if audio { *a } else { *v }).count() as i32;
    // Le opzioni stanno su due colonne, per non allungare troppo la finestra.
    let options_h = if more { (count + 1) / 2 * 28 + 8 } else { 0 };
    let log_h = if log_open { 104 } else { 0 };
    let top = 164;
    // Sotto la coda: titolo, due scelte, i due riquadri, nota (due righe), destinazione, firma e un po' d'aria.
    let below = 6 + 18 + 58 + 10 + 28 + options_h + 28 + log_h + 6 + NOTE_H + 8 + 18 + 46 + 18 + 12;
    (top, below, options_h, log_h)
}

/// L'altezza minima dell'area interna con i riquadri aperti di ora (px di progetto).
pub fn min_height() -> i32 {
    let (audio, more, log_open) = with(|p| (p.audio, p.more, p.log_open)).unwrap_or((false, false, false));
    let (top, below, _, _) = metrics(audio, more, log_open);
    BASE_HEIGHT.max(top + 44 + below)
}

/// Mette ogni controllo al suo posto e adatta la finestra a ciò che è aperto e alla misura scelta dall'utente.
pub unsafe fn layout(hwnd: HWND) {
    let Some((audio, more, log_open, shown)) = with(|p| (p.audio, p.more, p.log_open, p.shown)) else { return };
    if !shown {
        return;
    }
    let visible_options: Vec<i32> = OPTION_IDS.iter().filter(|(_, v, a)| if audio { *a } else { *v }).map(|o| o.0).collect();
    let (top, below, options_h, log_h) = metrics(audio, more, log_open);
    // La larghezza in più data dall'utente va al contenuto; l'altezza in più alla coda.
    let (user_w, user_h) = user_size();
    let dx = (user_w - BASE_W).max(0);
    set_extra(hwnd, dx, extra().1);
    let w = WIDTH + dx;
    let client_h = BASE_HEIGHT.max(top + 44 + below).max(user_h);
    let queue_h = client_h - top - below;
    put(hwnd, Y_TITLE, LEFT, 44, w, 32);
    put(hwnd, Y_HINT, LEFT, 78, w, 22);
    put(hwnd, Y_EDIT, LEFT, 104, w - 110, 32);
    put(hwnd, Y_ADD, LEFT + w - 102, 104, 102, 32);
    put(hwnd, Y_COUNT, LEFT, 146, w, 16);
    put(hwnd, Y_QUEUE, LEFT, top, w, queue_h);
    let mut y = top + queue_h + 6;
    put(hwnd, Y_CAP_WHAT, LEFT, y, w, 16);
    y += 18;
    let half = (w - 10) / 2;
    put(hwnd, Y_VIDEO, LEFT, y, half, 58);
    put(hwnd, Y_AUDIO, LEFT + w - half, y, half, 58);
    y += 58 + 10;
    put(hwnd, Y_MORE, LEFT, y, w, 28);
    y += 28;
    for (id, v, a) in OPTION_IDS {
        let on = more && (if audio { *a } else { *v });
        show_id(hwnd, *id, on);
        if let Some((label, _)) = OPTION_LABELS.iter().find(|(_, c)| c == id) {
            show_id(hwnd, *label, on);
        }
    }
    if more {
        y += 4;
        let col = w / 2;
        for (i, id) in visible_options.iter().enumerate() {
            let x = LEFT + 6 + (i as i32 % 2) * col;
            let row_y = y + (i as i32 / 2) * 28;
            if matches!(*id, Y_QUALITY | Y_FORMAT | Y_SUBS) {
                let label = OPTION_LABELS.iter().find(|(_, c)| c == id).map(|(l, _)| *l).unwrap_or(0);
                put(hwnd, label, x, row_y, 108, 26);
                put(hwnd, *id, x + 110, row_y, col - 122, 220);
            } else {
                put(hwnd, *id, x, row_y, col - 10, 26);
            }
        }
        y += (visible_options.len() as i32 + 1) / 2 * 28;
        y += 4;
    }
    let _ = options_h;
    put(hwnd, Y_LOGT, LEFT, y, w, 28);
    y += 28;
    show_id(hwnd, Y_LOG, log_open);
    show_id(hwnd, Y_COPY, log_open);
    if log_open {
        put(hwnd, Y_COPY, LEFT + w - 70, y - 26, 70, 24);
        put(hwnd, Y_LOG, LEFT, y + 2, w, 96);
        y += log_h;
    }
    y += 6;
    put(hwnd, Y_NOTE, LEFT, y, w, NOTE_H);
    y += NOTE_H + 8;
    put(hwnd, Y_CAP_DEST, LEFT, y, 300, 16);
    y += 18;
    put(hwnd, Y_DEST, LEFT, y, w - 340, 36);
    put(hwnd, Y_CHANGE, LEFT + w - 330, y + 1, 80, 34);
    put(hwnd, Y_GO, LEFT + w - 240, y, 240, 38);
    y += 46;
    put(hwnd, Y_POWERED, LEFT, y, 420, 18);
    put(hwnd, Y_BAR, LEFT, client_h - 3, w, 3);
    put(hwnd, ID_SIDE, 0, 40, 190, client_h - 40);
    put(hwnd, ID_HEADER, 0, 0, BASE_W + dx, 40);
    resize_client(hwnd, client_h);
    let _ = InvalidateRect(Some(hwnd), None, true);
}

/// Si apre la scheda: nasconde i controlli dei file e mostra quelli di YouTube.
pub unsafe fn enter(hwnd: HWND) {
    if active() {
        return;
    }
    with(|p| p.shown = true);
    for id in LEGACY_IDS {
        show_id(hwnd, *id, false);
    }
    for (base, n) in [(ID_FORMAT0, 24), (ID_TOOL0, 24), (OPT_CTL0, 12), (OPT_LABEL0, 12), (OPT_VALUE0, 12)] {
        for i in 0..n {
            if let Ok(c) = GetDlgItem(Some(hwnd), base + i) {
                let _ = ShowWindow(c, SW_HIDE);
            }
        }
    }
    for id in FIXED_IDS {
        show_id(hwnd, *id, true);
    }
    show_id(hwnd, Y_BAR, busy());
    refresh_texts(hwnd);
    layout(hwnd);
    let _ = InvalidateRect(Some(ctl(hwnd, ID_SIDE)), None, true);
}

/// Si torna a una categoria di file: i controlli dei file si ricreano con `render_category`.
pub unsafe fn leave(hwnd: HWND) {
    with(|p| p.shown = false);
    for id in FIXED_IDS.iter().chain([Y_LOG, Y_COPY, Y_BAR].iter()).copied().chain(OPTION_IDS.iter().map(|o| o.0)).chain(OPTION_LABELS.iter().map(|o| o.0)) {
        show_id(hwnd, id, false);
    }
    for id in LEGACY_IDS {
        show_id(hwnd, *id, true);
    }
}

unsafe fn refresh_texts(hwnd: HWND) {
    let (n, running, audio, note, dest, log) = with(|p| (p.rows.len(), p.running, p.audio, p.note.clone(), p.dest.clone(), p.log.clone())).unwrap_or_default();
    set_text(
        hwnd,
        Y_COUNT,
        &match n {
            0 => "CODA · NESSUN LINK".to_string(),
            1 => "CODA · 1 LINK".to_string(),
            n => format!("CODA · {n} LINK"),
        },
    );
    let what = if audio { "audio" } else { "video" };
    set_text(
        hwnd,
        Y_GO,
        &if running {
            "Annulla".to_string()
        } else {
            match n {
                0 | 1 => format!("Scarica {what}"),
                n => format!("Scarica {n} link ({what})"),
            }
        },
    );
    let default_note = youtube::first_use_note().unwrap_or_default();
    set_text(hwnd, Y_NOTE, if note.is_empty() { &default_note } else { &note });
    set_text(hwnd, Y_DEST, &dest.display().to_string());
    if log.is_empty() {
        trascrizione::set_log_hint(ctl(hwnd, Y_LOG), "Qui vedrai i passi del download man mano che Cedro li fa.");
    } else {
        let crlf = log.replace('\n', "\r\n");
        trascrizione::set_log_text(ctl(hwnd, Y_LOG), &crlf);
    }
    let list = ctl(hwnd, Y_QUEUE);
    SendMessageW(list, LB_RESETCONTENT, Some(WPARAM(0)), Some(LPARAM(0)));
    for i in 0..n {
        let placeholder = HSTRING::from(format!("{i}"));
        SendMessageW(list, LB_ADDSTRING, Some(WPARAM(0)), Some(LPARAM(placeholder.as_ptr() as isize)));
    }
    let _ = EnableWindow(ctl(hwnd, Y_ADD), !running);
    let _ = InvalidateRect(Some(hwnd), None, true);
}

fn log_line(p: &mut Page, text: &str) {
    p.log.push_str(text);
    p.log.push('\n');
    if p.log.len() > 40_000 {
        let cut = p.log.len() - 30_000;
        let at = p.log.char_indices().map(|(i, _)| i).find(|i| *i >= cut).unwrap_or(0);
        p.log.drain(..at);
    }
}

unsafe fn add_links(hwnd: HWND, text: &str) -> usize {
    let mut added = 0;
    with(|p| {
        for url in youtube::urls_in(text) {
            if p.rows.iter().any(|r| r.url == url) {
                continue;
            }
            p.rows.push(Row { url, title: String::new(), detail: String::new(), status: "In attesa".into(), state: 0 });
            added += 1;
        }
    });
    refresh_texts(hwnd);
    added
}

unsafe fn request(hwnd: HWND) -> Request {
    let (urls, audio, dest) = with(|p| (p.rows.iter().map(|r| r.url.clone()).collect::<Vec<_>>(), p.audio, p.dest.clone())).unwrap_or_default();
    let mut r = Request::new(urls, audio, dest);
    let (q, f, s) = (combo_get(hwnd, Y_QUALITY), combo_get(hwnd, Y_FORMAT), combo_get(hwnd, Y_SUBS));
    r.quality = if audio { AUDIO_QUALITY.get(q).map(|x| x.1).unwrap_or(1) } else { VIDEO_QUALITY.get(q).map(|x| x.1).unwrap_or(1080) };
    let formats = if audio { AUDIO_FORMATS } else { VIDEO_FORMATS };
    r.format = formats.get(f).copied().unwrap_or(formats[0]).to_string();
    r.subtitles = if audio { 0 } else { SUBTITLES.get(s).map(|x| x.1).unwrap_or(0) };
    r.thumbnail = checked(hwnd, Y_THUMB);
    r.chapters = checked(hwnd, Y_CHAP);
    r.split_chapters = checked(hwnd, Y_SPLIT);
    r.skip_sponsors = checked(hwnd, Y_SPON);
    r.normalize = audio && checked(hwnd, Y_NORM);
    r.playlist = checked(hwnd, Y_PLAY);
    r.skip_existing = checked(hwnd, Y_SKIP);
    r
}

unsafe fn start(hwnd: HWND) {
    if busy() {
        with(|p| p.cancel.store(true, std::sync::atomic::Ordering::Relaxed));
        set_text(hwnd, Y_GO, "Annullo…");
        let _ = EnableWindow(ctl(hwnd, Y_GO), false);
        return;
    }
    let typed = text_of(hwnd, Y_EDIT);
    if !typed.trim().is_empty() && add_links(hwnd, &typed) > 0 {
        set_text(hwnd, Y_EDIT, "");
    }
    if with(|p| p.rows.is_empty()).unwrap_or(true) {
        with(|p| p.note = "Incolla prima un link di YouTube.".into());
        refresh_texts(hwnd);
        return;
    }
    let req = request(hwnd);
    let cancel = Arc::new(AtomicBool::new(false));
    with(|p| {
        for r in p.rows.iter_mut() {
            r.status = "In attesa".into();
            r.state = 0;
        }
        p.running = true;
        p.progress = 0.0;
        p.note.clear();
        p.cancel = cancel.clone();
        // Come nella trascrizione, mentre lavora si vede cosa sta succedendo.
        p.log_open = true;
        p.more = false;
    });
    show_id(hwnd, Y_BAR, true);
    refresh_texts(hwnd);
    layout(hwnd);
    let raw = hwnd.0 as isize;
    std::thread::spawn(move || {
        let post = |m: Msg| {
            let first = {
                let mut q = EVENTS.lock().unwrap_or_else(|e| e.into_inner());
                q.push(m);
                q.len() == 1
            };
            if first {
                unsafe {
                    let _ = PostMessageW(Some(HWND(raw as *mut _)), WM_APP_YT, WPARAM(0), LPARAM(0));
                }
            }
        };
        let total = req.urls.len();
        crate::progress_win::start(total, &format!("YouTube · {total} link"));
        let mut last = usize::MAX;
        let result = youtube::run(
            &req,
            &mut |e| {
                match &e {
                    Event::Progress { index, label, .. } if *index != last => {
                        last = *index;
                        crate::progress_win::step(*index, &format!("{} di {total} · {label}", index + 1));
                    }
                    Event::File(path) => crate::progress_win::produced(path),
                    _ => {}
                }
                post(Msg::Event(e));
            },
            &cancel,
        );
        match &result {
            Ok(files) => crate::progress_win::finish(true, &format!("✓ Fatto · {} file da YouTube", files.len())),
            Err(e) if cancel.load(std::sync::atomic::Ordering::Relaxed) && e == "Annullato" => crate::progress_win::dismiss(),
            Err(e) => crate::progress_win::finish(false, &format!("Errore · {e}")),
        }
        post(Msg::Finished(result));
    });
}

/// Applica al pannello gli eventi che il thread del download ha lasciato.
pub unsafe fn pump(hwnd: HWND) {
    let events: Vec<Msg> = std::mem::take(&mut *EVENTS.lock().unwrap_or_else(|e| e.into_inner()));
    let mut finished = false;
    with(|p| {
        for m in events {
            match m {
                Msg::Event(Event::Log(text)) => log_line(p, &text),
                Msg::Event(Event::Progress { index, total, percent, label }) => {
                    p.progress = ((index as f32 + percent / 100.0) / total.max(1) as f32).clamp(0.0, 1.0);
                    if let Some(r) = p.rows.get_mut(index) {
                        r.status = format!("{percent:.0}%");
                        r.state = 0;
                    }
                    p.note = label;
                }
                Msg::Event(Event::File(_)) => {}
                Msg::Event(Event::Info { index, info }) => {
                    if let Some(r) = p.rows.get_mut(index) {
                        r.detail = match info.entries {
                            Some(n) => format!("{} · playlist di {n} video", info.author),
                            None => format!("{} · {}", info.author, youtube::clock(info.seconds)),
                        };
                        r.title = info.title;
                    }
                }
                Msg::Event(Event::LinkDone { index, result }) => {
                    if let Some(r) = p.rows.get_mut(index) {
                        match result {
                            Ok(n) => (r.status, r.state) = (if n > 1 { format!("Fatto · {n} file") } else { "Fatto".into() }, 1),
                            Err(e) => (r.status, r.state) = (format!("Errore · {e}"), 2),
                        }
                    }
                }
                Msg::Finished(result) => {
                    finished = true;
                    p.running = false;
                    match result {
                        Ok(files) => {
                            p.note = format!("Fatto: {} file in {}", files.len(), p.dest.display());
                            log_line(p, "Finito");
                        }
                        Err(e) => {
                            log_line(p, &e);
                            p.note = e;
                        }
                    }
                }
            }
        }
    });
    if finished {
        show_id(hwnd, Y_BAR, false);
        let _ = EnableWindow(ctl(hwnd, Y_GO), true);
    }
    refresh_texts(hwnd);
    let _ = InvalidateRect(ctl(hwnd, Y_BAR).into(), None, false);
}

/// I clic sui controlli della scheda.
pub unsafe fn command(hwnd: HWND, id: i32, code: u32) {
    match id {
        Y_ADD => {
            let text = text_of(hwnd, Y_EDIT);
            if add_links(hwnd, &text) > 0 {
                set_text(hwnd, Y_EDIT, "");
                with(|p| p.note.clear());
            } else {
                with(|p| p.note = "Questo non sembra un link di YouTube.".into());
            }
            refresh_texts(hwnd);
        }
        Y_VIDEO | Y_AUDIO => {
            let audio = id == Y_AUDIO;
            if with(|p| std::mem::replace(&mut p.audio, audio) != audio).unwrap_or(false) {
                refresh_choices(hwnd);
                layout(hwnd);
                refresh_texts(hwnd);
            }
        }
        Y_MORE => {
            // Un riquadro alla volta: due aperti insieme non starebbero nello schermo.
            with(|p| {
                p.more = !p.more;
                if p.more {
                    p.log_open = false;
                }
            });
            layout(hwnd);
        }
        Y_COPY => trascrizione::copy_log(hwnd, ctl(hwnd, Y_LOG)),
        Y_LOGT => {
            with(|p| {
                p.log_open = !p.log_open;
                if p.log_open {
                    p.more = false;
                }
            });
            layout(hwnd);
        }
        Y_QUALITY | Y_FORMAT if code == CBN_SELCHANGE => {
            let (q, f) = (combo_get(hwnd, Y_QUALITY), combo_get(hwnd, Y_FORMAT));
            with(|p| {
                let i = p.audio as usize;
                p.quality_sel[i] = q;
                p.format_sel[i] = f;
            });
            refresh_texts(hwnd);
        }
        Y_CHANGE => {
            if let Some(path) = pick(hwnd, true).into_iter().next() {
                with(|p| p.dest = path);
                refresh_texts(hwnd);
            }
        }
        Y_GO => start(hwnd),
        _ => {}
    }
}

/// I colori dei testi fissi della scheda.
pub fn color(id: i32) -> Option<COLORREF> {
    match id {
        Y_TITLE => Some(PRIMARY),
        Y_HINT | Y_COUNT | Y_CAP_WHAT | Y_CAP_DEST | Y_POWERED | Y_NOTE => Some(GREY),
        x if owns(x) => Some(TEXT),
        _ => None,
    }
}

unsafe fn fill(hdc: HDC, rc: &RECT, color: COLORREF) {
    let b = CreateSolidBrush(color);
    FillRect(hdc, rc, b);
    let _ = DeleteObject(HGDIOBJ(b.0));
}

unsafe fn frame(hdc: HDC, rc: &RECT, color: COLORREF, thick: i32) {
    let b = CreateSolidBrush(color);
    for i in 0..thick {
        let r = RECT { left: rc.left + i, top: rc.top + i, right: rc.right - i, bottom: rc.bottom - i };
        FrameRect(hdc, &r, b);
    }
    let _ = DeleteObject(HGDIOBJ(b.0));
}

/// Il disegno dei controlli della scheda.
pub unsafe fn draw(di: &DRAWITEMSTRUCT) {
    let hdc = di.hDC;
    let mut rc = di.rcItem;
    let id = di.CtlID as i32;
    let selected = di.itemState.0 & ODS_SELECTED.0 != 0;
    let disabled = di.itemState.0 & ODS_DISABLED.0 != 0;
    SetBkMode(hdc, TRANSPARENT);
    match id {
        Y_ADD | Y_GO => {
            fill(hdc, &rc, if disabled { LINE } else if selected { HOVER } else { PRIMARY });
            SelectObject(hdc, HGDIOBJ(font(&FONT_BOLD).0));
            SetTextColor(hdc, if disabled { GREY } else { WHITE });
            let mut text = [0u16; 128];
            let n = GetWindowTextW(di.hwndItem, &mut text) as usize;
            draw_text(hdc, &String::from_utf16_lossy(&text[..n]), &mut rc, DT_SINGLELINE | DT_VCENTER | DT_CENTER | DT_END_ELLIPSIS);
        }
        Y_VIDEO | Y_AUDIO => {
            let audio = with(|p| p.audio).unwrap_or(false);
            let on = (id == Y_AUDIO) == audio;
            fill(hdc, &rc, if on { DROP_BG } else { WHITE });
            frame(hdc, &rc, if on { PRIMARY } else if selected { HOVER } else { LINE }, 2);
            let (title, note) = if id == Y_AUDIO { ("Solo audio", "Per ascoltare, senza immagini") } else { ("Video", "Immagini e suono, come su YouTube") };
            let mut t = RECT { left: rc.left + px(14), top: rc.top + px(7), right: rc.right - px(8), bottom: rc.top + px(32) };
            SelectObject(hdc, HGDIOBJ(font(&FONT_BOLD).0));
            SetTextColor(hdc, PRIMARY);
            draw_text(hdc, title, &mut t, DT_SINGLELINE | DT_LEFT | DT_VCENTER);
            let mut n = RECT { left: rc.left + px(14), top: rc.top + px(32), right: rc.right - px(8), bottom: rc.bottom - px(4) };
            SelectObject(hdc, HGDIOBJ(font(&FONT).0));
            SetTextColor(hdc, GREY);
            draw_text(hdc, note, &mut n, DT_SINGLELINE | DT_LEFT | DT_VCENTER | DT_END_ELLIPSIS);
        }
        Y_MORE | Y_LOGT => {
            fill(hdc, &rc, WHITE);
            let open = with(|p| if id == Y_MORE { p.more } else { p.log_open }).unwrap_or(false);
            let line = RECT { top: rc.bottom - 1, ..rc };
            fill(hdc, &line, LINE);
            SelectObject(hdc, HGDIOBJ(font(&FONT_BOLD).0));
            SetTextColor(hdc, if selected { HOVER } else { PRIMARY });
            rc.left += px(4);
            let label = if id == Y_MORE { "Altre opzioni" } else { "Cosa sta succedendo" };
            draw_text(hdc, &format!("{}  {label}", if open { "▾" } else { "▸" }), &mut rc, DT_SINGLELINE | DT_VCENTER | DT_LEFT);
        }
        Y_BAR => {
            fill(hdc, &rc, LINE);
            let p = with(|p| p.progress).unwrap_or(0.0);
            let done = RECT { right: rc.left + ((rc.right - rc.left) as f32 * p) as i32, ..rc };
            fill(hdc, &done, PRIMARY);
        }
        Y_QUEUE => {
            if di.itemID == u32::MAX {
                return;
            }
            fill(hdc, &rc, WHITE);
            let row = with(|p| p.rows.get(di.itemID as usize).map(|r| (r.url.clone(), r.title.clone(), r.detail.clone(), r.status.clone(), r.state))).flatten();
            let Some((url, title, detail, status, state)) = row else { return };
            SelectObject(hdc, HGDIOBJ(font(&FONT).0));
            SetTextColor(hdc, match state { 1 => OK, 2 => FAIL, _ => GREY });
            let mut s = RECT { left: rc.left + (rc.right - rc.left) / 2, right: rc.right - px(10), ..rc };
            draw_text(hdc, &status, &mut s, DT_SINGLELINE | DT_VCENTER | DT_RIGHT | DT_END_ELLIPSIS);
            SetTextColor(hdc, TEXT);
            let mut t = RECT { left: rc.left + px(10), right: s.left - px(8), top: rc.top + px(3), bottom: rc.top + px(23) };
            draw_text(hdc, if title.is_empty() { &url } else { &title }, &mut t, DT_SINGLELINE | DT_VCENTER | DT_LEFT | DT_END_ELLIPSIS);
            if !detail.is_empty() {
                SelectObject(hdc, HGDIOBJ(font(&FONT_SMALL).0));
                SetTextColor(hdc, GREY);
                let mut d = RECT { left: rc.left + px(10), right: s.left - px(8), top: rc.top + px(23), bottom: rc.bottom - px(2) };
                draw_text(hdc, &detail, &mut d, DT_SINGLELINE | DT_VCENTER | DT_LEFT | DT_END_ELLIPSIS);
            }
        }
        _ => {}
    }
}
