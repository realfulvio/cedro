//! Prova ripetibile del percorso usato da «Video dal web», senza scaricare episodi interi.
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("Uso: web-adapter-check PERCORSO_YT_DLP URL [CARTELLA_CAMPIONE]");
        std::process::exit(2);
    }
    let helper = PathBuf::from(&args[0]);
    let info = cedro::youtube::probe(&helper, &args[1], true, &AtomicBool::new(false)).unwrap_or_else(|e| {
        eprintln!("{e}"); std::process::exit(1);
    });
    println!("Titolo: {}; elementi: {:?}", info.title, info.entries);
    if let Some(dest) = args.get(2) {
        let dest = PathBuf::from(dest);
        std::fs::create_dir_all(&dest).unwrap();
        let mut request = cedro::youtube::Request::new(vec![args[1].clone()], false, dest.clone());
        request.thumbnail = false;
        request.chapters = false;
        request.playlist = false;
        let mut cmd = std::process::Command::new(&helper);
        cmd.args(cedro::youtube::build_args(&request, cedro::youtube::ffmpeg_path().as_deref(), None, false))
            .args(["--download-sections", "*0-5", "--socket-timeout", "12", "--retries", "0", "--", &args[1]])
            .current_dir(&dest);
        let status = cmd.status().expect("avvio yt-dlp");
        if !status.success() { std::process::exit(1); }
    }
}
