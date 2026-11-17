//! SAF: descriptor con nome/estensione, ripristino URI e alberi con struttura preservata.
use super::*;
use std::fs::File;
use std::io::{Seek, SeekFrom};
use std::os::fd::{AsRawFd, FromRawFd};
use std::sync::OnceLock;
use std::collections::HashMap;

thread_local! {
    static NOTICE: std::cell::RefCell<Option<Box<dyn Fn(String)>>> = const {std::cell::RefCell::new(None)};
    static IMPORT: std::cell::RefCell<Option<(u32,Arc<std::sync::atomic::AtomicBool>,Arc<std::sync::atomic::AtomicU32>)>> = const {std::cell::RefCell::new(None)};
}
static SIGNALS: OnceLock<Mutex<HashMap<u32,jni::objects::GlobalRef>>> = OnceLock::new();
pub fn contesto<T>(id:u32,cancel:Arc<std::sync::atomic::AtomicBool>,prog:Arc<std::sync::atomic::AtomicU32>,notice:impl Fn(String)+'static,f:impl FnOnce()->T)->T {
    IMPORT.with(|s|*s.borrow_mut()=Some((id,cancel,prog)));
    NOTICE.with(|s|*s.borrow_mut()=Some(Box::new(notice)));
    struct Restore;
    impl Drop for Restore {fn drop(&mut self){NOTICE.with(|s|{s.borrow_mut().take();});IMPORT.with(|s|{if let Some((id,_,_))=s.borrow_mut().take(){SIGNALS.get_or_init(Default::default).lock().unwrap_or_else(|e|e.into_inner()).remove(&id);}});}}
    let _restore=Restore; f()
}
fn cancellato()->bool {IMPORT.with(|s|s.borrow().as_ref().is_some_and(|(_,c,_)|c.load(std::sync::atomic::Ordering::Acquire)))}
fn controlla()->Result<(),String> {if cancellato(){Err("Annullato".into())}else{Ok(())}}
pub fn annulla(app:&AndroidApp,id:u32) {
    let signal=SIGNALS.get_or_init(Default::default).lock().unwrap_or_else(|e|e.into_inner()).get(&id).cloned();
    if let Some(signal)=signal {let app=app.clone();std::thread::spawn(move ||{let _=con_attivita(&app,|env,_|env.call_method(signal.as_obj(),"cancel","()V",&[]).map(|_|()).map_err(|e|jerr(env,e)));});}
}
struct Lettura(File);
impl std::io::Read for Lettura {
    fn read(&mut self,buf:&mut [u8])->std::io::Result<usize> {
        loop {
            if cancellato(){return Err(std::io::Error::new(std::io::ErrorKind::Interrupted,"Annullato"));}
            let mut p=libc::pollfd{fd:self.0.as_raw_fd(),events:libc::POLLIN,revents:0};
            let ready=unsafe{libc::poll(&mut p,1,100)};
            if ready>0 {return std::io::Read::read(&mut self.0,buf);}
            if ready<0 {let e=std::io::Error::last_os_error();if e.kind()!=std::io::ErrorKind::Interrupted{return Err(e);}}
        }
    }
}

// Le lease restano per la sessione: UI, preview e lavori possono usare il percorso senza una race
// con la chiusura del fd. Non si chiude mai un input ancora usato nella sessione.
static COPIE: Mutex<()> = Mutex::new(());
static APERTI: OnceLock<Mutex<HashMap<PathBuf, Arc<File>>>> = OnceLock::new();
fn aperti() -> &'static Mutex<HashMap<PathBuf, Arc<File>>> { APERTI.get_or_init(Default::default) }
pub fn lease(path: &Path) -> Option<Arc<File>> { aperti().lock().unwrap_or_else(|e| e.into_inner()).get(path).cloned() }

/// Il processo eredita anche i file associati al MRXS, sotto la stessa importazione isolata.
pub fn leases(paths: &[PathBuf]) -> Vec<Arc<File>> {
    let map=aperti().lock().unwrap_or_else(|e|e.into_inner());
    map.iter().filter(|(p,_)| paths.iter().any(|src| *p==src || src.parent().is_some_and(|dir|p.starts_with(dir)))).map(|(_,f)|f.clone()).collect()
}

pub fn frame<T>(env:&mut JNIEnv,f:impl FnOnce(&mut JNIEnv)->Result<T,String>)->Result<T,String>{
    env.push_local_frame(64).map_err(|e|jerr(env,e))?;
    let result=f(env);
    let popped=unsafe{env.pop_local_frame(&JObject::null())}.map_err(|e|jerr(env,e));
    popped?;
    result
}

fn segnale<'a>(env:&mut JNIEnv<'a>)->jni::errors::Result<JObject<'a>> {
    let signal=env.new_object("android/os/CancellationSignal","()V",&[])?;
    if let Some(id)=IMPORT.with(|s|s.borrow().as_ref().map(|v|v.0)){
        SIGNALS.get_or_init(Default::default).lock().unwrap_or_else(|e|e.into_inner()).insert(id,env.new_global_ref(&signal)?);
    }
    if cancellato(){env.call_method(&signal,"cancel","()V",&[])?;}
    Ok(signal)
}

fn descriptor(env: &mut JNIEnv, att: &JObject, uri: &JObject) -> Result<File, String> {
    let resolver = env.call_method(att, "getContentResolver", "()Landroid/content/ContentResolver;", &[]).and_then(|v| v.l()).map_err(|e| jerr(env,e))?;
    let r = env.new_string("r").map_err(|e| e.to_string())?;
    controlla()?;
    let signal=env.new_object("android/os/CancellationSignal","()V",&[]).map_err(|e|jerr(env,e))?;
    let id=IMPORT.with(|s|s.borrow().as_ref().map(|v|v.0));
    if let Some(id)=id {SIGNALS.get_or_init(Default::default).lock().unwrap_or_else(|e|e.into_inner()).insert(id,env.new_global_ref(&signal).map_err(|e|e.to_string())?);}
    if cancellato(){let _=env.call_method(&signal,"cancel","()V",&[]);}
    let opened=env.call_method(&resolver,"openFileDescriptor","(Landroid/net/Uri;Ljava/lang/String;Landroid/os/CancellationSignal;)Landroid/os/ParcelFileDescriptor;", &[JValue::Object(uri),JValue::Object(&r),JValue::Object(&signal)]).and_then(|v|v.l());
    if let Some(id)=id {SIGNALS.get_or_init(Default::default).lock().unwrap_or_else(|e|e.into_inner()).remove(&id);}
    let pfd=opened.map_err(|e|jerr(env,e))?;
    if pfd.is_null() { return Err("Impossibile aprire il file condiviso".into()); }
    let fd = env.call_method(&pfd,"detachFd","()I",&[]).and_then(|v|v.i()).map_err(|e|jerr(env,e))?;
    let _ = env.delete_local_ref(pfd);
    if fd < 0 { return Err("Descriptor SAF non valido".into()); }
    let file=unsafe {File::from_raw_fd(fd)};
    if unsafe {libc::fcntl(fd,libc::F_SETFD,libc::FD_CLOEXEC)}<0{return Err(std::io::Error::last_os_error().to_string());}
    Ok(file)
}

fn collega(path: &Path, file: File) -> Result<(), String> {
    let mut map = aperti().lock().unwrap_or_else(|e|e.into_inner());
    // Il launcher eredita questi fd soltanto nel figlio (pre_exec), con lo stesso numero.
    let target = format!("/proc/self/fd/{}",file.as_raw_fd());
    let tmp = path.with_file_name(format!(".{}.link",nome_di(path)));
    std::os::unix::fs::symlink(target,&tmp).map_err(|e|e.to_string())?;
    std::fs::rename(&tmp,path).map_err(|e|e.to_string())?;
    let file=Arc::new(file);
    cedro::input_file::register(file.as_raw_fd(),file.clone());
    map.insert(path.to_path_buf(),file);
    Ok(())
}

pub fn ripristina(app: &AndroidApp, path: &Path) -> Result<(), String> {
    if lease(path).is_some() { return Ok(()); }
    let side = path.with_file_name(format!(".{}.uri",nome_di(path)));
    if !side.exists() { return if path.is_file() { Ok(()) } else { Err("File non disponibile: sceglilo di nuovo".into()) }; }
    let text = std::fs::read_to_string(side).map_err(|e|e.to_string())?;
    con_attivita(app,|env,att| {
        let text = env.new_string(text).map_err(|e|e.to_string())?;
        let uri = env.call_static_method("android/net/Uri","parse","(Ljava/lang/String;)Landroid/net/Uri;",&[JValue::Object(&text)]).and_then(|v|v.l()).map_err(|e|jerr(env,e))?;
        let file = descriptor(env,att,&uri)?;
        collega(path,file)
    })
}

/// Soltanto all'avvio: conserva i dodici import più recenti e tutti i riferimenti delle revisioni.
pub fn pulisci(base:&Path,protetti:&[PathBuf]) {
    let mut protetti=protetti.to_vec();
    #[derive(serde::Deserialize)]
    struct Origine {source:PathBuf}
    let revisioni=cedro::trascrizione::data_dir().join("revisioni");
    if let Ok(entries)=std::fs::read_dir(revisioni){for e in entries.flatten(){
        if let Ok(file)=File::open(e.path()){
            match serde_json::from_reader::<_,Origine>(file){
                Ok(v)=>protetti.push(v.source),
                // A malformed review might still reference a cached input: defer cleanup entirely.
                Err(_)=>return,
            }
        }
    }}
    let mut dirs:Vec<_>=std::fs::read_dir(base).into_iter().flatten().filter_map(Result::ok).filter(|e|e.file_type().is_ok_and(|t|t.is_dir())).filter_map(|e|Some((e.metadata().ok()?.modified().ok()?,e.path()))).collect();
    dirs.sort_by(|a,b|b.0.cmp(&a.0));
    for (age,p) in dirs.into_iter().skip(12){
        if age.elapsed().is_ok_and(|d|d.as_secs()>7*24*3600)&&!protetti.iter().any(|r|r.starts_with(&p)){
            let _=std::fs::remove_dir_all(p);
        }
    }
}

pub fn ripristina_tutti(app:&AndroidApp,base:&Path) {
    fn paths(dir:&Path,out:&mut Vec<PathBuf>){
        if let Ok(list)=std::fs::read_dir(dir){for e in list.flatten(){
            let p=e.path();
            if e.file_type().is_ok_and(|t|t.is_dir()){paths(&p,out);}
            else if let Some(n)=p.file_name().and_then(|n|n.to_str()).and_then(|n|n.strip_prefix('.')).and_then(|n|n.strip_suffix(".uri")){out.push(dir.join(n));}
        }}
    }
    let mut v=Vec::new();paths(base,&mut v);
    for p in v {if let Err(e)=ripristina(app,&p){log::warn!("SAF: {}: {e}",p.display());}}
}

fn spazio(env: &mut JNIEnv, dir: &Path) -> Result<u64,String> {
    let s=env.new_string(dir.to_string_lossy()).map_err(|e|e.to_string())?;
    let stat=env.new_object("android/os/StatFs","(Ljava/lang/String;)V",&[JValue::Object(&s)]).map_err(|e|jerr(env,e))?;
    let n=env.call_method(&stat,"getAvailableBytes","()J",&[]).and_then(|v|v.j()).map_err(|e|jerr(env,e))?;
    Ok((n.max(0) as u64).saturating_sub(128*1024*1024))
}

pub(super) fn copia_uri(env: &mut JNIEnv, att: &JObject, uri: &JObject, mime: &str, base: &Path) -> Result<Condiviso,String> {
    let dir=crate::storage::nuova_cartella(base).map_err(|e|e.to_string())?;
    let r=importa(env,att,uri,mime,&dir,None);
    if r.is_err(){aperti().lock().unwrap_or_else(|e|e.into_inner()).retain(|p,_|!p.starts_with(&dir));let _=std::fs::remove_dir_all(&dir);}
    r
}

fn importa(env: &mut JNIEnv, att: &JObject, uri: &JObject, mime: &str, dir: &Path, nome: Option<String>) -> Result<Condiviso,String> {
    controlla()?;
    let resolver=env.call_method(att,"getContentResolver","()Landroid/content/ContentResolver;",&[]).and_then(|v|v.l()).map_err(|e|jerr(env,e))?;
    let mut mime=mime.to_string();
    if mime.is_empty() {
        mime=env.call_method(&resolver,"getType","(Landroid/net/Uri;)Ljava/lang/String;",&[JValue::Object(uri)]).and_then(|v|v.l()).ok().and_then(|o|stringa(env,o)).unwrap_or_default();
        let _=env.exception_clear();
    }
    let mut nome=nome.unwrap_or_default();
    if nome.is_empty() {
        let null=JObject::null();
        let signal=segnale(env).map_err(|e|jerr(env,e))?;
        let cur=env.call_method(&resolver,"query","(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;Landroid/os/CancellationSignal;)Landroid/database/Cursor;",&[JValue::Object(uri),JValue::Object(&null),JValue::Object(&null),JValue::Object(&null),JValue::Object(&null),JValue::Object(&signal)]).and_then(|v|v.l()).map_err(|e|jerr(env,e))?;
        if !cur.is_null() {
            let read=(|| {
                if env.call_method(&cur,"moveToFirst","()Z",&[]).and_then(|v|v.z()).map_err(|e|jerr(env,e))? {
                    let col=env.new_string("_display_name").map_err(|e|e.to_string())?;
                    let i=env.call_method(&cur,"getColumnIndex","(Ljava/lang/String;)I",&[JValue::Object(&col)]).and_then(|v|v.i()).map_err(|e|jerr(env,e))?;
                    if i>=0 { nome=env.call_method(&cur,"getString","(I)Ljava/lang/String;",&[JValue::Int(i)]).and_then(|v|v.l()).ok().and_then(|o|stringa(env,o)).unwrap_or_default(); }
                }
                Ok::<_,String>(())
            })();
            let _=env.call_method(&cur,"close","()V",&[]);
            read?;
        }
    }
    if nome.is_empty() {
        nome=env.call_method(uri,"getLastPathSegment","()Ljava/lang/String;",&[]).and_then(|v|v.l()).ok().and_then(|o|stringa(env,o)).unwrap_or_default();
        let _=env.exception_clear();
    }
    if nome.is_empty() { nome="condiviso".into(); }
    let mut nome=crate::storage::nome_sicuro(&nome);
    if Path::new(&nome).extension().is_none() { nome.push('.'); nome.push_str(estensione_da_mime(&mime)); }
    std::fs::create_dir_all(dir).map_err(|e|e.to_string())?;
    let path=dir.join(&nome);
    if path.symlink_metadata().is_ok() { return Err(format!("Nome duplicato nella cartella: {nome}")); }
    let mut file=descriptor(env,att,uri)?;
    let diretto = file.metadata().is_ok_and(|m|m.len()>=32*1024*1024) || matches!(path.extension().and_then(|e|e.to_str()).unwrap_or("").to_ascii_lowercase().as_str(),"svs"|"ndpi"|"mrxs"|"czi"|"nd2"|"lif");
    let seekable=file.seek(SeekFrom::Start(0)).is_ok();
    if diretto && seekable {
        let value=env.call_method(uri,"toString","()Ljava/lang/String;",&[]).and_then(|v|v.l()).map_err(|e|jerr(env,e))?;
        let value=stringa(env,value).ok_or("Indirizzo SAF assente")?;
        collega(&path,file)?;
        crate::storage::atomico(&dir.join(format!(".{nome}.uri")),value.as_bytes()).map_err(|e|e.to_string())?;
    } else {
        // Un provider a pipe richiede una copia: mai un Vec della dimensione del file.
        let _lock=COPIE.lock().unwrap_or_else(|e|e.into_inner());
        let budget=spazio(env,dir)?;
        NOTICE.with(|s|if let Some(notice)=s.borrow().as_ref(){notice(if seekable {format!("Copia privata · spazio disponibile: {}",cedro::size_words(budget))} else {format!("Il provider richiede una copia · spazio disponibile: {}",cedro::size_words(budget))});});
        let total=file.metadata().map(|m|m.len()).unwrap_or(0);
        crate::storage::copia(Lettura(file),&path,budget,&std::sync::atomic::AtomicBool::new(false), |n| {
            if total>0{IMPORT.with(|s|if let Some((_,_,prog))=s.borrow().as_ref(){prog.store((n.saturating_mul(1000)/total).min(1000) as u32,std::sync::atomic::Ordering::Relaxed);});}
        }).map_err(|e|e.to_string())?;
    }
    controlla()?;
    Ok(Condiviso {path,nome})
}

pub(super) fn figli(env: &mut JNIEnv, att: &JObject, tree: &JObject, base: &Path) -> Result<Vec<Condiviso>,String> {
    let dir=crate::storage::nuova_cartella(base).map_err(|e|e.to_string())?;
    let id=env.call_static_method("android/provider/DocumentsContract","getTreeDocumentId","(Landroid/net/Uri;)Ljava/lang/String;",&[JValue::Object(tree)]).and_then(|v|v.l()).map_err(|e|jerr(env,e))?;
    let id=stringa(env,id).ok_or("Cartella SAF senza identificatore")?;
    let mut out=Vec::new();
    if let Err(e)=visita(env,att,tree,&id,&dir,0,&mut out){aperti().lock().unwrap_or_else(|e|e.into_inner()).retain(|p,_|!p.starts_with(&dir));let _=std::fs::remove_dir_all(&dir);return Err(e);}
    // I file ausiliari MRXS restano accanto al .mrxs ma non diventano lavori separati.
    let mrxs:Vec<PathBuf>=out.iter().filter(|c|c.path.extension().and_then(|e|e.to_str()).is_some_and(|e|e.eq_ignore_ascii_case("mrxs"))).map(|c|c.path.with_extension("")).collect();
    out.retain(|c| !mrxs.iter().any(|d|c.path.starts_with(d)));
    Ok(out)
}

fn visita(env: &mut JNIEnv, att: &JObject, tree: &JObject, id: &str, dir: &Path, depth: usize, out: &mut Vec<Condiviso>) -> Result<(),String> {
    controlla()?;
    if depth>64 { return Err("Cartella troppo annidata".into()); }
    std::fs::create_dir_all(dir).map_err(|e|e.to_string())?;
    let rows=env.with_local_frame(32,|env| -> Result<Vec<(String,String,String)>,jni::errors::Error> {
        let id=env.new_string(id)?;
        let list=env.call_static_method("android/provider/DocumentsContract","buildChildDocumentsUriUsingTree","(Landroid/net/Uri;Ljava/lang/String;)Landroid/net/Uri;",&[JValue::Object(tree),JValue::Object(&id)])?.l()?;
        let resolver=env.call_method(att,"getContentResolver","()Landroid/content/ContentResolver;",&[])?.l()?;
        let cls=env.find_class("java/lang/String")?;
        let columns=env.new_object_array(3,cls,JObject::null())?;
        for (i,name) in ["document_id","mime_type","_display_name"].iter().enumerate() { let s=env.new_string(name)?; env.set_object_array_element(&columns,i as i32,s)?; }
        let null=JObject::null();
        let signal=segnale(env)?;
        let cur=env.call_method(&resolver,"query","(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;Landroid/os/CancellationSignal;)Landroid/database/Cursor;",&[JValue::Object(&list),JValue::Object(&columns),JValue::Object(&null),JValue::Object(&null),JValue::Object(&null),JValue::Object(&signal)])?.l()?;
        let r=(|| {
            let mut rows=Vec::new();
            while env.call_method(&cur,"moveToNext","()Z",&[])?.z()? {
                let row=env.with_local_frame(8,|env| -> Result<_,jni::errors::Error> {
                    let mut vals=Vec::new();
                    for i in 0..3 { let v=env.call_method(&cur,"getString","(I)Ljava/lang/String;",&[JValue::Int(i)])?.l()?; vals.push(stringa(env,v).unwrap_or_default()); }
                    Ok((vals.remove(0),vals.remove(0),vals.remove(0)))
                })?;
                rows.push(row);
            }
            Ok(rows)
        })();
        let _=env.call_method(&cur,"close","()V",&[]);
        r
    }).map_err(|e|jerr(env,e))?;
    for (id,mime,name) in rows {
        controlla()?;
        let name=crate::storage::nome_sicuro(&name);
        if mime=="vnd.android.document/directory" {
            visita(env,att,tree,&id,&dir.join(name),depth+1,out)?;
        } else {
            // A frame per file avoids overflowing JNI locals on large slide folders.
            env.push_local_frame(48).map_err(|e|jerr(env,e))?;
            let r=(|| {
                let id=env.new_string(id).map_err(|e|e.to_string())?;
                let uri=env.call_static_method("android/provider/DocumentsContract","buildDocumentUriUsingTree","(Landroid/net/Uri;Ljava/lang/String;)Landroid/net/Uri;",&[JValue::Object(tree),JValue::Object(&id)]).and_then(|v|v.l()).map_err(|e|jerr(env,e))?;
                importa(env,att,&uri,&mime,dir,Some(name))
            })();
            let _=unsafe { env.pop_local_frame(&JObject::null()) };
            out.push(r?);
        }
    }
    Ok(())
}
