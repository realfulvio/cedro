//! La parte Android: finestra, tocchi, intento di «Condividi» (JNI) e salvataggio. L'interfaccia vera è in `ui`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use android_activity::input::{InputEvent, KeyAction, Keycode, MotionAction};
use android_activity::{AndroidApp, InputStatus, MainEvent, PollEvent};
use cedro::ui::gfx::{Fonts, GlyphCache, Image, Painter};
use cedro::ui::theme::ThemeId;
use jni::objects::{JByteArray, JObject, JString, JValue};
use jni::{JNIEnv, JavaVM};

use crate::ui::{App, Archivio, Effetto, Lavoro};

/// Dove finiscono le copie.
const DOCUMENTI: &str = "/storage/emulated/0/Documents";

/// Quello che arriva dal menu Condividi.
struct Condiviso {
    path: PathBuf,
    nome: String,
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

/// Dall'attività di Cedro: chiude e si torna all'app di prima.
fn chiudi(app: &AndroidApp) {
    let Ok(vm) = (unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut _) }) else { return };
    let Ok(mut env) = vm.attach_current_thread() else { return };
    let attivita = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
    let _ = env.call_method(&attivita, "finish", "()V", &[]);
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
}

/// Le barre di sistema e dei gesti, in pixel: sinistra, alto, destra, basso.
fn margini(app: &AndroidApp) -> (f32, f32, f32, f32) {
    let Ok(vm) = (unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut _) }) else { return (0.0, 0.0, 0.0, 0.0) };
    let Ok(mut env) = vm.attach_current_thread() else { return (0.0, 0.0, 0.0, 0.0) };
    let attivita = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
    let r = (|| -> Result<(f32, f32, f32, f32), jni::errors::Error> {
        let finestra = env.call_method(&attivita, "getWindow", "()Landroid/view/Window;", &[])?.l()?;
        let decor = env.call_method(&finestra, "getDecorView", "()Landroid/view/View;", &[])?.l()?;
        let ins = env.call_method(&decor, "getRootWindowInsets", "()Landroid/view/WindowInsets;", &[])?.l()?;
        if ins.is_null() {
            return Ok((0.0, 0.0, 0.0, 0.0));
        }
        let mut v = [0f32; 4];
        for (i, nome) in ["getSystemWindowInsetLeft", "getSystemWindowInsetTop", "getSystemWindowInsetRight", "getSystemWindowInsetBottom"].into_iter().enumerate() {
            v[i] = env.call_method(&ins, nome, "()I", &[])?.i()? as f32;
        }
        Ok((v[0], v[1], v[2], v[3]))
    })();
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    r.unwrap_or((0.0, 0.0, 0.0, 0.0))
}

/// Icone della barra di stato scure (tema chiaro) o chiare (tema scuro).
fn icone_barre(app: &AndroidApp, scuro: bool) {
    let Ok(vm) = (unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut _) }) else { return };
    let Ok(mut env) = vm.attach_current_thread() else { return };
    let attivita = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
    let _ = (|| -> Result<(), jni::errors::Error> {
        let finestra = env.call_method(&attivita, "getWindow", "()Landroid/view/Window;", &[])?.l()?;
        let decor = env.call_method(&finestra, "getDecorView", "()Landroid/view/View;", &[])?.l()?;
        // SYSTEM_UI_FLAG_LIGHT_STATUS_BAR (0x2000) e LIGHT_NAVIGATION_BAR (0x10): icone scure.
        let flag = if scuro { 0 } else { 0x2000 | 0x10 };
        env.call_method(&decor, "setSystemUiVisibility", "(I)V", &[JValue::Int(flag)])?;
        Ok(())
    })();
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
}

type Casella<T> = Arc<Mutex<Option<T>>>;

fn converti(app: AndroidApp, esito: Casella<Lavoro>, file: PathBuf, azione: &'static str, etichetta: String, qualita: u8) {
    let invia = |l: Lavoro| *esito.lock().unwrap_or_else(|e| e.into_inner()) = Some(l);
    if !assicura_scrittura(&app) {
        invia(Lavoro::Errore("Serve il permesso di scrivere in Documenti.".into()));
        return;
    }
    let out = PathBuf::from(DOCUMENTI);
    if let Err(e) = std::fs::create_dir_all(&out) {
        invia(Lavoro::Errore(format!("Documenti non è scrivibile: {e}")));
        return;
    }
    match cedro::perform_into(&file, azione, qualita, Some(&out)) {
        Ok(o) => {
            let nome = o.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            log::info!("{etichetta}: salvato {} ({})", o.path.display(), o.message);
            invia(Lavoro::Fatto { nome, msg: o.message });
        }
        Err(e) => {
            log::error!("{etichetta}: conversione fallita: {e}");
            invia(Lavoro::Errore(e.to_string()));
        }
    }
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

fn file_di(path: &Path, nome: String) -> Archivio {
    let dim = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    Archivio { path: path.to_path_buf(), nome, dim }
}

/// L'ultimo file ricevuto, per quando si apre Cedro dall'icona.
fn ultimo_condiviso(cartella: &Path) -> Option<PathBuf> {
    std::fs::read_dir(cartella).ok()?.filter_map(|e| e.ok()).filter(|e| e.path().is_file()).max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok()).map(|e| e.path())
}

#[no_mangle]
fn android_main(app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("Cedro"));
    let dati = app.internal_data_path().unwrap_or_else(|| PathBuf::from("/data/local/tmp"));
    // Il motore cerca le sue impostazioni in $HOME/.config/cedro.
    std::env::set_var("HOME", &dati);
    let file_tema = dati.join("tema.txt");
    let tema = std::fs::read_to_string(&file_tema).map(|t| ThemeId::from_key(&t)).unwrap_or(ThemeId::WindowsEssenziale);
    let mut ui = App::new(tema);
    icone_barre(&app, tema.dark());
    let mut insets = (0.0, 0.0, 0.0, 0.0);
    let mut leggi_insets = true;
    let mut tentativi = 0;

    let cartella = dati.join("condivisi");
    let miniatura: Casella<Image> = Arc::new(Mutex::new(None));
    let esito: Casella<Lavoro> = Arc::new(Mutex::new(None));
    let (mut percorso, da_condivisione) = match leggi_condivisione(&app, &cartella) {
        Ok(Some(c)) => {
            log::info!("ricevuto {} ({})", c.nome, c.path.display());
            (Some((c.path, c.nome)), true)
        }
        Ok(None) => (ultimo_condiviso(&cartella).map(|p| { let n = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(); (p, n) }), false),
        Err(e) => {
            log::error!("condivisione: {e}");
            ui.lavoro = Lavoro::Errore(e);
            (None, false)
        }
    };
    if let Some((path, nome)) = percorso.take() {
        ui.imposta_file(file_di(&path, nome), cedro::kind_of(&path), da_condivisione);
        let slot = miniatura.clone();
        std::thread::spawn(move || {
            if let Some(img) = cedro::ui::media::thumb_of(&path, 600) {
                *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(img);
            }
        });
    }

    let mut cache = GlyphCache::new();
    let mut finestra: Option<android_activity::ndk::native_window::NativeWindow> = None;
    let mut sporco = true;
    let mut esci = false;
    let mut animando = false;

    while !esci {
        let attesa = if animando { 8 } else { 150 };
        app.poll_events(Some(Duration::from_millis(attesa)), |ev| {
            if let PollEvent::Main(m) = ev {
                match m {
                    MainEvent::InitWindow { .. } => {
                        finestra = app.native_window();
                        sporco = true;
                        leggi_insets = true;
                        tentativi = 0;
                    }
                    MainEvent::TerminateWindow { .. } => finestra = None,
                    MainEvent::WindowResized { .. } | MainEvent::RedrawNeeded { .. } | MainEvent::ContentRectChanged { .. } | MainEvent::ConfigChanged { .. } => {
                        sporco = true;
                        leggi_insets = true;
                    }
                    MainEvent::Destroy => esci = true,
                    _ => {}
                }
            }
        });
        let scala = app.config().density().map(|d| d as f32 / 160.0).unwrap_or(2.6).max(1.0);
        if let Ok(mut it) = app.input_events_iter() {
            loop {
                let letto = it.next(|e| {
                    let mut eff = Effetto::Niente;
                    let mut stato = InputStatus::Handled;
                    match e {
                        InputEvent::MotionEvent(m) => {
                            let ptr = m.pointer_at_index(0);
                            let (x, y) = (ptr.x() / scala, ptr.y() / scala);
                            match m.action() {
                                MotionAction::Down => eff = ui.tocca_giu(x, y),
                                MotionAction::Move => eff = ui.tocca_muovi(x, y),
                                MotionAction::Up => eff = ui.tocca_su(x, y),
                                MotionAction::Cancel => {
                                    ui.tocca_annulla();
                                    eff = Effetto::Ridisegna;
                                }
                                _ => {}
                            }
                        }
                        InputEvent::KeyEvent(k) => {
                            if k.key_code() == Keycode::Back {
                                // Il «indietro» chiude prima i fogli; se non c'è altro lo lasciamo al sistema.
                                if k.action() == KeyAction::Up {
                                    if !ui.indietro() {
                                        chiudi(&app);
                                    }
                                    eff = Effetto::Ridisegna;
                                }
                            } else {
                                stato = InputStatus::Unhandled;
                            }
                        }
                        _ => stato = InputStatus::Unhandled,
                    }
                    match eff {
                        Effetto::Niente => {}
                        Effetto::Ridisegna => sporco = true,
                        Effetto::Converti { id, etichetta, qualita } => {
                            sporco = true;
                            if let Some(f) = ui.file.clone() {
                                let (a, es) = (app.clone(), esito.clone());
                                std::thread::spawn(move || converti(a, es, f.path, id, etichetta, qualita));
                            }
                        }
                        Effetto::Esci => chiudi(&app),
                        Effetto::SalvaTema(t) => {
                            let _ = std::fs::write(&file_tema, t.key());
                            icone_barre(&app, t.dark());
                            sporco = true;
                        }
                    }
                    stato
                });
                if !letto {
                    break;
                }
            }
        }
        if let Some(img) = miniatura.lock().unwrap_or_else(|e| e.into_inner()).take() {
            ui.miniatura = Some(img);
            sporco = true;
        }
        if let Some(l) = esito.lock().unwrap_or_else(|e| e.into_inner()).take() {
            ui.lavoro = l;
            sporco = true;
        }
        if leggi_insets && finestra.is_some() {
            let m = margini(&app);
            ui.margini = (m.0 / scala, m.1 / scala, m.2 / scala, m.3 / scala);
            if m != insets {
                insets = m;
                sporco = true;
            }
            // Appena aperta la finestra le barre possono non esserci ancora: si riprova per un po'.
            tentativi += 1;
            leggi_insets = m == (0.0, 0.0, 0.0, 0.0) && tentativi < 40;
        }
        animando = ui.animando();
        if sporco || animando {
            if let Some(f) = &finestra {
                let (w, h) = (f.width() as u32, f.height() as u32);
                if w > 0 && h > 0 {
                    let mut p = Painter::new(w, h, scala, Fonts::current(), &mut cache);
                    ui.disegna(&mut p, w as f32 / scala, h as f32 / scala);
                    presenta(f, &p);
                    sporco = false;
                }
            }
        }
    }
}
