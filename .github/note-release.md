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

**Versione di manutenzione**: nessuna funzione nuova, solo errori trovati rileggendo tutto il codice di Windows, Windows on ARM e Linux.

- «Ottimizza per il web» non manda più in crash Cedro con un MP4 rovinato.
- Un download interrotto (modelli della trascrizione, motori, yt-dlp, ffmpeg, deno) non lascia più sul disco file a metà da centinaia di MB.
- Se chiudi Cedro mentre trascrive, il motore si ferma subito invece di tenere il processore al massimo fino alla fine del file.
- Su Linux i file provvisori della trascrizione e dell'aggiornamento stanno in una cartella che solo tu puoi leggere.
- Un documento Word costruito apposta per gonfiarsi a GB non riempie più la memoria.
- «Duplica» su un file senza estensione crea «nome-copia» (prima «nome-copia.copia»).
- Le impostazioni non si perdono se il PC si spegne mentre le salva; il registro attività non cresce più all'infinito.
- Su Windows le conversioni video una dopo l'altra sono più affidabili.

Dalla 0.18.4: guida iniziale rifatta in sei passi, Linux che si aggiorna da solo, riga rossa degli errori che sparisce da sola.

Dalla 0.18.3: su Linux le finestre restano chiare e leggibili anche col tema scuro di KDE o GNOME.

Dalla 0.18.2: trascrizione italiana molto più precisa e nuova scheda «Video dal web».

Dalla 0.18.0: versione nativa per Windows on ARM, con trascrizione ottimizzata (base, dotprod, i8mm e Vulkan per le schede Adreno), scheda Sistema con le istruzioni del processore, YouTube con strumenti ARM64 e la nuova firma col logo hcok.

L'installatore non è firmato: la prima volta Windows può dire «Windows ha protetto il PC». Clicca «Ulteriori informazioni», poi «Esegui comunque». Con Smart App Control acceso va spento dalle impostazioni di Windows.
