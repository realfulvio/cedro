# Task7 — matrice ed evidenze Android realmente disponibili

Audit documentale del17novembre2026. Nessun ADB, test, build o modifica di produzione eseguiti. Letta la matrice originale `~/prove-cedro-android/esiti/2026-11-16-mi9t/esiti.md`: **79ID, tutti ancora Da eseguire**. La matrice è un piano, non un registro aggiornato. Questa appendice la riconcilia con gli artifact presenti senza modificarla e senza trasformare una sottoprova in PASS dell'intero ID.

## Come leggere

- **P**: esiste evidenza del Mi9T per il sottocaso descritto; non certifica tutto l'ID o il nuovo candidato. **P-base** appartiene soltanto all'APK0.45; **P-KO** è evidenza fisica di fallimento.
- **H**: sola prova eseguita host/core/renderer, anche quando pertinente a una parte dell'ID; non è providerJNI, audio o GUI Android reale.
- **N**: non individuato un esito eseguito specifico nelle fonti esaminate. Non significa funzione assente e non viene convertito in NA. Sorgenti/test scritti o fixture copiate non contano come esecuzioni.
- Le categorie combinate esplicitano granularità differenti. Il nuovo APK finale deve essere associato al proprio commit/hash prima di qualsiasi promozione di esito.

## Identità degli APK e delle prove

Radice evidenze fisiche **PROOT**: `~/Documenti/cedro-android-anteprima/prove-mi9t/`. Radice host **HROOT**: `~/Documenti/cedro-android-anteprima/verifiche/`.

| Pacchetto/prova | Identità verificabile | Limite di attribuzione |
| --- | --- | --- |
| Base estratta dal telefono | `PROOT/cedro-0.45.0-installato.apk`, baseline-apk-metadata.txt:0.45.0/4500;58.734.937byte da02-base | SHA sorgente della base **non stabilito**. Non attribuire l'APK4500 a0a3a64b: quello è il checkout iniziale, già incompatibile con il crateAndroid prima dei fix. |
| Anteprima db3a1a5 | `apk/anteprima-db3a1a5.json` dichiara commit e metadata; APK firmato63.089.063B, SHA256 `a25a4a9134859d67285d43470aa2711522be3c4502c9434f9d0a4ce303176ed0`; package.txt aggiornato00:22:42 | Version0.53.0-anteprima/5299. Manifest di build+directory prova attribuiscono db3; non la sola versionCode. |
| Anteprima4c67b2c | APK firmato63.465.895B, SHA256 `67c057968a44401a5f979327cd5988a113434a6c9afc0dad9656d0a8813d0bf5`; package.txt aggiornato01:46:26; reportQA/probe e dir4c | Ancora0.53.0-anteprima/5299. Directory+handoff controller identificano4c, non un numero versione differente. Non c'è un manifestJSON4c equivalente a quello db3. |
| Checkpoint8e98780 | HROOT/8e98780: core/Android/NDK/check/renderer; task-6-report.md registra mirati | **Host**, nonAPK8e installato nelle evidenze esaminate. Eventuali commit più recenti/build in corso non ereditano questi risultati automaticamente. |

Byte e SHA dei due APK candidati sono stati ricalcolati in sola lettura durante questo audit e coincidono con i file indicati. Non è stato confrontato un APK attualmente installato via ADB. `versionCode=5299` è condiviso da due binari differenti.

## Matrice completa

| ID | Prova del piano | Evidenza migliore | Commit/versione effettiva | Cosa è provato e cosa manca |
| --- | --- | --- | --- | --- |
| A1 | Installazione e avvio | P | db3a1a5; 4c67b2c | Package e Home fisici; install/avvio intermedi osservati. Non è installazione del nuovo finale. |
| A2 | Aggiornamento conserva tema/lingua/Lavori | P | 0.45→db3a1a5→4c67b2c | Preferenze-prima JSON e screenshot dopo, vecchi Lavori ancora presenti. Solo tema Cedro/lingua telefono; non verifica tutte le preferenze o un tema alternativo. |
| A3 | Rotazione mantiene stato | H | f5dcc48;8e98780 | Test player/export nelle due geometrie e renderer; nessuna sequenza fisica di rotazione con lavoro/stato preservato. |
| A4 | Tinte e font130/200 | P-KO base / H fix | 0.45/4500;8e98780 host | Font100/200 identici nella base (KO); fix misure font e renderer scala2 host. Manca130/200 fisico sul nuovo APK e tutte le tinte. |
| A5 | Rimuovi animazioni | N | — | Nessun toggle OS Rimuovi animazioni con prima/dopo fisico o test specifico eseguito identificato. |
| A6 | Tre avvii e RAM | P-base | 0.45/4500 | Tre cold start251/213/275ms e un campioneRAM; mancano tre avvii e RAM del nuovo candidato. |
| B1 | Foto condivisa→PDF, Apri e Condividi | P | db3a1a5 | Foto12MP→PDF valido, Apri e chooser Condividi/ritorno a Cedro documentati. Ingresso foto tramite intent di test, non intero flusso da app fotografica; titolo chooser numerico3749 osservato. |
| B2 | PDF da provider | P parziale | db3a1a5 | PDF prodotto condiviso realmente a Cedro dal chooser/provider; non equivale a scegliere un PDF esterno con OPEN_DOCUMENT e provarne gli strumenti. |
| B3 | DOCX da provider | H | 8e98780 | engine::tests::pdf_e_office_da_grant_saf_non_spostano_il_cursore PASS host. Non query/provider JNI reale né DOCX/XLSX scelto sul Mi9T. |
| B4 | XLSX da provider | H | 8e98780 | engine::tests::pdf_e_office_da_grant_saf_non_spostano_il_cursore PASS host. Non query/provider JNI reale né DOCX/XLSX scelto sul Mi9T. |
| B5 | HEIC condiviso | H | 8e98780 | heic_photo_opens_with_its_colors PASS core; nessuna condivisione HEIC fisica documentata. |
| B6 | Audio Opus/M4A | H | 8e98780 | Opus roundtrip core PASS; nessun ingresso Opus/M4A reale sul Mi9T. MP3→WAV della base non sostituisce questi due ingressi. |
| B7 | Video condiviso | P parziale | db3a1a5;0.45 base | Import video/worker e annullamento intermedi, conversione60s480p solo base. Nessun completamento video condiviso sul candidato nuovo. |
| B8 | Link YouTube | P | db3a1a5 | Screenshot Lavori YouTube Me at the zoo-2.mp4 + file reale e ffprobe19s. Un video di base, non tutte le qualità/componenti. |
| B9 | Link web | P | db3a1a5 | W3C trailer tramite SEND text a processo aperto: job36 completo, file valido H264/AAC. Vecchia navbar4 visibile dopo intent: difetto storico, da retest finale. |
| B10 | P7M Apri con | N | — | P7M Apri con fisico non individuato; suite firma core non prova MIME/intent/chooser. |
| B11 | Condivisione multipla | N | — | SEND_MULTIPLE fisico non individuato. Test batch web/pubblicazione non equivalgono a condivisione multipla da altra app. |
| B12 | Nuovo intento app aperta | P | 0.45;db3a1a5 | Intent SEND a processo aperto con immagini/PDF/testo; route vecchia errata osservata. Non prova intent ravvicinati/multipli sul finale. |
| B13 | Nomi spazi/accenti/emoji/lunghi | H | 8e98780 | Test nomi storage e documenti Unicode; nessun lotto fisico completo spazi/accenti/emoji/lunghi. Nome download con spazi da solo insufficiente. |
| B14 | Numera/Sostituisci/Salta | H | 8e98780 | publication legacy_replace_e_cancel e collisioni/storage PASS host; tutte tre policy Number/Replace/Skip fisiche non documentate. |
| C1 | Immagini formati/strumenti/OCR | P parziale | db3a1a5;8e98780 host | Solo fotoJPEG→PDF/OCR fisico; roundtrip immagini e TGA host. Non tutti formati/strumenti/OCR della matrice. |
| C2 | PDF conversioni e strumenti | H | 8e98780 | Core PDF Word/Excel/split/password/metadati ecc. e AndroidUI test; Apri PDF risultato non è esecuzione di questi strumenti sul telefono. |
| C3 | Office5DOCX/DOC/ODT/RTF/XLSX/XLS/ODS | H | 8e98780 | Core document_roundtrip_without_office/document_pdf e ingresso grant PASS. CinqueDOCX+DOC/ODT/RTF/XLSX/XLS/ODS, confronto desktop/pagine/font fisici assenti. |
| C4 | Audio/video/ZIP | P-base / H | 0.45/4500;8e98780 host | Base MP3180s→WAV e video60s→MP4 con output verificati. ZIP e catalogo intero non fisici sul candidato; non promuovere intera riga. |
| C5 | Opzioni cambiano output | H | 8e98780 | Test opzioni e output core/UI; nessun confronto A/B fisico di opzione e output nuovo identificato. |
| C6 | AnnullamentoCPUidle≤5s senza parziali | P parziale | db3a1a5 | Worker video sparisce entro2.171s e web-stallo entro2.014s; log annullato/uscite0. Non CPUidle misurata, né scansione completa parziali in tutti i namespace. |
| C7 | Lavoro in background e notifica | P-KO / incompleto | archivio4c67b2c | FATAL RemoteServiceException ServizioLavoro02:38:43 nell’archivio4c; non una prova background+notifica riuscita. Trigger/correzione richiedono nuovo APK. |
| D1 | Dialogo243.3s, onda e avanzamento | P-base | 0.45/4500 | Dialogo243.3s completo1000.002s,4voci, outputTXT e PSS. FLEURS8s4c non sostituisce benchmark dialogo/onda/avanzamento finale. |
| D2 | Player±10s,velocità,viste | P parziale / H | 4c67b2c;f5dcc48/8e98780 host | Esistono catture audio1x e risultato8s; controlli/player riferiti nei report, ma niente oracle completo ±10s/velocità/viste/ascolto indipendente in questo audit. Host layout/player PASS. |
| D3 | Diarizzazione automatica | P-base | 0.45/4500 | Auto trova4voci; ground truth non verificata. Non chiude diarizzazione candidato, né Esattamente3. |
| D4 | Riassunto senzaOOM | H parziale | f5dcc48;8e98780 | Test raggiungibilità pulsante riassunto e renderer. Nessuna esecuzione modello4B sul Mi9T senzaOOM provata. |
| D5 | Referto bozza e conferme | H | f5dcc48;8e98780 | Scene referto e test layout/righe/font host; nessuna bozza/conferme/export referto fisico completo. |
| D6 | Nessun parlato silenzio/tono/ronzio | P parziale | 4c67b2c | Silenzio30s: UI Nessun parlato, job40 completo1.809s uscite0. Tono/ronzio/rumore solo detector hostf5dcc48; non intera riga fisica. |
| D7 | Rivedi riaperto con audio | P parziale | 4c67b2c | TXT FLEURS e catture audio presenti; coldPlay riferito Task6. Salva/riapri Rivedi con seek/audio corretto dopo process death non dimostrato integralmente. |
| E1 | YouTube componenti e qualità | P parziale | db3a1a5 | Download YouTube19s valido documentato. Installazione iniziale componenti, selezione tutte qualità e rispettivi output non documentati: stato pendente Task4 stale solo per il download base. |
| E2 | Solo audio | H | db3a1a5;8e98780 | Test request/voce/qualità/batch/manifest e renderer web; nessun completamento fisico audio-only/testo/video-leggero/playlist individuato. |
| E3 | Testo | H | db3a1a5;8e98780 | Test request/voce/qualità/batch/manifest e renderer web; nessun completamento fisico audio-only/testo/video-leggero/playlist individuato. |
| E4 | Video leggero | H | db3a1a5;8e98780 | Test request/voce/qualità/batch/manifest e renderer web; nessun completamento fisico audio-only/testo/video-leggero/playlist individuato. |
| E5 | Playlist piccola | H | db3a1a5;8e98780 | Test request/voce/qualità/batch/manifest e renderer web; nessun completamento fisico audio-only/testo/video-leggero/playlist individuato. |
| E6 | Link diretto | P | db3a1a5 | W3C52.208333s,H264864×480+AAC,6248964B,9.777s job; proprietà reali verificate. Non originale byte-identico né prova tutte qualità. |
| E7 | Internet Archive | N | — | InternetArchive e URL non valido non eseguiti fisicamente identificabili; dati di piano/fonti non sono esiti. |
| E8 | Link non valido | N | — | InternetArchive e URL non valido non eseguiti fisicamente identificabili; dati di piano/fonti non sono esiti. |
| E9 | Interruzione rete/ripresa | P parziale / H | db3a1a5 | Cancel socket controllato65536B poi stallo fisico; Range/ripresa solo loopback host4/4. Non spegnimentoWiFi/ripresa reale né DNS/Maven telefono. |
| F1 | P7M valido | H | 8e98780 | Core firma PASS: p7m_valida/pdf_firmato/contenuto_cambiato/pdf_cambiato/tsd_m7m_tsr/rapporto. Non UI telefono, salvataggio/condivisione o avvisi fisici. |
| F2 | PDF firmato | H | 8e98780 | Core firma PASS: p7m_valida/pdf_firmato/contenuto_cambiato/pdf_cambiato/tsd_m7m_tsr/rapporto. Non UI telefono, salvataggio/condivisione o avvisi fisici. |
| F3 | AvvisoSHA1 | N | — | Nessuna prova specifica avvisoSHA1 individuata. Non derivata dal PASS generale della suite firma. |
| F4 | Modifica dopo firma | H | 8e98780 | Core firma PASS: p7m_valida/pdf_firmato/contenuto_cambiato/pdf_cambiato/tsd_m7m_tsr/rapporto. Non UI telefono, salvataggio/condivisione o avvisi fisici. |
| F5 | TSD/TSR | H | 8e98780 | Core firma PASS: p7m_valida/pdf_firmato/contenuto_cambiato/pdf_cambiato/tsd_m7m_tsr/rapporto. Non UI telefono, salvataggio/condivisione o avvisi fisici. |
| F6 | Rapporto salva/condividi | H | 8e98780 | Core firma PASS: p7m_valida/pdf_firmato/contenuto_cambiato/pdf_cambiato/tsd_m7m_tsr/rapporto. Non UI telefono, salvataggio/condivisione o avvisi fisici. |
| G1 | DICOM sintetico | H | af4e692;8e98780 | DICOM sintetico/frame/finestra e anonimizzazione core/host; non selezione/export reale telefono. |
| G2 | ND2 pubblico | H sintetico | 8e98780 | nd2_e_lif_si_convertono_come_gli_ome e parser sintetici PASS; test file_vero ND2/LIF sono ignored. Fixture pubbliche nel corpus non risultano eseguite sul telefono. |
| G3 | LIF pubblico | H sintetico | 8e98780 | nd2_e_lif_si_convertono_come_gli_ome e parser sintetici PASS; test file_vero ND2/LIF sono ignored. Fixture pubbliche nel corpus non risultano eseguite sul telefono. |
| G4 | SVS | H | 8e98780 | Anteprima fixtureSVS e parser/regioni host; nessun SVS fisico completo. |
| G5 | NDPI | P parziale | db3a1a5 KO→4c67b2c PASS preview | SAF reale OS-3.ndpi1.47GB; Panoramica/centro/ritorno con metadati122880×77824 e probe. Export/anonimizzazione/tessere non eseguiti. |
| G6 | MRXS cartella | H | db3a1a5;8e98780 | MRXS copia anonima alias+dati annidati e pubblicazione strutturata host. Albero SAF e campione565MB fisici non provati. |
| G7 | CZI | H sintetico | 8e98780 | CZI sintetico+JPEGXR e preview fixture PASS; czi::prove::file_vero ignored. Nessuna prova fisica. |
| G8 | Caso/campione persistenza | H | 8e98780 | caso_snapshot_nomi_e_quaderno_restano_dopo_riavvio PASS su modello/UIhost; non persistenzareale/clipboardtelefono. |
| G9 | Copia quaderno | H | 8e98780 | caso_snapshot_nomi_e_quaderno_restano_dopo_riavvio PASS su modello/UIhost; non persistenzareale/clipboardtelefono. |
| G10 | File grande/budget | P parziale | 4c67b2c | GrandeNDPI preview riuscito; PSSUI55.6/70.4MB, non piccoUI+worker. Budget/spazioinsufficiente/assenza copia disco non osservati. |
| H1 | Confronto tutte schermateHTMLA | H parziale | af4e692;db3a1a5;f5dcc48;8e98780 | Renderer e campioni visuali confrontati; non tutte schermateHTMLA sul nuovo APK. Gap referto corto rilevato in f5dcc48, fix successivo host. |
| H2 | Quattro tinte e temi precedenti | P parziale / H | db3a1a5/4c67b2c;0a3bca4 host | Cedro/Melanzana visibile e conservato, non tutte9palette fisiche; roundtrip9temiAndroid host0a3bca4 PASS. |
| H3 | Sei lingue e ripiego | H / IT fisico | db3a1a5;8e98780 | Italiano fisico, i18n5lingue e sceneIT/DE/PT host; nessuna scansione6lingue+fallback sul telefono. |
| H4 | Comandi visibili | P parziale | db3a1a5;4c67b2c | Controlli di flussi realmente fotografati; navbarlegacy4 dopo import su db3 osservata. Non prova intero catalogo e nuovi controlli finale. |
| H5 | Target48dp | H | Task1;8e98780 | Test geometrie/target e font host. Nessuna verifica fisica esaustiva48dp. |
| H6 | Barre di sistema | P parziale | db3a1a5;4c67b2c | Status/navigationbar visibili negli screenshot portraitCedro; non tutte tinte/stati/font/orientazioni. |
| H7 | Ampiezza tablet olandscape | H | af4e692;f5dcc48;8e98780 | Landscape800×400 renderer; nessun tablet o landscape fisico verificato. |
| H8 | Barra fissa dopo scroll senza tocchi sui controlli sottostanti | P-KO base / H fix | 0.45/4500;db3a1a5;8e98780 host | Tap barra dopo scroll altera tema nella base; regressione offset/nav anche intermedia. Fix viewport/queue host non prova tocchi reali finale. |
| I1 | Corpus malformati zeroANR | H | 8e98780 | Test malformati specifici PASS host; corpus fisico zeroANR non eseguito. FATAL servizio nell’archivio4c è failure lifecycle distinta, non risultato del corpus. |
| I2 | Coda20misti | H parziale | 8e98780 | Test batch2audio/manifeste/pubblicazione; non20misti reali sul telefono. |
| I3 | Process death nessunlavorofantasma | H | 8e98780 | Journal arresti prima/dopo commit, tombstone e recupero host; nessun force-stop durante lavoro+relaunch fisico con verifica anti-fantasma. |
| I4 | Spazio insufficiente simulato | H | 8e98780 | storage collisioni_budget_e_annullamento e policy spazio host; spazioinsufficiente fisico non simulato. |
| I5 | Permessi negati | P limitato | db3a1a5;4c67b2c | fileURI senza grant/riapertura pathname negati osservati. È scopedstorage previsto/probe; non rifiuto utente del grant o recovery di tutte le permissionUI. |
| I6 | Nessuna rete | H | 8e98780 | Policyrete/cancel host; niente sessione dispositivo realmente offline documentata. |
| I7 | 30conversioni RAM | N | — | 30conversioni e trendRAM candidato non individuati. |
| J1 | Avvio base/finale | P-base | 0.45/4500 | Tre avviiCOLD base in launches.json; candidatoHome osservata ma triplettofinale e comparazione non archiviati qui. |
| J2 | FPSscorrimento | P-base | 0.45/4500 | 53frame/52intervalli,38.55fps nel campione. Nessun FPS finale; renderPNG non èfpsAndroid, gfxinfoNativeActivity zero non utilizzabile. |
| J3 | Cinque carichi tempo/PSS | P parziale | 0.45;db3a1a5;4c67b2c | BaseJPG/MP3/video+dialogo, intermediPDF/W3C/silenzio/FLEURS con campioni. Non cinque carichi su stesso candidato finale. |
| J4 | Batteria lavoro lungo | N | — | Batteria lavoro lungo non misurata; telefonoUSB in carica, temperature/percentuali non stimano autonomia. |
| J5 | DimensioneAPK | P artifact | 0.45;db3a1a5;4c67b2c | APKlocali presenti e byte/hash verificabili; non auditdimensione/risorse del prossimoAPK finale. |

## Indice delle prove fisiche e dei loro limiti

I percorsi seguenti sono relativi a PROOT, salvo diversa indicazione. Sono file effettivamente presenti e letti o ispezionati in questo audit.

| Evidenza | ID a cui contribuisce | Risultato realmente supportato |
| --- | --- | --- |
| `launches.json`, `meminfo-idle.txt`, `latency-*.txt`; report `docs/android-rapporti/02-base.md` | A6,J1,J2 | Solo base0.45: tre COLD, RAM e campione SurfaceFlinger38,55fps. `gfxinfo.txt` zero frame non diventa misura valida. |
| `prima-anteprima/preferenze-osservate.json` e PNG; `anteprima-db3a1a5/impostazioni.png`, `lavori.png`; Lavori nello screenshot silenzio4c | A2,H2,H3 | Prima/dopo parziale: Cedro/Melanzana, italiano automatico e lavori preesistenti osservabili. Non roundtrip di ogni impostazione. |
| `anteprima-db3a1a5/foto-pdf-1/{misure.json,logcat-Cedro.txt,risultato.pdf,schermata.png}`; `pdf-aperto.png`, `pdf-condividi.png`, `pdf-condiviso-a-cedro.png` | B1,B2,B12,C1,J3 | Job23 `pdf` completo,1uscita,1,809s motore+pubblicazione, PSSUID massimo campionato219642KiB. PDF riaperto qui solo per metadata:1paginaA4,946343B,PDF1.4. Chooser reale visibile; titolo3749 è un difettoUX storico, non prova nome corretto finale. |
| `anteprima-db3a1a5/video-cancel.json`, `video-worker.png`, `video-annullato.png` | B7,C6 | Worker rilevato fino1,913s e assente al campione2,171s dopo cancel. Non campionamentoCPU né ricerca esaustiva di file privati parziali. |
| `anteprima-db3a1a5/web-stall-cancel.json`, `web-server.jsonl`; Task4 runtime intermedio | C6,E9 | Peer del server controllato riceve65.536B poi stallo; worker assente entro2,014s, zero uscite nel log. Non Wi-Fi spento/ripresa Range reale. |
| `anteprima-db3a1a5/web-w3c-video/{misure.json,logcat-Cedro.txt,trailer.mp4,ffprobe.json,schermata.png}` | B9,E6,J3 | Job36 completo,1uscita,9,777s, PSSUID219941KiB campionato. OutputH264864×480+AAC,52,208333s,6248964B. Non mediana e non Originale byte-identico. |
| `anteprima-db3a1a5/youtube-video/{risultato.png,risultato.mp4,ffprobe.json}` | B8,E1 | PNG mostra lavoro YouTube fatto e nome Me at the zoo-2.mp4; file reale964169B,19s,H264320×240+AAC con streamaccessori. Mancano log cronometrici/qualità/componenti iniziali. **Il download base non è più solo pianificato**, benché appendici Task4 precedenti lo chiamassero ancora pendente; il resto di E1 resta aperto. |
| `anteprima-db3a1a5/saf/`, `docs/android-rapporti/qa-saf-report.md` | G5,G10,I5 | GrantSAF reale del grandeNDPI ottenuto, anteprimaEACCES riprodotta dopo reset: KO di quel checkpoint, non fallimento del grant. |
| `anteprima-4c67b2c/saf/{saf-probe-log.txt,saf-grant.txt,05-overview-controls.png,06-center-sample-*.png,07-return-overview.png,center-timing-mem.txt}`; reportQA4c nell'area ESECUZIONE | G5,G10 | Nuovo checkpoint: panoramica/centro/ritorno riusciti. `fstat=1473064608`, `pread=16`, `pathname_open=EACCES` nei tre worker. EACCES è il **probe atteso** e non errore UI. Centro completo entro2,4s; PSSsoloUI55,6/70,4MB. Lato1024 è limite sorgente, PNG privato non misurato; assenza copia disco non osservata. |
| `anteprima-4c67b2c/silenzio/{misure.json,logcat-Cedro.txt,schermata.png}` | D6,J3 |30s silenzio: job40 completo in1,809s, zero uscite, testo UI Nessun parlato riconosciuto; PSSUID245421KiB campionato. Non tono/ronzio/rumore fisici. |
| `anteprima-4c67b2c/fleurs-breve/{misure.json,logcat-Cedro.txt,risultato.txt,schermata.png,audio-1x*.opus,scrcpy-audio-1x.log}` | D1 parziale,D2,D7,J3 |8sFLEURS: job42 completo101,212s,1uscita, PSSUID1556738KiB campionato; TXT italiano presente. La presenza delle catture non sostituisce oracle di ascolto/seek/velocità, né243,3s/diarizzazione/referto. |
| `base-dialogo-buona-auto/risultato.json`, TXT/log/PSS/thermal | D1,D3,J3 | Baseline0.45:1000,002s, una ripetizione,4voci rilevate, picco campionato1724202KiB; ground truth parlanti non stabilita. |
| `base-silenzio/risultato.json`, `silenzio.txt`; `idle-dopo-silenzio/meminfo.txt` | D6,J3 | BaselineKO: trascrive “you” dal silenzio e mantiene memoria residente elevata in quel campione. Non prova leak generalizzato né esito del candidato. |
| `anteprima-4c67b2c/service-crash.txt` | C7,triage robustezza | Contiene storico eterogeneo: FATAL ServizioLavoro alle02:38:43 è successivo all'install4c01:46:26 ed è archiviato lì. Il linker `libavdevice.so.61 not found` è01:43:19, **prima** dell'install4c: non attribuirlo a4c dalla sola directory. I crash init agosto/12nov non sono Cedro candidato. Root deve correlare installazioni/processi per qualsiasi attribuzione più stretta. |

La scala di prova rimane: file/output+metadata+log+schermo sono forti per la sottoprova; uno screenshot isolato di un bottone non prova la sua action. Le misureUID sono somme campionate sequenzialmente: worker brevissimi e picchi tra campioni possono sfuggire; servizi codec di sistema esclusi.

## Host: prove identificate senza promuoverle a telefono

Fonti principali:

- `docs/android-rapporti/task-3-report.md`: af4e692,29/29Androidhost, DICOM/frame/finestra e render156PNG. FixtureND2/LIF/CZI pubbliche nel corpus non equivalgono a run sul dispositivo.
- `task-4-report.md`: db3a1a5,40/40Android con/senzaYouTube; coreRange4/4 e fakeyt-dlp con retry429; HTTPMavenloopback con socket già aperti. Non DNS/MavenAndroid o staccoWi-Fi.
- `task-5-report.md`: f5dcc48, player erroreAAudio1/1, rivedi3/3, detector no-parlato e voceFLEURS host; render300PNG con gap referto corto osservato. Non suono del telefono o modello riassunto4B.
- `task-8-report.md`:0a3bca4, settings18/18, theme4/4, edizione2/2, Android41/41; core585PASS/15ignored. Migrazione temi desktop non è aggiornamento AndroidA2 fisico.
- `task-6-report.md` e **log locali effettivi HROOT/8e98780**: fixplayer1/1, TGA1/1, photo1/1, MRXS1/1, a11y3/3, input4/4, suiteAndroid; test ingressi `pdf_e_office_da_grant_saf_non_spostano_il_cursore` PASS nel corelog. Questi test usano registry/fd/host, non ContentResolver di un provider reale.
- Stessi corelog8e: `heic_photo_opens_with_its_colors`, `p7m_valida`, `pdf_firmato`, `contenuto_cambiato`, `tsd_m7m_tsr`, `rapporto` PASS; `nd2::prove::file_vero`, `lif::prove::file_vero`, `czi::prove::file_vero` esplicitamente **ignored**. Nessun PASS ai pubblici reali deriva da queste tre righe.

I report degli implementer contengono sezioni storiche “test scritti ma non eseguiti” seguite da appendici controller. Per l'attribuzione contano l'appendice e il log del commit esplicito, senza cancellare i fallimenti intermedi: per esempio3d1629b ha2FAILcore poi corretti;8e98780 copre i fix, non certifica commit successivi per inferenza.

Il driver JAR QA dell'accessibilità è stato compilato (`qa-a11y-driver.md`), ma il risultato di `AccessibilityNodeInfo.performAction` non è presente tra gli artifact fisici esaminati in questo audit: non conta come actionPASS. L'oracleRED/GREEN resta al controller.

## Batch finale breve e pertinente sul nuovo APK

Proposta di smoke **circa10–15min**, subordinata a binario finale identificato e nessun altro lavoro attivo. Questo batch chiude sottocasi di rischio recente; **non chiude79ID**, il benchmark dialogo completo,5DOCX/20fogli, tutti i codec/Labs,30conversioni o batteria. Nessun comando seguente è stato eseguito dall'audit.

1. **Identità, aggiornamento e avvii** (A1/A2/A6/J1/J5): archiviare commit, APKfirmato/hash/byte e package; confrontare tema/lingua/Lavori con JSON prima. TreCOLD solo quando non esistono lavori in esecuzione, poi un meminfo assestato. Non usare5299 per distinguere binari.
2. **Bridge e layout** (H4/H5/H8 e gate Task6): daHome, eseguire JAR QA `Node.performAction` Lavori→Impostazioni→scroll; verificare contenuto, non solo booleano. In aggiunta controllo manuale hover/focus e back/header dopo scroll. Provare fontOS130/200/una lingua lunga con ripristino del valore originale già autorizzato dal controller: renderer2.0 host non sostituisce il telefono. Non attivare servizi accessibilità o modificare impostazioni per inferenza da questo report.
3. **Un giro reale file/provider** (B1/B2/B3/B4/B12,C1/C2/C3 parziali): scegliere mediante SAF `foto-12mp.jpg`→PDF; aprire e condividere quel risultato aCedro dal chooser, controllando nomefile/MIME e navbar3. Poi un DOCX Unicode e un XLSX piccolo da provider→PDF con output leggibile. **Resta aperto** il confronto5DOCX e lo stress20fogli/50000righe; non sostituirli con questi due file.
4. **SAF grande e capacità native** (G5/G10,I5 parziali): OS-3.ndpi reale via picker, overview→centro→overview, probe dove presente. Ripetere un media seekable>32MB→anteprima/cancel per il percorsoMediaExtractor cambiato; confermare che non cade nel pathnameopen negato. Nessun export pieno del vetrino. Se interessa il fallbackffmpeg, verificare `fd` nel binario del candidato e un caso realmente instradato lì: il downloadYouTube giàriuscito non prova quel ramo.
5. **Voce breve e servizio** (D2/D6/D7,C7): silenzio30s senza output inventato, FLEURS8s con testo e play/pausa/±10s/una velocità, lasciare la route e riaprireRivedi. Durante un lavoro controllato mettere l'app in background e tornare, osservando notifica e logServizioLavoro per evitare la regressioneFATAL precedente. Non associare a questo smoke una garanzia su diarizzazione/referto/riassunto4B.
6. **Cancel e pubblicazione** (C6/E6/E9 parziali): video sintetico60s o serverstallo già predisposto, Annulla e campionamentoUID fino a workerassente entro5s; verificareCPUidle e assenza di risultati nuovi/parziali pubblici del solo fixture. Un W3C breve completato fornisce controlloMediaStore/output. RangeWi-Fi e permessi negati interattivi restano casi separati.

### Comandi di raccolta pronti, da eseguire soltanto dal controller

Percorso d'esempio nuovo e univoco **da sostituire con il commit effettivo**, senza sovrascrivere evidenceintermedia:

```bash
matrix_run=~/Documenti/cedro-android-anteprima/prove-mi9t/finale-COMMIT
mkdir -p "$matrix_run"
adb -s 2f6d6ba7 shell dumpsys package it.cedro.android > "$matrix_run/package.txt"
adb -s 2f6d6ba7 shell dumpsys activity services it.cedro.android > "$matrix_run/services-before.txt"
adb -s 2f6d6ba7 exec-out screencap -p > "$matrix_run/home-before.png"
```

TriplettoCOLD, **solo senza lavoro attivo**, non durante download/trascrizioni:

```bash
for matrix_n in 1 2 3; do
  adb -s 2f6d6ba7 shell am force-stop it.cedro.android
  adb -s 2f6d6ba7 shell am start -W -n it.cedro.android/.CedroActivity \
    > "$matrix_run/cold-$matrix_n.txt"
done
adb -s 2f6d6ba7 shell dumpsys meminfo it.cedro.android > "$matrix_run/meminfo-idle.txt"
```

BridgeQA: esegue vereaction, **mai Svuota**, quindi va lanciato quando si vuole dedicare il telefono al test e non contemporaneamente ad altraUiAutomation:

```bash
adb -s 2f6d6ba7 push \
  ~/prove-cedro-android/bridge-jar/out/cedro-bridge-probe.jar \
  /data/local/tmp/cedro-bridge-probe.jar
adb -s 2f6d6ba7 shell uiautomator runtest /data/local/tmp/cedro-bridge-probe.jar \
  -c it.cedro.android.probe.LegacyBridgeProbe#testBridgeActions -s \
  > "$matrix_run/a11y-actions.txt" 2>&1
```

CampionamentoUID esistente durante un lavoro avviato dall'UI reale; questo comando non sceglie file o avvia conversioni:

```bash
python ~/prove-cedro-android/misura-processi.py \
  --dest "$matrix_run/processi" --secondi 120
adb -s 2f6d6ba7 logcat -d -v epoch -s Cedro:I CedroWorker:I AndroidRuntime:E \
  > "$matrix_run/log-after.txt"
```

Per un job completo, `misura-candidato.py` già raccoglie avvio/fine correlati e PSSUID ma **esegue un tap**: usarlo solo dopo avere osservato i bounds reali del pulsante sul nuovoAPK (`--dest`, `--azione`, `--x`, `--y`, `--timeout`). Non riciclare coordinate del vecchio font/layout e non usarlo come prova di actionaccessibile. Per l'ingresso provider usare il veroSAF, non inventare intentURI con grant inesistenti.

Registrare sempre: ID e sottocaso, commit/hashAPK, input e SHA/provenienza, canale reale(SAF/SEND/solo percorso shell), azione UI, logjob, output riaperto, errore/cancel, tempi con metodo e PSSaggregata con limiti. Ogni riga rimasta fuori dal batch resta aperta.
