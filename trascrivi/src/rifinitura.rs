//! Le regolazioni di whisper che pesano sulla qualità del testo: il prompt iniziale e la frase fantasma
//! che i modelli a volte aggiungono quando l'audio finisce.

/// Il testo d'esempio dato a whisper prima dell'audio, per lingua. Whisper scrive «come il testo che lo precede»:
/// senza un esempio, sul parlato informale italiano esce tutto minuscolo e senza punteggiatura, e senza punti
/// Cedro non trova le frasi né i paragrafi. Il testo è generico (non suggerisce parole) e non finisce con un saluto,
/// che whisper tende poi a ripetere in fondo all'audio.
pub fn initial_prompt(lang: &str) -> Option<&'static str> {
    match lang {
        "it" => Some("Ciao, scusami. Allora, ti spiego: ho provato a chiamare, però non risponde nessuno. Va bene? Ci sentiamo stasera."),
        _ => None,
    }
}

/// I termini dei referti veterinari di anatomia patologica e citologia, scritti come in un referto:
/// whisper li riconosce molto meglio se li ha «già visti» nel testo che precede l'audio.
const VETERINARIA: &str = "Referto istologico veterinario, cane e gatto: mastocitoma cutaneo di grado II secondo Kiupel e Patnaik, \
carcinoma mammario tubulopapillare, linfoma multicentrico, emangiosarcoma splenico, osteosarcoma, melanoma orale, \
sarcoma dei tessuti molli, fibrosarcoma, adenoma perianale, istiocitoma, lipoma, plasmocitoma. Margini chirurgici indenni, \
invasione linfovascolare, conta mitotica 14 in 2,37 mm², anisocariosi, pleomorfismo nucleare, necrosi, Ki-67, \
immunoistochimica, agoaspirato, citologia, linfonodo regionale.";

/// Il glossario chiesto: «veterinaria» diventa la raccolta pronta, le altre parole restano come sono.
fn glossario(g: &str) -> String {
    g.split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| if t.eq_ignore_ascii_case("veterinaria") { VETERINARIA.to_string() } else { t.to_string() })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Il prompt completo: l'esempio della lingua e, se c'è, il glossario. Whisper tiene solo gli ultimi
/// 224 gettoni del prompt, quindi il glossario sta in fondo e il testo totale resta corto.
pub fn prompt(lang: &str, glossario_chiesto: Option<&str>) -> Option<String> {
    let g = glossario_chiesto.map(glossario).filter(|g| !g.is_empty()).map(|g| g.chars().filter(|c| *c != '\0').take(700).collect::<String>());
    match (initial_prompt(lang), g) {
        (Some(p), Some(g)) => Some(format!("{p} {g}")),
        (None, Some(g)) => Some(g),
        (Some(p), None) => Some(p.to_string()),
        (None, None) => None,
    }
}

/// Le frasi che whisper inventa sul silenzio o a fine audio (le ha imparate dai sottotitoli dei video).
const PHANTOMS: &[&str] = &[
    "grazie",
    "grazie a tutti",
    "grazie mille",
    "grazie per la visione",
    "grazie per lattenzione",
    "grazie per lascolto",
    "grazie e alla prossima",
    "ciao a tutti",
    "alla prossima",
];

/// I titoli di coda dei sottotitoli: nessuno li dice mai, si tolgono comunque in fondo.
const CREDITS: &[&str] = &["sottotitoli creati dalla comunità amaraorg", "sottotitoli a cura di qtss", "sottotitoli e revisione a cura di qtss"];

/// Se l'ultima frase è una di quelle inventate: lampo (meno di 0,6 secondi) o negli ultimi 0,3 secondi dell'audio.
/// Un «grazie» vero detto con calma dura di più e resta.
pub fn phantom_tail(text: &str, t0: i64, t1: i64, audio_end: i64) -> bool {
    let words = plain(text);
    let short = t1 - t0 < 60 || t0 >= audio_end - 30;
    CREDITS.contains(&words.as_str()) || (short && is_phantom(&words))
}

/// Solo lettere e spazi, minuscolo: «Grazie a tutti!» → «grazie a tutti».
fn plain(text: &str) -> String {
    let words: String = text
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect();
    words.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_phantom(words: &str) -> bool {
    if words.is_empty() {
        return false;
    }
    // «Grazie, grazie, grazie.»: la stessa parola ripetuta conta come una.
    let first = words.split(' ').next().unwrap_or("");
    let repeated = words.split(' ').all(|w| w == first);
    PHANTOMS.contains(&words) || (repeated && PHANTOMS.contains(&first))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn il_glossario_va_in_fondo_al_prompt() {
        let p = prompt("it", Some("veterinaria, Metacam")).unwrap();
        assert!(p.starts_with("Ciao, scusami.") && p.contains("mastocitoma") && p.ends_with(", Metacam"), "{p}");
        assert_eq!(prompt("en", Some("Metacam")).as_deref(), Some("Metacam"));
        assert_eq!(prompt("en", None), None);
        assert_eq!(prompt("it", Some(" , ")).as_deref(), initial_prompt("it"));
        // Il glossario veterinario resta corto: whisper ne tiene circa 224 gettoni.
        assert!(VETERINARIA.split_whitespace().count() < 90);
    }

    #[test]
    fn italian_has_a_punctuated_prompt_others_none() {
        let p = initial_prompt("it").unwrap();
        assert!(p.contains(',') && p.contains('.') && p.contains('?'));
        assert!(!p.to_lowercase().contains("grazie"));
        assert!(initial_prompt("en").is_none());
    }

    #[test]
    fn short_thanks_at_the_end_is_a_phantom() {
        // Le tre frasi viste nelle prove sul messaggio vocale (in centesimi di secondo).
        assert!(phantom_tail("Grazie.", 5630, 5660, 5665));
        assert!(phantom_tail("Grazie.", 5660, 5660, 5665));
        assert!(phantom_tail("Grazie, grazie, grazie.", 5650, 5700, 5665));
        assert!(phantom_tail("Sottotitoli creati dalla comunità Amara.org", 5000, 5400, 5400));
    }

    #[test]
    fn real_speech_stays() {
        assert!(!phantom_tail("Grazie.", 5000, 5150, 6000));
        assert!(!phantom_tail("Vedi te se riesci.", 5490, 5560, 5565));
        assert!(!phantom_tail("Grazie Marco, ci sentiamo.", 5600, 5650, 5665));
        assert!(!phantom_tail("", 5600, 5650, 5665));
    }
}
