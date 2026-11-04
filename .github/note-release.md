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

**La trascrizione è più precisa**, soprattutto sui messaggi vocali e sul parlato alla buona.

- Il testo esce con punteggiatura e maiuscole: prima, sul parlato informale, poteva uscire tutto minuscolo e in un blocco unico. Con le frasi chiuse dai punti anche i paragrafi tornano al loro posto.
- Whisper valuta più ipotesi per ogni frase invece di prendere la prima: meno parole storpiate o spezzate a metà.
- «Massima» usa ora il modello large-v3 completo (1,1 GB) al posto del turbo: è il più preciso, e il turbo già scaricato si cancella da solo quando arriva il nuovo.
- Niente più «Grazie.» inventato in fondo alla trascrizione quando nessuno l'ha detto.
