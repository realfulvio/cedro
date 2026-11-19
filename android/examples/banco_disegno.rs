//! Quanto costa un fotogramma alla risoluzione di un telefono vero (1080×2340, densità 2,75 come il Mi 9T),
//! col foglio e la cache riusati come nel ciclo dell'app. Con una cartella come secondo argomento salva anche
//! i PNG, per confrontare pixel per pixel prima e dopo una modifica al disegno.
//! Sul mini-PC: cargo run -p cedro-android --release --example banco_disegno -- 30 [/tmp/banco]
use std::alloc::{GlobalAlloc, Layout, System};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

/// Conta la memoria viva (e il massimo), per vedere quanto tengono le cache del disegno.
struct Contatore;
static VIVI: AtomicUsize = AtomicUsize::new(0);
static PICCO: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Contatore {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = System.alloc(l);
        if !p.is_null() {
            let v = VIVI.fetch_add(l.size(), Ordering::Relaxed) + l.size();
            PICCO.fetch_max(v, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        VIVI.fetch_sub(l.size(), Ordering::Relaxed);
        System.dealloc(p, l)
    }
}
#[global_allocator]
static CONTA: Contatore = Contatore;
const MB: f64 = 1024.0 * 1024.0;

use cedro::ui::gfx::{Fonts, GlyphCache, Painter};
use cedro::ui::theme::ThemeId;
use cedro::Kind;
use cedro_android::ui::{App, Archivio, Lavoro, Recente, Scheda, Stato};

const W: u32 = 1080;
const H: u32 = 2340;
const SCALA: f32 = 2.75;

fn prepara(scena: &str, tema: ThemeId) -> App {
    let mut app = App::new(tema);
    app.anim.calmo = true;
    app.vai_casa();
    app.vista_completa(true);
    app.margini = (0.0, 27.0, 0.0, 14.0);
    let path = PathBuf::from("assets/cedro/immagini.jpg");
    app.recenti.push(Recente { path: path.clone(), nome: "Riunione.m4a".into(), kind: Kind::Audio, thumb: None });
    app.recenti.push(Recente { path: path.clone(), nome: "Relazione.pdf".into(), kind: Kind::Pdf, thumb: cedro::ui::media::thumb_of(&path, 300) });
    match scena {
        "converti" | "pdf" | "trascrivi" | "video" => {
            let (k, nav) = match scena { "converti" => (Kind::Image, 1), "pdf" => (Kind::Pdf, 2), "trascrivi" => (Kind::Audio, 3), _ => (Kind::Video, 4) };
            app.aggiungi_coda(vec![(path.clone(), "Sopralluogo.heic".into(), k)]);
            app.info_voce(&path, cedro::ui::media::thumb_of(&path, 300), Some((4032, 3024)));
            app.vista_nav(nav);
        }
        "lavori" => {
            app.sopra = Some(Scheda::Lavori);
            for id in 0..12 {
                app.lavori.push(Lavoro { id, origine: format!("Sopralluogo-{id}.heic"), etichetta: "PDF".into(), conv: true, inizio: Instant::now(), quando: cedro_android::ui::adesso(), stato: Stato::Fatto { path: path.clone(), nome: format!("Sopralluogo-{id}.pdf"), msg: "1,2 MB".into() }, prog: Default::default(), sorgente: None, controlli: None });
            }
        }
        "impostazioni" => app.sopra = Some(Scheda::Aspetto),
        "condividi" => {
            app.imposta_file(Archivio { path: path.clone(), nome: "Sopralluogo.heic".into(), dim: 1234567 }, Kind::Image, true);
            app.miniatura = cedro::ui::media::thumb_of(&path, 300);
            app.scelta = cedro::conversioni::formats(Kind::Image).iter().position(|f| f.id == "pdf");
        }
        _ => {}
    }
    app
}

fn main() {
    let giri: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let png = std::env::args().nth(2).map(PathBuf::from);
    if let Some(d) = &png {
        std::fs::create_dir_all(d).unwrap();
    }
    let mut totale = 0.0;
    for (nome, tema) in [("inizio", ThemeId::Cedro), ("converti", ThemeId::Cedro), ("pdf", ThemeId::Cedro), ("trascrivi", ThemeId::Cedro), ("video", ThemeId::Cedro), ("lavori", ThemeId::Cedro), ("impostazioni", ThemeId::Cedro), ("condividi", ThemeId::Cedro), ("inizio", ThemeId::CedroNotte), ("inizio", ThemeId::WindowsEssenziale)] {
        let mut app = prepara(nome, tema);
        let mut cache = GlyphCache::new();
        let mut tempi = Vec::with_capacity(giri);
        for giro in 0..giri {
            let t = Instant::now();
            let mut p = Painter::new_riusando(W, H, SCALA, Fonts::for_theme(tema), &mut cache);
            p.accessibility_text = Some(Vec::new());
            app.disegna(&mut p, W as f32 / SCALA, H as f32 / SCALA);
            let semantica = app.accessibilita(&p);
            std::hint::black_box(semantica);
            tempi.push(t.elapsed().as_secs_f64() * 1000.0);
            if giro == 0 {
                if let Some(d) = &png {
                    p.save_png(&d.join(format!("{nome}-{}.png", tema.key()))).unwrap();
                }
            }
            let foglio = p.in_foglio();
            cache.ricicla(foglio);
        }
        let primo = tempi[0];
        let mut resto = tempi[1..].to_vec();
        resto.sort_by(|a, b| a.total_cmp(b));
        let mediana = resto.get(resto.len() / 2).copied().unwrap_or(primo);
        totale += mediana;
        let vivi = VIVI.load(Ordering::Relaxed) as f64 / MB;
        drop(cache);
        let senza_cache = VIVI.load(Ordering::Relaxed) as f64 / MB;
        println!("{nome:<13} {:<20} primo {primo:7.2} ms  mediana {mediana:7.2} ms  memoria {vivi:6.1} MB (cache {:5.1} MB)", tema.key(), vivi - senza_cache);
    }
    println!("picco {:.1} MB", PICCO.load(Ordering::Relaxed) as f64 / MB);
    println!("somma mediane {totale:.1} ms");
}
