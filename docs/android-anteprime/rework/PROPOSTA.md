# Cedro Android: proposta A approvata

Riferimento: `index.html` in questa cartella, autonomo e utilizzabile senza rete. Approvazione dell'utente del 16 novembre 2026: realizzare questa interfaccia, completare il lavoro e pubblicare Android dopo le prove sul Mi 9T. Il confronto con Xiaomi 13 è escluso per ora su sua indicazione.

## Il file al centro

Inizio apre il file e mostra sei attività riconoscibili: Converti, PDF, Trascrivi, Video dal web, Firma e Laboratorio. Ogni attività ha il proprio colore, icona e nome. I file recenti vengono dopo le attività. La barra inferiore ha tre destinazioni stabili: Inizio, Lavori, Impostazioni. Le attività si aprono dalle tessere, senza imporre una procedura a passi.

Quando arriva un file, una scheda compatta mostra anteprima, nome e dimensioni. I formati sono divisi per famiglia e restano tutti raggiungibili scorrendo. Gli strumenti hanno un gruppo distinto; quelli di laboratorio un gruppo con il proprio nome. Le opzioni rimangono esplicite. Il pulsante principale è sopra la barra inferiore e indica cosa verrà prodotto. Un file condiviso da un'altra app apre un foglio con anteprima, azioni e chiusura visibile.

Lavori conserva avanzamento, risultati ed errori; Apri, Condividi e Rivedi sono azioni dirette. Annulla deve fermare il lavoro reale. Le importazioni e il salvataggio devono lasciare libera l'interfaccia. Un errore di pubblicazione non deve cancellare il risultato privato.

## Schermate specifiche

- PDF mantiene pagine, unione, divisione, compressione, OCR ed esportazioni.
- Word e fogli di calcolo espongono le opzioni del motore condiviso, compresa la pagina di stampa.
- Trascrivi dà spazio alla traccia audio, all'ascolto e alla frase corrente. Testo, Frasi e Voci sono viste esplicite. I referti restano bozze finché i punti richiesti non sono confermati.
- Video dal web usa tessere per formato e qualità; playlist, sottotitoli e metadati sono scelte visibili. La preparazione dei componenti ha una riga di avanzamento.
- Laboratorio mostra file, livello/scena/canale/piano disponibili e azioni pertinenti. Caso/campione e copia del registro servono a organizzare il lavoro. Le immagini schematiche nell'HTML sono marcate come dimostrative, senza significato diagnostico.
- Firma distingue integrità, fiducia e avvisi, conservando il rapporto esportabile.

## Aspetto e adattamento

La proposta usa Figtree e le icone già presenti nel progetto. I quattro temi sono Cedro, Notte, Bosco e Carta; i temi precedenti restano selezionabili. Colori per attività, bordi netti, superfici chiare o scure e gerarchia tipografica sostituiscono le grandi aree decorative che sottraevano spazio ai comandi.

L'HTML permette di confrontare due alternative di navigazione: A, scelta dall'utente, e B con le attività persistenti. Include 20 stati, italiano e tedesco, 360×800, 393×873, 360×640, 800×400 e 768×960, scala del testo 100/130/200%. I bersagli hanno altezza minima 48 dp. Ridimensionamento, testo lungo e tastiera richiedono conferma anche nel renderer Rust e sul telefono.

Il prototipo usa contenuti di esempio dichiarati e non converte file. Nell'app entrano esclusivamente stato, comandi e dati reali. I quattro confronti «oggi» provengono dalla 0.45.0 installata sul Mi 9T; non sono render della 0.53.0, che alla revisione iniziale non compilava per Android.

## Verifica della proposta

Controllo automatico con Chromium/Playwright: 320 combinazioni regolari e 480 combinazioni con dimensioni/testi estremi; nessun errore JavaScript, overflow orizzontale o bersaglio corto rilevato nell'ultimo giro. Questo controllo riguarda l'HTML, non certifica accessibilità o prestazioni dell'APK.

Figtree è distribuito con SIL Open Font License: `assets/fonts/OFL-Figtree.txt`. Le immagini e le icone provengono dal kit Cedro. Nessuna dipendenza web o font remoto.
