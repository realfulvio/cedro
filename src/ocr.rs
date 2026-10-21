use std::path::Path;
use std::sync::OnceLock;

use image::DynamicImage;
use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use rten::Model;

const DETECTION: &[u8] = include_bytes!("../models/text-detection.onnx");
const RECOGNITION: &[u8] = include_bytes!("../models/text-recognition.onnx");

pub fn image_text(path: &Path) -> Result<String, String> {
    let image = image::open(path).map_err(|e| format!("immagine non letta: {e}"))?;
    recognize(&image)
}

pub fn pdf_pages_text(pdf: &[u8]) -> Result<String, String> {
    let rendered = render_pdf(pdf)?;
    if rendered.is_empty() {
        return Err("il PDF non ha pagine".into());
    }
    let mut body = String::new();
    let mut any = false;
    for (i, png) in rendered.iter().enumerate() {
        if i > 0 {
            body.push_str("\n\n");
        }
        body.push_str(&format!("--- pagina {} ---\n", i + 1));
        let image = image::load_from_memory(png).map_err(|e| e.to_string())?;
        match recognize(&image) {
            Ok(text) => {
                any = true;
                body.push_str(&text);
            }
            Err(_) => body.push_str("(nessun testo)"),
        }
    }
    if !any {
        return Err("nessun testo letto nelle pagine".into());
    }
    Ok(body)
}

fn recognize(image: &DynamicImage) -> Result<String, String> {
    let engine = engine()?;
    let rgb = image.to_rgb8();
    let source = ImageSource::from_bytes(rgb.as_raw(), rgb.dimensions()).map_err(|e| e.to_string())?;
    let input = engine.prepare_input(source).map_err(|e| e.to_string())?;
    let text = engine.get_text(&input).map_err(|e| e.to_string())?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        Err("nessun testo riconosciuto".into())
    } else {
        Ok(trimmed.to_string())
    }
}

fn engine() -> Result<&'static OcrEngine, String> {
    static ENGINE: OnceLock<Result<OcrEngine, String>> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let detection = Model::load(DETECTION.to_vec()).map_err(|e| format!("modello OCR: {e}"))?;
            let recognition = Model::load(RECOGNITION.to_vec()).map_err(|e| format!("modello OCR: {e}"))?;
            OcrEngine::new(OcrEngineParams {
                detection_model: Some(detection),
                recognition_model: Some(recognition),
                ..Default::default()
            })
            .map_err(|e| format!("OCR non pronto: {e}"))
        })
        .as_ref()
        .map_err(|e| e.clone())
}

fn render_pdf(bytes: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    use hayro::hayro_interpret::InterpreterSettings;
    use hayro::hayro_syntax::Pdf;
    use hayro::vello_cpu::color::palette::css::WHITE;
    use hayro::{PixmapSettings, RenderCache, RenderSettings, render};

    let pdf = Pdf::new(bytes.to_vec()).map_err(|e| format!("PDF non disegnato: {e:?}"))?;
    let cache = RenderCache::new();
    let interpreter = InterpreterSettings::default();
    let settings = RenderSettings::default();
    let pixmap = PixmapSettings { x_scale: 1.5, y_scale: 1.5, bg_color: WHITE };
    let mut pages = Vec::new();
    for page in pdf.pages().iter().take(20) {
        let rendered = render(page, &cache, &interpreter, &settings, &pixmap);
        pages.push(rendered.into_png().map_err(|e| format!("pagina non disegnata: {e}"))?);
    }
    Ok(pages)
}
