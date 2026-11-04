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
