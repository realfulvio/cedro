//! Prova da riga di comando sul telefono (adb shell), senza schermo: `prova azione file [qualita]`.
//! Stampa tempo e messaggio del motore. Serve a misurare le conversioni e le accelerazioni.
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 3 {
        eprintln!("uso: prova azione file [qualita]");
        return;
    }
    std::env::set_var("HOME", "/data/local/tmp/home");
    let out = PathBuf::from("/data/local/tmp/out");
    std::fs::create_dir_all(&out).unwrap();
    if a[1] == "trascrivi" {
        // prova trascrivi file [modello 0..2] [gpu 0/1]
        let model = a.get(3).and_then(|q| q.parse().ok()).unwrap_or(0);
        let gpu = a.get(4).is_some_and(|g| g == "1");
        let req = cedro::trascrizione::Request { format: cedro::trascrizione::Format::Text, model, gpu, avx512: false };
        let t = Instant::now();
        cedro::trascrizione::run(&[PathBuf::from(&a[2])], &req, Some(&out), &cedro::Options::default(), &|e| match e {
            cedro::trascrizione::Event::Log(l) => println!("{} {}", l.key, l.text),
            cedro::trascrizione::Event::Percent { percent, .. } => println!("{percent}%"),
            cedro::trascrizione::Event::Finished { result, .. } => println!("FINE {:?}", result),
        });
        println!("totale {} ms", t.elapsed().as_millis());
        return;
    }
    let q = a.get(3).and_then(|q| q.parse().ok()).unwrap_or(82);
    let t = Instant::now();
    let r = cedro::perform_into(&PathBuf::from(&a[2]), &a[1], q, Some(&out));
    let ms = t.elapsed().as_millis();
    match r {
        Ok(o) => println!("OK {} ms · {} · {}", ms, o.path.display(), o.message),
        Err(e) => println!("ERRORE {} ms · {}", ms, e),
    }
}
