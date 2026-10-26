**Versione nativa per Windows on ARM**: per i PC con processore ARM (Snapdragon X e simili) c'è un installatore a parte, **Cedro-Setup-0.18.0-arm64.exe**. Cedro gira senza emulazione, quindi è più veloce e consuma meno batteria.

**Trascrizione ottimizzata per ARM**: il motore di trascrizione ha tre versioni per ARM (base, dotprod e i8mm) e Cedro sceglie da solo la più veloce che il processore sa eseguire. Sui Snapdragon con scheda video Adreno può usare anche la versione Vulkan, come su x64.

**Scheda Sistema**: sui PC ARM le Impostazioni mostrano le istruzioni del processore (NEON, dotprod, i8mm). L'interruttore «Accelerazione i8mm» prende il posto di «Accelerazione AVX-512».

**YouTube su ARM**: yt-dlp, FFmpeg e Deno vengono scaricati nelle versioni native ARM64.

Questa versione è solo per Windows. Linux è in pausa fino alla 0.20.

Si aggiorna da solo: all'avvio Cedro vede questa versione e chiede se installarla, e su un PC ARM prende l'installatore ARM64. Oppure scarica il file qui sotto (**Cedro-Setup-0.18.0.exe** per i PC normali, **Cedro-Setup-0.18.0-arm64.exe** per Windows on ARM) e fai doppio clic: si installa sopra la versione precedente. I file `cedro-trascrivi-…` sono i motori della trascrizione: non serve scaricarli a mano, li prende Cedro la prima volta che trascrivi.

L'installatore non è firmato: la prima volta Windows può dire «Windows ha protetto il PC» (SmartScreen) o, con Smart App Control acceso, bloccarlo. Clicca «Ulteriori informazioni», poi «Esegui comunque»; con Smart App Control serve spegnerlo dalle impostazioni di Windows.

Gli SHA-256 di ogni file sono accanto al nome, qui sotto.
