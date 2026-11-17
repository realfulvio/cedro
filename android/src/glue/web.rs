//! Policy download Android: JNI soltanto nel genitore, attesa cancellabile senza polling UI.
use super::*;

pub(super) fn permesso(app:&AndroidApp,c:&Consegne,id:u32,nome:&str,bytes:u64)->Result<(),String>{
    fn a_consumo(app:&AndroidApp)->Result<bool,String>{con_attivita(app,|env,att|{
        let key=env.new_string("connectivity").map_err(|e|e.to_string())?;
        let manager=env.call_method(att,"getSystemService","(Ljava/lang/String;)Ljava/lang/Object;",&[JValue::Object(&key)]).and_then(|v|v.l()).map_err(|e|jerr(env,e))?;
        env.call_method(manager,"isActiveNetworkMetered","()Z",&[]).and_then(|v|v.z()).map_err(|e|jerr(env,e))
    })}
    let mut mobile=a_consumo(app)?;
    let liberi=con_attivita(app,|env,_|{
        let path=app.internal_data_path().ok_or("Cartella privata non disponibile")?;
        let path=env.new_string(path.to_string_lossy()).map_err(|e|e.to_string())?;
        let stat=env.new_object("android/os/StatFs","(Ljava/lang/String;)V",&[JValue::Object(&path)]).map_err(|e|jerr(env,e))?;
        env.call_method(&stat,"getAvailableBytes","()J",&[]).and_then(|v|v.j()).map(|n|n.max(0) as u64).map_err(|e|jerr(env,e))
    })?;
    if !crate::web::spazio_sufficiente(bytes,liberi){return Err(format!("{} {}: {} / {}",cedro::i18n::tr("Spazio insufficiente per il componente. Libera spazio e riprova."),cedro::i18n::tr("Spazio libero"),cedro::size_words(liberi),cedro::size_words(bytes)));}
    let cancel=token(c,id);
    let (lock,changed)=&*c.rete;
    let mut policy=lock.lock().unwrap_or_else(|e|e.into_inner());
    if !crate::web::rete_consentita(*policy,mobile)||bytes>=100_000_000 {
        *c.download_attesa.lock().unwrap_or_else(|e|e.into_inner())=Some(crate::web::AttesaDownload{nome:nome.into(),bytes,liberi,mobile,da:std::time::Instant::now()});
        app.create_waker().wake();
    }
    let mut ultimo_controllo=std::time::Instant::now();
    // Lo spazio è informativo; soltanto la policy di rete può sospendere il download.
    loop {
        if cancel.load(std::sync::atomic::Ordering::Acquire){*c.download_attesa.lock().unwrap_or_else(|e|e.into_inner())=None;return Err("Annullato".into());}
        if ultimo_controllo.elapsed()>Duration::from_secs(2){
            mobile=a_consumo(app)?;ultimo_controllo=std::time::Instant::now();
            if let Some(a)=c.download_attesa.lock().unwrap_or_else(|e|e.into_inner()).as_mut(){a.mobile=mobile;}
            app.create_waker().wake();
        }
        if crate::web::rete_consentita(*policy,mobile){break;}
        let (next,_)=changed.wait_timeout(policy,Duration::from_millis(150)).unwrap_or_else(|e|e.into_inner());policy=next;
    }
    // La nota con byte/spazio rimane visibile mentre arrivano i primi progressi.
    app.create_waker().wake();Ok(())
}
