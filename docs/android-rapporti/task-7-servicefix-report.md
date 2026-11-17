# Task 7 — correzione lifecycle ServizioLavoro

## Ambito e stato

Sottoattività sorgente sulla base `8e98780`, ramo `codex/android-allinea-0.53`.
Stato: **DONE_WITH_CONCERNS**. Correzione Java implementata e verificata staticamente;
NON è un PASS del gate fisico Task 7. Build, installazione, prova Mi 9T, review
indipendente e pubblicazione restano al controller. Nessun Rust, cargo, rustc,
formatter, accesso remoto o adb eseguito da questo agente.

## Evidenza e diagnosi

Crash registrato nel PID 20708, 17 novembre 02:38:43:
`android.app.RemoteServiceException: Context.startForegroundService() did not then call Service.startForeground()`
per `it.cedro.android/.ServizioLavoro`.
Fonte locale:
`/home/accorsiluca/Documenti/cedro-android-anteprima/prove-mi9t/anteprima-4c67b2c/service-crash.txt`.
Il controller ha riprodotto il problema aprendo `dialogo.wav` via ACTION_VIEW
dopo la fine del player FLEURS/Rivedi; nessuna conversione attiva avviata prima
dell'importazione.

Il confine osservabile nel sorgente è il seguente: Rust invia
`startForegroundService` con titolo/testo/percento e usa `stopService` quando
`in_corso()` torna a zero. Java prima promuoveva esclusivamente in
`onStartCommand`, dopo la preparazione del WakeLock; il ramo `ferma` usciva
senza promozione. `onCreate` non soddisfaceva il contratto foreground.
Un lavoro breve può pertanto far incrociare avvio e arresto mentre la consegna
dei callback Java è ancora in coda. L'interleaving esatto del crash NON è
ricostruibile dal solo estratto AndroidRuntime: questa è l'ipotesi di race
supportata dal percorso di codice, non una prova temporale completa.

La documentazione Android conferma il timeout di promozione e richiede che il
servizio appena creato chiami startForeground entro pochi secondi:
[Android Developers — Troubleshoot foreground services](https://developer.android.com/develop/background-work/services/fgs/troubleshooting#internal-exception).

## Modifica

Un solo file di produzione: `android/java/it/cedro/android/ServizioLavoro.java`.

- Dopo la creazione del canale, `onCreate` pubblica subito una notifica
  indeterminata con titolo `Cedro`, prima di preparare il WakeLock. Non acquisisce
  il lock. La promozione non dipende più dall'arrivo di `onStartCommand`.
- Il metodo privato `aggiornaNotifica` riutilizza esattamente builder, ID,
  PendingIntent, icona e selezione API/type dataSync già presenti; i comandi
  ordinari sostituiscono subito titolo/testo/percento con i valori reali Rust.
  Il lock di un'ora viene acquisito solo dopo l'aggiornamento del comando reale.
- `null` e `ferma` terminano senza acquisire il lock; `stopSelf(id)` evita che
  uno stop relativo a un comando vecchio annulli un avvio con ID più recente.
- Cleanup comune e idempotente: rilascio del lock se posseduto e
  `stopForeground(true)` sia nel ramo stop sia in `onDestroy`. Anche lo stop
  esterno Rust passa da `onDestroy`, rimuovendo esplicitamente la notifica.

Nessuna nuova stringa traducibile (solo il marchio Cedro già presente), nessuna
modifica a manifest, permessi, dipendenze, avvio automatico, Rust, player/audio
oppure report di altri task. `START_NOT_STICKY` e durata del lock restano invariati.

## Verifiche eseguite

Controllo statico prima della modifica, con Python 3 sul file letto dal disco:

```python
from pathlib import Path
s = Path('android/java/it/cedro/android/ServizioLavoro.java').read_text()
create = s.split('public void onCreate()', 1)[1].split('@Override', 1)[0]
assert 'aggiornaNotifica("Cedro", "", -1)' in create, 'RED: onCreate non promuove il servizio prima dei comandi start/stop'
```

Output atteso e ottenuto (exit 1):
`AssertionError: RED: onCreate non promuove il servizio prima dei comandi start/stop`.
Questo rileva la mancanza del presidio nel sorgente, non riproduce la race Android.

Dopo la modifica: `git diff --check`, exit 0, nessun output. Lettura completa del
diff e controllo Python delle seguenti invarianti del sorgente, exit 0:

```text
PASS: onCreate promuove prima della preparazione del WakeLock
PASS: onCreate non acquisisce il WakeLock
PASS: null/ferma terminano senza acquisire il WakeLock
PASS: titolo/testo/percento del comando aggiornano la notifica prima del lock
PASS: promozione preserva rami API e tipo dataSync
PASS: cleanup idempotente rilascia lock e rimuove notifica
PASS: onDestroy effettua cleanup prima di super
7/7 invarianti statiche; nessuna esecuzione Android o compilazione
```

Le verifiche statiche NON equivalgono a un test Java/Android, a una build, né a
un ciclo TDD comportamentale. Non è stato creato un test che simula il framework
Android e non sono stati dichiarati PASS runtime.

## Self-review e limiti

Diff riletto integralmente: promozione iniziale raggiungibile senza Intent e
senza lock, campi Rust conservati, ramo stop e distruzione idempotenti, nessuna
acquisizione su null/ferma, nessun file estraneo incluso nel commit. L'API
`startForeground` mantiene i rami >=29 e precedenti compatibili con minSdk 26.

La correzione protegge la finestra tra creazione e consegna/arresto del servizio;
non può risolvere un eventuale blocco del main thread antecedente persino a
`onCreate`. L'estratto log non permette di escludere tale ritardo. Una eventuale
riproduzione dopo questa correzione richiede log lifecycle temporizzati e
analisi del main thread; non giustifica un PASS presunto.

Al controller: ricompilare/ripacchettare la parte Java, ripetere l'apertura WAV
originale e più cicli rapidi avvia/termina lavoro, verificare assenza di crash,
notifica e lock residui a fine lavoro; eseguire almeno un lavoro reale passando
in background e verificare avanzamento/output e pulizia finale. Le prove fisiche
non sono state eseguite da questo agente.
