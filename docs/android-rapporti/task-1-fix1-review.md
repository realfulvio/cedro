# Task 1 — re-review statica fix round 1

Pacchetto esaminato: `review-39622bd..a0740da.diff`, commit fix `a0740da`. Letti brief, vincoli globali, rapporto aggiornato e review precedente. Ambito limitato ai tre finding richiesti e alle eventuali regressioni introdotte dal solo fix diff.

## Esito dei finding

| Finding | Stato | Evidenza statica |
|---|---|---|
| Important1 — gettoni più stretti di48dp | **ADDRESSED** | `android/src/ui/opzioni.rs:243` applica `max(TOCCO).min(w)` prima del wrapping; il rettangolo usa la stessa larghezza e altezza `TOCCO`. La costante vale48 (`android/src/ui.rs:57`). Nei contenitori disponibili di almeno48dp, anche «No», «1» e «Sì» raggiungono48×48dp senza allargarsi oltre il contenitore. |
| Minor1 — altezza restituita42, disegno48 | **ADDRESSED** | `android/src/ui/opzioni.rs:260` restituisce `cy + TOCCO - y`, coincidente con il bordo inferiore dell'ultima riga disegnata. Il gruppo vuoto restituisce0 (`:241`). |
| Minor2 — piede del foglio fisso76, CTA multilinea dinamica | **ADDRESSED** | L'etichetta effettiva viene risolta prima della clip. `android/src/ui/condivisione.rs:22` misura il piede mediante `misure_azione_a`; `:24` lo sottrae alla viewport usata anche da `scorri_max`. Lo stesso helper è usato dal disegno della CTA (`android/src/ui/casa/telefono.rs:165`) e dalla pagina attività (`:39`). La formula condivisa (`:426`) conserva la precedente geometria della CTA e rende dinamica anche la riserva del foglio. |

## Nuove regressioni del fix diff

- **Critical:** nessuna rilevata staticamente.
- **Important:** nessuna rilevata staticamente.
- **Minor:** nessuna rilevata staticamente.

I test sorgente aggiunti coprono dimensioni e ingombro dei gettoni, wrapping e gruppo vuoto (`android/src/ui/opzioni.rs:467`), nonché scorrimento, separazione fra contenuto e CTA multilinea e dispatch del tocco nel foglio (`android/src/ui/casa/telefono.rs:544`). Sono copertura predisposta, **NON ESEGUITA**: nessun esito PASS è attribuito a questi test.

## Osservazioni esterne, non bloccanti per il fix

Il caso sintetico a180dp esercita il wrapping ma non sostituisce rendering finale IT/DE alle dimensioni richieste né verifica fontScale130/200 sul telefono. Restano valide le limitazioni e i rinvii dichiarati nel rapporto, compresi i warning vendor già tracciati; nessuna nuova richiesta di modifica fuori scope.

## Verdict

**Spec compliance del fix:** i tre finding risultano risolti alla lettura statica. **Quality del fix:** approvabile staticamente, senza ulteriori correzioni richieste in questo round.

L'accettazione complessiva di Task1 e il passaggio ad APK/release restano subordinati ai gate obbligatori del controller: check finale, test Android host/UI e i18n, rendering finale A e temi precedenti, revisione PNG e collaudo Mi9T previsto. I comandi e la copertura elencati nell'appendice fix del rapporto sono dichiarati **NON ESEGUITI**; la presente review non li certifica.

## Metodo e limiti

Diff letto una sola volta. Unica verifica puntuale aggiuntiva: ricerca della definizione di `TOCCO`, necessaria per confermare il minimo48dp, e dei numeri di riga delle espressioni già esaminate. Nessun nuovo git, cargo, fmt/rustfmt, build, test, sorgente o demo; nessun accesso o carico su dev/win-test, ancora riservati all'altra chat. Nessuna operazione sul Mi9T o sui suoi dati. Unico file scritto: questo rapporto.
