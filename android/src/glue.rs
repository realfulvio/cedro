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

use crate::ui::{App, Archivio, Effetto, Info, Recente, Sorgente, Stato};

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
    if mime.starts_with("text/") {
        // Un link condiviso da un browser o da un'altra app: «Video dal web».
        let k = j!(env.new_string("android.intent.extra.TEXT"));
        let testo = j!(env.call_method(&intento, "getStringExtra", "(Ljava/lang/String;)Ljava/lang/String;", &[JValue::Object(&k)])).l().ok().and_then(|o| stringa(env, o)).unwrap_or_default();
        return match cedro::webvideo::find_url(&testo) {
            Some(url) => Ok(Some(Condiviso { nome: url.clone(), path: PathBuf::from(url) })),
            None => Err("il testo condiviso non contiene un link".into()),
        };
    }
    let chiave = j!(env.new_string("android.intent.extra.STREAM"));
    let uri = j!(j!(env.call_method(&intento, "getParcelableExtra", "(Ljava/lang/String;)Landroid/os/Parcelable;", &[JValue::Object(&chiave)])).l());
    if uri.is_null() {
        return Err("la condivisione non contiene un file".into());
    }
    copia_uri(env, attivita, &uri, &mime, cartella).map(Some)
}

/// Il tipo del file per l'interfaccia: un vocale .opus (WhatsApp, Telegram) si trascrive, quindi conta come audio.
fn tipo_di(path: &Path) -> cedro::Kind {
    if cedro::trascrizione::only_transcribable(path) {
        cedro::Kind::Audio
    } else {
        cedro::kind_of(path)
    }
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

    // Un file:// non ha colonne: il nome è l'ultimo pezzo del percorso.
    if nome.is_empty() {
        if let Ok(v) = env.call_method(uri, "getLastPathSegment", "()Ljava/lang/String;", &[]) {
            nome = v.l().ok().and_then(|o| stringa(env, o)).unwrap_or_default();
        }
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
        }
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

/// Chiede il permesso di scrivere in Documenti (se manca) e aspetta la risposta. Da Android 11 non serve: i file che
/// Cedro crea in Documenti sono suoi. Da Android 13 chiede anche il permesso per la notifica di avanzamento (senza aspettare).
fn assicura_scrittura(app: &AndroidApp) -> bool {
    let Ok(vm) = (unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut _) }) else { return false };
    let Ok(mut env) = vm.attach_current_thread() else { return false };
    let attivita = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
    let api = env.get_static_field("android/os/Build$VERSION", "SDK_INT", "I").and_then(|v| v.i()).unwrap_or(0);
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    let perm = match api {
        30.. => "android.permission.POST_NOTIFICATIONS",
        _ => "android.permission.WRITE_EXTERNAL_STORAGE",
    };
    let controlla = |env: &mut JNIEnv| -> bool {
        let Ok(p) = env.new_string(perm) else { return false };
        let r = env.call_method(&attivita, "checkSelfPermission", "(Ljava/lang/String;)I", &[JValue::Object(&p)]).and_then(|v| v.i());
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
        }
        r.map(|v| v == 0).unwrap_or(false)
    };
    if controlla(&mut env) || (30..33).contains(&api) {
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
    if api >= 33 {
        return true;
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

/// Il testo negli appunti (vuoto se non c'è).
fn appunti(app: &AndroidApp) -> String {
    con_attivita(app, |env, att| {
        let nome = env.new_string("clipboard").map_err(|e| e.to_string())?;
        let gestore = env.call_method(att, "getSystemService", "(Ljava/lang/String;)Ljava/lang/Object;", &[JValue::Object(&nome)]).and_then(|v| v.l()).map_err(|e| jerr(env, e))?;
        let clip = env.call_method(&gestore, "getPrimaryClip", "()Landroid/content/ClipData;", &[]).and_then(|v| v.l()).map_err(|e| jerr(env, e))?;
        if clip.is_null() {
            return Ok(String::new());
        }
        let voce = env.call_method(&clip, "getItemAt", "(I)Landroid/content/ClipData$Item;", &[JValue::Int(0)]).and_then(|v| v.l()).map_err(|e| jerr(env, e))?;
        let t = env.call_method(&voce, "coerceToText", "(Landroid/content/Context;)Ljava/lang/CharSequence;", &[JValue::Object(att)]).and_then(|v| v.l()).map_err(|e| jerr(env, e))?;
        if t.is_null() {
            return Ok(String::new());
        }
        let st = env.call_method(&t, "toString", "()Ljava/lang/String;", &[]).and_then(|v| v.l()).map_err(|e| jerr(env, e))?;
        Ok(stringa(env, st).unwrap_or_default())
    })
    .unwrap_or_default()
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
            Sorgente::Altri | Sorgente::Link => ("*/*", &[]),
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
        // Un indirizzo che finisce col nome del file (ArchiviProvider), non col numero di MediaStore.
        let rel = path.strip_prefix("/storage/emulated/0").map_err(|_| "il file non è nella memoria del telefono")?;
        // Gli spazi e i segni nel nome (es. «Movie Trailers ( Coming Soon ).mp4») vanno codificati: Telegram e altre app non li reggono.
        let codificato: String = rel.to_string_lossy().bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~/".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
        let indirizzo = env.new_string(format!("content://it.cedro.android.files{codificato}")).map_err(|e| e.to_string())?;
        let uri = env
            .call_static_method("android/net/Uri", "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&indirizzo)])
            .and_then(|v| v.l())
            .map_err(|e| jerr(env, e))?;
        let uri = env.new_global_ref(uri).map_err(|e| e.to_string())?;
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
            let titolo = env.new_string(cedro::i18n::tr("Condividi")).map_err(|e| e.to_string())?;
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
fn campo_testo(env: &mut JNIEnv, classe: &str, nome: &str) -> Option<String> {
    let r = env.get_static_field(classe, nome, "Ljava/lang/String;").and_then(|v| v.l());
    match r {
        Ok(o) => stringa(env, o),
        Err(_) => {
            if env.exception_check().unwrap_or(false) {
                let _ = env.exception_clear();
            }
            None
        }
    }
}

fn riga_di(testo: &str, chiave: &str) -> Option<String> {
    testo.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        (k.trim() == chiave).then(|| v.trim().to_string())
    })
}

fn kb(testo: &str, chiave: &str) -> Option<f64> {
    riga_di(testo, chiave)?.split_whitespace().next()?.parse::<f64>().ok()
}

fn gb(kib: f64) -> String {
    format!("{:.1}", kib / 1024.0 / 1024.0).replace('.', ",")
}

/// Cosa c'è nel telefono e cosa usa Cedro: modello, Android, processore, memoria, istruzioni, accelerazioni.
fn leggi_info(app: &AndroidApp) -> Info {
    let (modello, android, soc) = con_attivita(app, |env, _att| {
        let marca = campo_testo(env, "android/os/Build", "MANUFACTURER").unwrap_or_default();
        let nome = campo_testo(env, "android/os/Build", "MODEL").unwrap_or_default();
        let versione = campo_testo(env, "android/os/Build$VERSION", "RELEASE").unwrap_or_default();
        let api = env.get_static_field("android/os/Build$VERSION", "SDK_INT", "I").and_then(|v| v.i()).unwrap_or(0);
        let soc = campo_testo(env, "android/os/Build", "SOC_MODEL").filter(|s| !s.is_empty() && s != "unknown");
        let modello = if nome.to_lowercase().starts_with(&marca.to_lowercase()) || marca.is_empty() { nome } else { format!("{} {nome}", marca.chars().next().map(|c| c.to_uppercase().collect::<String>() + &marca[c.len_utf8()..]).unwrap_or_default()) };
        Ok((modello, format!("{versione} (API {api})"), soc))
    })
    .unwrap_or_default();
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let processore = soc
        .or_else(|| riga_di(&cpuinfo, "Hardware").filter(|s| !s.is_empty()))
        .or_else(|| std::fs::read_to_string("/sys/devices/soc0/machine").ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "non indicata dal telefono".into());
    let core = (0..64).take_while(|i| std::path::Path::new(&format!("/sys/devices/system/cpu/cpu{i}")).exists()).count().max(1);
    let grandi = core_grandi();
    let core_testo = match grandi {
        Some(g) => format!("{core} ({g} grandi + {} piccoli)", core - g.min(core)),
        None => core.to_string(),
    };
    let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let memoria = match (kb(&meminfo, "MemTotal"), kb(&meminfo, "MemAvailable")) {
        (Some(t), Some(l)) => format!("{} GB ({} GB liberi)", gb(t), gb(l)),
        (Some(t), None) => format!("{} GB", gb(t)),
        _ => "non indicata dal telefono".into(),
    };
    let gpu = ["/sys/class/kgsl/kgsl-3d0/gpu_model", "/sys/kernel/gpu/gpu_model"]
        .iter()
        .find_map(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "non indicata dal telefono".into());
    let funzioni = riga_di(&cpuinfo, "Features").unwrap_or_default();
    let ha = |f: &str| funzioni.split_whitespace().any(|x| x == f);
    let hw = cedro::accel::hardware();
    let neon = ha("asimd") || ha("neon");
    let dotprod = ha("asimddp") || hw.dotprod;
    let i8mm = ha("i8mm") || hw.avx512;
    let mut calcolo = "processore".to_string();
    if dotprod {
        calcolo += ", dotprod";
    }
    if i8mm {
        calcolo += ", i8mm";
    }
    let r = |k: &str, v: String| (k.to_string(), v);
    Info {
        generali: vec![r("Telefono", modello), r("Android", android), r("Processore", processore), r("Core", core_testo), r("Memoria", memoria), r("Scheda video", gpu)],
        istruzioni: vec![("NEON".into(), neon), ("dotprod".into(), dotprod), ("i8mm".into(), i8mm)],
        accelerazioni: vec![
            ("Trascrizione".into(), calcolo, true),
            ("Video MP4".into(), if hw.gpu.is_some() { "codificatore del telefono".into() } else { "non disponibile".into() }, hw.gpu.is_some()),
        ],
    }
}

/// Mette un testo negli appunti.
fn copia_negli_appunti(app: &AndroidApp, testo: &str) {
    let _ = con_attivita(app, |env, att| {
        let nome = env.new_string("clipboard").map_err(|e| e.to_string())?;
        let gestore = env.call_method(att, "getSystemService", "(Ljava/lang/String;)Ljava/lang/Object;", &[JValue::Object(&nome)]).and_then(|v| v.l()).map_err(|e| jerr(env, e))?;
        let etichetta = env.new_string("Cedro").map_err(|e| e.to_string())?;
        let contenuto = env.new_string(testo).map_err(|e| e.to_string())?;
        let clip = env
            .call_static_method("android/content/ClipData", "newPlainText", "(Ljava/lang/CharSequence;Ljava/lang/CharSequence;)Landroid/content/ClipData;", &[JValue::Object(&etichetta), JValue::Object(&contenuto)])
            .and_then(|v| v.l())
            .map_err(|e| jerr(env, e))?;
        env.call_method(&gestore, "setPrimaryClip", "(Landroid/content/ClipData;)V", &[JValue::Object(&clip)]).map_err(|e| jerr(env, e))?;
        Ok(())
    });
}

/// La lingua del telefono (`Locale.getDefault().getLanguage()`): italiano o, per ogni altra, inglese.
fn lingua_del_telefono(app: &AndroidApp) -> cedro::i18n::Lang {
    let r = con_attivita(app, |env, _att| {
        let locale = env.call_static_method("java/util/Locale", "getDefault", "()Ljava/util/Locale;", &[]).and_then(|v| v.l()).map_err(|e| jerr(env, e))?;
        let lingua = env.call_method(&locale, "getLanguage", "()Ljava/lang/String;", &[]).and_then(|v| v.l()).map_err(|e| jerr(env, e))?;
        let lingua: String = env.get_string(&jni::objects::JString::from(lingua)).map_err(|e| jerr(env, e))?.into();
        Ok(lingua)
    });
    cedro::i18n::from_tag(&r.unwrap_or_default())
}

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
    if azione == "web-altra-app" {
        // YouTube: il link va a un'app che lo scarica (il selettore di Android propone quelle installate).
        let url = file.to_string_lossy().into_owned();
        match apri_link(&app, &url) {
            Ok(()) => c.esiti.lock().unwrap_or_else(|e| e.into_inner()).push((id, Stato::Fatto { path: PathBuf::new(), nome: String::new(), msg: "Link passato a un'altra app".into() })),
            Err(e) => c.esiti.lock().unwrap_or_else(|e| e.into_inner()).push((id, Stato::Errore(e))),
        }
        return;
    }
    if azione.starts_with("web-") {
        return web(&app, &c, id, &file, &out, azione, &prog);
    }
    if azione == "trascrivi" {
        return trascrivi(&app, &c, id, &file, &out, &prog, qualita as usize);
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

/// Passa un link al selettore di Android (ACTION_VIEW): ci sono le app che lo sanno scaricare.
fn apri_link(app: &AndroidApp, url: &str) -> Result<(), String> {
    con_attivita(app, |env, attivita| {
        let u = env.new_string(url).map_err(|e| e.to_string())?;
        let uri = env.call_static_method("android/net/Uri", "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&u)]).and_then(|v| v.l()).map_err(|e| jerr(env, e))?;
        let azione = env.new_string("android.intent.action.VIEW").map_err(|e| e.to_string())?;
        let intento = env.new_object("android/content/Intent", "(Ljava/lang/String;Landroid/net/Uri;)V", &[JValue::Object(&azione), JValue::Object(&uri)]).map_err(|e| jerr(env, e))?;
        let titolo = env.new_string(cedro::i18n::tr("Scarica con…")).map_err(|e| e.to_string())?;
        let scelta = env.call_static_method("android/content/Intent", "createChooser", "(Landroid/content/Intent;Ljava/lang/CharSequence;)Landroid/content/Intent;", &[JValue::Object(&intento), JValue::Object(&titolo)]).and_then(|v| v.l()).map_err(|e| jerr(env, e))?;
        env.call_method(attivita, "startActivity", "(Landroid/content/Intent;)V", &[JValue::Object(&scelta)]).map_err(|e| jerr(env, e))?;
        Ok(())
    })
}

/// «Video dal web»: capisce il link, scarica il file in Documenti e, per «Solo audio» o «Video leggero», lo converte col motore.
fn web(app: &AndroidApp, c: &Consegne, id: u32, link: &Path, out: &Path, azione: &str, prog: &Arc<std::sync::atomic::AtomicU32>) {
    use cedro::webvideo::{download, resolve, Want};
    let invia = |s: Stato| c.esiti.lock().unwrap_or_else(|e| e.into_inner()).push((id, s));
    let url = link.to_string_lossy().into_owned();
    let want = match azione {
        "web-audio" => Want::Audio,
        "web-leggero" => Want::Light,
        _ => Want::Video,
    };
    let finito = |path: PathBuf, msg: String| {
        let nome = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let _ = con_attivita(app, |env, att| scansiona(env, att, &path, false));
        invia(Stato::Fatto { path, nome, msg });
    };
    let media = match resolve(&url, want) {
        Ok(m) => m,
        Err(e) => return invia(Stato::Errore(e)),
    };
    log::info!("web: {} → {} ({:?} byte)", url, media.url, media.size);
    // Il file scaricato va subito al suo posto se è quello che serve; altrimenti passa da una cartella di lavoro.
    let serve_conversione = (want == Want::Audio && !media.audio) || (want == Want::Light && media.size.is_none_or(|s| s > 30_000_000) && !media.url.contains("512kb"));
    let dest = if serve_conversione { out.join(".cedro-web") } else { out.to_path_buf() };
    let parte = if serve_conversione { 0.6 } else { 1.0 };
    let niente = std::sync::atomic::AtomicBool::new(false);
    let scaricato = match download(&media, &dest, &|f| prog.store((f * parte * 1000.0) as u32, std::sync::atomic::Ordering::Relaxed), &niente) {
        Ok(p) => p,
        Err(e) => return invia(Stato::Errore(e)),
    };
    if !serve_conversione {
        let dim = std::fs::metadata(&scaricato).map(|m| m.len()).unwrap_or(0);
        return finito(scaricato, format!("{} · {:.1} MB", media.title, dim as f64 / 1e6).replace('.', ","));
    }
    let formato = if want == Want::Audio { "mp3" } else { "mp4" };
    let p2 = prog.clone();
    let esito = cedro::progress::con(
        move |f| p2.store(((0.6 + f * 0.4) * 1000.0) as u32, std::sync::atomic::Ordering::Relaxed),
        || cedro::perform_into(&scaricato, formato, 82, Some(out)),
    );
    let _ = std::fs::remove_dir_all(&dest);
    match esito {
        Ok(o) => finito(o.path, o.message),
        Err(e) => invia(Stato::Errore(e.to_string())),
    }
}

/// Voce in testo con whisper.cpp (nell'APK): il modello scelto (Veloce = tiny, Buona = base) si scarica al primo uso. Salva un .txt in Documenti.
fn trascrivi(app: &AndroidApp, c: &Consegne, id: u32, file: &std::path::Path, out: &std::path::Path, prog: &std::sync::atomic::AtomicU32, modello: usize) {
    use cedro::trascrizione::{run, Event, Format, Request};
    let invia = |s: Stato| c.esiti.lock().unwrap_or_else(|e| e.into_inner()).push((id, s));
    let req = Request { format: Format::Text, model: modello.min(1), gpu: false, avx512: false };
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
            .map(|(_, path)| Recente { nome: nome_di(&path), kind: tipo_di(&path), thumb: cedro::ui::media::thumb_of(&path, 160), path })
            .collect();
        *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(recenti);
    });
}

/// Mette un file nell'interfaccia e prepara, in un thread, l'anteprima (foto, prima pagina o onda).
fn carica_file(ui: &mut App, c: &Consegne, cartella: &Path, path: PathBuf, nome: String, da_condivisione: bool) {
    if crate::ui::e_link(&path) {
        // «Video dal web»: nessuna anteprima, il sito si legge quando si sceglie cosa farne.
        let host = nome.split_once("://").map(|(_, r)| r).unwrap_or(&nome).split('/').next().unwrap_or("").to_string();
        ui.imposta_file(Archivio { path, nome: host, dim: 0 }, cedro::Kind::Video, true);
        return;
    }
    let kind = tipo_di(&path);
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
    // La lingua: quella scelta nell'Aspetto, o quella del telefono (Android non ha LANG).
    let file_lingua = dati.join("lingua.txt");
    ui.lingua_telefono = lingua_del_telefono(&app);
    ui.lingua = match std::fs::read_to_string(&file_lingua).unwrap_or_default().trim() {
        "it" => "it",
        "en" => "en",
        _ => "auto",
    };
    // Da qui la lingua la decide solo Cedro per Android: leggere le impostazioni del motore non la cambia.
    cedro::i18n::fix();
    ui.applica_lingua();
    ui.info = leggi_info(&app);
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
                        Effetto::Copia(testo) => {
                            copia_negli_appunti(&app, &testo);
                            sporco = true;
                        }
                        Effetto::SalvaLingua(l) => {
                            let _ = std::fs::write(&file_lingua, l);
                            sporco = true;
                        }
                        Effetto::SalvaTema(t) => {
                            let _ = std::fs::write(&file_tema, t.key());
                            icone_barre(&app, t.dark());
                            sporco = true;
                        }
                        Effetto::SalvaLavori => {
                            let _ = std::fs::write(&file_lavori, ui.serializza());
                            sporco = true;
                        }
                        Effetto::Scegli(Sorgente::Link) => {
                            match cedro::webvideo::find_url(&appunti(&app)) {
                                Some(url) => carica_file(&mut ui, &consegne, &cartella, PathBuf::from(&url), url, true),
                                None => log::info!("negli appunti non c'è un link"),
                            }
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
                let testo = if n > 1 { cedro::i18n::tr_string(&format!("{n} lavori in corso")) } else { nome };
                servizio(&app, Some((cedro::i18n::tr("Cedro sta lavorando").as_ref(), &testo, medio.map(|m| m as i32).unwrap_or(-1))));
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
