## Scarica questo

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** (quasi tutti i PC) | [**Cedro-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Setup-{versione}.exe) |
| **Windows portable** (x64, senza installazione) | [**Cedro-Portable-{versione}-x64.zip**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Portable-{versione}-x64.zip) |
| **Linux** (Wayland con KDE Plasma o GNOME) | [**cedro-{versione}-linux-x64.tar.gz**](https://github.com/{repo}/releases/download/v{versione}/cedro-{versione}-linux-x64.tar.gz) |

Scarica il file e fai doppio clic: si installa sopra la versione che hai (su Linux: estrai il tar.gz e lancia `./install.sh`, senza permessi di amministratore). Se hai già Cedro non serve nemmeno questo: si aggiorna da solo («Controlla aggiornamenti» nel menu dell'icona).
I motori della trascrizione non vanno scaricati a mano: Cedro li prende da solo la prima volta che trascrivi.

## Novità della {versione}

**Correzioni della lettura degli strumenti di laboratorio (Cedro 4 Labs) e finestra più leggera nelle altre lingue.**

- **Messaggi d'errore degli strumenti tradotti.** Quando un export non si legge, il motivo (colonna mancante, tempo non leggibile, versione non supportata…) ora si vede in inglese, spagnolo, tedesco, francese e portoghese, non più in italiano: un centinaio di messaggi dei 34 formati.
- **CSV piatto giusto per l'Excel italiano.** Con il punto e virgola come separatore anche il CSV piatto di Allotrope scrive i decimali con la virgola, come il CSV della tabella: Excel lo apre con i numeri al posto giusto.
- **Niente «2 pagine» sui file di dati.** Nella coda l'export di uno strumento non mostra più un numero di pagine che non ha senso per una tabella di misure.
- **Finestra più leggera in inglese, spagnolo, tedesco, francese e portoghese.** I testi con numeri («12 file», «3 pagine», «2 file su 5»), ridisegnati a ogni fotogramma, ora si traducono una volta sola e poi si ricordano: da 9,7 a 0,12 microsecondi per testo, 80 volte meno lavoro.
