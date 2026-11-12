## Scarica questo

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** (quasi tutti i PC) | [**Cedro-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Setup-{versione}.exe) |
| **Windows portable** (x64, senza installazione) | [**Cedro-Portable-{versione}-x64.zip**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Portable-{versione}-x64.zip) |
| **Linux** (Wayland con KDE Plasma o GNOME) | [**cedro-{versione}-linux-x64.tar.gz**](https://github.com/{repo}/releases/download/v{versione}/cedro-{versione}-linux-x64.tar.gz) |

Scarica il file e fai doppio clic: si installa sopra la versione che hai (su Linux: estrai il tar.gz e lancia `./install.sh`, senza permessi di amministratore). Se hai già Cedro non serve nemmeno questo: si aggiorna da solo («Controlla aggiornamenti» nel menu dell'icona).
I motori della trascrizione non vanno scaricati a mano: Cedro li prende da solo la prima volta che trascrivi.

## Novità della {versione}

**Cedro 4 Labs legge i vetrini Hamamatsu e 3DHistech, e i referti dettati restano bozza finché non sono controllati.**

- Si aprono i vetrini Hamamatsu `.ndpi` (NanoZoomer) e 3DHistech `.mrxs` (Pannoramic, il file con la sua cartella accanto) come già gli Aperio `.svs`: anteprima, immagine del livello scelto, foto del vetrino e miniatura. I pixel sono gli stessi che dà OpenSlide.
- Copia anonima, codice del caso, tessere per l'AI, zona per la conta mitotica e pacchetto per il consulto valgono anche per questi vetrini: via l'etichetta, la foto del vetrino con l'etichetta, i codici a barre, la data, il nome del vetrino e del progetto.
- Nella revisione di una trascrizione con «Referti veterinari» acceso, ogni frase con numeri e misure (2,5 cm, tre mitosi) e ogni parola che cambia il senso se sentita male (non, assente, destro, maligno…) va riascoltata e confermata, o corretta sul posto. Finché ne resta una, il file si salva come «bozza» con l'elenco dei punti in cima.
