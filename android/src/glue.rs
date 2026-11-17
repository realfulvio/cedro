//! La parte Android: finestra, tocchi, intento di «Condividi» (JNI) e salvataggio. L'interfaccia vera è in `ui`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use android_activity::input::{InputEvent, KeyAction, KeyMapChar, Keycode, MotionAction};
use android_activity::{AndroidApp, InputStatus, MainEvent, PollEvent};
use cedro::ui::gfx::{Fonts, GlyphCache, Image, Painter};
use cedro::ui::theme::ThemeId;
use jni::objects::{JObject, JString, JValue};
mod saf;
mod web;
use saf::{copia_uri, figli};
use jni::{JNIEnv, JavaVM};

use crate::publication::{Destinazione, Registro, StatoUscita};
use crate::ui::{App, Archivio, Compito, Completamento, Effetto, Info, Recente, Sorgente, Stato, Tasto};

/// Dove finiscono le copie.
const DOCUMENTI: &str = "/storage/emulated/0/Documents";

/// «Se il file esiste già» delle Impostazioni, per i file pubblicati in MediaStore (0 numera, 1 sostituisci, 2 salta).
static ESISTE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

fn ricorda_esiste(e: cedro::Exists) {
    let v = match e {
        cedro::Exists::Replace => 1,
        cedro::Exists::Skip => 2,
        _ => 0,
    };
    ESISTE.store(v, std::sync::atomic::Ordering::Relaxed);
}

/// Quello che arriva dal menu Condividi.
struct Condiviso {
    path: PathBuf,
    nome: String,
}

fn jerr(env: &mut JNIEnv, e: jni::errors::Error) -> String {
    if env.exception_check().unwrap_or(false) {
        // Il messaggio dell'eccezione Java dice cosa è successo davvero.
        let ecc = env.exception_occurred();
        let _ = env.exception_clear();
        if let Ok(ecc) = ecc {
            if let Some(m) = env.call_method(&ecc, "toString", "()Ljava/lang/String;", &[]).and_then(|v| v.l()).ok().and_then(|o| stringa(env, o)) {
                let _ = env.exception_clear();
                return m;
            }
            let _ = env.exception_clear();
        }
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
        "image/heic" | "image/heif" => "heic",
        "image/avif" => "avif",
        "image/jxl" => "jxl",
        "application/dicom" => "dcm",
        "application/vnd.ms-excel" => "xls",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => "xlsx",
        "application/vnd.oasis.opendocument.spreadsheet" => "ods",
        "application/pdf" => "pdf",
        "application/pkcs7-mime" | "application/x-pkcs7-mime" => "p7m",
        "application/pkcs7-signature" | "application/x-pkcs7-signature" => "p7s",
        "application/timestamped-data" => "tsd",
        "application/timestamp-reply" => "tsr",
        "application/zip" | "application/x-zip-compressed" => "zip",
        "text/plain" => "txt",
        "text/markdown" => "md",
        "text/html" => "html",
        "text/csv" => "csv",
        "application/json" => "json",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => "docx",
        "application/msword" => "doc",
        "application/vnd.oasis.opendocument.text" => "odt",
        "application/rtf" | "text/rtf" => "rtf",
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
        _ => "bin",
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
        "odt" => "application/vnd.oasis.opendocument.text",
        "rtf" => "application/rtf",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "xls" => "application/vnd.ms-excel",
        "ods" => "application/vnd.oasis.opendocument.spreadsheet",
        "doc" => "application/msword",
        "dcm" => "application/dicom",
        "heic" | "heif" => "image/heic",
        "p7m" => "application/pkcs7-mime",
        "p7s" => "application/pkcs7-signature",
        "tsd" => "application/timestamped-data",
        "tsr" => "application/timestamp-reply",
        "zip" => "application/zip",
        "opus" => "audio/ogg",
        "aif" | "aiff" => "audio/aiff",
        "avif" => "image/avif",
        "jxl" => "image/jxl",
        "ico" => "image/x-icon",
        "tga" => "image/x-tga",
        "pnm" | "ppm" | "pgm" | "pbm" => "image/x-portable-anymap",
        "exr" => "image/x-exr",
        "srt" => "application/x-subrip",
        "vtt" => "text/vtt",
        _ => "application/octet-stream",
    }
}

/// Legge l'intento che ha avviato l'attività: se è un ACTION_SEND, copia il flusso in un file privato.
fn importa_condivisione(app: &AndroidApp, cartella: &Path, c: &Consegne, ui: &mut App) {
    // Capture the exact intent on the UI thread; a subsequent onNewIntent cannot replace it.
    let intent = con_attivita(app, |env,att| {
        let i=env.call_method(att,"getIntent","()Landroid/content/Intent;",&[]).and_then(|v|v.l()).map_err(|e|jerr(env,e))?;
        let action=env.call_method(&i,"getAction","()Ljava/lang/String;",&[]).and_then(|v|v.l()).ok().and_then(|v|stringa(env,v)).unwrap_or_default();
        if !matches!(action.as_str(),"android.intent.action.SEND"|"android.intent.action.SEND_MULTIPLE"|"android.intent.action.VIEW") { return Ok(None); }
        env.new_global_ref(i).map(Some).map_err(|e|e.to_string())
    });
    let intent=match intent { Ok(Some(i))=>i, Ok(None)=>{ricarica_recenti(cartella.to_path_buf(),c); return;}, Err(e)=>{ui.errore_importazione(e); return;} };
    let (id,prog)=ui.nuova_importazione();
    c.ultimo_import.store(id,std::sync::atomic::Ordering::Release);
    if let Some(dir)=app.internal_data_path(){persisti(c,&dir.join("lavori.txt"),ui.serializza());}
    let cancel=token(c,id);
    let (a,dir,c)=(app.clone(),cartella.to_path_buf(),c.clone());
    std::thread::spawn(move || {
        let r=std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            saf::contesto(id,cancel,prog,{let sink=c.passo_riassunto.clone();move|text|sink.lock().unwrap_or_else(|e|e.into_inner()).push((id,text))},||con_attivita(&a,|env,att|leggi_condivisione_in(env,att,intent.as_obj(),&dir)))
        })).unwrap_or_else(|_|Err("Importazione interrotta".into()));
        c.importati.lock().unwrap_or_else(|e|e.into_inner()).push((id,None,r));
        a.create_waker().wake();
    });
}

fn leggi_condivisione_in(env: &mut JNIEnv, attivita: &JObject, intento: &JObject, cartella: &Path) -> Result<Vec<Condiviso>, String> {
    macro_rules! j {
        ($e:expr) => {
            match $e {
                Ok(v) => v,
                Err(e) => return Err(jerr(env, e)),
            }
        };
    }
    if intento.is_null() {
        return Ok(Vec::new());
    }
    let azione = j!(env.call_method(&intento, "getAction", "()Ljava/lang/String;", &[])).l().ok().and_then(|o| stringa(env, o));
    let mime = j!(env.call_method(&intento, "getType", "()Ljava/lang/String;", &[])).l().ok().and_then(|o| stringa(env, o)).unwrap_or_default();
    let chiave = j!(env.new_string("android.intent.extra.STREAM"));
    if azione.as_deref() == Some("android.intent.action.SEND_MULTIPLE") {
        // Più PDF condivisi insieme: si uniscono.
        let lista = j!(j!(env.call_method(&intento, "getParcelableArrayListExtra", "(Ljava/lang/String;)Ljava/util/ArrayList;", &[JValue::Object(&chiave)])).l());
        if lista.is_null() {
            return Err("la condivisione non contiene file".into());
        }
        let n = j!(j!(env.call_method(&lista, "size", "()I", &[])).i());
        let mut out = Vec::new();
        for i in 0..n {
            let item=saf::frame(env,|env|{
                let mut uri=env.call_method(&lista,"get","(I)Ljava/lang/Object;",&[JValue::Int(i)]).and_then(|v|v.l()).map_err(|e|jerr(env,e))?;
                if !uri.is_null()&&env.is_instance_of(&uri,"java/lang/String").map_err(|e|jerr(env,e))? {
                    uri=env.call_static_method("android/net/Uri","parse","(Ljava/lang/String;)Landroid/net/Uri;",&[JValue::Object(&uri)]).and_then(|v|v.l()).map_err(|e|jerr(env,e))?;
                }
                if uri.is_null(){Ok(None)}else{copia_uri(env,attivita,&uri,"",cartella).map(Some)}
            })?;
            if let Some(c)=item{out.push(c);}
        }
        return Ok(out);
    }
    if azione.as_deref() == Some("android.intent.action.VIEW") {
        // «Apri con Cedro» (un allegato firmato nella mail): il file è nei dati dell'intento.
        let uri = j!(j!(env.call_method(&intento, "getData", "()Landroid/net/Uri;", &[])).l());
        if uri.is_null() {
            return Ok(Vec::new());
        }
        return copia_uri(env, attivita, &uri, &mime, cartella).map(|c| vec![c]);
    }
    if azione.as_deref() != Some("android.intent.action.SEND") {
        return Ok(Vec::new());
    }
    let uri = j!(j!(env.call_method(&intento, "getParcelableExtra", "(Ljava/lang/String;)Landroid/os/Parcelable;", &[JValue::Object(&chiave)])).l());
    if uri.is_null() {
        if !mime.starts_with("text/") {
            return Err("la condivisione non contiene un file".into());
        }
        // Un link condiviso da un browser o da un'altra app: «Video dal web».
        let k = j!(env.new_string("android.intent.extra.TEXT"));
        let testo = j!(env.call_method(&intento, "getStringExtra", "(Ljava/lang/String;)Ljava/lang/String;", &[JValue::Object(&k)])).l().ok().and_then(|o| stringa(env, o)).unwrap_or_default();
        return match cedro::webvideo::find_url(&testo) {
            Some(url) => Ok(vec![Condiviso { nome: url.clone(), path: PathBuf::from(url) }]),
            None => Err("il testo condiviso non contiene un link".into()),
        };
    }
    copia_uri(env, attivita, &uri, &mime, cartella).map(|c| vec![c])
}

/// Il tipo del file per l'interfaccia: un vocale .opus (WhatsApp, Telegram) si trascrive, quindi conta come audio.
fn tipo_di(path: &Path) -> cedro::Kind {
    if cedro::trascrizione::only_transcribable(path) {
        cedro::Kind::Audio
    } else {
        cedro::kind_of(path)
    }
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
        if sorgente == Sorgente::Cartella {
            // Una cartella: il selettore dà l'albero, i file si leggono dopo (`figli`).
            let azione = env.new_string("android.intent.action.OPEN_DOCUMENT_TREE").map_err(|e| e.to_string())?;
            let intento = env.new_object("android/content/Intent", "(Ljava/lang/String;)V", &[JValue::Object(&azione)]).map_err(|e| jerr(env, e))?;
            env.call_method(&intento,"addFlags","(I)Landroid/content/Intent;",&[JValue::Int(1 | 64)]).map_err(|e|jerr(env,e))?;
            env.call_method(attivita, "startActivityForResult", "(Landroid/content/Intent;I)V", &[JValue::Object(&intento), JValue::Int(4242)]).map_err(|e| jerr(env, e))?;
            return Ok(());
        }
        let azione = env.new_string("android.intent.action.OPEN_DOCUMENT").map_err(|e| e.to_string())?;
        let intento = env.new_object("android/content/Intent", "(Ljava/lang/String;)V", &[JValue::Object(&azione)]).map_err(|e| jerr(env, e))?;
        let cat = env.new_string("android.intent.category.OPENABLE").map_err(|e| e.to_string())?;
        env.call_method(&intento, "addCategory", "(Ljava/lang/String;)Landroid/content/Intent;", &[JValue::Object(&cat)]).map_err(|e| jerr(env, e))?;
        let (tipo, altri): (&str, &[&str]) = match sorgente {
            Sorgente::Foto => ("image/*", &[]),
            Sorgente::Audio => ("audio/*", &[]),
            Sorgente::Altri | Sorgente::Link => ("*/*", &[]),
            Sorgente::PdfAltri => ("application/pdf", &[]),
            Sorgente::Coda | Sorgente::Cartella | Sorgente::Firmati | Sorgente::Originale => ("*/*", &[]),
            Sorgente::Documenti => (
                "*/*",
                &[
                    "application/pdf",
                    "text/*",
                    "application/json",
                    "application/msword",
                    "application/vnd.ms-excel",
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                    "application/vnd.oasis.opendocument.spreadsheet",
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
        if matches!(sorgente, Sorgente::PdfAltri | Sorgente::Coda) {
            let k = env.new_string("android.intent.extra.ALLOW_MULTIPLE").map_err(|e| e.to_string())?;
            env.call_method(&intento, "putExtra", "(Ljava/lang/String;Z)Landroid/content/Intent;", &[JValue::Object(&k), JValue::Bool(1)]).map_err(|e| jerr(env, e))?;
        }
        env.call_method(&intento,"addFlags","(I)Landroid/content/Intent;",&[JValue::Int(1 | 64)]).map_err(|e|jerr(env,e))?;
        env.call_method(attivita, "startActivityForResult", "(Landroid/content/Intent;I)V", &[JValue::Object(&intento), JValue::Int(4242)]).map_err(|e| jerr(env, e))?;
        Ok(())
    })
}

/// I file scelti nel selettore, se sono arrivati: li copia nella cartella dei file di Cedro.
fn importa_scelti(app: &AndroidApp, cartella: &Path, sorgente: Sorgente, c: &Consegne, ui: &mut App) -> bool {
    let arr=con_attivita(app,|env,att| {
        let arr=env.call_method(att,"scelti","()[Landroid/net/Uri;",&[]).and_then(|v|v.l()).map_err(|e|jerr(env,e))?;
        if arr.is_null() { return Ok(None); }
        env.new_global_ref(arr).map(Some).map_err(|e|e.to_string())
    });
    let arr=match arr { Ok(Some(a))=>a, Ok(None)=>return false, Err(e)=> { ui.errore_importazione(e); return true; } };
    let (id,prog)=ui.nuova_importazione();
    c.ultimo_import.store(id,std::sync::atomic::Ordering::Release);
    if let Some(dir)=app.internal_data_path(){persisti(c,&dir.join("lavori.txt"),ui.serializza());}
    let cancel=token(c,id);
    let (a,dir,c)=(app.clone(),cartella.to_path_buf(),c.clone());
    std::thread::spawn(move || {
        let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| saf::contesto(id,cancel,prog,{let sink=c.passo_riassunto.clone();move|text|sink.lock().unwrap_or_else(|e|e.into_inner()).push((id,text))},||con_attivita(&a,|env,att| {
            let local=env.new_local_ref(arr.as_obj()).map_err(|e|e.to_string())?;
            let arr=jni::objects::JObjectArray::from(local);
            let n=env.get_array_length(&arr).map_err(|e|jerr(env,e))?;
            let mut out=Vec::new();
            for i in 0..n {
                let v=saf::frame(env,|env|{
                    let uri=env.get_object_array_element(&arr,i).map_err(|e|jerr(env,e))?;
                    if sorgente==Sorgente::Cartella {figli(env,att,&uri,&dir)}
                    else {copia_uri(env,att,&uri,"",&dir).map(|c|vec![c])}
                })?;
                out.extend(v);
            }
            if sorgente==Sorgente::Originale {
                let result=out.first().ok_or_else(||"Nessun file scelto".to_string()).and_then(|f| {
                    use std::io::Read;
                    let mut file=std::fs::File::open(&f.path).map_err(|e|e.to_string())?;
                    let mut bytes=Vec::new();
                    let mut chunk=[0u8;64*1024];
                    let stop=token(&c,id);
                    loop {
                        if stop.load(std::sync::atomic::Ordering::Acquire){return Err("Annullato".into());}
                        let n=file.read(&mut chunk).map_err(|e|e.to_string())?;
                        if n==0{break;}
                        bytes.try_reserve(n).map_err(|_|"Memoria insufficiente per verificare il file originale".to_string())?;
                        bytes.extend_from_slice(&chunk[..n]);
                    }
                    Ok(bytes)
                });
                *c.originale.lock().unwrap_or_else(|e|e.into_inner())=Some((id,result));
            }
            Ok(out)
        })))).unwrap_or_else(|_|Err("Importazione interrotta".into()));
        c.importati.lock().unwrap_or_else(|e|e.into_inner()).push((id,Some(sorgente),result));
        a.create_waker().wake();
    });
    true
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
        let testo = path.to_string_lossy().into_owned();
        // Da Android 11 il risultato è già in MediaStore (`content://media/…`); prima, un indirizzo che finisce col nome
        // del file (ArchiviProvider).
        let indirizzo = if testo.starts_with("content://") {
            testo.clone()
        } else {
            let privato = app.internal_data_path().map(|p|p.join("uscite"));
            let relativo;
            let rel = if let Some(p) = privato.as_ref().and_then(|p|path.strip_prefix(p).ok()) {
                relativo = PathBuf::from("/privati").join(p); &relativo
            } else { path.strip_prefix("/storage/emulated/0").map_err(|_| "il file non è nella memoria del telefono")? };
            // Gli spazi e i segni nel nome (es. «Movie Trailers ( Coming Soon ).mp4») vanno codificati: Telegram e altre app non li reggono.
            let codificato: String = rel.to_string_lossy().bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~/".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
            format!("content://it.cedro.android.files{codificato}")
        };
        let indirizzo = env.new_string(indirizzo).map_err(|e| e.to_string())?;
        let uri = env
            .call_static_method("android/net/Uri", "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&indirizzo)])
            .and_then(|v| v.l())
            .map_err(|e| jerr(env, e))?;
        let uri = env.new_global_ref(uri).map_err(|e| e.to_string())?;
        let mime = if testo.starts_with("content://") {
            let resolver = env.call_method(attivita, "getContentResolver", "()Landroid/content/ContentResolver;", &[]).and_then(|v| v.l()).map_err(|e| jerr(env, e))?;
            let t = env.call_method(&resolver, "getType", "(Landroid/net/Uri;)Ljava/lang/String;", &[JValue::Object(uri.as_obj())]).and_then(|v| v.l()).map_err(|e| jerr(env, e))?;
            stringa(env, t).unwrap_or_else(|| "*/*".into())
        } else {
            mime_da_percorso(path).to_string()
        };
        let mime_s = env.new_string(&mime).map_err(|e| e.to_string())?;
        let azione = env.new_string(if condividi { "android.intent.action.SEND" } else { "android.intent.action.VIEW" }).map_err(|e| e.to_string())?;
        let intento = env.new_object("android/content/Intent", "(Ljava/lang/String;)V", &[JValue::Object(&azione)]).map_err(|e| jerr(env, e))?;
        if condividi {
            if !testo.starts_with("content://") {
                let nome_f = env.new_string(path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()).map_err(|e| e.to_string())?;
                let k = env.new_string("android.intent.extra.TITLE").map_err(|e| e.to_string())?;
                env.call_method(&intento, "putExtra", "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;", &[JValue::Object(&k), JValue::Object(&nome_f)]).map_err(|e| jerr(env, e))?;
            }
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
/// La cartella delle librerie dell'APK: l'unico posto da cui Android lascia partire dei programmi (Python, QuickJS, ffmpeg per YouTube).
fn cartella_librerie(app: &AndroidApp) -> Option<PathBuf> {
    con_attivita(app, |env, att| {
        let ai = env.call_method(att, "getApplicationInfo", "()Landroid/content/pm/ApplicationInfo;", &[]).and_then(|v| v.l()).map_err(|e| jerr(env, e))?;
        let dir = env.get_field(&ai, "nativeLibraryDir", "Ljava/lang/String;").and_then(|v| v.l()).map_err(|e| jerr(env, e))?;
        let dir: String = env.get_string(&jni::objects::JString::from(dir)).map_err(|e| jerr(env, e))?.into();
        Ok(PathBuf::from(dir))
    })
    .ok()
}

fn leggi_info(app: &AndroidApp) -> Info {
    let versione = con_attivita(app, |env, att| {
        let pm = env.call_method(att, "getPackageManager", "()Landroid/content/pm/PackageManager;", &[]).and_then(|v| v.l()).map_err(|e| jerr(env, e))?;
        let nome = env.call_method(att, "getPackageName", "()Ljava/lang/String;", &[]).and_then(|v| v.l()).map_err(|e| jerr(env, e))?;
        let pi = env
            .call_method(&pm, "getPackageInfo", "(Ljava/lang/String;I)Landroid/content/pm/PackageInfo;", &[JValue::Object(&nome), JValue::Int(0)])
            .and_then(|v| v.l())
            .map_err(|e| jerr(env, e))?;
        let vn = env.get_field(&pi, "versionName", "Ljava/lang/String;").and_then(|v| v.l()).map_err(|e| jerr(env, e))?;
        let vn: String = env.get_string(&jni::objects::JString::from(vn)).map_err(|e| jerr(env, e))?.into();
        let vc = env.get_field(&pi, "versionCode", "I").and_then(|v| v.i()).map_err(|e| jerr(env, e))?;
        Ok(format!("{vn} ({vc})"))
    })
    .unwrap_or_else(|_| "?".into());
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
        versione,
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
    azioni:Arc<Mutex<std::collections::HashMap<u32,String>>>,
    rete:Arc<(Mutex<u8>,std::sync::Condvar)>,
    download_attesa:Casella<crate::web::AttesaDownload>,
    componenti: Arc<Mutex<crate::web::Componenti>>,
    labs: Arc<Mutex<Vec<(u32,Result<(crate::labs::Info,Image),String>)>>>,
    caso_versione: Arc<std::sync::atomic::AtomicU32>,
    caso_lock: Arc<Mutex<()>>,
    caso_esito: Casella<Result<(),String>>,
    miniatura: Casella<(PathBuf, Image)>,
    onda: Casella<(PathBuf, Vec<f32>)>,
    recenti: Casella<Vec<Recente>>,
    esiti: Arc<Mutex<Vec<Completamento>>>,
    tokens: Arc<Mutex<std::collections::HashMap<u32, Arc<std::sync::atomic::AtomicBool>>>>,
    identita: Arc<Mutex<std::collections::HashMap<u32,crate::publication::Identita>>>,
    importati: Arc<Mutex<Vec<(u32, Option<Sorgente>, Result<Vec<Condiviso>, String>)>>>,
    originale: Casella<(u32,Result<Vec<u8>,String>)>,
    ultimo_import: Arc<std::sync::atomic::AtomicU32>,
    journal: Arc<Mutex<(bool,Option<(PathBuf,String)>)>>,
    recuperati: Casella<Vec<crate::publication::Registro>>,
    /// Pagine e segnalibri del PDF aperto, o l'errore (protetto da password).
    info_pdf: Casella<(PathBuf, Result<(u32, Vec<(u32, String)>), String>)>,
    /// Pagine e miniatura dei PDF da unire.
    unito: Arc<Mutex<Vec<(PathBuf, Option<u32>, Option<Image>)>>>,
    /// Le miniature delle pagine (da 0) del PDF aperto.
    pagine: Arc<Mutex<Vec<(PathBuf, u32, Image)>>>,
    /// Com'è andata la password: la copia senza, o l'errore.
    sblocco: Casella<Result<PathBuf, String>>,
    /// L'onda di una revisione aperta.
    onda_riv: Arc<Mutex<Vec<(PathBuf, Option<Vec<f32>>, f64)>>>,
    frasi: Arc<Mutex<Vec<(u32,PathBuf,Option<cedro::trascrizione::Segment>,f64)>>>,
    /// I punti da controllare di una trascrizione finita (lavoro, quanti).
    controlli: Arc<Mutex<Vec<(u32, usize)>>>,
    /// Il Riassunto AI: il passo in parole (lavoro, testo) e il risultato (lavoro, trascrizione, riassunto).
    passo_riassunto: Arc<Mutex<Vec<(u32, String)>>>,
    riassunti: Arc<Mutex<Vec<(u32, PathBuf, cedro::riassunto::Stored)>>>,
/// Miniatura, misure e pagine dei file in coda nella finestra principale.
    voci: Arc<Mutex<Vec<(PathBuf, Option<Image>, Option<(u32, u32)>, Option<u32>)>>>,
    /// Le pagine del documento firmato mostrato (`None`: finite).
    firma_pagine: Arc<Mutex<Vec<(PathBuf, Option<Image>)>>>,
}

fn registra(c:&Consegne,ui:&mut App){
    let ids:Vec<u32>=ui.lavori.iter().map(|l|l.id).collect();
    let mut map=c.identita.lock().unwrap_or_else(|e|e.into_inner());
    for id in ids{if let Some(v)=ui.identita(id){map.insert(id,v);}}
}

/// Uno scrittore transitorio: coalesca gli stati senza I/O nel loop degli eventi.
fn persisti(c:&Consegne,path:&Path,testo:String) {
    let mut state=c.journal.lock().unwrap_or_else(|e|e.into_inner());
    state.1=Some((path.to_path_buf(),testo));
    if state.0 {return;}
    state.0=true;
    let journal=c.journal.clone();
    drop(state);
    std::thread::spawn(move || loop {
        let next={
            let mut state=journal.lock().unwrap_or_else(|e|e.into_inner());
            match state.1.take(){Some(v)=>v,None=>{state.0=false;return;}}
        };
        if let Err(e)=crate::storage::atomico(&next.0,next.1.as_bytes()){log::error!("cronologico: {e}");}
    });
}

fn pulisci_incompleti(base:&Path,noti:&std::collections::HashSet<PathBuf>,prima:std::time::SystemTime){
    fn pronto(p:&Path)->bool {p.join(".batch-child").is_file()||p.join(".batch").is_file()||p.join(".pronti").is_file()||crate::publication::sorgente_revisione(p).is_some()||std::fs::read_dir(p).into_iter().flatten().filter_map(Result::ok).any(|e|e.file_type().is_ok_and(|t|t.is_dir())&&pronto(&e.path()))}
    if let Ok(entries)=std::fs::read_dir(base){for e in entries.flatten(){
        let p=e.path();let name=nome_di(&p);
        let generato=name.split_once('-').is_some_and(|(a,b)|!a.is_empty()&&!b.is_empty()&&a.chars().all(|c|c.is_ascii_hexdigit())&&b.chars().all(|c|c.is_ascii_hexdigit()));
        if generato&&e.file_type().is_ok_and(|t|t.is_dir())&&e.metadata().and_then(|m|m.modified()).is_ok_and(|t|t<prima)&&!noti.iter().any(|n|n.starts_with(&p))&&!pronto(&p){let _=std::fs::remove_dir_all(p);}
    }}
}

fn recupera_registro(app:&AndroidApp,dir:&Path)->Option<Registro>{
    let mut r=Registro::leggi(dir).or_else(|_|crate::publication::recupera_revisione(dir)).ok()?;
    for i in 0..r.elementi.len(){
        match r.elementi[i].stato.clone(){
            StatoUscita::Pubblicato(d)=>{let _=r.completato(dir,i,d);},
            StatoUscita::Preparato(mut d)=>{
                let committed=if d.path.to_string_lossy().starts_with("content://"){media_committed(app,&mut d)}else{Ok(crate::publication::legacy_committed(&d))};
                match committed{
                    Ok(true)=>{let _=r.completato(dir,i,d);},
                    Ok(false)=>{
                        // Il marker legacy resta, ma i byte parziali non occupano più spazio.
                        if let Some(temp)=&d.temporaneo{crate::publication::svuota_temporaneo(temp);}
                        r.elementi[i].errore=Some("Recuperato dopo un’interruzione".into());
                        let _=r.salva(dir);
                    }
                    Err(e)=>{r.elementi[i].errore=Some(e);let _=r.salva(dir);},
                }
            }
            StatoUscita::Privato=>{},
        }
    }
    Some(r)
}

fn recupera_uscite(app:&AndroidApp,base:&Path,prima:std::time::SystemTime)->Vec<Registro> {
    let mut out=Vec::new();
    let Ok(entries)=std::fs::read_dir(base) else{return out;};
    for e in entries.flatten(){
        let dir=e.path();if !e.file_type().is_ok_and(|t|t.is_dir())||!e.metadata().and_then(|m|m.modified()).is_ok_and(|t|t<prima){continue;}
        if let Some(r)=recupera_registro(app,&dir){out.push(r);}
        out.extend(recupera_uscite(app,&dir,prima));
    }
    out
}

fn nome_pubblico(env:&mut JNIEnv,resolver:&JObject,uri:&JObject)->Option<String>{
    let read=(||->Result<Option<String>,jni::errors::Error>{
        let col=env.new_string("_display_name")?;
        let columns=env.new_object_array(1,"java/lang/String",&col)?;
        let null=JObject::null();
        let cur=env.call_method(resolver,"query","(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;",&[JValue::Object(uri),JValue::Object(&columns),JValue::Object(&null),JValue::Object(&null),JValue::Object(&null)])?.l()?;
        if cur.is_null(){return Ok(None);}
        let result=(||{
            if !env.call_method(&cur,"moveToFirst","()Z",&[])?.z()?{return Ok(None);}
            let value=env.call_method(&cur,"getString","(I)Ljava/lang/String;",&[JValue::Int(0)])?.l()?;
            Ok(stringa(env,value))
        })();
        let _=env.call_method(&cur,"close","()V",&[]);result
    })();
    let _=env.exception_clear();read.ok().flatten()
}

fn media_committed(app:&AndroidApp,d:&mut Destinazione)->Result<bool,String>{
    con_attivita(app,|env,att|{
        let value=env.new_string(d.path.to_string_lossy()).map_err(|e|e.to_string())?;
        let uri=env.call_static_method("android/net/Uri","parse","(Ljava/lang/String;)Landroid/net/Uri;",&[JValue::Object(&value)]).and_then(|v|v.l()).map_err(|e|jerr(env,e))?;
        let resolver=env.call_method(att,"getContentResolver","()Landroid/content/ContentResolver;",&[]).and_then(|v|v.l()).map_err(|e|jerr(env,e))?;
        let col=env.new_string("is_pending").map_err(|e|e.to_string())?;
        let columns=env.new_object_array(1,"java/lang/String",&col).map_err(|e|jerr(env,e))?;
        let null=JObject::null();
        let cur=env.call_method(&resolver,"query","(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;",&[JValue::Object(&uri),JValue::Object(&columns),JValue::Object(&null),JValue::Object(&null),JValue::Object(&null)]).and_then(|v|v.l()).map_err(|e|jerr(env,e))?;
        if cur.is_null(){return Ok(false);}
        let result=(||->Result<bool,String>{
            if !env.call_method(&cur,"moveToFirst","()Z",&[]).and_then(|v|v.z()).map_err(|e|jerr(env,e))?{return Ok(false);}
            let pending=env.call_method(&cur,"getInt","(I)I",&[JValue::Int(0)]).and_then(|v|v.i()).map_err(|e|jerr(env,e))?;
            Ok(pending==0)
        })();
        let _=env.call_method(&cur,"close","()V",&[]);
        if result.as_ref().is_ok_and(|v|*v){
            if let Some(name)=nome_pubblico(env,&resolver,&uri){
                if let Some((parent,_))=d.nome.rsplit_once('/'){d.nome=format!("{parent}/{name}");}
            }
        }
        result
    })
}

fn token(c: &Consegne, id: u32) -> Arc<std::sync::atomic::AtomicBool> {
    c.tokens.lock().unwrap_or_else(|e| e.into_inner()).entry(id).or_default().clone()
}

fn protetto(c: &Consegne, id: u32, f: impl FnOnce()) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err() {
        c.esiti.lock().unwrap_or_else(|e| e.into_inner()).push(Completamento::semplice(id, Stato::Errore("Il motore si è interrotto".into())));
    }
    let annullato=c.tokens.lock().unwrap_or_else(|e|e.into_inner()).get(&id).is_some_and(|t|t.load(std::sync::atomic::Ordering::Acquire));
    c.componenti.lock().unwrap_or_else(|e|e.into_inner()).finisci(id,annullato);
    c.tokens.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
}

fn processo(app: &AndroidApp, c: &Consegne, id: u32, file: &Path, azione: &str, qualita: u8, voce:crate::worker::Voce, out: &Path, opts: cedro::Options, unisci: Vec<(PathBuf, Option<cedro::PageSel>)>, prog: &std::sync::atomic::AtomicU32, web:Option<crate::web::Scelte>) -> Result<(PathBuf, String), String> {
    let libs = cartella_librerie(app).ok_or("Il motore non è in questo APK")?;
    let cancel = token(c, id);
    c.passo_riassunto.lock().unwrap_or_else(|e|e.into_inner()).push((id,"Aspetto che finisca l’altro lavoro…".into()));
    let _permit=cedro::resources::acquire(Some(&cancel)).ok_or("Annullato")?;
    c.azioni.lock().unwrap_or_else(|e|e.into_inner()).insert(id,azione.into());
    log::info!("lavoro avvio id={id} azione={azione}");
    let mut input_paths=vec![file.to_path_buf()];
    input_paths.extend(unisci.iter().map(|u|u.0.clone()));
    let leases=saf::leases(&input_paths);
    if let Some(identity)=c.identita.lock().unwrap_or_else(|e|e.into_inner()).get(&id){
        let bytes=serde_json::to_vec(identity).map_err(|e|e.to_string())?;
        crate::storage::atomico(&out.join(".lavoro"),&bytes).map_err(|e|e.to_string())?;
    }
    crate::worker::avvia(&libs, crate::worker::Richiesta { file: file.into(), azione: azione.into(), qualita, voce, out: out.into(), opts, unisci, sommario:None,anteprima:None,web }, &cancel, leases, |nome,bytes|web::permesso(app,c,id,nome,bytes), |e| match e {
        crate::worker::Evento::Audio(path,secs) => {c.frasi.lock().unwrap_or_else(|e|e.into_inner()).push((id,path,None,secs));},
        crate::worker::Evento::Frase(path,seg) => {c.frasi.lock().unwrap_or_else(|e|e.into_inner()).push((id,path,Some(seg),0.0));},
        crate::worker::Evento::Componente(s) => {c.componenti.lock().unwrap_or_else(|e|e.into_inner()).ricevi(id,s);app.create_waker().wake();},
        crate::worker::Evento::Progresso(n) => {if prog.swap(n, std::sync::atomic::Ordering::Relaxed)!=n {app.create_waker().wake();}},
        crate::worker::Evento::Passo(k, t) => {
            if let Some(t) = passo_trascrizione(&k, &t) { c.passo_riassunto.lock().unwrap_or_else(|e|e.into_inner()).push((id,t)); }
        }
        _ => {},
    })
}

/// Un solo completamento contiene stato, tutte le uscite e l'eventuale audio di revisione.
fn pubblica_lavoro(app: &AndroidApp, c: &Consegne, id: u32, out: &Path, principale: PathBuf, msg: String) -> Completamento {
    if principale.as_os_str().is_empty() { return Completamento::semplice(id,Stato::Fatto {path:principale,nome:String::new(),msg}); }
    let result=(||->Result<Completamento,String>{
        let mut identity=c.identita.lock().unwrap_or_else(|e|e.into_inner()).get(&id).cloned().ok_or("Identità del lavoro assente")?;
        if let Ok(file)=std::fs::File::open(out.join(".revisione-source")){
            let source:PathBuf=serde_json::from_reader(file).map_err(|e|e.to_string())?;
            if !source.starts_with(out)||!source.is_file(){return Err("Sorgente della revisione non disponibile".into());}
            identity.sorgente=Some(source);
        }
        let mut r=Registro::prepara(out,&principale,identity,msg)?;
        let mode=crate::web::Salvataggio::leggi(out).map(|s|s.mode).unwrap_or_else(||ESISTE.load(std::sync::atomic::Ordering::Relaxed));
        let reused=crate::web::riusi(&uscite(app).ok_or("Cartella assente")?,out,&mut r,|d|{
            if d.path.to_string_lossy().starts_with("content://"){media_committed(app,d).unwrap_or(false)}else{d.path.is_file()}
        });
        r.salva(out)?;
        let cancel=token(c,id);
        for i in 0..r.elementi.len(){
            if cancel.load(std::sync::atomic::Ordering::Acquire){r.elementi[i].errore=Some("Annullato".into());r.salva(out)?;continue;}
            if let Some((_,d))=reused.iter().find(|(index,_)|*index==i){
                r.prenota(out,i,d.clone())?;r.completato(out,i,d.clone())?;continue;
            }
            let p=r.elementi[i].privato.clone();
            let relative=r.sottocartella(i);
            let base=r.pubblico.clone();
            let structured=r.gruppo.is_some();
            let result=consegna(app,&p,&relative,base.as_deref(),structured,mode,&cancel,&mut |d|r.prenota(out,i,d));
            match result {
                Ok(d)=>if let Err(e)=r.completato(out,i,d){r.elementi[i].errore=Some(e);},
                Err(e)=>r.elementi[i].errore=Some(e),
            }
            // In particolare non si cancella il privato se il journal non è sincronizzato.
            r.salva(out)?;
        }
        let outputs=r.righe();
        let (path,nome,_)=outputs.first().cloned().ok_or("Il lavoro non ha prodotto file")?;
        let failed=outputs.iter().any(|u|u.2.is_some());
        let msg=if cancel.load(std::sync::atomic::Ordering::Acquire){cedro::i18n::tr("Pubblicazione annullata: le copie complete restano in Cedro").into_owned()}
            else if failed{format!("{} · {}",r.messaggio,cedro::i18n::tr("Salvataggio incompleto: copie disponibili in Cedro"))}
            else{col_nome_vero(&r.messaggio,&principale,&nome)};
        Ok(Completamento{batch:None,id,stato:Stato::Fatto{path,nome,msg},uscite:Some(outputs),sorgente:r.lavoro.sorgente})
    })();
    result.unwrap_or_else(|e|Completamento::semplice(id,Stato::Errore(e)))
}

fn scrivi(app:&AndroidApp,c:&Consegne,id:u32,nome:String,dati:Vec<u8>,poi:u8) {
    c.azioni.lock().unwrap_or_else(|e|e.into_inner()).insert(id,"esporta-revisione".into());
    log::info!("lavoro avvio id={id} azione=esporta-revisione");
    let result=(||->Result<Completamento,String>{
        let base=uscite(app).ok_or("Cartella assente")?;
        let dir=crate::storage::nuova_cartella(&base).map_err(|e|e.to_string())?;
        let _scarto=Scarto(dir.clone());
        let file=dir.join(crate::storage::nome_sicuro(&nome));
        let cancel=token(c,id);
        crate::storage::copia(dati.as_slice(),&file,dati.len() as u64,&cancel,|_|{}).map_err(|e|e.to_string())?;
        let result=pubblica_lavoro(app,c,id,&dir,file,cedro::size_words(dati.len() as u64));
        if poi>0 {if let Stato::Fatto{path,..}=&result.stato{if let Err(e)=apri_o_condividi(app,path,poi==2){log::warn!("condivisione: {e}");}}}
        Ok(result)
    })();
    c.esiti.lock().unwrap_or_else(|e|e.into_inner()).push(result.unwrap_or_else(|e|Completamento::semplice(id,Stato::Errore(e))));
}

struct Scarto(PathBuf);
impl Drop for Scarto {
    fn drop(&mut self){
        fn pronto(dir:&Path)->bool {
            dir.join(".batch-child").is_file() || dir.join(".batch").is_file() || dir.join(".pronti").is_file() || crate::publication::sorgente_revisione(dir).is_some() || std::fs::read_dir(dir).into_iter().flatten().filter_map(Result::ok).any(|e|e.file_type().is_ok_and(|t|t.is_dir())&&pronto(&e.path()))
        }
        let _=std::fs::remove_dir_all(self.0.join(".scaricato"));
        if !pronto(&self.0){let _=std::fs::remove_dir_all(&self.0);}
    }
}

#[allow(clippy::too_many_arguments)]
fn converti(voce:crate::worker::Voce, web_scelte:Option<crate::web::Scelte>, app: AndroidApp, c: Consegne, id: u32, file: PathBuf, azione: &'static str, qualita: u8, prog: Arc<std::sync::atomic::AtomicU32>, opts: Option<Box<cedro::Options>>, unisci: Vec<(PathBuf, Option<cedro::PageSel>)>) {
    let invia = |s: Stato| c.esiti.lock().unwrap_or_else(|e| e.into_inner()).push(Completamento::semplice(id, s));
    if !assicura_scrittura(&app) {
        invia(Stato::Errore("Serve il permesso di scrivere in Documenti.".into()));
        return;
    }
    // Da Android 11 il motore scrive in una cartella di Cedro e il risultato passa a Documenti/Cedro con MediaStore:
    // alcuni telefoni (Xiaomi 13, Android 16) non lasciano creare file in Documenti col percorso.
    let base = app.internal_data_path().unwrap_or_else(|| PathBuf::from("/data/local/tmp")).join("uscite");
    let out = match crate::storage::nuova_cartella(&base) {
        Ok(p) => p,
        Err(e) => { invia(Stato::Errore(e.to_string())); return; }
    };
    let _scarto=Scarto(out.clone());
    let cancel = token(&c,id);
    if cancel.load(std::sync::atomic::Ordering::Acquire) { invia(Stato::Errore("Annullato".into())); return; }
    if azione == "web-altra-app" {
        c.azioni.lock().unwrap_or_else(|e|e.into_inner()).insert(id,azione.into());
        log::info!("lavoro avvio id={id} azione={azione}");
        // YouTube: il link va a un'app che lo scarica (il selettore di Android propone quelle installate).
        let url = file.to_string_lossy().into_owned();
        match apri_link(&app, &url) {
            Ok(()) => c.esiti.lock().unwrap_or_else(|e| e.into_inner()).push(Completamento::semplice(id, Stato::Fatto { path: PathBuf::new(), nome: String::new(), msg: "Link passato a un'altra app".into() })),
            Err(e) => c.esiti.lock().unwrap_or_else(|e| e.into_inner()).push(Completamento::semplice(id, Stato::Errore(e))),
        }
        return;
    }
    let o = opts.map(|b| *b).unwrap_or_default();
    if azione.starts_with("web-") {
        return web(&app, &c, id, &file, &out, azione, &prog, web_scelte.unwrap_or_default(), o);
    }
    if azione == "trascrivi" {
        return trascrivi(&app, &c, id, &file, &out, &prog, voce, o);
    }
    let esito = processo(&app,&c,id,&file,azione,qualita,Default::default(),&out,o,unisci,&prog,None);
    if cancel.load(std::sync::atomic::Ordering::Acquire) {
        let _ = std::fs::remove_dir_all(&out);
        invia(Stato::Errore("Annullato".into()));
    } else {
        match esito {
            Ok((path,msg)) => {let done=pubblica_lavoro(&app,&c,id,&out,path,msg);c.esiti.lock().unwrap_or_else(|e|e.into_inner()).push(done);},
            Err(e) => {
                let _ = std::fs::remove_dir_all(&out);
                invia(Stato::Errore(e));
            }
        }
    }
}

/// Staging privato su tutte le API; la consegna sceglie MediaStore o rename legacy.
fn uscite(app: &AndroidApp) -> Option<PathBuf> {
    app.internal_data_path().map(|p|p.join("uscite"))
}

/// La copia privata si elimina soltanto tramite Registro::completato, dopo il journal durevole.
fn consegna(app:&AndroidApp,path:&Path,sotto:&Path,base:Option<&str>,structured:bool,mode:u8,cancel:&std::sync::atomic::AtomicBool,checkpoint:&mut dyn FnMut(Destinazione)->Result<(),String>)->Result<Destinazione,String>{
    static PUBBLICAZIONE:Mutex<()>=Mutex::new(());
    let _lock=PUBBLICAZIONE.lock().unwrap_or_else(|e|e.into_inner());
    if cancel.load(std::sync::atomic::Ordering::Acquire){return Err("Annullato".into());}
    let api=con_attivita(app,|env,_|env.get_static_field("android/os/Build$VERSION","SDK_INT","I").and_then(|v|v.i()).map_err(|e|jerr(env,e)))?;
    if !crate::publication::mediastore_moderno(api){
        let dir=PathBuf::from(DOCUMENTI).join("Cedro").join(sotto);
        let d=crate::publication::legacy(path,&dir,if structured{1}else{mode},cancel,checkpoint)?;
        let _=con_attivita(app,|env,att|scansiona(env,att,&d.path,false));
        return Ok(d);
    }
    con_attivita(app,|env,att|pubblica(env,att,path,sotto,base,structured,mode,cancel,checkpoint))
}

fn pubblica(env:&mut JNIEnv,att:&JObject,privato:&Path,sotto:&Path,base:Option<&str>,structured:bool,mode:u8,cancel:&std::sync::atomic::AtomicBool,checkpoint:&mut dyn FnMut(Destinazione)->Result<(),String>)->Result<Destinazione,String>{
    let nome=nome_di(privato);let mime=mime_da_percorso(privato);
    let resolver=env.call_method(att,"getContentResolver","()Landroid/content/ContentResolver;",&[]).and_then(|v|v.l()).map_err(|e|jerr(env,e))?;
    let mut errors=Vec::new();
    for (collection,root) in [("android/provider/MediaStore$Files","Documents/Cedro"),("android/provider/MediaStore$Downloads","Download/Cedro")]{
        // Un insieme non viene diviso fra Documents e Downloads dopo il primo file completato.
        if base.is_some_and(|b|b!=root){continue;}
        if cancel.load(std::sync::atomic::Ordering::Acquire){return Err("Annullato".into());}
        let rel=Path::new(root).join(sotto).to_string_lossy().into_owned();
        let mut reserved=false;
        let result=pubblica_in(env,&resolver,collection,&rel,privato,&nome,mime,structured,mode,cancel,&mut |d|{reserved=true;checkpoint(d)});
        match result{
            Ok(d)=>return Ok(d),
            // Dopo la prenotazione non sovrascrivere l’URI con un fallback: anche un errore
            // JNI può arrivare dopo il commit del provider. Il ripristino verifica quell’URI.
            Err(e) if reserved=>return Err(e),
            Err(e)=>errors.push(format!("{rel}: {e}")),
        }
    }
    Err(format!("il telefono non lascia salvare il file\n---\n{}",errors.join("\n")))
}

fn pubblica_in(env: &mut JNIEnv, resolver: &JObject, raccolta: &str, rel: &str, privato: &Path, nome: &str, mime: &str, structured:bool, mode:u8, cancel:&std::sync::atomic::AtomicBool, checkpoint:&mut dyn FnMut(Destinazione)->Result<(),String>) -> Result<Destinazione, String> {
    use std::os::fd::FromRawFd;
    let valori = |env: &mut JNIEnv, coppie: &[(&str, Result<&str, i32>)]| -> Result<JObject<'static>, String> {
        let cv = env.new_object("android/content/ContentValues", "()V", &[]).map_err(|e| jerr(env, e))?;
        for (k, v) in coppie {
            let k = env.new_string(k).map_err(|e| e.to_string())?;
            match v {
                Ok(s) => {
                    let s = env.new_string(s).map_err(|e| e.to_string())?;
                    env.call_method(&cv, "put", "(Ljava/lang/String;Ljava/lang/String;)V", &[JValue::Object(&k), JValue::Object(&s)]).map_err(|e| jerr(env, e))?;
                }
                Err(n) => {
                    let n = env.call_static_method("java/lang/Integer", "valueOf", "(I)Ljava/lang/Integer;", &[JValue::Int(*n)]).and_then(|v| v.l()).map_err(|e| jerr(env, e))?;
                    env.call_method(&cv, "put", "(Ljava/lang/String;Ljava/lang/Integer;)V", &[JValue::Object(&k), JValue::Object(&n)]).map_err(|e| jerr(env, e))?;
                }
            }
        }
        // Gli oggetti locali vivono fino al ritorno a Java: qui il thread è di Rust e resta agganciato.
        Ok(unsafe { JObject::from_raw(cv.into_raw()) })
    };
    let esterno = env.new_string("external").map_err(|e| e.to_string())?;
    let base = env.call_static_method(raccolta, "getContentUri", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&esterno)]).and_then(|v| v.l()).map_err(|e| jerr(env, e))?;
    // Se il nome c'è già vale la scelta delle Impostazioni, come sul computer: MediaStore da solo farebbe «x (2)».
    let mut scelto = nome.to_string();
    let mut sostituito = None;
    if let Some(uri) = cerca(env, resolver, &base, rel, nome) {
        match if structured{1}else{mode} {
            2 => {
                let testo = env.call_method(&uri, "toString", "()Ljava/lang/String;", &[]).and_then(|v| v.l()).map_err(|e| jerr(env, e))?;
                let testo = stringa(env, testo).ok_or("indirizzo vuoto")?;
                let d=Destinazione{path:testo.into(),nome:format!("Già presente: {rel}/{nome}"),temporaneo:None};
                checkpoint(d.clone())?;
                return Ok(d);
            }
            1 => {
                // L'originale resta finché la nuova copia non è stata sincronizzata e pubblicata.
                sostituito = Some(env.new_global_ref(&uri).map_err(|e|e.to_string())?);
            }
            _ => scelto = libero_in(env, resolver, &base, rel, nome),
        }
    }
    let nome = scelto.as_str();
    let cv = valori(env, &[("_display_name", Ok(nome)), ("mime_type", Ok(mime)), ("relative_path", Ok(rel)), ("is_pending", Err(1))])?;
    let uri = env
        .call_method(resolver, "insert", "(Landroid/net/Uri;Landroid/content/ContentValues;)Landroid/net/Uri;", &[JValue::Object(&base), JValue::Object(&cv)])
        .and_then(|v| v.l())
        .map_err(|e| jerr(env, e))?;
    if uri.is_null() {
        return Err("MediaStore non ha creato il file".into());
    }
    let nulla = JObject::null();
    let uri_text=env.call_method(&uri,"toString","()Ljava/lang/String;",&[]).and_then(|v|v.l()).map_err(|e|jerr(env,e))?;
    let uri_text=stringa(env,uri_text).ok_or("indirizzo vuoto")?;
    let d=Destinazione{path:uri_text.clone().into(),nome:format!("{rel}/{nome}"),temporaneo:None};
    if let Err(e)=checkpoint(d){
        let _=env.call_method(resolver,"delete","(Landroid/net/Uri;Ljava/lang/String;[Ljava/lang/String;)I",&[JValue::Object(&uri),JValue::Object(&nulla),JValue::Object(&nulla)]);
        let _=env.exception_clear();return Err(e);
    }
    let scrivi = |env: &mut JNIEnv| -> Result<(), String> {
        let w = env.new_string("w").map_err(|e| e.to_string())?;
        let pfd = env
            .call_method(resolver, "openFileDescriptor", "(Landroid/net/Uri;Ljava/lang/String;)Landroid/os/ParcelFileDescriptor;", &[JValue::Object(&uri), JValue::Object(&w)])
            .and_then(|v| v.l())
            .map_err(|e| jerr(env, e))?;
        let fd = env.call_method(&pfd, "detachFd", "()I", &[]).and_then(|v| v.i()).map_err(|e| jerr(env, e))?;
        let mut dest = unsafe { std::fs::File::from_raw_fd(fd) };
        let mut src = std::fs::File::open(privato).map_err(|e| e.to_string())?;
        use std::io::{Read,Write};
        let mut buffer=[0u8;64*1024];
        loop {
            if cancel.load(std::sync::atomic::Ordering::Acquire){return Err("Annullato".into());}
            let n=src.read(&mut buffer).map_err(|e|e.to_string())?;
            if n==0{break;}
            dest.write_all(&buffer[..n]).map_err(|e|e.to_string())?;
        }
        dest.sync_all().map_err(|e| e.to_string())?;
        let cv = valori(env, &[("is_pending", Err(0))])?;
        let aggiornati=env.call_method(resolver, "update", "(Landroid/net/Uri;Landroid/content/ContentValues;Ljava/lang/String;[Ljava/lang/String;)I", &[JValue::Object(&uri), JValue::Object(&cv), JValue::Object(&nulla), JValue::Object(&nulla)])
            .and_then(|v|v.i()).map_err(|e| jerr(env, e))?;
        if aggiornati!=1 { return Err("MediaStore non ha completato il salvataggio".into()); }
        Ok(())
    };
    if let Err(e) = scrivi(env) {
        let _ = env.call_method(resolver, "delete", "(Landroid/net/Uri;Ljava/lang/String;[Ljava/lang/String;)I", &[JValue::Object(&uri), JValue::Object(&nulla), JValue::Object(&nulla)]);
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
        }
        return Err(e);
    }
    if let Some(old)=sostituito {
        let deleted=env.call_method(resolver,"delete","(Landroid/net/Uri;Ljava/lang/String;[Ljava/lang/String;)I",&[JValue::Object(old.as_obj()),JValue::Object(&nulla),JValue::Object(&nulla)]).and_then(|v|v.i()).unwrap_or(0);
        let _=env.exception_clear();
        if deleted>0 {
            if let Ok(cv)=valori(env,&[("_display_name",Ok(nome))]) {
            let _=env.call_method(resolver,"update","(Landroid/net/Uri;Landroid/content/ContentValues;Ljava/lang/String;[Ljava/lang/String;)I",&[JValue::Object(&uri),JValue::Object(&cv),JValue::Object(&nulla),JValue::Object(&nulla)]);
            let _=env.exception_clear();
            }
        }
    }
    // Nome effettivo letto al ripristino o alla consegna; URI già registrato prima della copia.
    let visto=nome_pubblico(env,resolver,&uri).unwrap_or_else(||nome.to_string());
    Ok(Destinazione{path:uri_text.into(),nome:format!("{rel}/{visto}"),temporaneo:None})
}

/// L'errore con quello che serve per capirlo: azione, file di partenza, cartella e se ci si può scrivere.
fn con_dettagli(e: &str, azione: &str, file: &Path, out: &Path) -> String {
    let (breve, prima) = e.split_once("\n---\n").unwrap_or((e, ""));
    let dim = std::fs::metadata(file).map(|m| format!("{} byte", m.len())).unwrap_or_else(|e| format!("non leggibile: {e}"));
    let prova = out.join(".cedro-prova");
    let scrivibile = match std::fs::write(&prova, b"cedro") {
        Ok(()) => {
            let _ = std::fs::remove_file(&prova);
            "sì".to_string()
        }
        Err(e) => format!("no ({e})"),
    };
    let mut d = format!("{breve}\n---\nAzione: {azione}\nFile: {} ({dim})\nCartella: {} (scrivibile: {scrivibile})", file.display(), out.display());
    if !prima.is_empty() {
        d += &format!("\n{prima}");
    }
    d
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
/// «Testo» scarica l'audio e lo trascrive. YouTube passa da yt-dlp, se l'APK lo contiene.
#[allow(clippy::too_many_arguments)]
fn web(app: &AndroidApp, c: &Consegne, id: u32, link: &Path, out: &Path, azione: &str, prog: &Arc<std::sync::atomic::AtomicU32>, scelte: crate::web::Scelte, opts: cedro::Options) {
    #[cfg(feature = "youtube")]
    if cedro::youtube::is_youtube_url(&link.to_string_lossy()) {
        return youtube(app,c,id,&link.to_string_lossy(),out,azione,prog,scelte,opts);
    }
    let result=processo(app,c,id,link,azione,scelte.media_quality,scelte.voce,out,opts,Vec::new(),prog,Some(scelte));
    let stato=match result {
        Ok((path,msg))=>pubblica_lavoro(app,c,id,out,path,msg),
        Err(e)=>{let mut done=Completamento::semplice(id,Stato::Errore(e));done.sorgente=crate::publication::sorgente_revisione(out);drop(Scarto(out.into()));done},
    };
    c.esiti.lock().unwrap_or_else(|e|e.into_inner()).push(stato);
}

/// YouTube con yt-dlp (Python, QuickJS e ffmpeg dall'APK; le librerie si scaricano al primo uso).
#[cfg(feature = "youtube")]
#[allow(clippy::too_many_arguments)]
fn youtube(app: &AndroidApp, c: &Consegne, id: u32, url: &str, out: &Path, azione: &str, prog: &Arc<std::sync::atomic::AtomicU32>, scelte: crate::web::Scelte, opts: cedro::Options) {
    use cedro::youtube::{run, Event};
    let invia = |s: Stato| c.esiti.lock().unwrap_or_else(|e| e.into_inner()).push(Completamento::semplice(id, s));
    let testo = azione == "web-testo";
    // Una cartella per lavoro: due lavori insieme non si cancellano i file a vicenda.
    let lavoro = out.join(format!(".cedro-yt-{id}"));
    let _ = std::fs::remove_dir_all(&lavoro);
    if let Err(e)=crate::web::Salvataggio::nuovo(url,&scelte,&opts).salva(out){return invia(Stato::Errore(e));}
    let r = scelte.richiesta(url,azione,lavoro.clone());
    // Per «Testo» lo scaricamento è la prima parte del lavoro, la trascrizione la seconda.
    let parte = if testo { 0.4 } else { 1.0 };
    let niente = token(c,id);
    let Some(_permit)=cedro::resources::acquire(Some(&niente)) else {return invia(Stato::Errore("Annullato".into()));};
    c.azioni.lock().unwrap_or_else(|e|e.into_inner()).insert(id,azione.into());
    log::info!("lavoro avvio id={id} azione={azione}");
    let (state,wake)=(c.componenti.clone(),app.create_waker());
    cedro::componenti::osserva(Some(Arc::new(move |s|{state.lock().unwrap_or_else(|e|e.into_inner()).ricevi(id,s.map(crate::web::Componente::da_core));wake.wake();})));
    struct Osserva;
    impl Drop for Osserva{fn drop(&mut self){cedro::componenti::verifica_download(None);cedro::componenti::osserva(None);cedro::componenti::azzera();}}
    let _osserva=Osserva;
    let (pa,pc)=(app.clone(),c.clone());
    cedro::componenti::verifica_download(Some(Arc::new(move|nome,bytes|web::permesso(&pa,&pc,id,nome,bytes))));
    let mut nota=String::new();
    let esito = run(
        &r,
        &mut |e| match e {
            Event::Progress { percent, .. } => prog.store((percent * 10.0 * parte) as u32, std::sync::atomic::Ordering::Relaxed),
            Event::Log(l) => {if l.contains("scarico il video senza"){nota=l.clone();}c.passo_riassunto.lock().unwrap_or_else(|e|e.into_inner()).push((id,l));},
            _ => {}
        },
        &niente,
    );
    match esito {
        Ok(f) if !f.is_empty() => {},
        Ok(_) => { let _=std::fs::remove_dir_all(&lavoro); return invia(Stato::Errore("yt-dlp non ha scaricato niente".into())); }
        Err(e) => { let _=std::fs::remove_dir_all(&lavoro); return invia(Stato::Errore(e)); }
    };
    let files=match crate::web::uscite_download(&lavoro){
        Ok(all)=>if testo{
            // Sottotitoli/copertine non sono sorgenti da trascrivere.
            all.into_iter().filter(|p|cedro::trascrizione::transcribable(p)).collect::<Vec<_>>()
        }else{all},
        Err(e)=>return invia(Stato::Errore(e)),
    };
    if files.is_empty(){return invia(Stato::Errore("yt-dlp non ha scaricato niente".into()));}
    if testo {
        let identity=c.identita.lock().unwrap_or_else(|e|e.into_inner()).get(&id).cloned();
        let result=identity.ok_or_else(||"Identità del lavoro assente".to_string()).and_then(|identity|crate::web::Batch::prepara(out,&files,&identity,opts,scelte.voce));
        drop(_osserva);drop(_permit); // Nessun permit attraversa l'avvio dei figli.
        match result {
            Ok(mut batch)=>{
                let _=std::fs::remove_dir_all(&lavoro);
                let stato=if niente.load(std::sync::atomic::Ordering::Acquire){
                    for child in &mut batch.figli{child.fine=true;}
                    if let Err(e)=batch.salva(){return invia(Stato::Errore(e));}
                    Stato::Errore("Annullato".into())
                }else{Stato::Fatto{path:PathBuf::new(),nome:String::new(),msg:"Audio pronti: trascrizioni aggiunte ai lavori".into()}};
                c.esiti.lock().unwrap_or_else(|e|e.into_inner()).push(Completamento{batch:Some(batch),id,stato,uscite:None,sorgente:None});
            },
            Err(e)=>invia(Stato::Errore(e)),
        }
    } else {
        // Sposta tutte le uscite, incluse le sottocartelle, fuori dal download provvisorio.
        let mut renamed=Vec::new();
        for file in &files {
            let rel=match file.strip_prefix(&lavoro){Ok(p)=>p,Err(_)=>return invia(Stato::Errore("Risultato fuori dalla cartella del lavoro".into()))};
            let dest=out.join(rel.parent().unwrap_or(Path::new("")));
            match crate::web::nomina(file,&dest,&opts){Ok(p)=>renamed.push(p),Err(e)=>return invia(Stato::Errore(e))}
        }
        let file=renamed[0].clone();
        let titolo=if nota.is_empty(){nome_di(&file)}else{nota};
        let _=std::fs::remove_dir_all(&lavoro);
        let done=pubblica_lavoro(app,c,id,out,file,titolo);
        c.esiti.lock().unwrap_or_else(|e|e.into_inner()).push(done);
        // Non eliminare la cartella: contiene le copie recuperabili se MediaStore fallisce.
    }
}

/// Voce in testo con whisper.cpp (nell'APK): il modello scelto (Veloce, Buona, Massima: gli stessi del computer) si scarica al primo uso. Salva un .txt in Documenti.
fn trascrivi(app: &AndroidApp, c: &Consegne, id: u32, file: &std::path::Path, out: &std::path::Path, prog: &std::sync::atomic::AtomicU32, voce: crate::worker::Voce, opts: cedro::Options) {
    let result = processo(app,c,id,file,"trascrivi",82,voce,out,opts,Vec::new(),prog,None);
    let stato = match result {
        Ok((path,msg)) => {
            if let Some(n)=crate::ui::da_controllare(file) { c.controlli.lock().unwrap_or_else(|e|e.into_inner()).push((id,n)); }
            pubblica_lavoro(app,c,id,out,path,msg)
        }
        Err(e) => {let mut done=Completamento::semplice(id,Stato::Errore(e));done.sorgente=crate::publication::sorgente_revisione(out);drop(Scarto(out.into()));done},
    };
    c.esiti.lock().unwrap_or_else(|e|e.into_inner()).push(stato);
}

/// Il Riassunto AI sul telefono: lo stesso `cedro::riassunto` del computer, col motore `cedro-riassumi` dentro l'APK.
fn riassumi(app:&AndroidApp,c: &Consegne, id: u32, source: PathBuf, lines: Vec<String>, names: Vec<String>, prog: &std::sync::atomic::AtomicU32) {
    let result=(||->Result<cedro::riassunto::Stored,String>{
        let libs=cartella_librerie(app).ok_or("Il motore non è in questo APK")?;
        let base=app.internal_data_path().ok_or("Cartella assente")?.join("riassunti-lavori");
        let out=crate::storage::nuova_cartella(&base).map_err(|e|e.to_string())?;
        let cancel=token(c,id);
        let _permit=cedro::resources::acquire(Some(&cancel)).ok_or("Annullato")?;
        c.azioni.lock().unwrap_or_else(|e|e.into_inner()).insert(id,"riassumi".into());
        log::info!("lavoro avvio id={id} azione=riassumi");
        let request=crate::worker::Richiesta {file:source.clone(),azione:"riassumi".into(),qualita:0,voce:Default::default(),out:out.clone(),opts:Default::default(),unisci:Vec::new(),sommario:Some((lines,names)),anteprima:None,web:None};
        let result=crate::worker::avvia(&libs,request,&cancel,Vec::new(),|nome,bytes|web::permesso(app,c,id,nome,bytes),|e|match e {
            crate::worker::Evento::Componente(s)=>{c.componenti.lock().unwrap_or_else(|e|e.into_inner()).ricevi(id,s);app.create_waker().wake();},
            crate::worker::Evento::Progresso(n)=>prog.store(n,std::sync::atomic::Ordering::Relaxed),
            crate::worker::Evento::Passo(_,t)=>c.passo_riassunto.lock().unwrap_or_else(|e|e.into_inner()).push((id,t)),
            _=>{},
        }).and_then(|(path,_)|{
            let value=std::fs::read_to_string(path).map_err(|e|e.to_string())?;
            let value=serde_json::from_str(&value).map_err(|e|e.to_string())?;
            cedro::riassunto::Stored::from_json(&value).ok_or("Riassunto non leggibile".into())
        });
        let _=std::fs::remove_dir_all(out);
        result
    })();
    let stato=match result {
        Ok(s)=>{c.riassunti.lock().unwrap_or_else(|e|e.into_inner()).push((id,source,s));Stato::Fatto{path:PathBuf::new(),nome:String::new(),msg:"Riassunto pronto in Rivedi".into()}},
        Err(e)=>Stato::Errore(e),
    };
    c.esiti.lock().unwrap_or_else(|e|e.into_inner()).push(Completamento::semplice(id,stato));
}


fn presenta(finestra: &android_activity::ndk::native_window::NativeWindow, p: &Painter) {
    use android_activity::ndk::hardware_buffer_format::HardwareBufferFormat;
    // Il formato si imposta una volta sola per finestra (ogni volta costava una riconfigurazione).
    static FORMATO: std::sync::Mutex<usize> = std::sync::Mutex::new(0);
    {
        let mut f = FORMATO.lock().unwrap_or_else(|e| e.into_inner());
        let id = finestra.ptr().as_ptr() as usize;
        if *f != id {
            let _ = finestra.set_buffers_geometry(0, 0, Some(HardwareBufferFormat::R8G8B8A8_UNORM));
            *f = id;
        }
    }
    let Ok(mut guardia) = finestra.lock(None) else { return };
    let (gw, gh, stride) = (guardia.width(), guardia.height(), guardia.stride());
    let Some(dest) = guardia.bytes() else { return };
    let src = p.pixmap.data();
    let pw = p.pixmap.width() as usize;
    let ph = p.pixmap.height() as usize;
    let n = gw.min(pw) * 4;
    // Una riga alla volta, copiata in blocco (prima era un byte alla volta: fino a 30 ms per fotogramma).
    for y in 0..gh.min(ph) {
        let da = y * stride * 4;
        if da + n > dest.len() {
            break;
        }
        let riga = &src[y * pw * 4..y * pw * 4 + n];
        // SAFETY: `MaybeUninit<u8>` ha la stessa rappresentazione di `u8` e le zone non si sovrappongono.
        unsafe { std::ptr::copy_nonoverlapping(riga.as_ptr(), dest[da..da + n].as_mut_ptr() as *mut u8, n) };
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
        fn scan(dir: &Path,v: &mut Vec<(std::time::SystemTime,PathBuf)>) {
            if let Ok(entries)=std::fs::read_dir(dir) {
                for e in entries.flatten() {
                    let p=e.path();
                    if nome_di(&p).starts_with('.') { continue; }
                    if e.file_type().is_ok_and(|t|t.is_dir()) { scan(&p,v); }
                    else if let Ok(m)=e.metadata() { v.push((m.modified().unwrap_or(std::time::UNIX_EPOCH),p)); }
                }
            }
        }
        let mut v=Vec::new(); scan(&cartella,&mut v);
        v.sort_by(|a, b| b.0.cmp(&a.0));
        let recenti: Vec<Recente> = v
            .into_iter()
            .take(12)
            .map(|(_, path)| Recente { nome: nome_di(&path), kind: tipo_di(&path), thumb: if crate::labs::laboratorio(&path){None}else{cedro::ui::media::thumb_of(&path, 160)}, path })
            .collect();
        *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(recenti);
    });
}

/// Quello che arriva da «Condividi»: un file, o più PDF da unire.
fn ricevuti(ui: &mut App, c: &Consegne, cartella: &Path, v: Vec<Condiviso>) {
    // Con «Unisci» aperto, i PDF condivisi si aggiungono all'elenco.
    if ui.pdf.pagina == Some(crate::ui::Pagina::Unisci) && v.iter().all(|c| tipo_di(&c.path) == cedro::Kind::Pdf) {
        leggi_uniti(c, v.iter().map(|c| c.path.clone()).collect(), ui.pdf.usata.clone());
        ui.aggiungi_pdf(v.into_iter().map(|c| (c.path, c.nome)).collect());
        return;
    }
    // Un file firmato (.p7m, .p7s, .m7m, .tsd, .tsr): il visualizzatore delle firme.
    if v.len() == 1 && App::e_firmato(&v[0].nome) {
        log::info!("firmato {} ({})", v[0].nome, v[0].path.display());
        ui.apri_firmato(v[0].path.clone());
        return;
    }
    let mut v = v.into_iter();
    let Some(primo) = v.next() else { return };
    log::info!("ricevuto {} ({})", primo.nome, primo.path.display());
    carica_file(ui, c, cartella, primo.path, primo.nome, true);
    let resto: Vec<Condiviso> = v.collect();
    if resto.is_empty() {
        return;
    }
    if ui.kind == cedro::Kind::Pdf && resto.iter().all(|c| tipo_di(&c.path) == cedro::Kind::Pdf) {
        let altri: Vec<(PathBuf, String)> = resto.into_iter().map(|c| (c.path, c.nome)).collect();
        leggi_uniti(c, altri.iter().map(|a| a.0.clone()).collect(), None);
        ui.aggiungi_pdf(altri);
    } else {
        // File di tipi diversi: il foglio propone per prima cosa uno ZIP con tutti.
        ui.altri = resto.into_iter().map(|c| c.path).collect();
        ui.foglio = true;
    }
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
    if matches!(kind,cedro::Kind::Audio|cedro::Kind::Video) { prepara_onda(c,path.clone()); }
    ui.imposta_file(file_di(&path, nome.clone()), kind, da_condivisione);
    if crate::labs::laboratorio(&path) && kind==cedro::Kind::Image {
        ui.vai_casa();ui.vista_nav(6);ui.aggiungi_coda(vec![(path,nome,kind)]);
        ricarica_recenti(cartella.to_path_buf(),c);
        return;
    }
    if kind == cedro::Kind::Pdf {
        leggi_pdf(c, path.clone());
    }
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

/// Miniatura, misure e pagine dei file in coda, in un thread.
    /// Cache a risoluzione telefonica, letta fuori dal thread grafico anche per file lunghi.
fn prepara_onda(c: &Consegne, path: PathBuf) {
    let slot = c.onda_riv.clone();
    std::thread::spawn(move || {
        let secs = cedro::audio_duration(&path).unwrap_or(0.0);
        let bars = cedro::waveform(&path, 600);
        slot.lock().unwrap_or_else(|e|e.into_inner()).push((path,bars,secs));
    });
}

fn leggi_voci(c: &Consegne, files: Vec<PathBuf>) {
    for path in &files { if matches!(tipo_di(path), cedro::Kind::Audio | cedro::Kind::Video) { prepara_onda(c,path.clone()); } }
    let slot = c.voci.clone();
    std::thread::spawn(move || {
        for path in files {
            let kind = tipo_di(&path);
            let (thumb, misure) = if crate::labs::laboratorio(&path) {(None,None)} else {match kind {
                cedro::Kind::Image => cedro::ui::media::image_thumb(&path, 240).map(|(i, s)| (Some(i), Some(s))).unwrap_or((None, None)),
                _ => (cedro::ui::media::thumb_of(&path, 360), None),
            }};
            let pagine = (kind == cedro::Kind::Pdf).then(|| cedro::pdf_info(&path, None).ok().map(|(n, _)| n)).flatten();
            slot.lock().unwrap_or_else(|e| e.into_inner()).push((path, thumb, misure, pagine));
        }
    });
}

/// Pagine e segnalibri di un PDF, in un thread (se è protetto, l'errore lo dice).
fn leggi_pdf(c: &Consegne, path: PathBuf) {
    let slot = c.info_pdf.clone();
    std::thread::spawn(move || {
        let r = cedro::pdf_info(&path, None);
        *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some((path, r));
    });
}

/// Pagine e miniatura dei PDF da unire, in un thread.
fn leggi_uniti(c: &Consegne, files: Vec<PathBuf>, password: Option<String>) {
    let slot = c.unito.clone();
    std::thread::spawn(move || {
        for path in files {
            let pagine = cedro::pdf_info(&path, password.as_deref()).ok().map(|(n, _)| n);
            let thumb = cedro::ui::media::thumb_of(&path, 200);
            slot.lock().unwrap_or_else(|e| e.into_inner()).push((path, pagine, thumb));
        }
    });
}

/// Un PDF protetto: la password vale? Se sì, una copia senza password nella cartella dei file.
fn sblocca(c: &Consegne, path: PathBuf, password: String, cartella: &Path) {
    let slot = c.sblocco.clone();
    let dir = cartella.join("sbloccati");
    std::thread::spawn(move || {
        let r = std::fs::create_dir_all(&dir).map_err(|e| e.to_string()).and_then(|_| {
            let out = dir.join(path.file_name().unwrap_or_default());
            cedro::pdf_unlock(&path, &password, &out).map(|_| out)
        });
        *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
    });
}

/// Le miniature delle pagine chieste (da 0), in un thread.
fn miniature_pagine(c: &Consegne, path: PathBuf, pagine: Vec<u32>) {
    let slot = c.pagine.clone();
    std::thread::spawn(move || {
        let idx: Vec<usize> = pagine.iter().map(|n| *n as usize).collect();
        cedro::ui::media::pdf_page_thumbs(&path, &idx, 280.0, &mut |n, img| {
            if let Some(img) = img {
                slot.lock().unwrap_or_else(|e| e.into_inner()).push((path.clone(), n as u32, img));
            }
            true
        });
    });
}

/// Vero se Android ha le animazioni spente (durata delle animazioni a 0).
fn animazioni_spente(app: &AndroidApp) -> bool {
    con_attivita(app, |env, att| {
        let resolver = env.call_method(att, "getContentResolver", "()Landroid/content/ContentResolver;", &[]).and_then(|v| v.l()).map_err(|e| jerr(env, e))?;
        let nome = env.new_string("animator_duration_scale").map_err(|e| e.to_string())?;
        let v = env
            .call_static_method("android/provider/Settings$Global", "getFloat", "(Landroid/content/ContentResolver;Ljava/lang/String;F)F", &[JValue::Object(&resolver), JValue::Object(&nome), JValue::Float(1.0)])
            .and_then(|v| v.f())
            .map_err(|e| jerr(env, e))?;
        Ok(v == 0.0)
    })
    .unwrap_or(false)
}

/// Le pagine del documento dentro un file firmato, una alla volta: PDF, immagine, testo o Word.
fn pagine_firmato(c: &Consegne, path: PathBuf) {
    let slot = c.firma_pagine.clone();
    std::thread::spawn(move || {
        let manda = |img: Option<Image>| slot.lock().unwrap_or_else(|e| e.into_inner()).push((path.clone(), img));
        const LATO: f32 = 1200.0;
        const MASSIMO: usize = 40;
        match tipo_di(&path) {
            cedro::Kind::Pdf => {
                let n = cedro::pdf_info(&path, None).map(|(n, _)| n as usize).unwrap_or(0).min(MASSIMO);
                let pagine: Vec<usize> = (0..n).collect();
                cedro::ui::media::pdf_page_thumbs(&path, &pagine, LATO, &mut |_, img| {
                    if let Some(i) = img {
                        manda(Some(i));
                    }
                    true
                });
            }
            cedro::Kind::Image => {
                if let Some((i, _)) = cedro::ui::media::image_thumb(&path, LATO as u32) {
                    manda(Some(i));
                }
            }
            cedro::Kind::Text | cedro::Kind::Doc => {
                let n = cedro::ui::preview::page_count(&path).unwrap_or(1).min(MASSIMO);
                for k in 0..n {
                    match cedro::ui::preview::render_page(&path, k, LATO) {
                        Some(i) => manda(Some(i)),
                        None => break,
                    }
                }
            }
            _ => {}
        }
        manda(None);
    });
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
    // La cartella temporanea del motore (ZIP, audio a flussi): /data/local/tmp non è nostra.
    let tmp = dati.join("tmp");
    let _ = std::fs::create_dir_all(&tmp);
    std::env::set_var("TMPDIR", &tmp);
    if let Some(librerie) = cartella_librerie(&app) {
        cedro::riassunto::android::set_libs(librerie.clone());
        cedro::youtube::android::set_dirs(librerie, dati.clone());
    }
    if std::env::var_os("RTEN_NUM_THREADS").is_none() {
        if let Some(n) = core_grandi() {
            log::info!("core grandi: {n}");
            std::env::set_var("RTEN_NUM_THREADS", n.to_string());
        }
    }
    let file_tema = dati.join("tema.txt");
    let file_lavori = dati.join("lavori.txt");
    let mut tema = std::fs::read_to_string(&file_tema).map(|t| crate::ui::tema_da_chiave(&t)).unwrap_or(ThemeId::Cedro);
    // Con la 0.45 arriva il nuovo Cedro: chi aveva ancora il tema di partenza di prima lo vede una volta sola;
    // se poi sceglie di nuovo «Windows essenziale» resta.
    let marca_045 = dati.join("tema-045");
    if !marca_045.exists() {
        if tema == ThemeId::WindowsEssenziale {
            tema = ThemeId::Cedro;
            let _ = std::fs::write(&file_tema, tema.key());
        }
        let _ = std::fs::write(&marca_045, "1");
    }
    let mut ui = App::new(tema);
    {
        let sveglia = app.create_waker();
        ui.sveglia = Arc::new(move || sveglia.wake());
    }
    // «Rimuovi animazioni» nelle impostazioni di accessibilità: le animazioni restano ferme.
    ui.anim.calmo = animazioni_spente(&app);
    let file_caratteri = dati.join("caratteri.txt");
    // Le impostazioni del motore (qualità, DPI, OCR) stanno nel suo config.txt; il resto qui.
    ui.imp = cedro::read_settings();
    ui.labs.caso = cedro::caso::carica();
    ui.qualita = ui.imp.quality.clamp(40, 95);
    let file_rete=dati.join("rete-download.txt");
    ui.rete=std::fs::read_to_string(&file_rete).ok().and_then(|s|s.trim().parse::<u8>().ok()).filter(|n|*n<=2).unwrap_or(0);
    let file_impostazioni = dati.join("impostazioni.txt");
    if let Ok(t) = std::fs::read_to_string(&file_impostazioni) {
        let mut righe = t.lines();
        ui.esiste = match righe.next().unwrap_or("") {
            "sostituisci" => cedro::Exists::Replace,
            "salta" => cedro::Exists::Skip,
            _ => cedro::Exists::Number,
        };
        ui.modello_nome = righe.next().unwrap_or("").to_string();
    }
    ricorda_esiste(ui.esiste);
    ui.grandi = std::fs::read_to_string(&file_caratteri).is_ok_and(|t| t.trim() == "grandi");
    // La lingua: quella scelta nell'Aspetto, o quella del telefono (Android non ha LANG).
    let file_lingua = dati.join("lingua.txt");
    ui.lingua_telefono = lingua_del_telefono(&app);
    let scelta = std::fs::read_to_string(&file_lingua).unwrap_or_default();
    ui.lingua = crate::ui::LINGUE.iter().map(|l| l.0).find(|k| *k == scelta.trim()).unwrap_or("auto");
    // Da qui la lingua la decide solo Cedro per Android: leggere le impostazioni del motore non la cambia.
    cedro::i18n::fix();
    ui.applica_lingua();
    ui.info = leggi_info(&app);
    if let Ok(t) = std::fs::read_to_string(&file_lavori) {
        ui.carica_lavori(&t);
    }
    icone_barre(&app, crate::ui::tinte::scuro(tema));
    let mut insets = (0.0, 0.0, 0.0, 0.0);
    let mut leggi_insets = true;
    let mut tentativi = 0;

    let cartella = dati.join("condivisi");
    let consegne = Consegne::default();
    {
        let (a,dir,c)=(app.clone(),cartella.clone(),consegne.clone());
        let protetti:Vec<PathBuf>=ui.lavori.iter().filter_map(|l|l.sorgente.clone()).collect();
        let noti:std::collections::HashSet<PathBuf>=ui.uscite.values().flatten().map(|u|u.0.clone()).collect();
        let prima=std::time::SystemTime::now();
        std::thread::spawn(move || {
            saf::pulisci(&dir,&protetti);
            saf::ripristina_tutti(&a,&dir);
            if let Some(base)=uscite(&a){
                let mut batches=Vec::new();
                if let Ok(entries)=std::fs::read_dir(&base){for entry in entries.flatten(){
                    if let Ok(batch)=crate::web::Batch::leggi(&entry.path()){
                        let _=batch.materializza();batches.push(batch);
                    }
                }}
                pulisci_incompleti(&base,&noti,prima);
                let mut recovered=recupera_uscite(&a,&base,prima);
                for batch in &mut batches{
                    // materializza può aggiornare i timestamp: le ricevute dei figli si leggono
                    // esplicitamente dal manifest, anche se la scansione le ha saltate.
                    batch.recupera_ricevute(|dir|recupera_registro(&a,dir));
                    recovered.retain(|r|!batch.figli.iter().any(|f|f.identita.chiave==r.lavoro.chiave));
                }
                *c.recuperati.lock().unwrap_or_else(|e|e.into_inner())=Some(recovered);
                // Ricevute e figli dello stesso batch arrivano nello stesso completamento.
                c.esiti.lock().unwrap_or_else(|e|e.into_inner()).extend(batches.into_iter().map(|batch|Completamento{batch:Some(batch),id:u32::MAX,stato:Stato::Fatto{path:PathBuf::new(),nome:String::new(),msg:String::new()},uscite:None,sorgente:None}));
            }
            ricarica_recenti(dir,&c);a.create_waker().wake();
        });
    }
    importa_condivisione(&app,&cartella,&consegne,&mut ui);
    registra(&consegne,&mut ui);
    persisti(&consegne,&file_lavori,ui.serializza());


    *consegne.rete.0.lock().unwrap_or_else(|e|e.into_inner())=ui.rete;
    let mut cache = GlyphCache::new();
    let mut finestra: Option<android_activity::ndk::native_window::NativeWindow> = None;
    let mut sporco = true;
    let mut esci = false;
    let mut animando = false;
    let mut ultimo_disegno = std::time::Instant::now();
    let mut attesa_scelta: Option<Sorgente> = None;
    let mut tastiera = false;
    let mut accento: Option<char> = None;
    let mut chieste: std::collections::HashSet<(PathBuf, u32)> = std::collections::HashSet::new();
    let mut servizio_attivo = false;
    let mut ultimo_servizio = std::time::Instant::now();
    let mut ultimo_audio=std::time::Instant::now();
    let mut progressi=Vec::new();
    let mut ultima_notifica=None;

    while !esci {
        // Mentre qualcosa si muove si disegna al ritmo dello schermo (~60 al secondo): si aspetta solo
        // quanto manca ai 16 ms dall'ultimo fotogramma. Da fermo si aspettano gli eventi.
        let attesa = if animando { Duration::from_millis(16).saturating_sub(ultimo_disegno.elapsed()) } else { Duration::from_millis(150) };
        app.poll_events(Some(attesa), |ev| {
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
                    MainEvent::Pause => {ui.pausa_audio();sporco=true;},
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
                        InputEvent::KeyEvent(k) if ui.campo.is_some() && k.key_code() != Keycode::Back => {
                            // La tastiera scrive nel campo attivo.
                            if k.action() == KeyAction::Down {
                                let t = match k.key_code() {
                                    Keycode::Del => Some(Tasto::Canc),
                                    Keycode::Enter | Keycode::NumpadEnter => Some(Tasto::Invio),
                                    codice => match app.device_key_character_map(k.device_id()).and_then(|m| {
                                        let c = m.get(codice, k.meta_state())?;
                                        Ok(match (c, accento.take()) {
                                            (KeyMapChar::CombiningAccent(a), _) => {
                                                accento = Some(a);
                                                None
                                            }
                                            (KeyMapChar::Unicode(ch), Some(a)) => Some(m.get_dead_char(a, ch)?.unwrap_or(ch)),
                                            (KeyMapChar::Unicode(ch), None) => Some(ch),
                                            (KeyMapChar::None, _) => None,
                                        })
                                    }) {
                                        Ok(Some(ch)) => Some(Tasto::Car(ch)),
                                        _ => None,
                                    },
                                };
                                if let Some(t) = t {
                                    eff = ui.tasto(t);
                                }
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
                        Effetto::SalvaRete=>{
                            *consegne.rete.0.lock().unwrap_or_else(|e|e.into_inner())=ui.rete;
                            consegne.rete.1.notify_all();
                            let (path,value)=(file_rete.clone(),ui.rete.to_string());std::thread::spawn(move||{let _=crate::storage::atomico(&path,value.as_bytes());});
                            sporco=true;
                        }
                        Effetto::Niente => {}
                        Effetto::Annulla(id) => { token(&consegne,id).store(true,std::sync::atomic::Ordering::Release); saf::annulla(&app,id); sporco=true; }
                        Effetto::Ridisegna => sporco = true,
                        Effetto::Converti { voce, web, id, file, azione, qualita, prog, opts, unisci } => {
                            registra(&consegne,&mut ui);
                            persisti(&consegne,&file_lavori,ui.serializza());
                            sporco = true;
                            let (a, c) = (app.clone(), consegne.clone());
                            token(&c,id);
                            std::thread::spawn(move || protetto(&c,id,|| converti(voce,web,a, c.clone(), id, file, azione, qualita, prog, opts, unisci)));
                        }
                        Effetto::Tanti(compiti) => {
                            registra(&consegne,&mut ui);
                            persisti(&consegne,&file_lavori,ui.serializza());
                            sporco = true;
                            let (a, c) = (app.clone(), consegne.clone());
                            for compito in &compiti { token(&c,compito.id); }
                            // Uno dopo l'altro: il motore lavora meglio su un file alla volta.
                            std::thread::spawn(move || {
                                for Compito { voce, web, id, file, azione, qualita, prog, opts, unisci } in compiti {
                                    protetto(&c,id,|| converti(voce,web,a.clone(), c.clone(), id, file, azione, qualita, prog, opts, unisci));
                                }
                            });
                        }
                        Effetto::Incolla => {
                            ui.incolla(appunti(&app));
                            sporco = true;
                        }
                        Effetto::LeggiVoci(v) => {
                            leggi_voci(&consegne, v);
                            sporco = true;
                        }
                        Effetto::LeggiPdf(path, altri) => {
                            leggi_pdf(&consegne, path);
                            if !altri.is_empty() {
                                leggi_uniti(&consegne, altri, None);
                            }
                            sporco = true;
                        }
                        Effetto::OndaRevisione(path, _) => {
                            prepara_onda(&consegne, path);
                            sporco = true;
                        }
                        Effetto::ScriviPoi { id, nome, dati, poi } => {
                            let nome=ui.nome_esportato(id,nome);
                            registra(&consegne,&mut ui);
                            persisti(&consegne,&file_lavori,ui.serializza());
                            let (a,c)=(app.clone(),consegne.clone());
                            token(&c,id);
                            std::thread::spawn(move ||protetto(&c,id,||scrivi(&a,&c,id,nome,dati,poi)));
                            sporco=true;
                        }
                        Effetto::Scrivi { id, nome, dati } => {
                            let nome=ui.nome_esportato(id,nome);
                            registra(&consegne,&mut ui);
                            persisti(&consegne,&file_lavori,ui.serializza());
                            let (a,c)=(app.clone(),consegne.clone());
                            token(&c,id);
                            std::thread::spawn(move ||protetto(&c,id,||scrivi(&a,&c,id,nome,dati,0)));
                            sporco=true;
                        }
                        Effetto::Sblocca(pw) => {
                            if let Some(f) = &ui.file {
                                sblocca(&consegne, f.path.clone(), pw, &cartella);
                            }
                            sporco = true;
                        }
                        Effetto::Esci => chiudi(&app),
                        Effetto::Riassumi { id, source, lines, names, prog } => {
                            registra(&consegne,&mut ui);
                            persisti(&consegne,&file_lavori,ui.serializza());
                            let (a,c) = (app.clone(),consegne.clone());
                            token(&c,id);
                            std::thread::spawn(move || protetto(&c,id,|| riassumi(&a,&c, id, source, lines, names, &prog)));
                            sporco = true;
                        }
                        Effetto::Copia(testo) => {
                            copia_negli_appunti(&app, &testo);
                            sporco = true;
                        }
                        Effetto::SalvaLingua(l) => {
                            let _ = std::fs::write(&file_lingua, l);
                            sporco = true;
                        }
                        Effetto::SalvaCaratteri(g) => {
                            let _ = std::fs::write(&file_caratteri, if g { "grandi" } else { "normali" });
                            sporco = true;
                        }
                        Effetto::SalvaCaso => {
                            let c=consegne.clone();let caso=ui.labs.caso.clone();
                            let generation=c.caso_versione.fetch_add(1,std::sync::atomic::Ordering::AcqRel)+1;
                            std::thread::spawn(move||{
                                let _guard=c.caso_lock.lock().unwrap_or_else(|e|e.into_inner());
                                if c.caso_versione.load(std::sync::atomic::Ordering::Acquire)!=generation{return;}
                                let path=cedro::config_dir().join("lab/caso.txt");
                                let r=match caso {
                                    Some(v)=>crate::storage::atomico(&path,format!("{}\n{}\n",v.code,v.note).as_bytes()),
                                    None=>match std::fs::remove_file(&path){Err(e) if e.kind()!=std::io::ErrorKind::NotFound=>Err(e),_=>Ok(())},
                                }.map_err(|e|e.to_string());
                                *c.caso_esito.lock().unwrap_or_else(|e|e.into_inner())=Some(r);
                            });
                            sporco=true;
                        }
                        Effetto::SalvaImpostazioni => {
                            let _ = cedro::write_settings(&ui.imp);
                            let esiste = match ui.esiste {
                                cedro::Exists::Replace => "sostituisci",
                                cedro::Exists::Skip => "salta",
                                _ => "numera",
                            };
                            let _ = std::fs::write(&file_impostazioni, format!("{esiste}\n{}", ui.modello_nome));
                            ricorda_esiste(ui.esiste);
                            sporco = true;
                        }
                        Effetto::SalvaTema(t) => {
                            let _ = std::fs::write(&file_tema, t.key());
                            icone_barre(&app, crate::ui::tinte::scuro(t));
                            sporco = true;
                        }
                        Effetto::SalvaLavori => {
                            registra(&consegne,&mut ui);
                            persisti(&consegne,&file_lavori,ui.serializza());
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
                                Ok(()) => attesa_scelta = Some(s),
                                Err(e) => log::error!("selettore: {e}"),
                            }
                            sporco = true;
                        }
                        Effetto::UsaFile(p) => {
                            let (id,_)=ui.nuova_importazione();
                            consegne.ultimo_import.store(id,std::sync::atomic::Ordering::Release);
                            let (a,c)=(app.clone(),consegne.clone());
                            std::thread::spawn(move || {
                                let r=saf::ripristina(&a,&p).map(|_|vec![Condiviso {nome:nome_di(&p),path:p}]);
                                c.importati.lock().unwrap_or_else(|e|e.into_inner()).push((id,Some(Sorgente::Altri),r));
                                a.create_waker().wake();
                            });
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
            importa_condivisione(&app,&cartella,&consegne,&mut ui);
            sporco = true;
        }
        if ui.firma_arrivi() {
            sporco = true;
        }
        for (di, img) in std::mem::take(&mut *consegne.firma_pagine.lock().unwrap_or_else(|e| e.into_inner())) {
            ui.firma_pagina(&di, img);
            sporco = true;
        }
        if let Some(doc) = ui.fpag.chiedi.take() {
            pagine_firmato(&consegne, doc);
        }
        if let Some(sorgente) = attesa_scelta {
            if importa_scelti(&app,&cartella,sorgente,&consegne,&mut ui) { attesa_scelta=None; }
        }
        if let Some((id,r))=consegne.originale.lock().unwrap_or_else(|e|e.into_inner()).take() {
            if id==consegne.ultimo_import.load(std::sync::atomic::Ordering::Acquire) {
                match r {Ok(data)=>ui.originale_firmato(data),Err(e)=>ui.errore_importazione(e)}
                sporco=true;
            }
        }
        if let Some(groups)=consegne.recuperati.lock().unwrap_or_else(|e|e.into_inner()).take(){
            for r in groups{ui.recupera(r);}
            registra(&consegne,&mut ui);
            persisti(&consegne,&file_lavori,ui.serializza());
            sporco=true;
        }
        let imports=std::mem::take(&mut *consegne.importati.lock().unwrap_or_else(|e|e.into_inner()));
        for (id,sorgente,result) in imports {
            consegne.tokens.lock().unwrap_or_else(|e|e.into_inner()).remove(&id);
            ui.finisce(id,match &result {Ok(_)=>Stato::Fatto {path:PathBuf::new(),nome:String::new(),msg:"File importati".into()},Err(e)=>Stato::Errore(e.clone())});
            registra(&consegne,&mut ui);
            persisti(&consegne,&file_lavori,ui.serializza());
            if result.is_err(){ui.scheda=crate::ui::Scheda::Lavori;ui.sopra=Some(crate::ui::Scheda::Lavori);}
            if id!=consegne.ultimo_import.load(std::sync::atomic::Ordering::Acquire)&&!matches!(sorgente,Some(Sorgente::Coda|Sorgente::Cartella|Sorgente::PdfAltri)){sporco=true;continue;}
            let Some(sorgente)=sorgente else {
                match result {
                    Ok(v) if !v.is_empty()=>ricevuti(&mut ui,&consegne,&cartella,v),
                    Ok(_)=>ricarica_recenti(cartella.clone(),&consegne),
                    Err(_)=>{},
                }
                sporco=true;
                continue;
            };
            match Some(result) {
                Some(Ok(v)) if matches!(sorgente, Sorgente::Coda | Sorgente::Cartella) => {
                    leggi_voci(&consegne, v.iter().map(|c| c.path.clone()).collect());
                    ui.aggiungi_coda(v.into_iter().map(|c| { let k = tipo_di(&c.path); (c.path, c.nome, k) }).collect());
                    sporco = true;
                }
                Some(Ok(v)) if sorgente == Sorgente::Firmati => {
                    if let Some(c) = v.into_iter().next() {
                        log::info!("firmato scelto {} ({})", c.nome, c.path.display());
                        ui.apri_firmato(c.path);
                    }
                    sporco = true;
                }
                Some(Ok(v)) if sorgente == Sorgente::Originale => {
                    let _=v;
                    sporco = true;
                }
                Some(Ok(v)) if sorgente == Sorgente::PdfAltri => {
                    let pdf: Vec<(PathBuf, String)> = v.into_iter().filter(|c| tipo_di(&c.path) == cedro::Kind::Pdf).map(|c| (c.path, c.nome)).collect();
                    leggi_uniti(&consegne, pdf.iter().map(|p| p.0.clone()).collect(), ui.pdf.usata.clone());
                    ui.aggiungi_pdf(pdf);
                    sporco = true;
                }
                Some(Ok(v)) => {
                    if let Some(c) = v.into_iter().next() {
                        log::info!("scelto {} ({})", c.nome, c.path.display());
                        carica_file(&mut ui, &consegne, &cartella, c.path, c.nome, false);
                    }
                    sporco = true;
                }
                Some(Err(e)) => {
                    let _=e;
                    sporco=true;
                }
                None => {}
            }
        }
        // Sincronizzare prima degli esiti invalida anche una sorgente appena rimossa.
        let (annulla,richiesta)=ui.sincronizza_labs();
        if let Some(id)=annulla {
            if let Some(cancel)=consegne.tokens.lock().unwrap_or_else(|e|e.into_inner()).get(&id){cancel.store(true,std::sync::atomic::Ordering::Release);}
            sporco=true;
        }
        // Anteprime isolate dal processo UI: i cambi di selezione cancellano il worker precedente.
        if let Some((id,path,scelta))=richiesta {
            let (a,c)=(app.clone(),consegne.clone());
            let cancel=token(&c,id);
            let wake=app.create_waker();
            std::thread::spawn(move||{
                let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
                    let _permit=cedro::resources::acquire(Some(&cancel)).ok_or("Annullato")?;
                    let base=a.internal_data_path().ok_or("Cartella privata non disponibile")?.join("anteprime");
                    let out=crate::storage::nuova_cartella(&base).map_err(|e|e.to_string())?;
                    let _scarto=Scarto(out.clone());
                    let libs=cartella_librerie(&a).ok_or("Il motore non è in questo APK")?;
                    let leases=saf::leases(&[path.clone()]);
                    let r=crate::worker::Richiesta{file:path,azione:"anteprima".into(),qualita:82,voce:Default::default(),out,opts:Default::default(),unisci:vec![],sommario:None,anteprima:Some(scelta),web:None};
                    let (png,meta)=crate::worker::avvia(&libs,r,&cancel,leases,|nome,bytes|web::permesso(&a,&c,id,nome,bytes),|_|{})?;
                    let info=serde_json::from_str(&meta).map_err(|e|format!("Anteprima: {e}"))?;
                    let image=cedro::ui::media::thumb_of(&png,1024).ok_or("Anteprima non leggibile")?;
                    Ok((info,image))
                })).unwrap_or_else(|_|Err("Il motore si è interrotto".into()));
                c.tokens.lock().unwrap_or_else(|e|e.into_inner()).remove(&id);
                c.labs.lock().unwrap_or_else(|e|e.into_inner()).push((id,result));
                wake.wake();
            });
            sporco=true;
        }
        for (id,result) in std::mem::take(&mut *consegne.labs.lock().unwrap_or_else(|e|e.into_inner())) {
            ui.ricevi_labs(id,result);sporco=true;
        }
        if let Some(result)=consegne.caso_esito.lock().unwrap_or_else(|e|e.into_inner()).take(){
            if let Err(e)=result {ui.errore_importazione(format!("{}: {e}",cedro::i18n::tr("Caso non salvato"))); if let Some(l)=ui.lavori.first_mut(){l.origine="Caso o campione".into();}}
            sporco=true;
        }
        if let Some((p, r)) = consegne.info_pdf.lock().unwrap_or_else(|e| e.into_inner()).take() {
            if ui.file.as_ref().is_some_and(|f| f.path == p) {
                match r {
                    Ok((n, bm)) => ui.info_pdf(n, bm),
                    Err(e) if e.contains("protetto") => ui.chiedi_password(),
                    Err(e) => log::warn!("PDF: {e}"),
                }
                sporco = true;
            }
        }
        for (p, n, img) in std::mem::take(&mut *consegne.unito.lock().unwrap_or_else(|e| e.into_inner())) {
            ui.info_unito(&p, n, img);
            sporco = true;
        }
        for (p, img, misure, pagine) in std::mem::take(&mut *consegne.voci.lock().unwrap_or_else(|e| e.into_inner())) {
            if let Some(n) = pagine {
                ui.pdf_pagine.insert(p.clone(), n);
            }
            ui.info_voce(&p, img, misure);
            sporco = true;
        }
        for (p, n, img) in std::mem::take(&mut *consegne.pagine.lock().unwrap_or_else(|e| e.into_inner())) {
            if ui.file.as_ref().is_some_and(|f| f.path == p) {
                ui.pdf.miniature.insert(n, img);
                sporco = true;
            }
        }
        for (p,bars,secs) in std::mem::take(&mut *consegne.onda_riv.lock().unwrap_or_else(|e|e.into_inner())) {
            ui.onda_trascrizione(&p,bars,secs);
            sporco = true;
        }
        for (id,path,seg,secs) in std::mem::take(&mut *consegne.frasi.lock().unwrap_or_else(|e|e.into_inner())) {
            ui.frase_trascritta(id,path,seg,secs);
            sporco = true;
        }
        for (id, nota) in std::mem::take(&mut *consegne.passo_riassunto.lock().unwrap_or_else(|e| e.into_inner())) {
            ui.passo_riassunto(id, nota);
            sporco = true;
        }
        for (_, source, s) in std::mem::take(&mut *consegne.riassunti.lock().unwrap_or_else(|e| e.into_inner())) {
            ui.riassunto_pronto(&source, s);
            sporco = true;
        }
        for (id, n) in std::mem::take(&mut *consegne.controlli.lock().unwrap_or_else(|e| e.into_inner())) {
            if let Some(l) = ui.lavori.iter_mut().find(|l| l.id == id) {
                l.controlli = Some(n);
            }
            sporco = true;
        }
        if let Some(r) = consegne.sblocco.lock().unwrap_or_else(|e| e.into_inner()).take() {
            match r {
                Ok(path) => {
                    ui.sbloccato(path.clone());
                    leggi_pdf(&consegne, path.clone());
                    let (c2, p2) = (consegne.clone(), path);
                    std::thread::spawn(move || {
                        if let Some(img) = cedro::ui::media::thumb_of(&p2, 600) {
                            *c2.miniatura.lock().unwrap_or_else(|e| e.into_inner()) = Some((p2.clone(), img));
                        }
                    });
                }
                Err(e) => ui.password_sbagliata(e),
            }
            sporco = true;
        }
        if let Some((p, img)) = consegne.miniatura.lock().unwrap_or_else(|e| e.into_inner()).take() {
            // Anche nell'elenco di Unisci, se il file è già lì.
            ui.info_unito(&p, None, Some(img.clone()));
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
        let esiti=std::mem::take(&mut *consegne.esiti.lock().unwrap_or_else(|e|e.into_inner()));
        if !esiti.is_empty(){
            for mut e in esiti{
                if let Some(batch)=e.batch.take(){
                    let tasks=ui.ricevi_batch(&batch);
                    registra(&consegne,&mut ui);
                    let (a,c)=(app.clone(),consegne.clone());
                    for (id,_,_) in &tasks{token(&c,*id);}
                    std::thread::spawn(move||{
                        let mut batch=batch;
                        for (id,index,prog) in tasks {
                            let child=batch.figli[index].clone();
                            if child.out.join(".pronti").is_file(){
                                let result=recupera_registro(&a,&child.out).filter(|r|r.lavoro.chiave==child.identita.chiave)
                                    .map(|r|Completamento::recuperato(id,r))
                                    .unwrap_or_else(||Completamento::semplice(id,Stato::Errore("Registro dei risultati non valido".into())));
                                c.esiti.lock().unwrap_or_else(|e|e.into_inner()).push(result);
                                c.tokens.lock().unwrap_or_else(|e|e.into_inner()).remove(&id);
                                let _=batch.finisci(index);a.create_waker().wake();continue;
                            }
                            protetto(&c,id,||trascrivi(&a,&c,id,&child.source,&child.out,&prog,batch.voce,batch.opts.clone()));
                            if let Err(_)=batch.finisci(index){log::warn!("batch: checkpoint finale non riuscito");}
                            a.create_waker().wake();
                        }
                    });
                }
                let esito=match &e.stato{Stato::Fatto{..} if e.uscite.as_ref().is_some_and(|v|v.iter().any(|u|u.2.is_some()))=>"parziale",Stato::Fatto{..}=>"completo",Stato::Errore(t) if t.contains("Annullato")=>"annullato",Stato::Errore(_)=>"errore",_=>"in-corso"};
                let azione=consegne.azioni.lock().unwrap_or_else(|e|e.into_inner()).remove(&e.id).unwrap_or_else(||"recupero".into());
                if e.id!=u32::MAX{log::info!("lavoro fine id={} azione={} esito={} uscite={}",e.id,azione,esito,e.uscite.as_ref().map_or(0,Vec::len));}
                ui.completa(e);
            }
            registra(&consegne,&mut ui);
            persisti(&consegne,&file_lavori,ui.serializza());
            sporco=true;
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
        let attesa_download={
            let mut a=consegne.download_attesa.lock().unwrap_or_else(|e|e.into_inner());
            if a.as_ref().is_some_and(|a|a.da.elapsed()>Duration::from_secs(8)&&crate::web::rete_consentita(ui.rete,a.mobile)){*a=None;}
            a.clone()
        };
        if ui.download_attesa!=attesa_download{ui.download_attesa=attesa_download;sporco=true;}
        let componente=consegne.componenti.lock().unwrap_or_else(|e|e.into_inner()).attuale();
        if ui.componente!=componente {ui.componente=componente;sporco=true;}
        let nuovi:Vec<_>=ui.lavori.iter().filter(|l|l.stato==Stato::InCorso).map(|l|(l.id,l.prog.load(std::sync::atomic::Ordering::Relaxed))).collect();
        if nuovi!=progressi {progressi=nuovi;sporco=true;}
        // Il servizio in primo piano vive finché c'è un lavoro in corso; la notifica segue l'avanzamento.
        if ultimo_servizio.elapsed() > Duration::from_millis(700) || (servizio_attivo && ui.in_corso().0 == 0) {
            let (n, medio, nome) = ui.in_corso();
            if n > 0 {
                let testo = if n > 1 { cedro::i18n::tr_string(&format!("{n} lavori in corso")) } else { nome };
                let (mut testo,mut percento)=ui.componente.as_ref().map(|c|(c.testo(),c.percento())).unwrap_or((testo,medio.map(|m|m as i32).unwrap_or(-1)));
                if let Some(a)=&ui.download_attesa{
                    if !crate::web::rete_consentita(ui.rete,a.mobile){
                        testo=format!("{} · {} · {}",cedro::i18n::tr("In attesa"),cedro::i18n::tr(&a.nome),cedro::i18n::tr(if ui.rete==1{"Solo Wi-Fi"}else{"Chiedi una volta su rete mobile"}));percento=-1;
                    }else if a.bytes>=100_000_000{testo.push_str(&format!(" · {}: {}",cedro::i18n::tr("Spazio libero"),cedro::size_words(a.liberi)));}
                }
                let next=(testo,percento);
                if ultima_notifica.as_ref()!=Some(&next){servizio(&app,Some((cedro::i18n::tr("Cedro sta lavorando").as_ref(),&next.0,next.1)));ultima_notifica=Some(next);}
                servizio_attivo = true;
                ultimo_servizio = std::time::Instant::now();
            } else if servizio_attivo {
                servizio(&app, None);
                servizio_attivo = false;
                ultima_notifica=None;
            }
        }
        // La tastiera segue il campo attivo.
        if std::mem::take(&mut ui.mostra_tastiera) && ui.campo.is_some() {
            tastiera = true;
            app.show_soft_input(true);
        }
        if ui.campo.is_some() != tastiera {
            tastiera = ui.campo.is_some();
            if tastiera {
                app.show_soft_input(true);
            } else {
                app.hide_soft_input(false);
            }
        }
        animando = ui.animando();
        if ultimo_audio.elapsed()>=Duration::from_millis(200) {
            let playing=ui.suona_audio();
            let focus=con_attivita(&app,|env,a|env.call_method(a,"audioFocus","(Z)Z",&[JValue::Bool(playing as u8)]).and_then(|r|r.z()).map_err(|e|e.to_string())).unwrap_or(false);
            if playing {if !focus {ui.pausa_audio();}sporco=true;}
            ultimo_audio=std::time::Instant::now();
        }
        if sporco || animando {
            if let Some(f) = &finestra {
                let (w, h) = (f.width() as u32, f.height() as u32);
                if w > 0 && h > 0 {
                    ultimo_disegno = std::time::Instant::now();
                    let mut p = Painter::new_riusando(w, h, scala, Fonts::for_theme(ui.tema), &mut cache);
                    p.larger = ui.grandi;
                    ui.disegna(&mut p, w as f32 / scala, h as f32 / scala);
                    presenta(f, &p);
                    let foglio = p.in_foglio();
                    cache.ricicla(foglio);
                    sporco = false;
                    // Le pagine che il disegno voleva e non c'erano.
                    if let Some(path) = ui.file.as_ref().map(|f| f.path.clone()) {
                        let nuove: Vec<u32> = std::mem::take(&mut ui.pdf.chiedi).into_iter().filter(|n| chieste.insert((path.clone(), *n))).collect();
                        if !nuove.is_empty() {
                            miniature_pagine(&consegne, path, nuove);
                        }
                    }
                }
            }
        }
    }
}

/// La riga del registro della trascrizione da mostrare in Lavori finché non c'è la percentuale, se serve.
fn passo_trascrizione(key: &str, text: &str) -> Option<String> {
    let t = match key {
        "Modello" | "Motore" | "Voci" if !text.starts_with("pronto") => format!("{key}: {text}"),
        "" if text.starts_with("Scaricati") || text.starts_with("Rete interrotta") || text.starts_with("Un altro lavoro") => text.to_string(),
        _ => return None,
    };
    let mut c = t.chars();
    Some(c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default())
}

/// Il file `nome` nella cartella `rel` di MediaStore, se c'è (e Cedro lo può vedere).
fn cerca<'a>(env: &mut JNIEnv<'a>, resolver: &JObject, base: &JObject, rel: &str, nome: &str) -> Option<JObject<'a>> {
    let trovato = (|| -> Result<Option<JObject<'a>>, jni::errors::Error> {
        let classe = env.find_class("java/lang/String")?;
        let id = env.new_string("_id")?;
        let proiez = env.new_object_array(1, &classe, &id)?;
        let filtro = env.new_string("relative_path IN (?, ?) AND _display_name = ? AND is_pending = 0")?;
        let vuoto = env.new_string("")?;
        let argomenti = env.new_object_array(3, &classe, &vuoto)?;
        for (i, a) in [format!("{rel}/"), rel.to_string(), nome.to_string()].iter().enumerate() {
            let a = env.new_string(a)?;
            env.set_object_array_element(&argomenti, i as i32, &a)?;
        }
        let nulla = JObject::null();
        let cur = env
            .call_method(resolver, "query", "(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;", &[JValue::Object(base), JValue::Object(&proiez), JValue::Object(&filtro), JValue::Object(&argomenti), JValue::Object(&nulla)])?
            .l()?;
        if cur.is_null() {
            return Ok(None);
        }
        let mut uri = None;
        if env.call_method(&cur, "moveToFirst", "()Z", &[])?.z()? {
            let n = env.call_method(&cur, "getLong", "(I)J", &[JValue::Int(0)])?.j()?;
            let u = env.call_static_method("android/content/ContentUris", "withAppendedId", "(Landroid/net/Uri;J)Landroid/net/Uri;", &[JValue::Object(base), JValue::Long(n)])?.l()?;
            uri = Some(u);
        }
        let _ = env.call_method(&cur, "close", "()V", &[]);
        Ok(uri)
    })();
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    trovato.ok().flatten()
}

/// Il primo nome libero in `rel` come sul computer: «x-2.txt», «x-3.txt»…
fn libero_in(env: &mut JNIEnv, resolver: &JObject, base: &JObject, rel: &str, nome: &str) -> String {
    let (stem, ext) = match nome.rfind('.') {
        Some(i) if i > 0 => (&nome[..i], &nome[i..]),
        _ => (nome, ""),
    };
    for n in 2..1000 {
        let prova = format!("{stem}-{n}{ext}");
        if cerca(env, resolver, base, rel, &prova).is_none() {
            return prova;
        }
    }
    nome.to_string()
}

/// Con «Salta» nelle Impostazioni: il file `nome` è già in Documenti/Cedro (o Download/Cedro)? Indirizzo e nome visibile.
fn gia_salvato(app: &AndroidApp, nome: &str) -> Option<(String, String)> {
    if ESISTE.load(std::sync::atomic::Ordering::Relaxed) != 2 || uscite(app).is_none() {
        return None;
    }
    con_attivita(app, |env, att| {
        let resolver = env.call_method(att, "getContentResolver", "()Landroid/content/ContentResolver;", &[]).and_then(|v| v.l()).map_err(|e| jerr(env, e))?;
        let esterno = env.new_string("external").map_err(|e| e.to_string())?;
        for (raccolta, rel) in [("android/provider/MediaStore$Files", "Documents/Cedro"), ("android/provider/MediaStore$Downloads", "Download/Cedro")] {
            let base = env.call_static_method(raccolta, "getContentUri", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&esterno)]).and_then(|v| v.l()).map_err(|e| jerr(env, e))?;
            if let Some(uri) = cerca(env, &resolver, &base, rel, nome) {
                let testo = env.call_method(&uri, "toString", "()Ljava/lang/String;", &[]).and_then(|v| v.l()).map_err(|e| jerr(env, e))?;
                return Ok(stringa(env, testo).map(|t| (t, format!("{rel}/{nome}"))));
            }
        }
        Ok(None)
    })
    .ok()
    .flatten()
}

/// Il messaggio del motore nomina il file come l'ha scritto lui: se MediaStore l'ha salvato con un altro nome
/// («x-2.txt»), si mette quello vero.
fn col_nome_vero(msg: &str, privato: &Path, visto: &str) -> String {
    let prima = nome_di(privato);
    let dopo = visto.rsplit('/').next().unwrap_or(visto);
    if prima.is_empty() || prima == dopo || visto.starts_with("Già presente") {
        return msg.to_string();
    }
    msg.replace(&prima, dopo)
}
