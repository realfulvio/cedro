//! Il piano del riassunto: le istruzioni per il modello (in italiano) e come dividere una riunione lunga.
//!
//! La prova su WS-ACCLUC (3 novembre 2026, Qwen3.5-4B) ha deciso il metodo: fino a qualche migliaio di
//! token il testo si legge in una volta sola, ed è più fedele sui dettagli; oltre, letto tutto insieme,
//! un modello da 4 miliardi confonde chi ha detto cosa. A pezzi da circa 4000 token (appunti per ogni
//! pezzo, poi il riassunto dagli appunti) le attribuzioni tornano giuste. Qui c'è solo logica pura,
//! provata dai test: il modello e i token stanno in `main.rs`.

/// Oltre questi token la trascrizione non si legge più in una volta sola.
pub const SINGLE_MAX: usize = 6000;
/// I token di trascrizione per ogni pezzo.
pub const PART_MAX: usize = 4000;
/// Il contesto del modello: basta per un pezzo o per una lettura sola, con le istruzioni e la risposta.
pub const CONTEXT: u32 = 8192;
/// Quanto può scrivere al massimo: gli appunti di un pezzo e il riassunto finale.
pub const NOTES_TOKENS: usize = 700;
pub const SUMMARY_TOKENS: usize = 1200;

/// Le regole per il riassunto finale: i quattro titoli che Cedro riconosce, sempre gli stessi.
const SUMMARY_RULES: &str = "Scrivi il riassunto SOLO con quello che c'è nel testo, senza inventare nulla e senza aggiungere commenti. \
Usa esattamente questi quattro titoli, in quest'ordine:\n\
## In breve\n\
Da 3 a 5 frasi: di cosa si è parlato e come è finita.\n\
## Chi ha detto cosa\n\
Una riga per persona, così: «- Nome: cosa ha detto, chiesto o proposto». Usa i nomi esattamente come sono scritti nel testo \
e attribuisci a ognuno solo quello che ha detto lui.\n\
## Decisioni\n\
Una riga «- …» per ogni decisione presa davvero. Una proposta, un'ipotesi o un'intenzione non è una decisione. \
Se non ce ne sono scrivi solo «- Nessuna».\n\
## Da fare\n\
Una riga «- cosa fare (Nome di chi lo fa)» per ogni compito assegnato o preso. Tra parentesi va la persona che lo farà \
(chi se l'è preso o a chi è stato affidato), non chi lo chiede. Se non si sa chi lo fa, non mettere le parentesi. \
Se non ce ne sono scrivi solo «- Nessuno».\n\
Non aggiungere motivi, cause o dettagli che nel testo non ci sono. \
Scrivi in italiano semplice e corretto, con numeri, date e nomi esatti, senza ripetere le stesse frasi.";

/// Le istruzioni quando la trascrizione si legge in una volta sola.
pub fn system_single() -> String {
    format!(
        "Sei un segretario preciso che scrive in italiano il riassunto di una riunione. Riceverai la trascrizione: \
ogni riga ha l'orario tra parentesi quadre, il nome di chi parla e cosa dice.\n{SUMMARY_RULES}"
    )
}

/// Le istruzioni per gli appunti di un pezzo.
pub fn system_notes() -> String {
    "Sei un segretario preciso che prende appunti in italiano. Riceverai un pezzo della trascrizione di una riunione: \
ogni riga ha l'orario tra parentesi quadre, il nome di chi parla e cosa dice.\n\
Scrivi appunti fedeli, SOLO con quello che c'è in questo pezzo:\n\
- una riga per ogni persona che parla, così: «- Nome: cosa dice», con numeri, date e nomi esatti;\n\
- poi, se ci sono, righe «DECISIONE: …» per le decisioni prese davvero (una proposta o un'intenzione non è una decisione);\n\
- e righe «DA FARE: cosa fare (Nome di chi lo fa)» per i compiti assegnati: tra parentesi chi lo farà, non chi lo chiede.\n\
Usa i nomi esattamente come sono scritti nel testo e attribuisci a ognuno solo quello che ha detto lui. \
Niente introduzioni né commenti."
        .to_string()
}

/// Le istruzioni per il riassunto finale dagli appunti dei pezzi.
pub fn system_from_notes() -> String {
    format!(
        "Sei un segretario preciso che scrive in italiano il riassunto di una riunione. Riceverai gli appunti presi pezzo \
per pezzo, in ordine di tempo: ogni riga dice chi ha parlato e cosa ha detto, più le decisioni e i compiti trovati.\n\
In «Chi ha detto cosa» metti ogni persona una volta sola, unendo quello che ha detto in tutti i pezzi. \
In «Decisioni» riporta TUTTE le righe DECISIONE degli appunti e in «Da fare» TUTTE le righe DA FARE, \
con lo stesso nome tra parentesi: non saltarne nessuna.\n{SUMMARY_RULES}"
    )
}

/// Le istruzioni per stringere gli appunti, quando una riunione lunghissima ne dà troppi per una lettura sola.
pub fn system_merge() -> String {
    "Sei un segretario preciso. Riceverai gli appunti di una parte di una riunione, presi pezzo per pezzo e in ordine di tempo. \
Riscrivili più corti nello stesso formato: righe «- Nome: cosa dice» (una per persona, unendo quello che ha detto), \
righe «DECISIONE: …» e righe «DA FARE: cosa fare (Nome)». Tieni tutte le decisioni, tutti i compiti, i numeri, le date \
e i nomi esatti; togli solo le ripetizioni. Non inventare nulla. Niente introduzioni né commenti."
        .to_string()
}

/// Il testo dell'utente per una lettura sola o per un pezzo.
pub fn user_transcript(lines: &[String], part: Option<(usize, usize)>) -> String {
    let head = match part {
        Some((i, n)) => format!("Pezzo {} di {} della trascrizione:\n\n", i + 1, n),
        None => "Trascrizione della riunione:\n\n".to_string(),
    };
    let mut out = head;
    for line in lines {
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Il testo dell'utente per il riassunto dagli appunti.
pub fn user_notes(notes: &[String]) -> String {
    let mut out = String::from("Appunti della riunione, pezzo per pezzo:\n");
    for (i, n) in notes.iter().enumerate() {
        out.push_str(&format!("\n### Pezzo {}\n{}\n", i + 1, n.trim()));
    }
    out
}

/// Il prompt nel formato di Qwen (ChatML), con il ragionamento spento: il blocco `<think>` vuoto dice al
/// modello di rispondere subito (come `enable_thinking=false` del suo modello di chat).
pub fn chatml(system: &str, user: &str) -> String {
    format!("<|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{user}<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n")
}

/// Divide le righe in gruppi consecutivi di al massimo `max` token ciascuno (`cost` dà i token di una riga).
/// Una riga più lunga di `max` resta da sola: non si taglia mai una frase a metà. Un ultimo gruppo
/// troppo piccolo (meno di un quarto) si unisce al precedente se ci sta, per non avere un pezzo quasi vuoto.
pub fn split(costs: &[usize], max: usize) -> Vec<std::ops::Range<usize>> {
    let mut parts = Vec::new();
    let (mut start, mut sum) = (0, 0);
    for (i, &c) in costs.iter().enumerate() {
        if i > start && sum + c > max {
            parts.push(start..i);
            start = i;
            sum = 0;
        }
        sum += c;
    }
    if start < costs.len() {
        parts.push(start..costs.len());
    }
    if parts.len() >= 2 {
        let last = parts[parts.len() - 1].clone();
        let prev = parts[parts.len() - 2].clone();
        let last_cost: usize = costs[last.clone()].iter().sum();
        let prev_cost: usize = costs[prev.clone()].iter().sum();
        if last_cost * 4 < max && prev_cost + last_cost <= max + max / 4 {
            parts.pop();
            *parts.last_mut().expect("due pezzi") = prev.start..last.end;
        }
    }
    parts
}

/// Pulisce la risposta del modello: niente blocchi `<think>` rimasti, niente spazi in testa e in coda.
pub fn clean(text: &str) -> String {
    let mut t = text;
    if let Some(end) = t.find("</think>") {
        t = &t[end + "</think>".len()..];
    }
    t.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_meetings_stay_in_one_piece() {
        assert_eq!(split(&[100, 200, 300], 4000), vec![0..3]);
        assert!(split(&[], 4000).is_empty());
    }

    #[test]
    fn long_meetings_are_cut_between_lines() {
        let costs = vec![1000; 10];
        let parts = split(&costs, 4000);
        assert_eq!(parts, vec![0..4, 4..8, 8..10]);
        // Ogni riga c'è una volta sola, in ordine.
        let all: Vec<usize> = parts.iter().flat_map(|r| r.clone()).collect();
        assert_eq!(all, (0..10).collect::<Vec<_>>());
    }

    #[test]
    fn a_tiny_last_piece_joins_the_one_before() {
        // 4000 + 500: il pezzo da 500 è meno di un quarto e ci sta col precedente (entro 5000).
        assert_eq!(split(&[2000, 2000, 500], 4000), vec![0..3]);
        // Ma non se il precedente diventa troppo grande.
        assert_eq!(split(&[4000, 1500], 4000), vec![0..1, 1..2]);
    }

    #[test]
    fn a_huge_line_stays_alone() {
        assert_eq!(split(&[100, 9000, 100], 4000), vec![0..1, 1..2, 2..3]);
    }

    #[test]
    fn prompt_turns_thinking_off_and_keeps_the_four_titles() {
        let p = chatml(&system_single(), &user_transcript(&["[00:00:04] Sindaco: Buonasera.".into()], None));
        assert!(p.ends_with("<|im_start|>assistant\n<think>\n\n</think>\n\n"));
        for t in ["## In breve", "## Chi ha detto cosa", "## Decisioni", "## Da fare"] {
            assert!(p.contains(t), "manca {t}");
        }
        assert!(p.contains("[00:00:04] Sindaco: Buonasera."));
        assert!(user_transcript(&[], Some((1, 3))).starts_with("Pezzo 2 di 3"));
        assert!(user_notes(&["- Sindaco: apre.".into()]).contains("### Pezzo 1\n- Sindaco: apre."));
        assert!(system_merge().contains("DA FARE"));
    }

    #[test]
    fn leftover_thinking_is_removed() {
        assert_eq!(clean("<think>bla</think>\n\n## In breve\nx"), "## In breve\nx");
        assert_eq!(clean("  ## In breve\nx \n"), "## In breve\nx");
    }
}
