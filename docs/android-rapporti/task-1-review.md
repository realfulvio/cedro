### Spec Compliance

- ❌ Issues found — requisito dei bersagli minimi48dp non soddisfatto dall’editor delle opzioni: `android/src/ui/opzioni.rs:242` e `:249` (Important1).
- ✅ Nel diff Task1 sono presenti le sei attività colorate, tre destinazioni inferiori, catalogo scorribile, gruppi strumenti/Laboratorio, chiusura del foglio e azione fissa: `android/src/ui/casa/telefono.rs:12`, `:60`, `:288`; `android/src/ui/condivisione.rs:15`, `:93`. Nessun cambiamento motore/JNI compare nei14 file del pacchetto.
- ✅ Dati dimostrativi confinati all’esempio host (`android/examples/vista_a.rs:16`); la home e il foglio leggono `recenti`, `casa.coda` e `file` reali (`android/src/ui/casa/telefono.rs:115`, `:252`; `android/src/ui/condivisione.rs:8`). La selezione del foglio aggiorna insieme modo e indice (`android/src/ui.rs:1145`).
- ⚠️ Cannot verify from diff — compilazione e test del commit39622bd, rendering finale IT/DE360×800,360×640,800×400 e conservazione visiva/funzionale dei cinque temi precedenti. Il log disponibile prova soltanto un rendering intermedio terminato correttamente (`task-1-render/render.log:753`); il rapporto dichiara espressamente successive modifiche non renderizzate. I69PNG non certificano il commit finale.
- ⚠️ Restano i gate del controller: check finale, test Android host/UI e i18n, render finale A e temi precedenti, poi APK e flussi Mi9T con prestazioni/RAM/dimensione. Nessun test è stato lanciato dal reviewer, sul notebook o su dev/win-test. FontScale JNI130/200%, workspace Labs/referti e nuovi controlli web/componenti sono rinvii espliciti del brief, non omissioni di Task1.

### Strengths

- `android/src/ui/casa/telefono.rs:288`: il catalogo deriva dalle operazioni reali, mantiene gli indici di dispatch, distingue Labs e percorre tutte le voci, eliminando il precedente sottoinsieme iniziale.
- `android/src/ui/condivisione.rs:31`: mantiene azioni speciali per link, ZIP multipli e firme, invece di sostituirle con azioni dimostrative.
- `android/src/ui/casa/telefono.rs:481` e `:538`: test sorgente con tocchi delle tre destinazioni, raggiungibilità del catalogo e regressioni di selezione/dispatch PDF e AVIF; utili, ma non ancora eseguiti.
- `android/src/ui/casa/telefono.rs:436`: misurazione e disegno condividono il wrapping tradotto; le cinque tabelle linguistiche hanno le aggiunte corrispondenti. Il supporto delle parole lunghe è esplicito, benché manchi la prova finale visiva.

### Issues

#### Critical (Must Fix)

- Nessuna criticità accertata dal diff di Task1.

#### Important (Should Fix)

1. `android/src/ui/opzioni.rs:242` / `:249` — **Larghezza dei gettoni inferiore a48dp.** Il nuovo rettangolo è alto48, ma `tw` resta la larghezza misurata del testo più28, senza limite inferiore. Opzioni reali brevi come «No» e «1» (`src/conversioni.rs:485`, `:606`) producono bersagli più stretti di48dp; il controllo oggi aggiunto ai test copre nav e CTA, non questi gettoni. Portare anche la larghezza ad almeno48dp, rispettando lo spazio disponibile e il wrapping delle righe. È un requisito esplicito di accettazione del task.

#### Minor (Nice to Have)

1. `android/src/ui/opzioni.rs:259` — **Altezza restituita rimasta42dp.** Dopo aver portato i gettoni da42 a48, il metodo continua a comunicare al chiamante una riga finale di42. Aggiornare la misura restituita alla geometria disegnata; altrimenti il layout sottostima di6dp il componente.
2. `android/src/ui/condivisione.rs:18`, `:93` e `android/src/ui/casa/telefono.rs:163` — **Riserva del piede del foglio fissa, azione dinamica.** Il corpo riserva76dp, mentre `azione_fissa_a` può occupare più spazio quando il testo va su più righe. Il normale caso di una riga coincide; con una CTA lunga il piede copre parte del corpo e `scorri_max` usa una vista più alta di quella effettiva. Derivare la clip del corpo dalla stessa misura della CTA, come già fa la pagina attività. Da verificare con il rendering finale e con la scala testo del task6.
3. `task-1-render/render.log:741`, `:744` — **Evidenza di compilazione non priva di warning.** Il log intermedio contiene36 warning di rav1d e un import `mix` inutilizzato. Il diff finale elimina l’import; senza check finale non esiste ancora evidenza pulita per HEAD. I warning vendor precedenti non richiedono modifiche al motore in Task1: registrarli separatamente dalla verifica della UI.

### Rischi controllati e limiti del controllo

- Rischio di troncamento/altezza incoerente del testo tradotto: controllo mirato del contratto `Painter::fit_size`, `text`, `text_wrap`, `wrap_height`, `wrap_lines` in `src/ui/gfx.rs:1307`. Confermato che `text` è monoriga con ellissi, mentre il nuovo helper usa wrapping; non è prova del rendering finale.
- Rischio di CTA del foglio su più righe: controllo mirato delle etichette effettive in `src/ui/voci.rs:100` e ricerca delle definizioni dei cataloghi. Rilevata la divergenza matematica fra riserva fissa e misura dinamica, classificata Minor in assenza di una riproduzione finale.
- Rischio di gettoni brevi: ricerca mirata delle scelte reali in `src/conversioni.rs:485`, `:606`; nessuna scansione generale del motore.
- Confronto con CSS e funzioni home/conversion/jobs/settings/modal/nav dell’HTML approvato, senza stampare gli asset base64. Esaminati `confronto-inizio.png`, `de-360x640-converti.png` e `de-800x400-condividi.png`: confermano l’impianto della revisione intermedia, non gli ultimi ritocchi.
- Il diff completo è stato letto una volta in blocchi sequenziali. Eccezione mirata autorizzata dal controller: `android/src/ui/casa.rs:717–733`, dispatcher non incluso negli hunk, per controllare che il nuovo `casa_cedro` continui a mostrare gli elenchi delle opzioni. Confermata la chiamata `foglio_elenco` dopo il match dei temi; rischio chiuso. Solo ricerca meccanica aggiuntiva dei numeri di riga delle espressioni già lette. Nessun git, cargo, fmt/rustfmt, stage o commit.
- Letti anche `00-CONTESTO-E-VINCOLI.md` e `03-IMPLEMENTA-IL-REWORK-GRAFICO.md` dal percorso esterno fornito dal controller: `/home/accorsiluca/Documenti/cedro-android-anteprima/piano/`. I vincoli correnti del brief prevalgono sui rinvii di integrazione, sullo Xiaomi13 escluso e sulla pubblicazione già autorizzata. La copertura finale393×873/tablet, le misure APK≤base+1MB e fps/avvio entro10% restano gate non verificati del piano complessivo.

### Assessment

**Spec compliance:** ❌ Needs fixes; ⚠️ gate finali non verificati.

**Task quality:** Needs fixes.

**Reasoning:** Il diff realizza l’impianto A e conserva il collegamento a stato e comandi reali entro lo scope UI. La violazione della larghezza minima dei bersagli va corretta; nessuna approvazione finale di compilazione, test o fedeltà visiva può essere tratta da render precedenti agli ultimi ritocchi.
