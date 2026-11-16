# Task 3 — Office, formati e Laboratorio Android

Stato: **DONE_WITH_CONCERNS — implementazione e verifiche statiche; gate Rust/Android/telefono ancora aperti**.
Ramo `codex/android-allinea-0.53`, base di review `89c422f2a65c0257a77a6bbf7ebb17a5056cb5dc`. Durante il lavoro il controller ha committato solo `docs/android-rapporti/02-base.md`; quel file, `PROMPT-CODEX-tracciadoc.md` e `foto-riga/` non sono stati modificati né inclusi da questo agente.

Letti brief, global-constraints, task-3-interfaces, 00-CONTESTO, 04, 07, 08, 09, HTML variante A (Labs/DICOM/caso), API effettive Task2 e motore condiviso. Nessun subagent, push, tag, release, adb, cargo, rustfmt, accesso dev/win-test da questo agente. Lease rilasciato a fine lavoro, ma il controller mantiene il controllo esclusivo dei carichi remoti, con benchmark altrui da preservare.

## Implementato

### Office, cataloghi e opzioni

- `righe()` generato una volta dai cataloghi del motore, con le sole azioni Android aggiuntive PDF/firma/trascrizione. DOCX/ODT/RTF/DOC hanno le uscite effettive PDF/DOCX/ODT/HTML/MD/testo e strip; tutti i formati/strumenti rimangono nella vista Completa.
- UI A: Semplice/Completa visibili, famiglie immagine derivate da `voci::family` (Comuni/Moderni/Tecnici/Professionali/Documento), colori del motore, strumenti a due colonne, descrizione della scelta corrente. Labs mostra strumenti e formati pertinenti; il catalogo Completa e Altre opzioni restano disponibili.
- `android/src/labs.rs::defs` collega `slide_option_defs`, `dicom_option_defs`, `micro_option_defs`, `sheet_option_defs` agli editor esistenti: OfficePage A4/A3/Letter/come file, stampa fogli, livello export, etichetta/macro, DICOM serie/16bit/anonimizzazione/finestra, canali/Z/istanti/scala/contrasto/16bit, figura, tessere, clinica RX. Il PageSize testuale non viene proposto al posto della carta Office nei fogli.
- MIME/manifest/SAF Office/HEIC provengono dal Task2; nessuna duplicazione. Preview Office/HEIC continua a usare i lettori condivisi della sorgente. Correzione Android-only font: ricerca nelle tre directory di sistema Android e fallback Roboto/Noto/Droid corrispondente alla famiglia; nessun nuovo font incorporato e nessuna modifica alla scelta font desktop.

### Anteprima di laboratorio reale

- Moduli focalizzati `android/src/labs.rs` e `android/src/ui/labs.rs`. Nessun dato demo nell’app.
- Apertura di file Labs condivisi instradata al Laboratorio; file multipli selezionabili come sorgente dell’anteprima. Vetrini/DICOM, Apri file, Apri cartella MRXS, caso, anteprima, strumenti e quaderno sono comandi visibili.
- Preview in **processo worker separato**, con permit risorse, lease SAF pertinenti ereditate, identità richiesta, annullamento process-group; il cambio selezione scarta risultati vecchi e cancella il worker precedente. Uscire dalla pagina cancella l’anteprima in corso. PNG/metadata temporanei non vengono pubblicati e sono eliminati dalla guardia dello staging.
- SVS/NDPI/MRXS: panoramica tramite lettore condiviso; selezione del livello reale legge solo un’area centrale fino a1024×1024 con `read_region`. La UI lo dichiara: non promette una panoramica piena a risoluzione massima.
- CZI/ND2/LIF: `Pila::open_serie`, piano/canale/Z/istante reali, compositore condiviso. DICOM: frame e finestra effettivi tramite `dicom::pixel`; nessun identificativo paziente nella descrizione sintetica della UI.
- I selettori sono esplicitamente **solo anteprima**. Le scelte di export conservano il contratto del desktop: tutte le scene CZI/LIF e modalità canali/Z/istanti del motore. Nessuna interpretazione clinica.

### Strumenti, casi e risultati

- Figura e RX aggregano i file selezionati (pannelli/logo), invece di inviare tanti lavori singoli. BagIt usa `package_with`, include tutti i file selezionati anche di categorie diverse, include la cartella associata MRXS e produce ZIP+SHA256, con pubblicazione strutturata Task2.
- Caso/campione: codice e nota privati, usa/togli, anteprima del nome, persistenza atomica nello stesso `config_dir()/lab/caso.txt` del motore; scrittore fuori UI serializzato, ultima modifica prevalente, errore salvataggio mostrato nei Lavori.
- `caso::opzioni` imposta `%data%_%caso%_%nome%` salvo modello personalizzato; `%caso%` tra i gettoni impostazioni. Nota esclusa da Options/nome/journal lavori. Esportazioni testo/rapporti via Scrivi/ScriviPoi usano il codice e il modello corrente; conversione e trascrizione ricevono Options senza perderle nel glue.
- Il caso del lavoro è uno snapshot, conservato nel journal UI e nell’identità publication con campo serde default per compatibilità. Cambiare caso non riattribuisce i lavori vecchi. Filtri «Lavori del caso», «Tutti i lavori», copia odierna per quaderno con caso e nomi, solo su comando esplicito.
- Messaggi del motore nei risultati su più righe, senza taglio alla prima riga, copia integrale. Gli avvisi RTL Office, allegati PDF e nota non diagnostica vengono evidenziati. Dividi/Pagine mostrano prima dell’operazione che gli allegati rimangono nell’originale.
- Firma: verificato staticamente che il viewer usa già `Report.level`, `Signature.level`, `Check.level` e `Level::Warn` in arancio; rapporto prodotto dal motore condiviso. Non reinventata la verifica SHA1/post-modifica. Confronto fixture desktop/Android ancora da eseguire.
- Testi aggiunti nelle cinque tabelle en/de/es/fr/pt. Il renderer `vista_a` aggiunge scene livello SVS, CZI, DICOM sintetico realmente decodificato, caso e Labs a caratteri grandi in IT/DE/PT, tre viewport. Fixture sintetica solo in `android/tests/support/dicom.rs`, inclusa da test/esempio, non dall’app di produzione.

## Correzioni condivise, autorizzate dal controller

1. `src/lib.rs`: reexport di `rx_with`, già usato dal desktop; nessun algoritmo nuovo.
2. `src/imgfmt.rs`, `src/ufficio/font.rs`: directory e fallback font solo sotto cfg Android; comportamento desktop invariato.
3. `src/engine_svslab.rs`: i nomi diretti di copia anonima MRXS e namespace tessere ignoravano Options.name_template/caso. Ora usano il `templated_stem` esistente, una sola volta; header/cartella rimangono accoppiati e il basename tessere alimenta anche il CSV. Collisioni continuano a produrre gruppi distinti; nessuna riscrittura a posteriori dei riferimenti.
4. `src/mrxs.rs`: la copia anonima ignorava alias a file (`file_type().is_file()`), quindi perdeva i dati SAF grandi. Copia ricorsiva dei percorsi relativi, alias ammessi solo a file regolari; directory symlink/non-regolari rifiutati. Lo sbiancamento dell’etichetta mantiene il percorso relativo completo con controllo di contenimento, invece di appiattirlo al basename. Cleanup originale su fallimento conservato.

Queste ultime due correzioni richiedono regressione host/desktop, non soltanto Android. Fixture MIRAX esistente resa accessibile solo nel cfg(test) per riusarla senza duplicare il generatore.

Dipendenze: `image`0.25 reso diretto nel crate Android con default-features=false, già nel lock e nel motore; serve a ridurre il composito prima di trasferirlo alla UI. Nessun nuovo pacchetto/versione/codec o font; licenza già presente nei crediti. Variazione APK **non misurata**.

## Verifiche eseguite realmente

- Tree-sitter-rust0.24.2/tree-sitter0.26.1 tramite Python3.13 già disponibile, parsing di27 file (tutti android/src, esempio e fixture nuovi, cinque sorgenti condivisi modificati): **nessun nodo ERROR/missing**. Non verifica tipi, borrow checker, JNI, linking o runtime.
- Python tomllib: Cargo.toml/Cargo.lock/android/Cargo.toml validi. XML manifest valido.
- TSV en/de/es/fr/pt: nessuna chiave duplicata o traduzione vuota; aggiunte le stringhe UI nuove senza sovrascrivere traduzioni preesistenti.
- `bash -n android/build.sh` => exit0.
- `git diff --check` => exit0.

Output ottenuto:

```
Rust syntax: 27 files; errors: []
TOML: 3 valid; XML: valid
TSV: 5 languages, no duplicates or empty translations
```

Il controller ha comunicato durante il lavoro: CI Android f19eb91 SUCCESS (run43953629576) e test host acc2b39 **16/17 riusciti**, uno fallito solo per `55.99997 !=56.0` nella misura f32 del tocco. Questi risultati **non sono prove di Task3**. Su sua istruzione, il test `foglio_con_azione_multilinea_non_copre_opzioni_o_tocchi` ora usa tolleranza0.001dp per quella sola uguaglianza, mantenendo le altre verifiche di raggiungibilità/CTA. Log controller: `/srv/dev/scratch/cedro-android-acc2b39-test.log`. Nessun layout cambiato per soddisfare il test.

## Test aggiunti — NON ESEGUITI da questo agente

- `labs::tests::opzioni_contestuali_ufficio_e_laboratorio`: definizioni corrette per DOCX/XLS/NDPI/DICOM/CZI, nessun duplicato, roundtrip selezione.
- `labs::tests::anteprima_fixture_svs_czi_e_livello_reale`: fixture reali repository, massimo1024, metadati, errore livello inesistente.
- `labs::tests::anteprima_dicom_cambia_frame_e_finestra`: DICOM sintetico2frame, confronto pixel cambiando frame/finestra, indice inesistente.
- `ui::labs::tests`: snapshot caso/nota esclusa/riavvio/quaderno, selettori preview distinti da export e rifiuto eventi vecchi, raggiungibilità scroll a360dp/caratteri grandi, catalogo completo contro motore, avvisi, pacchetto misto immagine+dati.
- `worker::tests::copia_anonima_micro_non_legge_il_file_e_pacchetto_contiene_i_dati`: errore non supportato prima dell’apertura CZI/ND2/LIF, BagIt e verifica impronte/sidecar.
- Core `caso_nei_gruppi_mirax_e_tessere_con_collisioni`: due copie MRXS con stesso caso, collisioni, riapertura pixel identici; due dataset tiles, namespace/basename con caso una volta, nomi CSV risolti e immagini riaperte.
- Core `copia_anonima_con_alias_fd_e_dati_annidati` (Linux): alias reale `/proc/self/fd`, dati annidati, copia materiale e riapertura pixel; directory symlink rifiutata senza parziali.

## Gate e comandi NON ESEGUITI per questo commit

Il controller sincronizza il commit nel PC di collaudo e gestisce i carichi (-j2/nice) senza toccare il benchmark altrui. Nessun comando Rust sul notebook.

```
cargo test --release -p cedro-android --lib -j2
cargo test --release -p cedro caso_nei_gruppi_mirax_e_tessere_con_collisioni -j2
cargo test --release -p cedro copia_anonima_con_alias_fd_e_dati_annidati -j2
cargo test --release -p cedro --test i18n_inglese -j2
cargo test --release -p cedro -j2
cargo ndk -t arm64-v8a -P 26 check -p cedro-android --all-targets -j2
cargo run --release -p cedro-android --example vista_a -- /tmp/vista-a-task3
android/build.sh
CEDRO_YOUTUBE=1 android/build.sh
tools/regressione-cli.sh
```

Sul Mi9T, esecuzione riservata al controller: Office5DOCX reali con confronto desktop±1pagina e font; Word100p/XLSX20fogli50000righe; HEIC12/48MP (i sintetici del corpus non verificano P3/rotazioni iPhone); DOC malevolo; banco rovinati con zero crash/ANR e RAM/CPU misurate; fixture firma SHA1/post-modifica; PDF metadati/allegati; DICOM anonimizzato rispetto desktop; SVS/NDPI≥1GB/MRXS565MB/CZI/ND2/LIF preview ed export, no copia cache per sorgente seekable; worker cancel; caso persistenza e nomi, quaderno, filtro lavori; screenshot IT/DE/PT e font grandi; tempo/PSS UI+worker/dimensione APK. Nessuna di queste misure è stata inventata o chiamata PASS.

## Self-review e concerns concreti

- Corrette durante self-review perdita Options nei rami trascrizione/web, esclusione silenziosa delle altre categorie nel pacchetto, copia MRXS che ignorava fd, nomi gruppo senza caso, risultato preview vecchio applicato al nuovo file, testo avvisi troncato, sfondo caso che restava aperto dalla navigazione, menu opzioni fogli duplicato, padding f32 nel test del Task1.
- Type-check/build/JNI/linking e test nuovi **ancora non eseguiti**: parsing non equivale a compilazione. Preview launcher/cancel/fd necessita prova sul dispositivo.
- Limite del core concordato: nessuna copia anonima nativa CZI/ND2/LIF. Il fallback strip del motore leggerebbe l’intero file per poi produrre PNG; Android restituisce errore contestuale prima della lettura. Conversione PNG ordinaria disponibile, mai dichiarata anonimia verificata. Tiles/mitosi/consult richiedono SVS/NDPI/MRXS; gli altri input ricevono errore esplicito del motore.
- I selettori scena/canale/Z non filtrano l’export: limite API condivisa esplicitato in UI e README. La preview di livello vetrino è l’area centrale, non un visualizzatore navigabile di ogni regione. Eventuale necessità di esportare una selezione arbitraria richiede una nuova API comune, non un algoritmo Android parallelo.
- DICOM mantiene `DicomFile::open` con lettura completa; microscopia può allocare un piano/composito completo prima della riduzione. Il worker isola e cancella ma non dimostra sufficienza RAM: misure obbligatorie, nessun limite arbitrario imposto. Originale firma staccata resta Vec fallibile fuori UI, come Task2.
- I download web in passthrough/YouTube ordinario non usano ancora il modello nomi con caso: è un’integrazione del successivo Task4, ora documentata; le Options sono già propagate fino ai worker e alla trascrizione. I testi/rapporti e conversioni trattati da Task3 applicano il caso.
- Il campo caso del journal è compatibile in lettura con i lavori precedenti (vuoto se assente); persistenza reale/crash injection ancora da provare. Il core caso.txt non memorizza la nota nei report o nelle impronte.
- Font fallback Android misurato solo dopo build; nessuna promessa di identiche metriche Word/desktop. Nessun font aggiunto senza misura/licenza.
- Nessun risultato clinico e nessuna nuova rete nel percorso Labs. Privacy anonimia va verificata sui file reali; pixel impressi richiedono controllo visivo.

## File del task

`android/src/{labs.rs,ui/labs.rs,lib.rs,glue.rs,worker.rs,publication.rs,ui.rs,ui/casa.rs,ui/casa/telefono.rs,ui/opzioni.rs,ui/pdf.rs}`; `android/examples/vista_a.rs`; `android/tests/support/dicom.rs`; `android/README.md`; `android/Cargo.toml`, Cargo.lock; `src/{lib.rs,imgfmt.rs,engine_svslab.rs,mrxs.rs,ufficio/font.rs}`; i18n cinque TSV; questo rapporto. Nessun file estraneo nello stage.
