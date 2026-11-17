### Finding Verdicts

- **I1 — Recupero batch e ricevute escluse dal mtime — ADDRESSED.** `android/src/web.rs:159` introduce il recupero esplicito delle directory dei figli dal manifest, verificando la chiave stabile. `android/src/glue.rs:1796` lo chiama con la riconciliazione condivisa `recupera_registro` e rimuove i figli dalla coda generale separata. Non dipende più dal cutoff temporale. Il ramo `.pronti` tardivo a `android/src/glue.rs:2311` costruisce e accoda sempre un completamento o un errore esplicito; `android/src/ui.rs:98` conserva uscite, messaggio e sorgente. Il test `ui::web::tests::recupero_batch_con_mtime_riscritto_e_ricevuta_pronta` (`android/src/ui/web.rs:137`) usa pubblicazione legacy reale, snapshot precedente al completamento, mtime successivo al cutoff e verifica ripartenza del solo secondo figlio, ricevuta corretta e doppia consegna.

- **I2 — SkipExisting riutilizza omonimi incompatibili — ADDRESSED.** `android/src/web.rs:272` ricava `mode` soltanto da `Options.exists`; il toggle continua a controllare il riuso delle ricevute compatibili tramite `salta`. La firma non corrispondente torna quindi alla policy scelta dall’utente, normalmente numerazione. Il test `web::salvataggio_tests::richieste_diverse_con_stesso_nome_pubblicano_i_propri_byte` (`android/src/web.rs:329`) pubblica realmente 480p, 1080p e un altro link omonimo, controlla tre percorsi distinti e i byte preservati, e verifica separatamente Replace/Skip espliciti. Il contratto delle ricevute identiche non è stato modificato dal fix.

- **I3 — Opzioni inattive nel generico → Testo — ADDRESSED.** `android/src/worker.rs:98` prepara l’audio mediante `cedro::perform_with`, applicando formato scelto, qualità 95/82/55 e normalizzazione; conserva il risultato in `.revisione` e registra il marker. La copia delle Options neutralizza naming/caso e conflitti esclusivamente per questa preparazione, preservando le Options originali per la trascrizione. `android/src/worker.rs:158` collega l’helper al ramo `text` effettivo. Il test `worker::tests::audio_web_testo_preparato_rispetta_formato_qualita_volume` (`android/src/worker.rs:310`) verifica PCM normalizzato/non normalizzato, MP3 decodificabili di dimensioni diverse, marker, nome caso e sopravvivenza dell’audio al cleanup dello scratch.

- **I4 — Annullamento Maven con socket aperto — ADDRESSED nel perimetro concordato.** `src/youtube/range.rs:8` limita connessione e durata globale della richiesta a 2 s; `src/youtube/range.rs:34` verifica il token prima/dopo HEAD, e `src/youtube/range.rs:79` lo verifica dopo gli errori GET/read prima dei retry. Le attese verificano il token ogni 100 ms (`src/youtube/range.rs:12`), senza thread di download staccati. Il prefisso già ricevuto determina il nuovo Range e azzera il conteggio degli errori senza progresso. Il modulo è usato dal percorso Android reale (`src/youtube.rs:1311`) e dai quattro test host: stallo HEAD, headers GET, body e nove interruzioni consecutive con avanzamento. La limitazione del resolver DNS nativo resta distinta, dichiarata e accettata dal controller; non si estende il claim ≤5 s al DNS o al telefono non misurati.

### New Breakage in the Fix Diff

- **None.** Non identificati nuovi difetti Critical, Important o Minor nel diff `46ec8ea..db3a1a5`. L’estrazione della riconciliazione delle ricevute conserva le transizioni precedenti; l’agent desktop generale rimane invariato, mentre il nuovo agent riguarda Maven Android/test.

### Verification Evidence

- **Checked:** letti il fix diff completo una volta, il ledger I1–I4 e gli aggiornamenti finali del rapporto. Gli aggiornamenti contengono nomi dei test di regressione, comandi effettivi ed estratti di output, riferiti dal controller alla copia immutabile **db3a1a5**.
- **Reported PASS sul candidato:** Android **40/40** sia con sia senza YouTube, inclusi i tre test I1–I3; Maven Range **4/4**, compresi i tre stalli con ritorno misurato <5 s; fake yt-dlp/429 **1/1**; core i18n **5/5**. Core test compilation, NDK all-targets con/senza YouTube e host Android all-targets risultano PASS nel rapporto. Il PASS Maven successivo supera esplicitamente il precedente stato PENDING nell’appendice, senza attribuirlo al checkpoint precedente.
- **Checked:** le regressioni nuove esercitano filesystem, pubblicazione, trasformazione audio e socket loopback reali; non si limitano a verificare snapshot o estensioni. Nessun test, cargo/Rust, adb, comando remoto o comando git rieseguito dal reviewer. Nessun controllo aggiuntivo nel codice invariato è stato necessario. Unico file scritto: questo rapporto.

### Out-of-Scope Observations

- **M2 deferred:** warning `function_casts_as_integer` del signal handler assegnato dal controller al Task6; non modificato dal fix e non riaperto in questo round.
- **Gates still pending:** integrazione i18n/rendering e prove sul Mi 9T rimangono sotto il controller. I risultati host non attestano risoluzioni effettive sul telefono, MediaStore reale, notifica in background, primo uso isolato, ripresa Wi-Fi, consumo CPU/RAM o comportamento dei file corrotti. Nessuna cancellazione dei dati personali è necessaria o autorizzata per chiuderli.
- **DNS boundary:** il limite dichiarato della risoluzione DNS nativa rimane un vincolo conosciuto, non una nuova regressione introdotta dal fix. Nessun claim di annullamento end-to-end ≤5 s fuori dai tre stalli di socket coperti.

### Verdict

**Fix round: All findings addressed, no new Critical/Important breakage.** I1, I2, I3 e I4 chiusi per questa review; 4 ADDRESSED, 0 NOT ADDRESSED, 0 nuovi finding.

**Spec compliance: ✅ per il perimetro del fix; ⚠️ accettazione runtime complessiva ancora da completare.**

**Task quality: Approved per questo fix round.** Il verdetto non costituisce autorizzazione tecnica alla pubblicazione prima dei gate runtime/render ancora aperti.
