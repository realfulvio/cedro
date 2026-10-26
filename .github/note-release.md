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

**Trascrizione italiana molto più precisa**: abbiamo misurato la trascrizione su 200 frasi italiane (parlato pulito e un «messaggio vocale» disturbato) e la vecchia «Buona» sbagliava il 7,5% delle parole (il 17% nel messaggio vocale). Ora i tre livelli sono più bravi: **Veloce** (264 MB) 7,5%, **Buona** (539 MB) 4,1%, **Massima** (574 MB) 2,6% e 5,7% nel messaggio vocale. Se il PC ha una scheda video che Cedro può usare, si parte da «Massima» (circa due minuti per un'ora di audio). I modelli vecchi che non servono più si tolgono da soli dopo il primo scaricamento.

**Nuova scheda «Video dal web»**, sotto YouTube: scarica video e audio dalle pagine di molti altri siti (RaiPlay, Vimeo, Internet Archive e oltre mille altri supportati da yt-dlp), con messaggi chiari se un sito non è supportato o il video è protetto.

Dalla 0.18.0: versione nativa per Windows on ARM, con trascrizione ottimizzata (base, dotprod, i8mm e Vulkan per le schede Adreno), scheda Sistema con le istruzioni del processore, YouTube con strumenti ARM64 e la nuova firma col logo hcok.

L'installatore non è firmato: la prima volta Windows può dire «Windows ha protetto il PC». Clicca «Ulteriori informazioni», poi «Esegui comunque». Con Smart App Control acceso va spento dalle impostazioni di Windows.
