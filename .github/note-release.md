## Scarica questo

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** (quasi tutti i PC) | [**Cedro-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Setup-{versione}.exe) |
| **Windows portable** (x64, senza installazione) | [**Cedro-Portable-{versione}-x64.zip**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Portable-{versione}-x64.zip) |
| **Linux** (Wayland con KDE Plasma o GNOME) | [**cedro-{versione}-linux-x64.tar.gz**](https://github.com/{repo}/releases/download/v{versione}/cedro-{versione}-linux-x64.tar.gz) |

Scarica il file e fai doppio clic: si installa sopra la versione che hai (su Linux: estrai il tar.gz e lancia `./install.sh`, senza permessi di amministratore). Se hai già Cedro non serve nemmeno questo: si aggiorna da solo («Controlla aggiornamenti» nel menu dell'icona).
I motori della trascrizione non vanno scaricati a mano: Cedro li prende da solo la prima volta che trascrivi.

## Novità della {versione}

**Cedro più veloce e più leggero: foto, PDF e video in meno tempo, la finestra che consuma meno mentre lavora.**

- **Foto a metà misura o a un lato massimo** circa 3 volte più veloci e con metà memoria (una foto da 12 MP: da 0,6 a 0,2 secondi). Più veloci anche miniature, figure, vetrini e lettura del testo, che rimpiccioliscono le immagini allo stesso modo.
- **Foto → PDF** più del doppio più veloce: la foto e la lettura del testo si preparano insieme.
- **PDF → JPG** una immagine per pagina 2,5 volte più veloce; tutti i JPEG in uscita si scrivono in circa metà tempo, con la stessa qualità. «Comprimi» resta quello di prima.
- **WebM → MP4** 2,5 volte più veloce e con un terzo di memoria in meno (VP9 nuovo); un file Zeiss CZI rovinato non può più far chiudere Cedro.
- **Windows, video → MP4**: i fotogrammi in attesa del codificatore non restano più tutti in memoria. Sui PC senza codificatore nella scheda video un video Full HD di 20 secondi arrivava a 2 GB e uno lungo poteva esaurire la memoria; ora la memoria resta bassa per tutta la durata.
- **La finestra** si disegna in metà tempo e, mentre un lavoro va avanti, si aggiorna 15 volte al secondo invece di 30: il processore resta al lavoro, non alla rotellina.
- I **caratteri** del sistema si leggono solo per la parte che serve: Word → PDF e i PDF con testo partono prima e occupano meno memoria.
- **Linux**: chiusa la finestra, la memoria torna al sistema; il menu delle applicazioni non si ricostruisce più a ogni avvio; l'elenco europeo dei certificatori si scarica a piccoli gruppi, con meno memoria.
