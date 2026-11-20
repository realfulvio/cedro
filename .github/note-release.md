## Scarica questo

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** (quasi tutti i PC) | [**Cedro-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Setup-{versione}.exe) |
| **Windows portable** (x64, senza installazione) | [**Cedro-Portable-{versione}-x64.zip**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Portable-{versione}-x64.zip) |
| **Linux** (Wayland con KDE Plasma o GNOME) | [**cedro-{versione}-linux-x64.tar.gz**](https://github.com/{repo}/releases/download/v{versione}/cedro-{versione}-linux-x64.tar.gz) |

Scarica il file e fai doppio clic: si installa sopra la versione che hai (su Linux: estrai il tar.gz e lancia `./install.sh`, senza permessi di amministratore). Se hai già Cedro non serve nemmeno questo: si aggiorna da solo («Controlla aggiornamenti» nel menu dell'icona).
I motori della trascrizione non vanno scaricati a mano: Cedro li prende da solo la prima volta che trascrivi.

## Novità della {versione}

**Trascrizione italiana più affidabile: niente più frasi ripetute o inventate, e «Massima» diventa large-v3.**

- **Niente più giri di frasi.** Prima whisper si portava il testo da una finestra di 30 secondi alla successiva e, su registrazioni lunghe o disturbate, poteva entrare in un giro (in una registrazione d'ufficio rumorosa: 37 frasi uguali di fila) o inventare parole sui silenzi. Ora Cedro trova i tratti in cui si parla (rilevatore di parlato Silero, 0,9 MB, scaricato la prima volta), trascrive ogni pezzo da solo, senza passare il testo dall'uno all'altro, e scarta le frasi identiche date di fila. Nella stessa registrazione gli errori sicuri del modello turbo scendono dal 17,6 % al 6,2 %; su 15 minuti di frasi con rumore l'errore sulle parole scende dal 4,8 % al 2,8 %.
- **«Massima» ora è large-v3** (1,08 GB, scaricato la prima volta; il modello da 574 MB di prima si toglie da solo). Era stato scartato perché su audio lungo «si incantava», ma la colpa era il testo passato tra le finestre, non il modello. Su frasi disturbate sbaglia il 2,7-2,9 % delle parole (il turbo 2,8-3,3 %) e su una registrazione vera rumorosa ha circa 3 volte meno errori sicuri del turbo. Costa circa il doppio del tempo: 6-7 minuti per un'ora di audio su una scheda video come la Quadro RTX 4000.
- **Veloce e Buona** restano quelli di prima, ma usano anche loro il rilevatore di parlato e il filtro delle ripetizioni.
