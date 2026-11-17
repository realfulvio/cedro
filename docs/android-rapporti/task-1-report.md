# Task 1 — UI Android A

Commit: `39622bd` — Porta su Android la grafica A e il foglio file completo.

Stato: DONE_WITH_CONCERNS — implementazione completa; gate finale di compilazione/test/render e collaudo telefono aperto per l’esclusiva mini-PC concessa all’altra attività.

Approvazione: il controller ha trasmesso l'approvazione utente della variante A di `docs/android-anteprime/rework/index.html` e l'autorizzazione a completamento/release senza altre pause. Questa consegna non pubblica: riguarda la UI prima dei task di integrazione e del collaudo finale.

## Mappa dei componenti

- `ui/casa/telefono.rs`: sostituita la vista delle quattro tinte con A. Home a sei attività, tre destinazioni, scheda file compatta, catalogo intero raggruppato, controlli della coda visibili, azione fissa. Componenti riusabili di sezioni, tessere, righe di opzioni, azione fissa; ritorno a capo anche nelle parole lunghe.
- `ui/casa.rs`: stato/navigazione conservati; ingresso Labs `Nav(6)`/`Casa.laboratorio`; selezione lingua/tema; azioni PDF dedicate integrate nel catalogo senza cambiare gli indici precedenti.
- `ui/condivisione.rs`: nuovo foglio reale, preview, catalogo completo, strumenti/Labs separati, chiusura esplicita. Conserva ZIP multipli, firme e link.
- `ui.rs`: instradamento A; selezione formato del foglio atomica (`FoglioFormato(modo,indice)`); altre viste e azioni precedenti conservate; avanzamenti indeterminati fermi con animazioni disattivate.
- `ui/tinte.rs`: superfici e selezioni allineate ai colori dell'HTML per quattro tinte; nessun tema precedente eliminato.
- `ui/opzioni.rs`: editor comprende gli strumenti, evitando che selezionare opzioni di Mitosi torni al primo formato; dispatch distingue conversione/strumento. Bersagli gettoni 48 dp.
- `ui/rivedi.rs`: resta il lettore/revisore reale; aggiunte etichette Number/Meaning richieste dal motore0.53, che bloccavano già la compilazione della base.
- `ui/pdf.rs`, `ui/firma.rs`, `ui/m3.rs`, `ui/anim.rs`: restano, con palette condivisa delle tinte e navigazione precedente. Nessuna funzione rimossa.
- `android/examples/vista_a.rs`: fixture solo nell'esempio host, matrice IT/DE,360×800,360×640,800×400,11 scene e quattro tinte.
- Traduzioni: nuove stringhe in en/de/es/fr/pt; i metodi Painter passano già da `tr()`; nomi utente protetti con `lit()`.

## Interfacce per i task seguenti

- Labs: `CasaAz::Nav(6)`, `Casa.laboratorio`, `catalogo_a` con gruppo `voci::is_lab`; ora apre e seleziona azioni Labs effettive del catalogo. Estendere questo ingresso con workspace/casi/SlideInfo/DicomInfo reali. Non esistono pulsanti demo o falsi casi.
- Componenti/download: nessuna riga con valori fittizi. Inserire la riga di stato reale nella vista lavori o nel corpo attività tramite stato fornito dal glue; task successivo deve definirne progressi, errori e riprova.
- Revisione referti: il revisore esistente resta collegato ai lavori; etichette Number/Meaning già compilabili. Comandi nuovi vanno legati allo stato reale della revisione.
- Font di sistema: il rendering usa la scelta `grandi` già esistente (+2px), con altezze misurate. Il controller ha confermato che0.45 ignora il fontScale Android (100/200% identici): lettura JNI e scala130/200 sono del task6, non modificabili qui senza uscire dallo scope glue/gfx.
- Animazioni: `anim.calmo` resta letto dal sistema nel glue; nessuna nuova animazione continua. Corrette anche le due barre indeterminate che prima avanzavano nonostante `calmo`.

## Verifiche

- Mini-PC: `/srv/dev/projects/cedro-android-fase1`, sincronizzati solo i file di questo task; nessun cargo sul notebook, nessuna modifica a main remoto, chiavi, firewall o servizi.
- Primo `cargo check -p cedro-android --release`: PASS (1m47s), prima delle ultime rifiniture; il controllo finale è da registrare.
- `cargo run -p cedro-android --release --example vista_a -- /srv/dev/scratch/cedro-ui-a`: PASS, compilazione+link8m27s,69 PNG prodotti. Log copiato in `task-1-render/render.log`. Non è una prova del commit finale: durante/dopo questa compilazione sono state rifinite traduzioni dei gruppi, stato opzioni/condivisione, testo selezione, centratura, testata62dp, nav66dp, colonne/altezza del foglio e fixture fotografica.
- Tempi host di69 disegni: min2,36ms, mediana8,09ms, max149,71ms (prima home/cache font fredda); tutti gli altri ≤17,81ms. Non equivalgono a fps Android e non dimostrano il limite+10% sul telefono.
- Confronti affiancati HTML/Rust salvati in `task-1-render/confronto-{inizio,converti,condividi,impostazioni}.png`. Matrice8×8 dei colori dominanti: home63/64 celle identiche; Converti52/64; Condividi28/64; Impostazioni50/64. La metrica verifica soprattutto fondo/disposizione, NON fedeltà pixel per pixel. JSON completo `confronto-colori-8x8.json`.
- Auto-review visiva dei PNG: home italiana vicina alla proposta; il primo rendering mostrava titoli tedeschi spezzati e formati/CTA allineati a sinistra. Corretto localmente adattando il corpo dei titoli tradotti e centrando testo/icone; testata/nav portate alle altezze osservate. Il foglio iniziale aveva tre colonne e partiva troppo in alto: corretto localmente a due colonne e90% dell’altezza. Queste correzioni **non sono ancora ri-renderizzate**.
- La fotografia dell’esempio iniziale era `docs/hero.png`; l’esempio finale usa `assets/cedro/immagini.jpg`, SHA-256 identico alla foto incorporata nell’HTML approvato (`f8b3f9dcc7267f352bf3f8272babed2d332e567b495a99a4450a95dad897b498`), e selezionaPDF per confronto equivalente. I dati di prova restano soltanto nell’esempio.
- `git diff --check`: PASS finale. Verifica locale struttura delle5 tabelle TSV: PASS.
- `cargo test -p cedro-android --release --lib`: **NON ESEGUITO**. Test sorgente aggiunti per catalogo completo da `android/tests/azioni.txt`, navigazione attraverso tocchi reali sul modello UI, CTA sopra la nav, Labs, ritorno Video→Home→file, opzioni strumenti e dispatch sharePDF/AVIF.
- Cargo check finale, test i18n Rust, esempio `vista` dei temi precedenti e nuovo rendering del commit: **NON ESEGUITI**. Il controller ha ordinato di lasciare finire solo il cargo corrente e poi cedere dev/win-test all’esclusiva TracciaDoc. Nessun nuovo carico è stato avviato dopo la fine; nessun cargo sul notebook.
- APK/RAM/fps/Mi9T: da eseguire dopo integrazione nel flusso del controller; non sono stati dichiarati verificati da questo task. Xiaomi13 escluso su istruzione corrente.

## Self-review

- La scelta del foglio aggiorna insieme modo/indice e disattiva il vecchio `foglio`: il pulsante legge la stessa azione della tessera. Prova regressione Image→PDF e Image→AVIF.
- Unisci/Dividi/Pagine/Comprimi non dipendono più dal sottoinsieme `conversioni::tools(Pdf)`, che li esclude per convenzione desktop.
- Labs senza file non abilita l'azione; con file seleziona un'azione Labs realmente visibile. Tornare alla home da Video poi aprire un file ripristina Converti.
- Il catalogo è interamente scorribile; azione principale/nav restano fuori dalla clip del contenuto. Layout a due colonne in orizzontale.

## Differenze deliberate dai dati dimostrativi e gate di accettazione

L'app usa file/coda/stati reali: nessuna trascrizione «pronta», caso, download o percentuale inventata. I recenti mostrano i file realmente registrati; il foglio espone anche tutti i formati del motore scorrendo, oltre ai sei iniziali dell'HTML; Labs mostra solo le azioni già disponibili, senza fingere workspace/casi/referti futuri. Seleziona/rimuovi file restano visibili come pulsanti48dp, comandi necessari per non perdere la gestione della coda.

Prima dell'APK/release il controller deve sincronizzare il commit finale e completare **nell'ordine**:

1. `cargo check -p cedro-android --release`;
2. `cargo test -p cedro-android --release --lib` e verifica i18n;
3. `cargo run -p cedro-android --release --example vista_a -- /srv/dev/scratch/cedro-ui-a-finale` e rendering `vista` dei temi precedenti;
4. revisione dei PNG finali IT/DE,360×640/800,800×400, confronto aggiornato; verifica font130/200 dopo il task6;
5. APK e collaudo Mi9T con tutti i flussi, misuraRAM/dimensioneAPK/fps/avvio. Nessuna pubblicazione prima di questi gate.

Il checkout remoto non deve essere considerato già sincronizzato al commit finale: contiene la revisione della build terminata e può richiedere gli ultimi aggiornamenti UI/TSV. Gli output già copiati sono prove della revisione intermedia dichiarata, non sostituiscono il gate finale.

## Fix round 1/5 — review statica su base39622bd

Commit fix: `a0740da`.

- **Important1 corretto:** larghezza dei gettoni `max(TOCCO).min(w)`, altezza `TOCCO`; le scelte brevi «No», «1» e «Sì» hanno bersagli48×48dp negli spazi disponibili di almeno48dp. Il wrapping usa già la larghezza vincolata, senza debordare dal contenitore.
- **Minor1 corretto:** l'altezza restituita termina al bordo inferiore dei gettoni48dp, anziché alla vecchia riga42dp. Il gruppo vuoto restituisce0.
- **Minor2 corretto:** `misure_azione_a` è l'unica formula condivisa fra pagina attività, foglio Condividi e disegno della CTA. L'etichetta del foglio viene risolta prima della clip; viewport e `scorri_max` riservano l'intero piede dinamico anche con testo su più righe.
- **Minor3 tracciato:** il log intermedio contiene36 warning vendor `rav1d`; fuori scope correggere codice vendor in questo task. L'import inutilizzato `mix` in `tinte.rs` è già rimosso nel commit39622bd. Non si dichiara il commit finale privo di warning senza un nuovo check.

Copertura sorgente aggiunta:

- `ui::opzioni::tests::gettoni_brevi_rispettano_bersagli_righe_e_altezza`: tre etichette reali brevi, contenitori48/104/328dp, hitbox minime, wrapping senza sovrapposizione né debordamento, altezza restituita uguale all'ingombro delle hitbox, gruppo vuoto.
- `ui::casa::telefono::tests::foglio_con_azione_multilinea_non_copre_opzioni_o_tocchi`: foglio reale Archive→Documenti inPDF, larghezza effettiva180dp e caratteri grandi; CTA effettivamente multilinea, scorrimento fino in fondo, ultima opzione interamente visibile sopra il piede, nessun formato sovrapposto al piede, tocco della CTA che restituisce il lavoro `z-pdf`.

Comando locale eseguito: `git diff --check` → exit0, nessun output. Auto-review statica del diff e dei contratti `gettoni`, `qualita_serve`, `uses_quality`, geometria/footer: completata.

Comando richiesto appena l'ambiente sarà disponibile: `cargo test -p cedro-android --release --lib` → **NON ESEGUITO**, dev/win-test ancora in esclusiva TracciaDoc. Anche cargo check finale e nuovo rendering sono **NON ESEGUITI**. Nessun Rust eseguito sul notebook, nessun fmt/rustfmt, nessun carico remoto avviato. Restano tutti i gate pre-APK/release già elencati sopra.
