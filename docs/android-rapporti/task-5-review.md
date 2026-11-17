# Task 5 — Review `0a3bca4..eb033a3`

## Spec Compliance

- ❌ Issues found: due problemi Important impediscono l'approvazione: accesso al riassunto nella vista predefinita su schermi bassi (`android/src/ui/rivedi.rs:539`) e ripresa del clock virtuale dopo un errore audio Android (`src/ui/player.rs:251`).
- ⚠️ Cannot verify from diff: benchmark dialogo Mi 9T rispetto a 1000,002 s (+5%), PSS aggregato rispetto a 1.724.202 KiB (+10%), onda entro 1,5 s, frequenza effettiva di rendering, voce reale/FLEURS, ascolto e salti a orecchio, focus/chiamata/cuffie, continuità background, riassunto 4B e confronto col referto desktop. Il rapporto li lascia correttamente aperti (`docs/android-rapporti/task-5-report.md:76`). Il controller deve chiudere questi gate e attestare le suite sull'head finale; i check precedenti non certificano `eb033a3`.

## Strengths

- Detector realmente condiviso, incluso direttamente dal sorgente desktop, prima di diarizzazione e Whisper (`src/whisper_android/mod.rs:13`, `src/whisper_android/mod.rs:130`); fixture sintetiche e prova opt-in di voce reale senza dipendenza CI da file privati (`tests/parlato_android.rs:1`, `trascrivi/src/parlato.rs:137`).
- Richiesta vocale tipizzata e retrocompatibile, con flag referto/glossario e segmenti associati alla sorgente nell'IPC (`android/src/worker.rs:7`, `android/src/worker.rs:170`, `android/src/worker.rs:174`). Le modifiche al passaggio dei figli mantengono il contratto Batch esistente (`android/src/glue.rs:2332`).
- Export unico Android/desktop, inclusi quattro formati e nome bozza/rivisto; persistenza delle conferme legata al contenuto, con regressioni specifiche sulla conferma e correzione parziale di un segmento (`src/ui/review.rs:132`, `src/ui/review.rs:149`, `src/ui/review.rs:685`, `src/ui/review.rs:705`; uso Android `android/src/ui/rivedi.rs:456`).
- Il testo live non è esportabile e l'annullamento non lo rende completo; gli eventi tardivi di un lavoro terminato sono ignorati (`android/src/ui/rivedi.rs:270`, `android/src/ui/rivedi.rs:285`, `android/src/ui/rivedi.rs:452`).

## Issues

### Critical (Must Fix)

- Nessuno rilevato.

### Important (Should Fix)

1. **Il nuovo lettore spinge fuori schermo il comando per avviare il riassunto.** `android/src/ui/rivedi.rs:539` e `android/src/ui/rivedi.rs:569`: in verticale la vista predefinita Onda e testo consuma circa 342 dp prima delle schede; il ramo senza riassunto mantiene una card fissa di 300 dp e il bottone a `y + 234` (`android/src/ui/rivedi.rs:879`, `android/src/ui/rivedi.rs:890`). A 360×640 il bottone è sotto il fondo; anche a 800×360 il fondo del bottone supera la viewport. Questo ramo non applica `rv.scorri` né imposta un intervallo scorribile: trascinare non lo rende raggiungibile. Il riassunto 4B resta nel motore, ma il rework ne nasconde l'avvio in geometrie esplicitamente previste. Rendere scorrevole/adattiva anche la card iniziale, oppure compattare il lettore per le schede secondarie. Estendere la verifica geometrica a Riassunto senza risultato: il test nuovo controlla soltanto Frasi (`android/src/ui/rivedi.rs:1099`).

2. **Dopo un errore audio, Play o un salto riavvia una riproduzione solo apparente.** `src/ui/player.rs:251`: il ramo Android imposta `failed=true`, `loading=false`, mette in pausa e termina il thread. Successivamente il pulsante Play richiama sempre `toggle()` (`android/src/ui/rivedi.rs:403`), e anche frase/onda/salto passano da `suona_da` (`android/src/ui/rivedi.rs:126`). `Player::toggle` non controlla `failed` (`src/ui/player.rs:147`); `Shared::set_paused(false)` avvia nuovamente `clock` quando `loading=false` (`src/ui/player.rs:50`). Non esiste più un thread audio, ma posizione, frase attiva e stato Pausa avanzano come se il suono funzionasse. Questo contraddice la correzione dichiarata e il requisito di comandi reali. Su Android impedire il resume del player fallito oppure ricrearlo con un tentativo audio effettivo; aggiungere una regressione mirata failure → Play/seek che verifichi assenza di avanzamento virtuale.

### Minor (Nice to Have)

- Nessuno distinto dai due problemi sopra.

## Assessment

**Task quality: Needs fixes.** La separazione worker/UI e il riuso del core sono coerenti con il piano; restano due percorsi funzionali introdotti dal rework che non rispettano il comportamento richiesto. Le prove fisiche e sull'head finale restano gate del controller, non risultati deducibili dalla review.

## Controlli e limiti della review

- Letti prima brief, interfacce, vincoli, specifica e rapporto; diff completo letto in sezioni. Il primo output da 1000 righe era troncato: recuperata la porzione omessa in blocchi più piccoli, senza rigenerare il diff.
- Rischio nominato **geometria del riassunto dopo l'aumento del lettore**: il relativo hunk interrompeva `riassunto` prima del ramo iniziale e del suo bottone; letta solo la continuazione `android/src/ui/rivedi.rs:875–965` per verificare posizione e scorrimento.
- Rischio nominato **contratto del nuovo stato audio failed e della pausa**: gli hunk di `Player` interrompevano i metodi di stato/ripresa e `audio_thread`; lette solo le sezioni necessarie di `src/ui/player.rs:30–85`, `108–125`, `130–246` e dei chiamanti `android/src/ui/rivedi.rs:400–422`.
- Rischio nominato **conferme persistite ed export dopo correzioni**: l'hunk di `Review::persist` non includeva `is_open`/`flag_of`; letta la sezione `src/ui/review.rs:205–315` e la continuazione del consumer del riassunto `android/src/ui/rivedi.rs:350–377`.
- Nessun test, build, accesso remoto, adb, comando git o modifica ai sorgenti. Nessun subagent. Nessuna nuova revisione dei difetti Task 6 già registrati (SAF NDPI EACCES e vecchia navbar dopo importazione).
