//! Le conversioni dei DICOM (TC, RX, ecografie, risonanze): la finestra della TC, tutte le immagini
//! della serie, i valori originali a 16 bit e l'anonimizzazione. Le voci che il DICOM non cambia
//! (l'OCR, l'icona) passano dal percorso comune delle immagini, che ne apre la prima immagine.

use std::path::{Path, PathBuf};

use image::DynamicImage;

use super::{err, image_target, jpeg_bytes, reserve, save_image, skipping, stem, unique_output, write_bytes, CedroResult, Options, Output};
use crate::dicom::{anon, pixel, tag, DicomFile};

/// `None` se la voce non è di quelle che i DICOM trattano a modo loro.
pub(super) fn perform(path: &Path, action: &str, quality: u8, dir: &Path, opts: &Options) -> Option<CedroResult<Output>> {
    if action != "strip" && (image_target(action).is_none() || action == "ico") {
        return None;
    }
    Some(run(path, action, quality, dir, opts))
}

/// Il nome dei file di uscita: con «Anonimizza» il codice del paziente (e serie e numero dell'immagine,
/// così i file di uno studio restano in ordine), altrimenti il nome del file.
fn base_name(f: &DicomFile, path: &Path, key: &[u8], anonymous: bool) -> String {
    if !anonymous {
        return stem(path);
    }
    let who = f.string(tag(0x0010, 0x0020)).or_else(|| f.string(tag(0x0010, 0x0010))).filter(|s| !s.trim().is_empty());
    let Some(who) = who else { return stem(path) };
    let mut name = anon::code_for(key, &who);
    if let Some(s) = f.number(tag(0x0020, 0x0011)) {
        name.push_str(&format!("-serie{}", s as i64));
    }
    if let Some(i) = f.number(tag(0x0020, 0x0013)) {
        name.push_str(&format!("-{:03}", i as i64));
    }
    name
}

fn window_of(opts: &Options) -> (pixel::Window, &'static str) {
    match opts.dicom_window {
        1 => (pixel::LUNG, "polmone"),
        2 => (pixel::BONE, "osso"),
        3 => (pixel::SOFT, "tessuti molli"),
        _ => (pixel::Window::File, ""),
    }
}

fn run(path: &Path, action: &str, quality: u8, dir: &Path, opts: &Options) -> CedroResult<Output> {
    let mut f = DicomFile::open(path).map_err(err)?;
    let key = anon::local_key();
    // «Togli dati nascosti» fa sempre un file anonimo: il nome del file non deve dire il paziente.
    let base = base_name(&f, path, &key, opts.dicom_anon || action == "strip");

    // «Togli dati nascosti» su un DICOM: resta un DICOM, anonimizzato.
    if action == "strip" {
        let report = anon::anonymize(&mut f, &key, anon::Choices::default());
        let out = reserve(dir, &format!("{base}-anonimo"), "dcm")?;
        write_bytes(&out, &f.write())?;
        return Ok(Output { path: out, message: anon::sentence(&report) });
    }

    let info = pixel::Info::of(&f).map_err(err)?;
    let (format, ext, label) = image_target(action).ok_or_else(|| err(format!("azione sconosciuta: {action}")))?;
    let stem_out = if label.is_empty() { base.clone() } else { format!("{base}-{label}") };
    let (window, window_name) = window_of(opts);
    let preset_used = !window_name.is_empty() && (info.modality == "CT" || info.intercept != 0.0);
    let sixteen = opts.dicom_16bit && matches!(action, "png" | "tiff") && info.monochrome();
    let frames: Vec<usize> = if opts.dicom_all_frames { (0..info.frames as usize).collect() } else { vec![0] };
    let quality = if action == "compress" { quality.min(crate::settings::read_settings().compress_quality()) } else { quality };

    // Un fotogramma pronto da salvare: finestra (o 16 bit), misure e ritocchi della voce.
    let picture = |i: usize| -> Result<(DynamicImage, i64, bool), String> {
        let fr = pixel::frame(&f, &info, i)?;
        let (mut img, offset, clipped) = if sixteen { pixel::render16(&info, &fr) } else { (pixel::render(&info, &fr, window), 0, false) };
        if let Some(side) = opts.max_side.filter(|s| *s > 0 && img.width().max(img.height()) > *s) {
            img = img.resize(side, side, image::imageops::FilterType::Lanczos3);
        }
        img = match action {
            "half" => img.resize_exact((img.width() / 2).max(1), (img.height() / 2).max(1), image::imageops::FilterType::Lanczos3),
            "gray" => DynamicImage::ImageLuma8(img.to_luma8()),
            "rot90" => img.rotate90(),
            _ => img,
        };
        Ok((img, offset, clipped))
    };

    let mut notes: Vec<String> = Vec::new();
    if preset_used {
        notes.push(format!("finestra {window_name}"));
    } else if !window_name.is_empty() {
        notes.push("finestra del file (polmone, osso e tessuti molli valgono per la TC)".into());
    }
    if info.frames < info.declared_frames {
        notes.push(format!("il file dice {} immagini ma ne contiene {}: è incompleto", info.declared_frames, info.frames));
    }

    if action == "pdf" {
        let out = reserve(dir, &stem_out, "pdf")?;
        let pages = in_parallel(&frames, |i| {
            let (img, _, _) = picture(i)?;
            jpeg_bytes(&img, 90, opts.white_background).map_err(|e| e.to_string())
        })
        .map_err(err)?;
        write_bytes(&out, &crate::pdf::pdf_from_jpegs(&pages))?;
        let mut message = if pages.len() == 1 { "PDF".to_string() } else { format!("PDF di {} pagine, un'immagine per pagina", pages.len()) };
        finish_message(&mut message, &notes, &f, opts);
        return Ok(Output { path: out, message });
    }

    // I nomi si scelgono qui, in ordine (la regola dei nomi che esistono già vale su questo thread);
    // decodifica e scrittura vanno poi in parallelo.
    let outs: Vec<(usize, PathBuf)> = if frames.len() == 1 {
        vec![(frames[0], reserve(dir, &stem_out, ext)?)]
    } else {
        let digits = frames.len().to_string().len().max(3);
        frames.iter().map(|i| (*i, unique_output(dir, &format!("{stem_out}-{:0digits$}", i + 1), ext))).filter(|(_, p)| !skipping(p)).collect()
    };
    if outs.is_empty() {
        return Err(err("i file esistono già: saltati"));
    }
    let written = in_parallel(&outs.iter().map(|(i, _)| *i).collect::<Vec<_>>(), |i| {
        let out = &outs.iter().find(|(k, _)| *k == i).ok_or("immagine non trovata")?.1;
        let (img, offset, clipped) = picture(i)?;
        match action {
            "avif" => std::fs::write(out, crate::imgfmt::avif_bytes(&img, quality)?).map_err(|e| e.to_string())?,
            "jxl" => std::fs::write(out, crate::imgfmt::jxl_bytes(&img)?).map_err(|e| e.to_string())?,
            _ => save_image(&img, out, format, quality, opts.white_background).map_err(|e| e.to_string())?,
        }
        Ok((img.width(), img.height(), offset, clipped))
    });
    let sizes = match written {
        Ok(s) => s,
        Err(e) => {
            for (_, p) in &outs {
                let _ = std::fs::remove_file(p);
            }
            return Err(err(e));
        }
    };
    let (w, h, offset, _) = sizes[0];
    let up = ext.to_uppercase();
    let mut message = if outs.len() == 1 { format!("{up} {w}×{h}") } else { format!("{} {up}, uno per immagine ({w}×{h})", outs.len()) };
    if sixteen {
        notes.push(match offset {
            0 => "valori originali a 16 bit".into(),
            32768 => format!("valori originali a 16 bit, spostati di {offset} (gli Hounsfield negativi)"),
            _ => format!("valori originali a 16 bit, spostati di {offset} perché stiano nei 16 bit"),
        });
        if sizes.iter().any(|s| s.2 != offset) {
            notes.push("lo spostamento non è lo stesso in tutte le immagini".into());
        }
        if sizes.iter().any(|s| s.3) {
            notes.push("alcuni valori non stavano nei 16 bit e sono stati tagliati".into());
        }
        if info.photometric == "MONOCHROME1" {
            notes.push("MONOCHROME1: i valori sono quelli del file, quindi il bianco e il nero appaiono scambiati".into());
        }
    }
    finish_message(&mut message, &notes, &f, opts);
    Ok(Output { path: outs[0].1.clone(), message })
}

fn finish_message(message: &mut String, notes: &[String], f: &DicomFile, opts: &Options) {
    for n in notes {
        message.push_str(" · ");
        message.push_str(n);
    }
    if opts.dicom_anon && pixel::burned_in(f) {
        message.push_str(". Attenzione: in questo tipo di immagine il nome può essere scritto dentro i pixel; controllala prima di condividerla");
    }
}

/// Esegue `job` per ogni indice, su tutti i core, e restituisce i risultati nell'ordine degli indici.
pub(super) fn in_parallel<T: Send>(items: &[usize], job: impl Fn(usize) -> Result<T, String> + Sync) -> Result<Vec<T>, String> {
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(items.len()).max(1);
    if threads == 1 {
        return items.iter().map(|i| job(*i)).collect();
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let slots: Vec<std::sync::Mutex<Option<Result<T, String>>>> = items.iter().map(|_| std::sync::Mutex::new(None)).collect();
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if k >= items.len() {
                    break;
                }
                let r = job(items[k]);
                let failed = r.is_err();
                *slots[k].lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
                if failed {
                    // Al primo errore gli altri si fermano: non serve finire una serie che non si salva.
                    next.store(items.len(), std::sync::atomic::Ordering::Relaxed);
                    break;
                }
            });
        }
    });
    let all: Vec<Option<Result<T, String>>> = slots.into_iter().map(|s| s.into_inner().unwrap_or_else(|e| e.into_inner())).collect();
    if let Some(Some(Err(e))) = all.iter().find(|r| matches!(r, Some(Err(_)))) {
        return Err(e.clone());
    }
    all.into_iter().map(|r| r.unwrap_or_else(|| Err("conversione interrotta".into()))).collect()
}

#[cfg(test)]
mod prove {
    use super::super::{perform_with, Exists, Options};
    use crate::dicom::prove::ct;
    use crate::dicom::{tag, DicomFile, Element, Syntax, Value, PIXEL_DATA};
    use std::path::PathBuf;

    fn scratch() -> PathBuf {
        // Un numero per prova: con il solo orologio due prove in parallelo potevano avere la stessa cartella
        // e una la cancellava sotto l'altra («file non trovato» in CI).
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let k = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("cedro-dicom-{}-{k}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Una TC di tre immagini 4×4 (−1000, 0, 400 HU…), con il proprietario e la clinica dentro.
    fn series(dir: &std::path::Path) -> PathBuf {
        let vals: Vec<i16> = (0..16).map(|i| -1000 + i * 100).collect();
        let mut f = DicomFile::read(&ct(4, 4, &vals, Syntax::ExplicitLE)).unwrap();
        let one = f.get(PIXEL_DATA).unwrap().bytes().to_vec();
        let three = [one.clone(), one.clone(), one].concat();
        f.body.retain(|e| e.tag != PIXEL_DATA);
        f.body.push(Element { tag: tag(0x0028, 0x0008), vr: *b"IS", value: Value::Bytes(b"3 ".to_vec()) });
        f.body.push(Element { tag: PIXEL_DATA, vr: *b"OW", value: Value::Bytes(three) });
        f.body.sort_by_key(|e| e.tag);
        let p = dir.join("Rossi_Fido_TC.dcm");
        std::fs::write(&p, f.write()).unwrap();
        p
    }

    fn opts() -> Options {
        Options { exists: Exists::Number, ..Options::default() }
    }

    #[test]
    fn una_serie_diventa_tre_png_con_il_codice_al_posto_del_nome() {
        let dir = scratch();
        let src = series(&dir);
        let out = perform_with(&src, "png", 90, Some(&dir), &opts()).unwrap();
        let names: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".png")).collect();
        assert_eq!(names.len(), 3, "{names:?}");
        assert!(names.iter().all(|n| n.starts_with("CED-") && !n.contains("Rossi")), "{names:?}");
        assert!(out.message.contains("3 PNG"), "{}", out.message);
        let img = image::open(&out.path).unwrap();
        assert_eq!((img.width(), img.height()), (4, 4));
        // Finestra del file 40/400: −1000 è nero, 400 HU e oltre bianchi.
        let g = img.to_luma8();
        assert_eq!(g.as_raw()[0], 0);
        assert_eq!(g.as_raw()[15], 255);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn finestra_osso_sedici_bit_e_pdf() {
        let dir = scratch();
        let src = series(&dir);
        let mut o = opts();
        o.dicom_window = 2;
        o.dicom_all_frames = false;
        o.dicom_anon = false;
        let out = perform_with(&src, "jpeg", 90, Some(&dir), &o).unwrap();
        assert!(out.path.file_name().unwrap().to_string_lossy().starts_with("Rossi_Fido_TC"));
        assert!(out.message.contains("finestra osso"), "{}", out.message);
        o.dicom_16bit = true;
        let out = perform_with(&src, "png", 90, Some(&dir), &o).unwrap();
        let img = image::open(&out.path).unwrap().to_luma16();
        assert_eq!(img.as_raw()[0], 32768 - 1000);
        assert!(out.message.contains("16 bit"), "{}", out.message);
        o.dicom_all_frames = true;
        let out = perform_with(&src, "pdf", 90, Some(&dir), &o).unwrap();
        let pdf = std::fs::read(&out.path).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        assert!(String::from_utf8_lossy(&pdf).contains("/Count 3"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn le_radiografie_per_il_proprietario_hanno_il_logo_che_poi_si_ricorda() {
        let dir = scratch();
        let src = series(&dir);
        let logo = dir.join("logo.png");
        image::RgbImage::from_pixel(60, 30, image::Rgb([0, 120, 200])).save(&logo).unwrap();
        let salvato = dir.join("config").join("logo-clinica.png");
        let mut o = opts();
        o.rx_clinica = "Clinica Veterinaria Estense".into();
        let out = super::super::rx_in(&[src.clone(), logo], Some(&dir), &o, &salvato).unwrap();
        assert_eq!(out.path.file_name().unwrap(), "fido-radiografie.pdf");
        assert!(out.message.contains("3 pagine") && out.message.contains("logo"), "{}", out.message);
        let pdf = std::fs::read(&out.path).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        assert!(salvato.exists());
        // La volta dopo basta il DICOM: il logo è quello ricordato.
        let again = super::super::rx_in(&[src], Some(&dir), &o, &salvato).unwrap();
        assert!(again.message.contains("con il logo"), "{}", again.message);
        assert!(super::super::rx_in(&[dir.join("logo.png")], Some(&dir), &o, &salvato).is_err());
    }

    #[test]
    fn togli_dati_nascosti_fa_un_dicom_anonimo() {
        let dir = scratch();
        let src = series(&dir);
        let out = perform_with(&src, "strip", 90, Some(&dir), &opts()).unwrap();
        assert_eq!(out.path.extension().unwrap(), "dcm");
        let bytes = std::fs::read(&out.path).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        for name in ["Rossi", "Fido", "San Rocco", "MC380260000123456", "Bianchi"] {
            assert!(!text.contains(name), "è rimasto «{name}»");
        }
        assert!(text.contains("Labrador"));
        let f = DicomFile::read(&bytes).unwrap();
        assert_eq!(crate::dicom::pixel::Info::of(&f).unwrap().frames, 3);
        assert!(out.message.contains("proprietario"), "{}", out.message);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn togli_dati_nascosti_non_mette_il_nome_nel_file_anche_senza_anonimizza() {
        let dir = scratch();
        let src = series(&dir);
        let mut o = opts();
        o.dicom_anon = false;
        let out = perform_with(&src, "strip", 90, Some(&dir), &o).unwrap();
        let name = out.path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("CED-") && !name.contains("Rossi"), "{name}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn una_serie_incompleta_si_converte_e_lo_si_dice() {
        // Il file dice 5 immagini ma ne contiene 3: prima la serie falliva tutta alla quarta.
        let dir = scratch();
        let src = series(&dir);
        let mut f = DicomFile::open(&src).unwrap();
        f.body.retain(|e| e.tag != tag(0x0028, 0x0008));
        f.body.push(Element { tag: tag(0x0028, 0x0008), vr: *b"IS", value: Value::Bytes(b"5 ".to_vec()) });
        f.body.sort_by_key(|e| e.tag);
        std::fs::write(&src, f.write()).unwrap();
        let out = perform_with(&src, "png", 90, Some(&dir), &opts()).unwrap();
        assert!(out.message.contains("3 PNG") && out.message.contains("incompleto"), "{}", out.message);
        std::fs::remove_dir_all(&dir).ok();
    }
}
