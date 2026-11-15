# Banco di prova di Cedro: protocollo (fissato PRIMA delle misure)

Questo file è il contratto. Metriche, soglie, dataset e regole di decisione sono scritti qui **prima** di
lanciare qualunque misura; il commit che lo introduce precede quelli con gli script e i risultati. Se una
soglia va cambiata dopo aver visto i numeri, si cambia in un commit a parte che dice perché, e il rapporto
lo segnala. Nessuna soglia si ritocca per far uscire bene Cedro.

Versione del protocollo: 1 · data: 2026-11-15 · Cedro misurato: tag del `Cargo.toml` (0.50.0 al momento della stesura).

## 1. Regole comuni

1. **Prove dirette.** Ogni numero della matrice viene da un comando in `bench/` che chiunque può rilanciare.
   Quello che non si può eseguire (programmi chiusi, a pagamento, solo macOS, solo cloud) sta nella matrice
   come «solo documentazione ufficiale» e non entra mai in una colonna di misure.
2. **Fonti primarie prima.** Specifiche (ISO 32000-1/-2, ECMA-376, DICOM PS3.x, ETSI EN 319 1xx), validatori
   ufficiali (veraPDF, EU DSS), dataset pubblici, tracker dei concorrenti. Le recensioni non contano.
3. **Ripetibilità.** Versioni di strumenti e dataset fissate in `bench/VERSIONI.md`; seme casuale fisso (42);
   le misure di tempo e memoria si ripetono 5 volte, si riporta la mediana e il minimo-massimo; la prima
   esecuzione a freddo e le altre a caldo si riportano a parte.
4. **Stesso hardware per il confronto.** Le misure di tempo/memoria si fanno solo sul PC di collaudo (Ryzen 7 5700U, 16 GB)
   e sulla VM Windows, mai sul cloud: il cloud serve per correttezza, non per prestazioni.
5. **Esito.** Per ogni metrica: valore Cedro, valore dei concorrenti, soglia, esito `ok` / `lacuna` / `non misurabile`.
6. **Che cos'è una lacuna.** (a) Un difetto **grave**: dato sbagliato senza avviso, firma non valida accettata
   come valida, crash o blocco, file prodotto non valido per il validatore ufficiale. Basta un caso. Oppure
   (b) una metrica in cui Cedro è peggiore del miglior concorrente misurabile **oltre il margine** scritto sotto.
   Oppure (c) una funzione che i concorrenti hanno e che l'utente di laboratorio usa (vedi §7), assente in Cedro.
7. **Intervalli.** Le differenze di WER si danno con intervallo di confidenza al 95 % (bootstrap pareggiato su
   file, 2000 ricampionamenti). Una differenza dentro l'intervallo non è una differenza.
8. **Concorrenti.** Per ogni area i tre di riferimento sono scelti in base a (i) uso reale, (ii) possibilità di
   eseguirli. Quelli non eseguibili restano nella matrice delle funzioni ma non nelle misure.

## 2. Area A: trascrizione

Concorrenti: **Buzz** (open source, Windows/Linux/macOS), **Vibe** (open source, Windows/Linux/macOS),
**whisper.cpp** come riferimento di motore (è lo stesso motore di Cedro, serve a separare «motore» da «app»);
fuori misura: **MacWhisper** (solo macOS), **Descript** (carica l'audio su un servizio: non adatto ai dati di laboratorio).

| Id | Metrica | Come | Soglia di parità |
| --- | --- | --- | --- |
| A1 | WER italiano | `jiwer` su FLEURS it_it (test), Common Voice 17 it (test, 500 clip a seme 42), MLS italiano (test, 200 clip) | Cedro ≤ miglior concorrente + **1,0 punti** (assoluti), IC 95 % |
| A2 | CER italiano | stesso set | ≤ miglior concorrente + 0,5 punti |
| A3 | Allucinazioni | 10 file da 60 s di silenzio, rumore bianco, musica, parlato lontano: conta parole prodotte | **0 parole** su silenzio e rumore; parlato lontano: ≤ concorrente |
| A4 | Tempo reale (RTF) | file italiano di 10 min, CPU sola e scheda video, PC di collaudo | ≤ miglior concorrente × 1,25 |
| A5 | Memoria di picco (RSS) | `/usr/bin/time -v` sullo stesso file | ≤ miglior concorrente × 1,25 |
| A6 | Robustezza | 50 file reali di contenitori e codec diversi (mp4, mkv, mov, m4a, ogg, opus, amr, webm, vocali WhatsApp, wav 8 kHz, audio senza traccia, 3 h): file che finiscono senza errore né crash | **50/50**; ogni errore deve dire chiaro cosa è andato storto |
| A7 | Chi parla (DER) | miscela sintetica di clip FLEURS/CV con parlanti noti (2, 3, 5 voci, senza sovrapposizione, 20 miscele) con `pyannote.metrics`/`dscore` | DER ≤ miglior concorrente + 5 punti; numero di voci stimato giusto ≥ 80 % |
| A8 | Lungo | 3 h di audio: nessuna deriva dei tempi (scarto tra tempo del testo e tempo vero ≤ 2 s a fine file), nessun ripetersi di frasi in loop (≥ 3 ripetizioni identiche di fila = difetto) | 0 difetti |
| A9 | Termini di dominio | 40 frasi di laboratorio (anatomia patologica, veterinaria, biologia molecolare) lette da voce sintetica e da 2 voci reali; WER sui soli termini tecnici, con e senza glossario | misura e basta (non ci sono concorrenti con glossario) |

Normalizzazione del testo (fissa, uguale per tutti): NFKC, minuscolo, punteggiatura tolta (apostrofo → spazio),
numeri come arrivano (nessuna conversione), spazi collassati. Il codice è in `bench/trascrizione/wer.py`.

## 3. Area B: PDF e ufficio

Concorrenti: **Foxit PDF Editor** (riferimento di realhcok) e **Adobe Acrobat** (chiusi: solo funzioni da documentazione
ufficiale, più confronto manuale a campione su PC-SVILUPPO se installati); misurabili: **LibreOffice** (Writer/Draw),
**pdf2docx**, **Ghostscript**, **qpdf**, **MuPDF**, **Tesseract**.

| Id | Metrica | Come | Soglia |
| --- | --- | --- | --- |
| B1 | Validità dei PDF prodotti | tutto ciò che Cedro scrive in PDF (da Word, Excel, immagini, unisci, dividi, comprimi, OCR) passa `qpdf --check` (struttura ISO 32000-1) e `veraPDF -f 0` (nessun errore di parsing); font incorporati con ToUnicode | **0 errori** su tutto il set |
| B2 | Word → PDF: fedeltà | su un corpus di DOCX/ODT/RTF/DOC (≥ 200 file, seme 42): numero di pagine uguale al riferimento LibreOffice; similarità visiva (SSIM a 72 dpi) pagina per pagina; testo (F1 a parole) | pagine uguali ≥ 90 %; SSIM mediana ≥ 0,90; F1 testo ≥ 0,99 |
| B3 | Excel → PDF | idem su XLSX/ODS/XLS con aree di stampa, più fogli, formule, date | stesse soglie di B2 |
| B4 | PDF → Word (riferimento Foxit) | andata e ritorno: PDF prodotto da un DOCX noto → convertito → riletto: F1 del testo, celle di tabella giuste (%), titoli riconosciuti (%), SSIM della resa; confronto con pdf2docx e LibreOffice | F1 ≥ miglior concorrente − 0,01; celle ≥ miglior concorrente − 2 punti |
| B5 | Validità DOCX/XLSX prodotti | schema ECMA-376 (validazione XSD con i file ufficiali) e riapertura in LibreOffice, python-docx, openpyxl, Apache POI | **0 errori di schema**; apre in tutti |
| B6 | Comprimi PDF | scansioni e PDF con foto: dimensione finale e SSIM contro Ghostscript `/ebook` | peso ≤ Ghostscript × 1,10 a SSIM ≥ 0,98 |
| B7 | OCR | 100 pagine italiane generate da testo noto (corpo 9-12 pt, rotazione ±2°, rumore, JPEG q50), CER e WER contro Tesseract `ita` | CER ≤ Tesseract + 2 punti |
| B8 | Unisci/dividi/estrai | PDF con firme, moduli, segnalibri, allegati, cifrati: cosa si perde (segnalibri, moduli, metadati XMP, dimensione pagina) | 0 perdite silenziose: o si conserva o si avvisa |
| B9 | Robustezza | `tools/regressione-cli.sh` + 1000 file rovinati a caso (fuzz, seme 42) | 0 crash, 0 blocchi > 60 s |
| B10 | Velocità e memoria | 10 PDF da 1, 10, 100, 500 pagine; tempo e RSS contro MuPDF/qpdf/pdf2docx | ≤ miglior concorrente × 1,5 |

## 4. Area C: firma digitale

Concorrenti: **ArubaSign** e **Dike/GoSign** (InfoCert), chiusi: solo funzioni dalla documentazione ufficiale e prova
manuale a campione (PC-SVILUPPO, chiavetta vera di realhcok); misurabili come riferimento: **EU DSS** (libreria ufficiale
della Commissione, è il motore del validatore europeo), **pyHanko**, **OpenSSL**, **Poppler `pdfsig`**.

| Id | Metrica | Come | Soglia |
| --- | --- | --- | --- |
| C1 | Verifica: concordanza | corpus generato da 3 produttori indipendenti (DSS, pyHanko, OpenSSL) + file di Cedro: CAdES-B/T/LT/LTA, PAdES-B/T/LT/LTA, attached/detached, firma multipla, controfirma, SHA-1/256/384/512, RSA/ECDSA, scaduti, revocati, manomessi in 12 modi (byte del documento, signedAttrs, digest, cert, ByteRange, aggiunta di contenuto dopo la firma). Il verdetto di Cedro (valida / non valida / non verificabile) contro quello di DSS | concordanza ≥ 99 % |
| C2 | **Falsi positivi** | quanti file manomessi Cedro dà per validi | **0** (grave) |
| C3 | Produzione: accettata dai validatori | CAdES e PAdES prodotti da Cedro con chiave software e con SoftHSM: DSS, pyHanko, `pdfsig`, `openssl cms -verify` devono dire «firma integra» | 100 % |
| C4 | Profili dichiarati | il livello che Cedro dice di produrre (B-B, B-T, B-LT, B-LTA) coincide con quello che DSS riconosce | 100 % |
| C5 | PDF con più firme e aggiornamenti incrementali | firmare un PDF già firmato non invalida la prima firma (DSS e pdfsig) | 100 % |
| C6 | Fiducia | elenco dei certificatori (EUTL/AgID) aggiornato da solo, con data dell'ultimo aggiornamento, rifiuto di liste con firma non valida | verificato per lettura codice + prova con lista manomessa |
| C7 | Tempi | verifica di 100 file firmati; firma di 100 file con SoftHSM | misura e basta |
| C8 | Formati | .p7m, .p7s, .m7m, .tsd, .tsr, PAdES, XAdES (vedi §7), ASiC-E | matrice |

## 5. Area D: vetrini e laboratorio

Concorrenti: **QuPath** (con OpenSlide e Bio-Formats), **Aperio ImageScope** (chiuso, solo Windows),
**OpenSlide** (riferimento di lettura; è quello che QuPath usa per SVS/NDPI/MRXS). Cedro **non** vuole essere un
programma di analisi: si confronta solo ciò che fa (aprire, esportare, anonimizzare, preparare figure e dati).

| Id | Metrica | Come | Soglia |
| --- | --- | --- | --- |
| D1 | Copertura dei formati | SVS, NDPI, MRXS, CZI, ND2, LIF, OME-TIFF, DICOM (anche WSI), VSI, SCN, TIFF generico: apre sì/no, su file veri | matrice |
| D2 | Correttezza dei pixel | regione 1024×1024 a ogni livello della piramide letta da Cedro e da OpenSlide/`tiffslide`/libCZI: errore medio assoluto e massimo (0-255) | media ≤ 0,5; massimo ≤ 4 (le differenze di IDCT JPEG stanno qui); lossless: **0** |
| D3 | Metadati | ingrandimento, µm/pixel, dimensioni, numero di livelli, etichetta e macro: uguali al riferimento | 100 % |
| D4 | Barra di scala | misura in µm di un oggetto di dimensione nota nel file | errore ≤ 1 % |
| D5 | Velocità | tempo per ottenere miniatura, regione a pieno ingrandimento, esportazione intera a livello 2; RSS di picco, contro OpenSlide | ≤ OpenSlide × 1,5 |
| D6 | Anonimizzazione DICOM | tutte le istruzioni dei profili di base di PS3.15 Annex E (tabella E.1-1) applicate a ≥ 100 file da dataset pubblici (TCIA, IDC) e dai file di prova di pydicom; si conta quanto resta di ciò che andava tolto, con riferimento `dicom-anonymizer` | **0 tag con dati identificativi residui** (grave) |
| D7 | Anonimizzazione di vetrini | etichetta e macro del vetrino (spesso contengono nome o codice) tolte, e metadati TIFF/XML (data, operatore, nome file) | 0 residui (grave) |
| D8 | Validità dei DICOM scritti | `dciodvfy` (dicom3tools) e `pydicom` rilettura | 0 errori |
| D9 | Robustezza | 1000 file rovinati a caso (fuzz, seme 42), file da 10 GB | 0 crash; memoria ≤ 2 GB su file da 10 GB |
| D10 | Figura per articolo | figura 85 mm / 175 mm a 300 e 600 dpi: dimensioni in pixel esatte, DPI nei metadati, barra di scala giusta, profilo colore | 100 % |

## 6. Graduatoria delle lacune (fissata prima)

Impatto = **Gravità × Frequenza × Esposizione**, ogni fattore da 1 a 3.
- Gravità: 3 = dato sbagliato senza avvisare / dato identificativo che resta / firma falsa accettata; 2 = funzione
  che serve non c'è o si ferma; 1 = lentezza, aspetto, scomodità.
- Frequenza: 3 = ogni giorno in laboratorio; 2 = ogni settimana; 1 = ogni tanto.
- Esposizione: quanti dei quattro profili (biotech, ricerca, veterinari, anatomia patologica) la incontrano: 3 = tre o quattro; 2 = due; 1 = uno.

A parità di impatto prima le lacune più economiche da chiudere. Le stime di frequenza vengono dai colloqui con
utenti reali (§8); finché non ci sono, sono scritte come **ipotesi** e il rapporto lo dice.

## 7. Cosa serve a un laboratorio vero (ipotesi da verificare con utenti)

Anatomia patologica: aprire vetrini di qualunque scanner (Aperio, Hamamatsu, 3DHistech, Leica, Philips, DICOM WSI),
togliere nome ed etichetta, esportare regioni e figure, refertare con modelli, conservare con impronta, DICOM sempre.
Biotech/ricerca: CZI/ND2/LIF/OME-TIFF, 16 bit, barre di scala, figure per riviste (300/600 dpi), dati di piastra, CSV/Excel
senza errori di locale, pacchetti di dati con integrità, trascrizione di riunioni e interviste. Veterinari: DICOM
veterinario (RX), anonimizzazione senza perdere specie/razza/età, referti per il proprietario. Tutti: firma digitale
italiana (CNS/Aruba Key/Dike), PEC e marca temporale, Word/Excel/PDF senza Office.

## 8. Cosa non si può misurare senza persone

Vedi il rapporto: test a tempo con compiti fissi, SUS (System Usability Scale), «tempo al primo risultato»,
errori di scelta, fiducia nei risultati (blind A/B con un patologo), prove con chiavette e scanner veri.
