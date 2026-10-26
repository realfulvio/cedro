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

**Modalità Semplice**: un'interfaccia essenziale, da attivare quando vuoi.

- Nel pannello Conversioni restano solo trascinare i file, scegliere il formato e premere «Converti». Video dal web è il link più «Video» o «Solo audio»; Trascrivi usa le impostazioni automatiche.
- Le altre opzioni non spariscono: sono a un clic, in «Altre opzioni».
- Si cambia in Impostazioni → Generale → «Modalità Semplice».
- Alla fine dell'installazione (su Linux al primo avvio) una finestra con due anteprime animate ti fa scegliere tra Semplice e Completa. Negli aggiornamenti non ti viene richiesto.

Dalla 0.18.5: versione di manutenzione con correzioni di robustezza e sicurezza trovate rileggendo il codice.

Dalla 0.18.4: guida iniziale rifatta in sei passi, Linux che si aggiorna da solo, riga rossa degli errori che sparisce da sola.

Dalla 0.18.3: su Linux le finestre restano chiare e leggibili anche col tema scuro di KDE o GNOME.

Dalla 0.18.2: trascrizione italiana molto più precisa e nuova scheda «Video dal web».

Dalla 0.18.0: versione nativa per Windows on ARM, con trascrizione ottimizzata (base, dotprod, i8mm e Vulkan per le schede Adreno), scheda Sistema con le istruzioni del processore, YouTube con strumenti ARM64 e la nuova firma col logo hcok.

L'installatore non è firmato: la prima volta Windows può dire «Windows ha protetto il PC». Clicca «Ulteriori informazioni», poi «Esegui comunque». Con Smart App Control acceso va spento dalle impostazioni di Windows.
