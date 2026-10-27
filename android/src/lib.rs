//! Cedro per Android, prototipo: una NativeActivity senza Java né Kotlin. Disegna la ruota di
//! `cedro::ui` (la Semplificata, in verticale) con tiny-skia su tutto lo schermo; dal menu
//! «Condividi» riceve un'immagine o un PDF (`content://` letto via JNI/ContentResolver) e, al tocco
//! di una fetta, la converte col motore di sempre e la salva in Documenti.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use android_activity::input::{InputEvent, MotionAction};
use android_activity::{AndroidApp, InputStatus, MainEvent, PollEvent};
use cedro::ui::gfx::{Align, Fonts, GlyphCache, Painter, Weight};
use cedro::ui::theme::ThemeId;
use cedro::ui::wheel::{Effect, WheelUi};
use jni::objects::{JByteArray, JObject, JString, JValue};
use jni::{JNIEnv, JavaVM};

/// Dove finiscono le copie.
const DOCUMENTI: &str = "/storage/emulated/0/Documents";

/// Quello che arriva dal menu Condividi.
struct Condiviso {
    path: PathBuf,
    nome: String,
}

/// Stato condiviso con il thread che converte.
#[derive(Default)]
struct Stato {
    messaggio: String,
    ok: bool,
    occupato: bool,
    sporco: bool,
}

fn jerr(env: &mut JNIEnv, e: jni::errors::Error) -> String {
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    format!("JNI: {e}")
}

fn stringa(env: &mut JNIEnv, o: JObject) -> Option<String> {
    if o.is_null() {
        return None;
    }
    let js = JString::from(o);
    env.get_string(&js).ok().map(|s| s.into())
}

fn estensione_da_mime(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "image/tiff" => "tiff",
        "application/pdf" => "pdf",
        _ => "jpg",
    }
}

/// Legge l'intento che ha avviato l'attività: se è un ACTION_SEND, copia il flusso in un file privato.
fn leggi_condivisione(app: &AndroidApp, cartella: &Path) -> Result<Option<Condiviso>, String> {
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut _) }.map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let attivita = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
    let r = leggi_condivisione_in(&mut env, &attivita, cartella);
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    r
}

fn leggi_condivisione_in(env: &mut JNIEnv, attivita: &JObject, cartella: &Path) -> Result<Option<Condiviso>, String> {
    macro_rules! j {
        ($e:expr) => {
            match $e {
                Ok(v) => v,
                Err(e) => return Err(jerr(env, e)),
            }
        };
    }
    let intento = j!(j!(env.call_method(attivita, "getIntent", "()Landroid/content/Intent;", &[])).l());
    if intento.is_null() {
        return Ok(None);
    }
    let azione = j!(env.call_method(&intento, "getAction", "()Ljava/lang/String;", &[])).l().ok().and_then(|o| stringa(env, o));
    if azione.as_deref() != Some("android.intent.action.SEND") {
        return Ok(None);
    }
    let mime = j!(env.call_method(&intento, "getType", "()Ljava/lang/String;", &[])).l().ok().and_then(|o| stringa(env, o)).unwrap_or_default();
    let chiave = j!(env.new_string("android.intent.extra.STREAM"));
    let uri = j!(j!(env.call_method(&intento, "getParcelableExtra", "(Ljava/lang/String;)Landroid/os/Parcelable;", &[JValue::Object(&chiave)])).l());
    if uri.is_null() {
        return Err("la condivisione non contiene un file".into());
    }
    let resolver = j!(j!(env.call_method(attivita, "getContentResolver", "()Landroid/content/ContentResolver;", &[])).l());

    // Il nome: OpenableColumns.DISPLAY_NAME.
    let mut nome = String::new();
    let nulla = JObject::null();
    if let Ok(cur) = env.call_method(
        &resolver,
        "query",
        "(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;",
        &[JValue::Object(&uri), JValue::Object(&nulla), JValue::Object(&nulla), JValue::Object(&nulla), JValue::Object(&nulla)],
    ) {
        if let Ok(cur) = cur.l() {
            if !cur.is_null() {
                if j!(j!(env.call_method(&cur, "moveToFirst", "()Z", &[])).z()) {
                    let col = j!(env.new_string("_display_name"));
                    let idx = j!(j!(env.call_method(&cur, "getColumnIndex", "(Ljava/lang/String;)I", &[JValue::Object(&col)])).i());
                    if idx >= 0 {
                        if let Ok(s) = env.call_method(&cur, "getString", "(I)Ljava/lang/String;", &[JValue::Int(idx)]) {
                            nome = s.l().ok().and_then(|o| stringa(env, o)).unwrap_or_default();
                        }
                    }
                }
                let _ = env.call_method(&cur, "close", "()V", &[]);
            }
        }
    }
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }

    // Il contenuto: openInputStream + read(byte[]).
    let flusso = j!(j!(env.call_method(&resolver, "openInputStream", "(Landroid/net/Uri;)Ljava/io/InputStream;", &[JValue::Object(&uri)])).l());
    if flusso.is_null() {
        return Err("impossibile aprire il file condiviso".into());
    }
    let buf: JByteArray = j!(env.new_byte_array(64 * 1024));
    let mut dati: Vec<u8> = Vec::new();
    let mut pezzo = vec![0i8; 64 * 1024];
    loop {
        let n = j!(j!(env.call_method(&flusso, "read", "([B)I", &[JValue::Object(&buf)])).i());
        if n < 0 {
            break;
        }
        if n == 0 {
            continue;
        }
        j!(env.get_byte_array_region(&buf, 0, &mut pezzo[..n as usize]));
        dati.extend(pezzo[..n as usize].iter().map(|b| *b as u8));
    }
    let _ = env.call_method(&flusso, "close", "()V", &[]);

    let ext_mime = estensione_da_mime(&mime);
    let base = if nome.is_empty() { "condiviso".to_string() } else { nome.clone() };
    let nome_file = if Path::new(&base).extension().is_some() { base } else { format!("{base}.{ext_mime}") };
    let sicuro: String = nome_file.chars().map(|c| if c == '/' || c == '\\' { '_' } else { c }).collect();
    std::fs::create_dir_all(cartella).map_err(|e| e.to_string())?;
    let path = cartella.join(&sicuro);
    std::fs::write(&path, &dati).map_err(|e| e.to_string())?;
    Ok(Some(Condiviso { path, nome: sicuro }))
}

/// Chiede il permesso di scrivere in Documenti (se manca) e aspetta la risposta.
fn assicura_scrittura(app: &AndroidApp) -> bool {
    let Ok(vm) = (unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut _) }) else { return false };
    let Ok(mut env) = vm.attach_current_thread() else { return false };
    let attivita = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
    let perm = "android.permission.WRITE_EXTERNAL_STORAGE";
    let controlla = |env: &mut JNIEnv| -> bool {
        let Ok(p) = env.new_string(perm) else { return false };
        let r = env.call_method(&attivita, "checkSelfPermission", "(Ljava/lang/String;)I", &[JValue::Object(&p)]).and_then(|v| v.i());
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
        }
        r.map(|v| v == 0).unwrap_or(false)
    };
    if controlla(&mut env) {
        return true;
    }
    let ok = (|| -> Result<(), jni::errors::Error> {
        let classe = env.find_class("java/lang/String")?;
        let p = env.new_string(perm)?;
        let arr = env.new_object_array(1, classe, &p)?;
        env.call_method(&attivita, "requestPermissions", "([Ljava/lang/String;I)V", &[JValue::Object(&arr), JValue::Int(7)])?;
        Ok(())
    })();
    if ok.is_err() && env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    for _ in 0..240 {
        if controlla(&mut env) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    false
}

fn converti(app: AndroidApp, stato: Arc<Mutex<Stato>>, file: PathBuf, azione: &'static str, etichetta: &'static str, qualita: u8) {
    let imposta = |m: String, ok: bool, occupato: bool| {
        let mut s = stato.lock().unwrap_or_else(|e| e.into_inner());
        s.messaggio = m;
        s.ok = ok;
        s.occupato = occupato;
        s.sporco = true;
    };
    imposta(format!("Converto in {etichetta}…"), true, true);
    if !assicura_scrittura(&app) {
        imposta("Serve il permesso di scrivere in Documenti".into(), false, false);
        return;
    }
    let out = PathBuf::from(DOCUMENTI);
    if let Err(e) = std::fs::create_dir_all(&out) {
        imposta(format!("Documenti non scrivibile: {e}"), false, false);
        return;
    }
    match cedro::perform_into(&file, azione, qualita, Some(&out)) {
        Ok(o) => {
            let nome = o.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            log::info!("salvato {}: {}", o.path.display(), o.message);
            imposta(format!("Salvato in Documenti: {nome}"), true, false);
        }
        Err(e) => {
            log::error!("conversione fallita: {e}");
            imposta(format!("Errore: {e}"), false, false);
        }
    }
}

struct Disegno {
    scala: f32,
    dx: f32,
    dy: f32,
}

fn disegna(ui: &WheelUi, stato: &Stato, finestra: &android_activity::ndk::native_window::NativeWindow, cache: &mut GlyphCache) -> Option<Disegno> {
    let (w, h) = (finestra.width() as u32, finestra.height() as u32);
    if w == 0 || h == 0 {
        return None;
    }
    let fonts = Fonts::current();
    let pal = ThemeId::WindowsEssenziale.palette();
    let (uw, uh) = ui.size();
    // La ruota sta in alto, con un po' d'aria sopra per la barra di stato e sotto per il messaggio.
    let riserva_alto = 0.04 * h as f32;
    let riserva_basso = 0.12 * h as f32;
    let scala = (w as f32 / uw).min((h as f32 - riserva_alto - riserva_basso) / uh);
    let dx = (w as f32 / scala - uw) / 2.0;
    let dy = riserva_alto / scala + ((h as f32 - riserva_alto - riserva_basso) / scala - uh) / 2.0;
    let mut p = Painter::new(w, h, scala, fonts, cache);
    p.clear(pal.bg);
    p.dx = dx;
    p.dy = dy;
    ui.draw(&mut p);
    p.dx = 0.0;
    p.dy = 0.0;
    let testo = if !stato.messaggio.is_empty() {
        stato.messaggio.clone()
    } else if ui.has_file {
        "Tocca una fetta per convertire".to_string()
    } else {
        "Dal menu Condividi scegli «Cedro»".to_string()
    };
    let colore = if stato.ok || stato.messaggio.is_empty() { pal.text } else { pal.bad };
    let lw = w as f32 / scala;
    let lh = h as f32 / scala;
    p.rrect(24.0, lh - riserva_basso / scala + 6.0, lw - 48.0, 74.0, 14.0, Some(pal.surface), Some((pal.border, 1.0)));
    p.text(&testo, 40.0, lh - riserva_basso / scala + 6.0, lw - 80.0, 74.0, 24.0, Weight::Regular, colore, Align::Center);
    presenta(finestra, &p);
    Some(Disegno { scala, dx, dy })
}

fn presenta(finestra: &android_activity::ndk::native_window::NativeWindow, p: &Painter) {
    use android_activity::ndk::hardware_buffer_format::HardwareBufferFormat;
    let _ = finestra.set_buffers_geometry(0, 0, Some(HardwareBufferFormat::R8G8B8A8_UNORM));
    let Ok(mut guardia) = finestra.lock(None) else { return };
    let (gw, gh, stride) = (guardia.width(), guardia.height(), guardia.stride());
    let Some(dest) = guardia.bytes() else { return };
    let src = p.pixmap.data();
    let pw = p.pixmap.width() as usize;
    let ph = p.pixmap.height() as usize;
    for y in 0..gh.min(ph) {
        let n = gw.min(pw) * 4;
        let da = y * stride * 4;
        if da + n > dest.len() {
            break;
        }
        for i in 0..n {
            dest[da + i] = std::mem::MaybeUninit::new(src[y * pw * 4 + i]);
        }
    }
}

#[no_mangle]
fn android_main(app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("Cedro"));
    let dati = app.internal_data_path().unwrap_or_else(|| PathBuf::from("/data/local/tmp"));
    // Il motore cerca le sue impostazioni in $HOME/.config/cedro.
    std::env::set_var("HOME", &dati);

    let stato = Arc::new(Mutex::new(Stato::default()));
    let mut ui = WheelUi::new(cedro::Kind::Image, 0, true);
    ui.dest_note = "Salva in Documenti".into();
    ui.quality = 82;
    let miniatura: Arc<Mutex<Option<cedro::ui::gfx::Image>>> = Arc::new(Mutex::new(None));
    let mut file: Option<PathBuf> = None;

    match leggi_condivisione(&app, &dati.join("condivisi")) {
        Ok(Some(c)) => {
            log::info!("ricevuto {} ({})", c.nome, c.path.display());
            let kind = cedro::kind_of(&c.path);
            ui.set_kind_mode(kind, 0);
            ui.has_file = true;
            ui.name = c.nome.clone();
            let slot = miniatura.clone();
            let p = c.path.clone();
            std::thread::spawn(move || {
                if let Some(img) = cedro::ui::media::thumb_of(&p, 420) {
                    *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(img);
                }
            });
            file = Some(c.path);
        }
        Ok(None) => {}
        Err(e) => {
            log::error!("condivisione: {e}");
            let mut s = stato.lock().unwrap();
            s.messaggio = format!("Errore: {e}");
            s.ok = false;
        }
    }

    let mut cache = GlyphCache::new();
    let mut disegno = Disegno { scala: 1.0, dx: 0.0, dy: 0.0 };
    let mut finestra: Option<android_activity::ndk::native_window::NativeWindow> = None;
    let mut sporco = true;
    let mut esci = false;

    while !esci {
        app.poll_events(Some(Duration::from_millis(100)), |ev| {
            if let PollEvent::Main(m) = ev {
                match m {
                    MainEvent::InitWindow { .. } => {
                        finestra = app.native_window();
                        sporco = true;
                    }
                    MainEvent::TerminateWindow { .. } => finestra = None,
                    MainEvent::WindowResized { .. } | MainEvent::RedrawNeeded { .. } | MainEvent::ContentRectChanged { .. } => sporco = true,
                    MainEvent::Destroy => esci = true,
                    _ => {}
                }
            }
        });
        if let Ok(mut it) = app.input_events_iter() {
            loop {
                let letto = it.next(|e| {
                    if let InputEvent::MotionEvent(m) = e {
                        if m.action() == MotionAction::Down {
                            let ptr = m.pointer_at_index(0);
                            let (x, y) = (ptr.x() / disegno.scala - disegno.dx, ptr.y() / disegno.scala - disegno.dy);
                            ui.pointer_move(x, y);
                            match ui.press(x, y) {
                                Effect::Commit(i) => match (&file, ui.list.get(i).copied()) {
                                    (Some(f), Some(fetta)) => {
                                        let occupato = stato.lock().unwrap().occupato;
                                        if !occupato {
                                            let (a, st, f) = (app.clone(), stato.clone(), f.clone());
                                            let q = ui.quality;
                                            std::thread::spawn(move || converti(a, st, f, fetta.id, fetta.label, q));
                                        }
                                    }
                                    _ => {
                                        let mut s = stato.lock().unwrap();
                                        s.messaggio = "Prima condividi un'immagine con Cedro".into();
                                        s.ok = false;
                                    }
                                },
                                _ => {}
                            }
                            sporco = true;
                        } else if m.action() == MotionAction::Up {
                            ui.release();
                            ui.pointer_leave();
                            sporco = true;
                        }
                    }
                    InputStatus::Handled
                });
                if !letto {
                    break;
                }
            }
        }
        if let Some(img) = miniatura.lock().unwrap_or_else(|e| e.into_inner()).take() {
            ui.thumb = Some(img);
            sporco = true;
        }
        let snap = {
            let mut s = stato.lock().unwrap_or_else(|e| e.into_inner());
            if s.sporco {
                s.sporco = false;
                sporco = true;
            }
            Stato { messaggio: s.messaggio.clone(), ok: s.ok, occupato: s.occupato, sporco: false }
        };
        if sporco {
            if let Some(f) = &finestra {
                if let Some(d) = disegna(&ui, &snap, f, &mut cache) {
                    disegno = d;
                    sporco = false;
                }
            }
        }
    }
}
