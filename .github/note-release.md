## Scarica questo

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** (quasi tutti i PC) | [**Cedro-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Setup-{versione}.exe) |
| **Windows portable** (x64, senza installazione) | [**Cedro-Portable-{versione}-x64.zip**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Portable-{versione}-x64.zip) |
| **Windows on ARM** (Snapdragon X e simili) | [**Cedro-ARM64-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-ARM64-Setup-{versione}.exe) |
| **Linux** (Wayland con KDE Plasma o GNOME) | [**cedro-{versione}-linux-x64.tar.gz**](https://github.com/{repo}/releases/download/v{versione}/cedro-{versione}-linux-x64.tar.gz) |

Scarica il file e fai doppio clic: si installa sopra la versione che hai (su Linux: estrai il tar.gz e lancia `./install.sh`, senza permessi di amministratore). Se hai già Cedro non serve nemmeno questo: si aggiorna da solo («Controlla aggiornamenti» nel menu dell'icona).
Non sai quale ti serve? *Impostazioni di Windows → Sistema → Informazioni*: se in «Tipo di sistema» c'è «basato su ARM», prendi quello ARM; altrimenti l'altro.
I motori della trascrizione non vanno scaricati a mano: Cedro li prende da solo la prima volta che trascrivi.

## Novità della {versione}

**Cedro per i laboratori: radiografie DICOM, microscopia, vetrini digitali, figure per gli articoli e pacchetti di dati con le impronte.**

- **DICOM** (TC, RX, ecografie): si convertono in PNG, JPEG, TIFF o PDF con la finestra del file o quelle classiche (polmone, osso, tessuti molli), tutta la serie o solo la prima immagine, anche con i valori originali a 16 bit per misurare. «Anonimizza» è acceso di serie: nome, nascita, medico, clinica e codici non escono; in veterinaria restano specie, razza, età e sesso.
- **Radiografie per il proprietario**: con un DICOM in coda, un PDF A4 con una pagina per lastra, il nome dell'animale, la specie, la razza, la parte del corpo e la data scritti in chiaro, il logo e il nome della clinica. Il logo basta metterlo in coda una volta: poi Cedro se lo ricorda.
- **Vetrini digitali (.svs)**: copia anonima (via etichetta, foto del vetrino, nome del file e data), **tessere per l'intelligenza artificiale** solo dove c'è tessuto, **zona per la conta mitotica** (2,37 mm² in 10 campi numerati, scelta dove ci sono più nuclei, senza pieghe né vetro) e **pacchetto per un consulto** con la scheda da compilare.
- **Microscopia**: si aprono i file Nikon (.nd2) e Leica (.lif), gli OME-TIFF e i TIFF di ImageJ a 16 bit, con canali, piani Z, contrasto automatico e barra di scala.
- **Figura per un articolo**: le immagini in pannelli A, B, C larghi una o due colonne (8,5 o 17,5 cm), a 300 o 600 dpi, con la barra di scala e la legenda accanto. La barra c'è anche sulle foto della fotocamera del microscopio, dall'obiettivo e dalla calibrazione del 10x (si scrive una volta).
- **Pacchetto dei dati**: dalla ruota, uno ZIP con l'impronta SHA-256 di ogni file (formato BagIt); in Archivi › Strumenti «Verifica» dice se qualcosa è cambiato.
- **Tabelle di laboratorio**: CSV e TSV in Excel e ritorno, con la virgola dei decimali giusta e senza i nomi dei geni trasformati in date.
- **Togli autore e dati nascosti** anche da PDF, Word, Excel, PowerPoint e OpenDocument.
- **Trascrizione**: nella finestra «Quante persone parlano?» la spunta «Referti veterinari» suggerisce al motore i termini di anatomia patologica.
