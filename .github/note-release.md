## Scarica questo

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** (quasi tutti i PC) | [**Cedro-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Setup-{versione}.exe) |
| **Windows portable** (x64, senza installazione) | [**Cedro-Portable-{versione}-x64.zip**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Portable-{versione}-x64.zip) |
| **Linux** (Wayland con KDE Plasma o GNOME) | [**cedro-{versione}-linux-x64.tar.gz**](https://github.com/{repo}/releases/download/v{versione}/cedro-{versione}-linux-x64.tar.gz) |

Scarica il file e fai doppio clic: si installa sopra la versione che hai (su Linux: estrai il tar.gz e lancia `./install.sh`, senza permessi di amministratore). Se hai già Cedro non serve nemmeno questo: si aggiorna da solo («Controlla aggiornamenti» nel menu dell'icona).
I motori della trascrizione non vanno scaricati a mano: Cedro li prende da solo la prima volta che trascrivi.

## Novità della {versione}

**Trascrizione italiana molto più precisa in tutti e tre i livelli: niente più frasi ripetute o inventate, e modelli più grandi.**

- **Niente più giri di frasi.** Prima whisper si portava il testo da una finestra di 30 secondi alla successiva e, su registrazioni lunghe o disturbate, poteva entrare in un giro (in una registrazione d'ufficio rumorosa: 37 frasi uguali di fila) o inventare parole sui silenzi. Ora Cedro trova i tratti in cui si parla (rilevatore di parlato Silero, 0,9 MB, scaricato la prima volta), trascrive ogni pezzo da solo, senza passare il testo dall'uno all'altro, e scarta le frasi identiche date di fila.
- **Ogni livello sale di un gradino.** Su 15 minuti di frasi italiane con rumore (errori sulle parole, contro il testo vero):
  - **Veloce**: da small a **large-v3-turbo** (574 MB), errori dall'8,7 % al 2,8 %;
  - **Buona**: da medium a **large-v3** (1,08 GB), dal 4,6 % al 2,7 %;
  - **Massima**: da large-v3-turbo a **large-v3 con una ricerca più ampia** (8 ipotesi invece di 5), dal 4,8 % al 2,7 %.
- **Referti dettati (Cedro 4 Labs) più giusti.** Su quattro referti veterinari dettati, con il glossario veterinario, «Massima» sbaglia il 12,2 % delle parole invece del 17,3 % e scrive giusti il 75 % dei termini tecnici invece del 68 %.
- **Costa più tempo, come previsto.** Con una scheda video (prova su Quadro RTX 4000) un'ora di audio richiede circa 3-4 minuti con «Veloce» e 7-8 minuti con «Buona» e «Massima». Senza scheda video Cedro parte da «Veloce», che sul processore (prova su Ryzen 7 5750G) richiede circa mezz'ora per un'ora di audio (con small ne bastavano circa 10); «Buona» e «Massima» sul processore ci mettono più o meno quanto dura l'audio. I modelli di prima (small e medium) si tolgono da soli quando arriva quello nuovo.
