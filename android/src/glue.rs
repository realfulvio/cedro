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

use crate::ui::{App, Archivio, Effetto, Recente, Sorgente, Stato};

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
        "text/plain" => "txt",
        "text/markdown" => "md",
        "text/html" => "html",
        "text/csv" => "csv",
        "application/json" => "json",
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "video/quicktime" => "mov",
        "video/x-matroska" => "mkv",
        m if m.starts_with("video/") => "mp4",
        "audio/mpeg" => "mp3",
        "audio/mp4" | "audio/x-m4a" | "audio/m4a" => "m4a",
        "audio/wav" | "audio/x-wav" => "wav",
        "audio/ogg" => "ogg",
        "audio/flac" => "flac",
        "audio/aac" => "aac",
        m if m.starts_with("audio/") => "m4a",
        m if m.starts_with("text/") => "txt",
        _ => "jpg",
    }
}

/// Il tipo MIME di un file dalla sua estensione (per «Apri» e «Condividi»).
fn mime_da_percorso(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "md" => "text/markdown",
        "html" => "text/html",
        "csv" => "text/csv",
        "json" => "application/json",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "wav" => "audio/x-wav",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "aac" => "audio/aac",
        "mp4" => "video/mp4",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        _ => "application/octet-stream",
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
    copia_uri(env, attivita, &uri, &mime, cartella).map(Some)
}

/// Copia il contenuto di un `content://` in un file privato di Cedro e ne dà nome e percorso.
fn copia_uri(env: &mut JNIEnv, attivita: &JObject, uri: &JObject, mime: &str, cartella: &Path) -> Result<Condiviso, String> {
    macro_rules! j {
        ($e:expr) => {
            match $e {
                Ok(v) => v,
                Err(e) => return Err(jerr(env, e)),
            }
        };
    }
    let resolver = j!(j!(env.call_method(attivita, "getContentResolver", "()Landroid/content/ContentResolver;", &[])).l());
    let mime_letto;
    let mime = if mime.is_empty() {
        mime_letto = env.call_method(&resolver, "getType", "(Landroid/net/Uri;)Ljava/lang/String;", &[JValue::Object(uri)]).ok().and_then(|v| v.l().ok()).and_then(|o| stringa(env, o)).unwrap_or_default();
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
        }
        mime_letto.as_str()
    } else {
        mime
    };

    // Il nome: OpenableColumns.DISPLAY_NAME.
    let mut nome = String::new();
    let nulla = JObject::null();
    if let Ok(cur) = env.call_method(
        &resolver,
        "query",
        "(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;",
        &[JValue::Object(uri), JValue::Object(&nulla), JValue::Object(&nulla), JValue::Object(&nulla), JValue::Object(&nulla)],
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
    let flusso = j!(j!(env.call_method(&resolver, "openInputStream", "(Landroid/net/Uri;)Ljava/io/InputStream;", &[JValue::Object(uri)])).l());
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

    let ext_mime = estensione_da_mime(mime);
    let base = if nome.is_empty() { "condiviso".to_string() } else { nome.clone() };
    let nome_file = if Path::new(&base).extension().is_some() { base } else { format!("{base}.{ext_mime}") };
    let sicuro: String = nome_file.chars().map(|c| if c == '/' || c == '\\' { '_' } else { c }).collect();
    std::fs::create_dir_all(cartella).map_err(|e| e.to_string())?;
    let path = cartella.join(&sicuro);
    std::fs::write(&path, &dati).map_err(|e| e.to_string())?;
    Ok(Condiviso { path, nome: sicuro })
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


/// Il solito: attacca il thread alla JVM e dà l'attività.
fn con_attivita<T>(app: &AndroidApp, f: impl FnOnce(&mut JNIEnv, &JObject) -> Result<T, String>) -> Result<T, String> {
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut _) }.map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let attivita = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
    let r = f(&mut env, &attivita);
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    r
}

/// Apre il selettore di file di Android (ACTION_OPEN_DOCUMENT). Il risultato lo legge `scelto`.
fn scegli(app: &AndroidApp, sorgente: Sorgente) -> Result<(), String> {
    con_attivita(app, |env, attivita| {
        let azione = env.new_string("android.intent.action.OPEN_DOCUMENT").map_err(|e| e.to_string())?;
        let intento = env.new_object("android/content/Intent", "(Ljava/lang/String;)V", &[JValue::Object(&azione)]).map_err(|e| jerr(env, e))?;
        let cat = env.new_string("android.intent.category.OPENABLE").map_err(|e| e.to_string())?;
        env.call_method(&intento, "addCategory", "(Ljava/lang/String;)Landroid/content/Intent;", &[JValue::Object(&cat)]).map_err(|e| jerr(env, e))?;
        let (tipo, altri): (&str, &[&str]) = match sorgente {
            Sorgente::Foto => ("image/*", &[]),
            Sorgente::Audio => ("audio/*", &[]),
            Sorgente::Altri => ("*/*", &[]),
            Sorgente::Documenti => (
                "*/*",
                &[
                    "application/pdf",
                    "text/*",
                    "application/json",
                    "application/msword",
                    "application/rtf",
                    "application/vnd.oasis.opendocument.text",
                    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                ],
            ),
        };
        let t = env.new_string(tipo).map_err(|e| e.to_string())?;
        env.call_method(&intento, "setType", "(Ljava/lang/String;)Landroid/content/Intent;", &[JValue::Object(&t)]).map_err(|e| jerr(env, e))?;
        if !altri.is_empty() {
            let classe = env.find_class("java/lang/String").map_err(|e| jerr(env, e))?;
            let primo = env.new_string(altri[0]).map_err(|e| e.to_string())?;
            let arr = env.new_object_array(altri.len() as i32, classe, &primo).map_err(|e| jerr(env, e))?;
            for (i, m) in altri.iter().enumerate() {
                let m = env.new_string(m).map_err(|e| e.to_string())?;
                env.set_object_array_element(&arr, i as i32, m).map_err(|e| jerr(env, e))?;
            }
            let chiave = env.new_string("android.intent.extra.MIME_TYPES").map_err(|e| e.to_string())?;
            env.call_method(&intento, "putExtra", "(Ljava/lang/String;[Ljava/lang/String;)Landroid/content/Intent;", &[JValue::Object(&chiave), JValue::Object(&arr)]).map_err(|e| jerr(env, e))?;
        }
        env.call_method(attivita, "startActivityForResult", "(Landroid/content/Intent;I)V", &[JValue::Object(&intento), JValue::Int(4242)]).map_err(|e| jerr(env, e))?;
        Ok(())
    })
}

/// Il file scelto nel selettore, se è arrivato: lo copia nella cartella dei file di Cedro.
fn scelto(app: &AndroidApp, cartella: &Path) -> Option<Result<Condiviso, String>> {
    let r = con_attivita(app, |env, attivita| {
        let uri = env.call_method(attivita, "scelto", "()Landroid/net/Uri;", &[]).and_then(|v| v.l()).map_err(|e| jerr(env, e))?;
        if uri.is_null() {
            return Ok(None);
        }
        copia_uri(env, attivita, &uri, "", cartella).map(Some)
    });
    match r {
        Ok(Some(c)) => Some(Ok(c)),
        Ok(None) => None,
        Err(e) => Some(Err(e)),
    }
}

/// Dice a Android che c'è un file nuovo in Documenti (così compare in Galleria e in File) e ne cerca il `content://`.
fn scansiona(env: &mut JNIEnv, attivita: &JObject, path: &Path, attesa: bool) -> Result<Option<jni::objects::GlobalRef>, String> {
    let ps = path.to_string_lossy().into_owned();
    let classe = env.find_class("java/lang/String").map_err(|e| jerr(env, e))?;
    let p = env.new_string(&ps).map_err(|e| e.to_string())?;
    let m = env.new_string(mime_da_percorso(path)).map_err(|e| e.to_string())?;
    let arr_p = env.new_object_array(1, &classe, &p).map_err(|e| jerr(env, e))?;
    let arr_m = env.new_object_array(1, &classe, &m).map_err(|e| jerr(env, e))?;
    let nulla = JObject::null();
    env.call_static_method(
        "android/media/MediaScannerConnection",
        "scanFile",
        "(Landroid/content/Context;[Ljava/lang/String;[Ljava/lang/String;Landroid/media/MediaScannerConnection$OnScanCompletedListener;)V",
        &[JValue::Object(attivita), JValue::Object(&arr_p), JValue::Object(&arr_m), JValue::Object(&nulla)],
    )
    .map_err(|e| jerr(env, e))?;
    if !attesa {
        return Ok(None);
    }
    let resolver = env.call_method(attivita, "getContentResolver", "()Landroid/content/ContentResolver;", &[]).and_then(|v| v.l()).map_err(|e| jerr(env, e))?;
    let esterno = env.new_string("external").map_err(|e| e.to_string())?;
    // Per foto, audio e video la raccolta giusta: il foglio di condivisione mostra così anteprima e nome.
    let raccolta = match mime_da_percorso(path).split('/').next().unwrap_or("") {
        "image" => "android/provider/MediaStore$Images$Media",
        "audio" => "android/provider/MediaStore$Audio$Media",
        "video" => "android/provider/MediaStore$Video$Media",
        _ => "android/provider/MediaStore$Files",
    };
    let base = env
        .call_static_method(raccolta, "getContentUri", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&esterno)])
        .and_then(|v| v.l())
        .map_err(|e| jerr(env, e))?;
    let col = env.new_string("_id").map_err(|e| e.to_string())?;
    let proiez = env.new_object_array(1, &classe, &col).map_err(|e| jerr(env, e))?;
    let sel = env.new_string("_data=?").map_err(|e| e.to_string())?;
    let args = env.new_object_array(1, &classe, &p).map_err(|e| jerr(env, e))?;
    for _ in 0..30 {
        let cur = env
            .call_method(
                &resolver,
                "query",
                "(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;",
                &[JValue::Object(&base), JValue::Object(&proiez), JValue::Object(&sel), JValue::Object(&args), JValue::Object(&nulla)],
            )
            .and_then(|v| v.l())
            .map_err(|e| jerr(env, e))?;
        if !cur.is_null() {
            let c = env.call_method(&cur, "moveToFirst", "()Z", &[]).and_then(|v| v.z()).unwrap_or(false);
            let id = if c { env.call_method(&cur, "getLong", "(I)J", &[JValue::Int(0)]).and_then(|v| v.j()).ok() } else { None };
            let _ = env.call_method(&cur, "close", "()V", &[]);
            if let Some(id) = id {
                let uri = env
                    .call_static_method("android/content/ContentUris", "withAppendedId", "(Landroid/net/Uri;J)Landroid/net/Uri;", &[JValue::Object(&base), JValue::Long(id)])
                    .and_then(|v| v.l())
                    .map_err(|e| jerr(env, e))?;
                return env.new_global_ref(uri).map(Some).map_err(|e| e.to_string());
            }
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    Err("Android non ha ancora registrato il file".into())
}

/// «Apri» (ACTION_VIEW) o «Condividi» (ACTION_SEND con selettore) sul risultato di un lavoro: serve un `content://`,
/// che si ottiene registrando il file in MediaStore, senza FileProvider.
fn apri_o_condividi(app: &AndroidApp, path: &Path, condividi: bool) -> Result<(), String> {
    con_attivita(app, |env, attivita| {
        let uri = scansiona(env, attivita, path, true)?.ok_or("nessun indirizzo per il file")?;
        let mime = mime_da_percorso(path);
        let mime_s = env.new_string(mime).map_err(|e| e.to_string())?;
        let azione = env.new_string(if condividi { "android.intent.action.SEND" } else { "android.intent.action.VIEW" }).map_err(|e| e.to_string())?;
        let intento = env.new_object("android/content/Intent", "(Ljava/lang/String;)V", &[JValue::Object(&azione)]).map_err(|e| jerr(env, e))?;
        if condividi {
            let nome_f = env.new_string(path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()).map_err(|e| e.to_string())?;
            let k = env.new_string("android.intent.extra.TITLE").map_err(|e| e.to_string())?;
            env.call_method(&intento, "putExtra", "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;", &[JValue::Object(&k), JValue::Object(&nome_f)]).map_err(|e| jerr(env, e))?;
            env.call_method(&intento, "setType", "(Ljava/lang/String;)Landroid/content/Intent;", &[JValue::Object(&mime_s)]).map_err(|e| jerr(env, e))?;
            let chiave = env.new_string("android.intent.extra.STREAM").map_err(|e| e.to_string())?;
            env.call_method(&intento, "putExtra", "(Ljava/lang/String;Landroid/os/Parcelable;)Landroid/content/Intent;", &[JValue::Object(&chiave), JValue::Object(uri.as_obj())])
                .map_err(|e| jerr(env, e))?;
        } else {
            env.call_method(&intento, "setDataAndType", "(Landroid/net/Uri;Ljava/lang/String;)Landroid/content/Intent;", &[JValue::Object(uri.as_obj()), JValue::Object(&mime_s)])
                .map_err(|e| jerr(env, e))?;
        }
        // Il permesso di lettura va anche nel ClipData, perché valga per l'app che riceve.
        let vuoto = env.new_string("").map_err(|e| e.to_string())?;
        let clip = env
            .call_static_method("android/content/ClipData", "newRawUri", "(Ljava/lang/CharSequence;Landroid/net/Uri;)Landroid/content/ClipData;", &[JValue::Object(&vuoto), JValue::Object(uri.as_obj())])
            .and_then(|v| v.l())
            .map_err(|e| jerr(env, e))?;
        env.call_method(&intento, "setClipData", "(Landroid/content/ClipData;)V", &[JValue::Object(&clip)]).map_err(|e| jerr(env, e))?;
        env.call_method(&intento, "addFlags", "(I)Landroid/content/Intent;", &[JValue::Int(1)]).map_err(|e| jerr(env, e))?;
        let da_avviare = if condividi {
            let titolo = env.new_string("Condividi").map_err(|e| e.to_string())?;
            env.call_static_method("android/content/Intent", "createChooser", "(Landroid/content/Intent;Ljava/lang/CharSequence;)Landroid/content/Intent;", &[JValue::Object(&intento), JValue::Object(&titolo)])
                .and_then(|v| v.l())
                .map_err(|e| jerr(env, e))?
        } else {
            intento
        };
        env.call_method(attivita, "startActivity", "(Landroid/content/Intent;)V", &[JValue::Object(&da_avviare)]).map_err(|e| jerr(env, e))?;
        Ok(())
    })
}

/// Il servizio in primo piano con la notifica dell'avanzamento (`percento` < 0: indeterminato). `None` lo ferma.
fn servizio(app: &AndroidApp, stato: Option<(&str, &str, i32)>) {
    let r = con_attivita(app, |env, att| {
        let intento = env.new_object("android/content/Intent", "()V", &[]).map_err(|e| jerr(env, e))?;
        let nome = env.new_string("it.cedro.android.ServizioLavoro").map_err(|e| e.to_string())?;
        env.call_method(&intento, "setClassName", "(Landroid/content/Context;Ljava/lang/String;)Landroid/content/Intent;", &[JValue::Object(att), JValue::Object(&nome)]).map_err(|e| jerr(env, e))?;
        match stato {
            None => {
                env.call_method(att, "stopService", "(Landroid/content/Intent;)Z", &[JValue::Object(&intento)]).map_err(|e| jerr(env, e))?;
            }
            Some((titolo, testo, percento)) => {
                for (k, v) in [("titolo", titolo), ("testo", testo)] {
                    let (k, v) = (env.new_string(k).map_err(|e| e.to_string())?, env.new_string(v).map_err(|e| e.to_string())?);
                    env.call_method(&intento, "putExtra", "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;", &[JValue::Object(&k), JValue::Object(&v)]).map_err(|e| jerr(env, e))?;
                }
                let k = env.new_string("percento").map_err(|e| e.to_string())?;
                env.call_method(&intento, "putExtra", "(Ljava/lang/String;I)Landroid/content/Intent;", &[JValue::Object(&k), JValue::Int(percento)]).map_err(|e| jerr(env, e))?;
                env.call_method(att, "startForegroundService", "(Landroid/content/Intent;)Landroid/content/ComponentName;", &[JValue::Object(&intento)]).map_err(|e| jerr(env, e))?;
            }
        }
        Ok(())
    });
    if let Err(e) = r {
        log::error!("servizio: {e}");
    }
}

type Casella<T> = Arc<Mutex<Option<T>>>;

/// Quello che i thread di lavoro consegnano al ciclo principale.
#[derive(Clone, Default)]
struct Consegne {
    miniatura: Casella<(PathBuf, Image)>,
    onda: Casella<(PathBuf, Vec<f32>)>,
    recenti: Casella<Vec<Recente>>,
    esiti: Arc<Mutex<Vec<(u32, Stato)>>>,
}

fn converti(app: AndroidApp, c: Consegne, id: u32, file: PathBuf, azione: &'static str, qualita: u8, prog: Arc<std::sync::atomic::AtomicU32>) {
    let invia = |s: Stato| c.esiti.lock().unwrap_or_else(|e| e.into_inner()).push((id, s));
    if !assicura_scrittura(&app) {
        invia(Stato::Errore("Serve il permesso di scrivere in Documenti.".into()));
        return;
    }
    let out = PathBuf::from(DOCUMENTI);
    if let Err(e) = std::fs::create_dir_all(&out) {
        invia(Stato::Errore(format!("Documenti non è scrivibile: {e}")));
        return;
    }
    if azione == "trascrivi" {
        return trascrivi(&app, &c, id, &file, &out, &prog);
    }
    let p2 = prog.clone();
    let esito = cedro::progress::con(move |f| p2.store((f * 1000.0) as u32, std::sync::atomic::Ordering::Relaxed), || cedro::perform_into(&file, azione, qualita, Some(&out)));
    match esito {
        Ok(o) => {
            let nome = o.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            log::info!("{azione}: salvato {} ({})", o.path.display(), o.message);
            // Il file compare subito in Galleria e in File.
            let _ = con_attivita(&app, |env, att| scansiona(env, att, &o.path, false));
            invia(Stato::Fatto { path: o.path, nome, msg: o.message });
        }
        Err(e) => {
            log::error!("{azione}: conversione fallita: {e}");
            invia(Stato::Errore(e.to_string()));
        }
    }
}

/// Voce in testo con whisper.cpp (nell'APK): il modello «Buona» (base) si scarica al primo uso. Salva un .txt in Documenti.
fn trascrivi(app: &AndroidApp, c: &Consegne, id: u32, file: &std::path::Path, out: &std::path::Path, prog: &std::sync::atomic::AtomicU32) {
    use cedro::trascrizione::{run, Event, Format, Request};
    let invia = |s: Stato| c.esiti.lock().unwrap_or_else(|e| e.into_inner()).push((id, s));
    let req = Request { format: Format::Text, model: 1, gpu: false, avx512: false };
    let esito: std::sync::Mutex<Option<Result<(PathBuf, String), String>>> = std::sync::Mutex::new(None);
    run(&[file.to_path_buf()], &req, Some(out), &cedro::Options::default(), &|e| match e {
        Event::Percent { percent, .. } => prog.store(percent as u32 * 10, std::sync::atomic::Ordering::Relaxed),
        Event::Log(l) => log::info!("trascrivi: {} {}", l.key, l.text),
        Event::Finished { result, .. } => *esito.lock().unwrap_or_else(|e| e.into_inner()) = Some(result),
    });
    match esito.into_inner().unwrap_or_else(|e| e.into_inner()) {
        Some(Ok((path, msg))) => {
            let nome = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let _ = con_attivita(app, |env, att| scansiona(env, att, &path, false));
            invia(Stato::Fatto { path, nome, msg });
        }
        Some(Err(e)) => invia(Stato::Errore(e)),
        None => invia(Stato::Errore("la trascrizione non è partita".into())),
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

fn nome_di(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Gli ultimi file usati (i più recenti della cartella di Cedro), con la miniatura, preparati fuori dal ciclo principale.
fn ricarica_recenti(cartella: PathBuf, c: &Consegne) {
    let slot = c.recenti.clone();
    std::thread::spawn(move || {
        let mut v: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(&cartella)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
            .collect();
        v.sort_by(|a, b| b.0.cmp(&a.0));
        let recenti: Vec<Recente> = v
            .into_iter()
            .take(12)
            .map(|(_, path)| Recente { nome: nome_di(&path), kind: cedro::kind_of(&path), thumb: cedro::ui::media::thumb_of(&path, 160), path })
            .collect();
        *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(recenti);
    });
}

/// Mette un file nell'interfaccia e prepara, in un thread, l'anteprima (foto, prima pagina o onda).
fn carica_file(ui: &mut App, c: &Consegne, cartella: &Path, path: PathBuf, nome: String, da_condivisione: bool) {
    let kind = cedro::kind_of(&path);
    ui.imposta_file(file_di(&path, nome), kind, da_condivisione);
    let (c2, p2) = (c.clone(), path.clone());
    std::thread::spawn(move || {
        if let Some(img) = cedro::ui::media::thumb_of(&p2, 600) {
            *c2.miniatura.lock().unwrap_or_else(|e| e.into_inner()) = Some((p2.clone(), img));
        } else if kind == cedro::Kind::Audio && std::fs::metadata(&p2).map(|m| m.len() < 25_000_000).unwrap_or(false) {
            if let Some(onda) = cedro::waveform(&p2, 48) {
                *c2.onda.lock().unwrap_or_else(|e| e.into_inner()) = Some((p2.clone(), onda));
            }
        }
    });
    ricarica_recenti(cartella.to_path_buf(), c);
}

/// Quanti core sono «grandi» (big.LITTLE): sono quelli con la capacità più alta. Sul Mi 9T due core A76 fanno quanto otto
/// insieme ai quattro piccoli, con molto meno calore: le reti dell'OCR (rten) girano su questi. None se i core sono tutti uguali.
fn core_grandi() -> Option<usize> {
    let mut cap = Vec::new();
    for i in 0..32 {
        let base = format!("/sys/devices/system/cpu/cpu{i}");
        let c = std::fs::read_to_string(format!("{base}/cpu_capacity")).or_else(|_| std::fs::read_to_string(format!("{base}/cpufreq/cpuinfo_max_freq")));
        match c.ok().and_then(|s| s.trim().parse::<u64>().ok()) {
            Some(v) => cap.push(v),
            None => break,
        }
    }
    let max = *cap.iter().max()?;
    let grandi = cap.iter().filter(|c| **c * 10 >= max * 9).count();
    (grandi < cap.len() && grandi >= 1).then_some(grandi.max(2).min(cap.len()))
}

#[no_mangle]
fn android_main(app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("Cedro"));
    let dati = app.internal_data_path().unwrap_or_else(|| PathBuf::from("/data/local/tmp"));
    // Il motore cerca le sue impostazioni in $HOME/.config/cedro.
    std::env::set_var("HOME", &dati);
    if std::env::var_os("RTEN_NUM_THREADS").is_none() {
        if let Some(n) = core_grandi() {
            log::info!("core grandi: {n}");
            std::env::set_var("RTEN_NUM_THREADS", n.to_string());
        }
    }
    let file_tema = dati.join("tema.txt");
    let file_lavori = dati.join("lavori.txt");
    let tema = std::fs::read_to_string(&file_tema).map(|t| ThemeId::from_key(&t)).unwrap_or(ThemeId::WindowsEssenziale);
    let mut ui = App::new(tema);
    if let Ok(t) = std::fs::read_to_string(&file_lavori) {
        ui.carica_lavori(&t);
    }
    icone_barre(&app, tema.dark());
    let mut insets = (0.0, 0.0, 0.0, 0.0);
    let mut leggi_insets = true;
    let mut tentativi = 0;

    let cartella = dati.join("condivisi");
    let consegne = Consegne::default();
    match leggi_condivisione(&app, &cartella) {
        Ok(Some(c)) => {
            log::info!("ricevuto {} ({})", c.nome, c.path.display());
            carica_file(&mut ui, &consegne, &cartella, c.path, c.nome, true);
        }
        Ok(None) => ricarica_recenti(cartella.clone(), &consegne),
        Err(e) => {
            log::error!("condivisione: {e}");
            ui.lavori.insert(0, crate::ui::Lavoro { id: 9999, origine: "Condivisione".into(), etichetta: "File".into(), conv: false, inizio: std::time::Instant::now(), quando: crate::ui::adesso(), stato: Stato::Errore(e), prog: Arc::new(std::sync::atomic::AtomicU32::new(1000)) });
            ricarica_recenti(cartella.clone(), &consegne);
        }
    }

    let mut cache = GlyphCache::new();
    let mut finestra: Option<android_activity::ndk::native_window::NativeWindow> = None;
    let mut sporco = true;
    let mut esci = false;
    let mut animando = false;
    let mut attesa_scelta = false;
    let mut servizio_attivo = false;
    let mut ultimo_servizio = std::time::Instant::now();

    while !esci {
        let attesa = if animando { 40 } else { 150 };
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
                                // Il «indietro» chiude prima le schermate; se non c'è altro lo lasciamo al sistema.
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
                        Effetto::Converti { id, file, azione, qualita, prog } => {
                            sporco = true;
                            let (a, c) = (app.clone(), consegne.clone());
                            std::thread::spawn(move || converti(a, c, id, file, azione, qualita, prog));
                        }
                        Effetto::Esci => chiudi(&app),
                        Effetto::SalvaTema(t) => {
                            let _ = std::fs::write(&file_tema, t.key());
                            icone_barre(&app, t.dark());
                            sporco = true;
                        }
                        Effetto::SalvaLavori => {
                            let _ = std::fs::write(&file_lavori, ui.serializza());
                            sporco = true;
                        }
                        Effetto::Scegli(s) => {
                            match scegli(&app, s) {
                                Ok(()) => attesa_scelta = true,
                                Err(e) => log::error!("selettore: {e}"),
                            }
                            sporco = true;
                        }
                        Effetto::UsaFile(p) => {
                            let nome = nome_di(&p);
                            carica_file(&mut ui, &consegne, &cartella, p, nome, false);
                            sporco = true;
                        }
                        Effetto::Apri(p) => {
                            let a = app.clone();
                            std::thread::spawn(move || {
                                if let Err(e) = apri_o_condividi(&a, &p, false) {
                                    log::error!("apri: {e}");
                                }
                            });
                        }
                        Effetto::Condividi(p) => {
                            let a = app.clone();
                            std::thread::spawn(move || {
                                if let Err(e) = apri_o_condividi(&a, &p, true) {
                                    log::error!("condividi: {e}");
                                }
                            });
                        }
                    }
                    stato
                });
                if !letto {
                    break;
                }
            }
        }
        // Una nuova condivisione mentre Cedro è già aperto.
        if con_attivita(&app, |env, att| env.call_method(att, "novita", "()Z", &[]).and_then(|v| v.z()).map_err(|e| jerr(env, e))).unwrap_or(false) {
            if let Ok(Some(c)) = leggi_condivisione(&app, &cartella) {
                log::info!("ricevuto {} ({})", c.nome, c.path.display());
                carica_file(&mut ui, &consegne, &cartella, c.path, c.nome, true);
                sporco = true;
            }
        }
        if attesa_scelta {
            match scelto(&app, &cartella) {
                Some(Ok(c)) => {
                    attesa_scelta = false;
                    log::info!("scelto {} ({})", c.nome, c.path.display());
                    carica_file(&mut ui, &consegne, &cartella, c.path, c.nome, false);
                    sporco = true;
                }
                Some(Err(e)) => {
                    attesa_scelta = false;
                    log::error!("selettore: {e}");
                }
                None => {}
            }
        }
        if let Some((p, img)) = consegne.miniatura.lock().unwrap_or_else(|e| e.into_inner()).take() {
            if ui.file.as_ref().is_some_and(|f| f.path == p) {
                ui.miniatura = Some(img);
                sporco = true;
            }
        }
        if let Some((p, onda)) = consegne.onda.lock().unwrap_or_else(|e| e.into_inner()).take() {
            if ui.file.as_ref().is_some_and(|f| f.path == p) {
                ui.onda = Some(onda);
                sporco = true;
            }
        }
        if let Some(r) = consegne.recenti.lock().unwrap_or_else(|e| e.into_inner()).take() {
            ui.recenti = r;
            sporco = true;
        }
        let esiti: Vec<(u32, Stato)> = std::mem::take(&mut *consegne.esiti.lock().unwrap_or_else(|e| e.into_inner()));
        if !esiti.is_empty() {
            for (id, s) in esiti {
                ui.finisce(id, s);
            }
            let _ = std::fs::write(&file_lavori, ui.serializza());
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
        // Il servizio in primo piano vive finché c'è un lavoro in corso; la notifica segue l'avanzamento.
        if ultimo_servizio.elapsed() > Duration::from_millis(700) || (servizio_attivo && ui.in_corso().0 == 0) {
            let (n, medio, nome) = ui.in_corso();
            if n > 0 {
                let testo = if n > 1 { format!("{n} lavori in corso") } else { nome };
                servizio(&app, Some(("Cedro sta lavorando", &testo, medio.map(|m| m as i32).unwrap_or(-1))));
                servizio_attivo = true;
                ultimo_servizio = std::time::Instant::now();
            } else if servizio_attivo {
                servizio(&app, None);
                servizio_attivo = false;
            }
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
