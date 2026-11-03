## Scarica questo

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** (quasi tutti i PC) | [**Cedro-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Setup-{versione}.exe) |
| **Windows on ARM** (Snapdragon X e simili) | [**Cedro-ARM64-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-ARM64-Setup-{versione}.exe) |
| **Linux** (Wayland con KDE Plasma o GNOME) | [**cedro-{versione}-linux-x64.tar.gz**](https://github.com/{repo}/releases/download/v{versione}/cedro-{versione}-linux-x64.tar.gz) |

Scarica il file e fai doppio clic: si installa sopra la versione che hai (su Linux: estrai il tar.gz e lancia `./install.sh`, senza permessi di amministratore). Se hai già Cedro non serve nemmeno questo: si aggiorna da solo («Controlla aggiornamenti» nel menu dell'icona).
Non sai quale ti serve? *Impostazioni di Windows → Sistema → Informazioni*: se in «Tipo di sistema» c'è «basato su ARM», prendi quello ARM; altrimenti l'altro.
I motori della trascrizione non vanno scaricati a mano: Cedro li prende da solo la prima volta che trascrivi.

## Novità della {versione}

**La trascrizione riconosce chi parla.**

- Quando premi Trascrivi, Cedro chiede «Quante persone parlano?». Puoi lasciare Automatico, che le conta da solo, oppure scegliere un numero da 1 a 8. Con 1 il riconoscimento non si fa e la trascrizione è veloce come prima.
- Nel testo ogni frase ha il suo interlocutore («Interlocutore 1», «Interlocutore 2»…) e ogni volta che cambia voce si va a capo. In cima al Word c'è la legenda delle voci, ognuna col suo colore e i minuti in cui ha parlato.
- In Rivedi trascrizione, sotto l'onda, c'è una striscia colorata che mostra chi parla. Nella scheda «Chi parla» dai i nomi (cambiano in tutte le frasi), ascolti un pezzo di ogni voce e con «Unisci a…» rimetti insieme una persona che Cedro ha diviso in due. Per cambiare la voce di una sola frase basta un clic sul suo nome.
- Si salva in Word, testo, .srt e .vtt, con o senza i nomi.
- Tutto resta sul PC. La prima volta Cedro scarica il riconoscimento delle voci, circa 40 MB.
