# Review Task8 — db3a1a5..0a3bca4

## Spec Compliance

- ✅ Conforme per quanto verificabile staticamente. Il desktop espone i cinque temi richiesti (`src/ui/theme.rs:30`), risolve ogni chiave ritirata a Melanzana (`src/ui/theme.rs:71`) e normalizza i percorsi effettivi di lettura e scrittura (`src/settings.rs:417`, `src/settings.rs:430`). La normalizzazione cambia soltanto `ui_theme` (`src/settings.rs:406`); la vecchia migrazione che modificava la vista viene rimossa.
- ✅ Bondeno non ha più un flag indipendente dalla risoluzione desktop (`src/edizione.rs:17`, `src/edizione.rs:25`). Android conserva un catalogo esplicito di nove temi e un parser indipendente dal target host (`src/ui/theme.rs:40`, `src/ui/theme.rs:76`, `android/src/ui.rs:47`, `android/src/glue.rs:1720`).
- ✅ Il diff resta limitato a temi, impostazioni, esempi e documentazione pertinente; nessuna modifica a workflow, manifest, versioni, palette conservate o motori. Le descrizioni del desktop vengono aggiornate nei README e nella guida (`README.md:52`, `README.en.md:52`, `src/ui/guida.rs:114`), con traduzioni nelle cinque lingue.
- ⚠️ Non verificabile dal diff: compilazione ed esecuzione dei test, avvio e salvataggio effettivi sulle piattaforme, interazione mouse/tastiera delle finestre Linux/Windows, resa dei font/titoli/stemmi dopo migrazione e persistenza sul telefono. I test sono dichiarati NON ESEGUITI in `task-8-report.md:18`; il controller deve completare i gate elencati in `task-8-report.md:54` prima della release. Nessuna prova runtime è stata eseguita da questa review.

## Strengths

- I parser desktop e Android sono distinti senza dipendere dal sistema che compila il renderer, evitando la regressione specifica degli esempi Android su Linux (`src/ui/theme.rs:71`, `src/ui/theme.rs:76`, `android/examples/vista.rs:147`).
- Le prove aggiunte esercitano file temporanei, tutti i temi ritirati, entrambe le viste, schemi precedenti, BOM/UTF-16, chiavi sconosciute e idempotenza (`src/settings.rs:591`, `src/settings.rs:618`, `src/settings.rs:637`). La regressione Android attraversa selezione UI, effetto di salvataggio, rilettura e disegno (`android/src/ui.rs:2446`).
- Il marker viene consumato solo dopo il salvataggio riuscito; il test dell'errore usa un fallimento reale del filesystem (`src/settings.rs:394`, `src/settings.rs:713`).

## Issues

### Critical (Must Fix)

- Nessuna.

### Important (Should Fix)

- Nessuna.

### Minor (Nice to Have)

- `src/settings.rs:386`, `src/settings.rs:400`: su una cartella nuova con entrambi i marker `da-bondeno` e `da-labs`, il primo passaggio crea `config.txt` con la vista Semplice; il passaggio Labs considera quindi le impostazioni già esistenti e non applica la vista Completa prevista per una nuova installazione Labs. Il tema finale è correttamente Labs e nessuna preferenza preesistente viene persa, quindi l'impatto è limitato al valore iniziale della vista in questa combinazione. Conservare l'informazione sull'assenza iniziale della configurazione prima di elaborare i due marker, oppure risolvere prima il marker vincente. Aggiungere al test `src/settings.rs:695` il caso con entrambi i marker e nessuna configurazione: attualmente verifica entrambi solo con configurazione esistente, e Labs da solo su installazione nuova.

## Focused checks outside the diff

- Rischio: un ingresso desktop alternativo conserva i temi ritirati nonostante il catalogo ristretto. Controllati i riferimenti a `ThemeId::from_key`/`THEMES` nei sorgenti desktop e i punti di creazione delle pagine: `src/ui/screen.rs:100`, `src/linux_settings.rs:563`, `src/settings_win.rs:466`, `src/aspetto_win.rs:75`. Passano dal parser/catalogo condiviso; nessun bypass trovato.
- Rischio: la contrazione del parser condiviso altera il renderer Android compilato su Linux. Controllati gli ingressi di selezione/rilettura e font in `android/src/glue.rs:1720`, `android/src/glue.rs:2394`, `android/examples/vista.rs:74`: tema Android esplicito e font del tema selezionato. Nessun ulteriore bypass trovato.
- Rischio: il nuovo salvataggio automatico della migrazione perde chiavi sconosciute o interrompe la scrittura della configurazione. Letto il solo confine di persistenza non mostrato interamente dal diff, `Settings::save_to` in `src/settings.rs:340`: rilegge il testo precedente, usa `merge_into`, sincronizza un file temporaneo e lo rinomina. Il diff corregge inoltre il BOM prima del filtraggio (`src/settings.rs:327`). Nessuna riscrittura produttiva o esecuzione di test effettuata.
- Lettura del pacchetto diff completata in sezioni per recuperare l'output iniziale troncato; nessun comando git, build, fmt, adb o carico remoto eseguito.

## Assessment

**Task quality: Approved** per la review statica, con una osservazione Minor; restano obbligatori i gate runtime del controller.

**Reasoning:** La separazione dei cataloghi e la normalizzazione nei percorsi reali soddisfano il contratto richiesto e preservano le preferenze. Le prove scritte coprono i principali rischi della migrazione; la combinazione dei due marker su installazione nuova merita una piccola correzione, senza bloccare questa approvazione statica.

**Findings:** 0 Critical, 0 Important, 1 Minor. **Cannot verify:** 1 gruppo di gate runtime/piattaforma.
