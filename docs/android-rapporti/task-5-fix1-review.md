# Task 5 — Scoped re-review fix round 1 (`eb033a3..f5dcc48`)

## Finding Verdicts

- **I1 — Il nuovo lettore spinge fuori schermo il comando per avviare il riassunto. — ADDRESSED.** Il corpo della scheda iniziale ora ha una viewport propria, clip e scorrimento calcolato dal contenuto; il comando rimane ancorato a `h - 60`, con altezza 52 dp (`android/src/ui/rivedi.rs:879–907`). Il test `initial_summary_action_stays_visible_and_reachable_on_short_screens` verifica tutte le otto combinazioni richieste di geometria/vista, rettangolo interamente visibile, posizione invariata dopo scroll e tocco reale che produce `Effetto::Riassumi` (`android/src/ui/rivedi.rs:1109`).
- **I2 — Dopo un errore audio, Play o un salto riavvia una riproduzione solo apparente. — ADDRESSED.** Lo stato fallito blocca sia `toggle` sia `seek`, `playing()` resta falso e la ripresa interna rispetta `failed` (`src/ui/player.rs:50`, `src/ui/player.rs:130`, `src/ui/player.rs:156`, `src/ui/player.rs:170`). La transizione `fail_audio` chiude il clock ed è usata dal ramo Android (`src/ui/player.rs:75`, `src/ui/player.rs:262`). Il test esercita quella stessa transizione, Play/seek ripetuti, ripresa interna, fine file e posizione invariata nel tempo (`src/ui/player.rs:404`).

## New Breakage in the Fix Diff

- Nessuna rilevata: 0 Critical, 0 Important, 0 Minor.

## Out-of-Scope Observations

- Nessuna nuova osservazione. I gate fisici e le verifiche Task 5 già aperti restano al controller; questa re-review non li considera superati.

## Verifiche

- Letto una volta il pacchetto completo del solo fix in due sezioni, il rapporto aggiornato e le finding originali. Nessun accesso supplementare ai sorgenti, nessun comando git, test, build, adb, remoto o subagent.
- Il rapporto contiene i comandi e l'output di copertura del controller sul sorgente `f5dcc48`: `ui::player::tests::failed_audio_cannot_resume_or_seek_a_virtual_clock`, **1 passed, 0 failed, 0,02 s**; `ui::rivedi::tests`, **3 passed, 0 failed, 0,51 s**, incluso il nuovo test delle otto combinazioni. Ambiente dichiarato: mini-PC, profilo test opt-level 1, override fdk-aac-sys O3. Non è evidenza di ascolto Android reale o benchmark.

## Verdict

- **Spec Compliance: ✅** per le due finding sottoposte alla re-review; ⚠️ invariati i gate runtime già elencati nella review iniziale.
- **Task quality: Approved** per questo fix round.
- **Fix round: All findings addressed, no new Critical/Important breakage.** 2 ADDRESSED, 0 NOT ADDRESSED.
