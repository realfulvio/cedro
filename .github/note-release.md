## Scarica questo

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** (quasi tutti i PC) | [**Cedro-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Setup-{versione}.exe) |
| **Windows on ARM** (Snapdragon X e simili) | [**Cedro-ARM64-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-ARM64-Setup-{versione}.exe) |
| **Linux** (Wayland con KDE Plasma o GNOME) | [**cedro-{versione}-linux-x64.tar.gz**](https://github.com/{repo}/releases/download/v{versione}/cedro-{versione}-linux-x64.tar.gz) |

Scarica il file e fai doppio clic: si installa sopra la versione che hai (su Linux: estrai il tar.gz e lancia `./install.sh`, senza permessi di amministratore). Se hai già Cedro non serve nemmeno questo: si aggiorna da solo («Controlla aggiornamenti» nel menu dell'icona).
Non sai quale ti serve? *Impostazioni di Windows → Sistema → Informazioni*: se in «Tipo di sistema» c'è «basato su ARM», prendi quello ARM; altrimenti l'altro.
I motori della trascrizione non vanno scaricati a mano: Cedro li prende da solo la prima volta che trascrivi.

### Comune di Bondeno

Il Comune di Bondeno non è più un'edizione a parte: è un tema di Cedro, in Impostazioni → Aspetto, con i colori, il carattere e lo stemma del Comune. Chi aveva l'edizione per il Comune si aggiorna da sola e si ritrova il tema già acceso. I file `Cedro-Bondeno-…` e `cedro-bondeno-…` di questa release sono copie degli stessi installatori, tenute solo perché la vecchia edizione cerca quei nomi: chi installa da zero prende i file qui sopra.

## Novità della {versione}

Versione di **correzioni e ottimizzazione**: tutta l'app desktop riletta (74 problemi trovati e corretti), nessuna funzione tolta.

**Più veloce** (stesso PC, prima → dopo):
- Video H.264 → MP4: **−43%** del tempo (decodifica e codifica in parallelo); WebM → MP4 **−17%** anche con l'audio in più, fino a −75% sui video lunghi; AVI con audio di 10 minuti da 73 s a 9 s.
- PDF → Word **−54%**, PDF → immagine **−23%**, PDF → testo −13%.
- Code con centinaia di file: l'interfaccia ridisegna in decine di millisecondi invece che in secondi.
- Programma più leggero: **−22%** di dimensione.

**Correzioni principali**:
- «Sostituisci» non cancella più il file vecchio se la conversione fallisce; «Salta il file» salta prima di lavorare.
- La trascrizione si ferma davvero con **Annulla**; yt-dlp, ffmpeg e deno non restano accesi se chiudi Cedro.
- Video: audio e immagini restano allineati anche dopo ore; audio 5.1/7.1 con le voci del canale centrale; WebM dei browser con la durata giusta; WebM con audio Opus su Linux non esce più muto; MP4 oltre 4 GB; AVI grandi (OpenDML) e registrazioni interrotte.
- Word: caselle di testo non più doppie, niente tabulazioni spurie; ZIP oltre 4 GB; file con nomi non UTF-8 su Linux.
- Windows: le conversioni dalla ruota non rallentano più mouse e tastiera; AltGr funziona nei campi (@ # [ ] €); niente perdite di risorse aprendo le Impostazioni.
- OCR su Linux rispetta la lingua scelta; impostazioni salvate in ANSI/UTF-16 non si azzerano più.

Dalla 0.23.2: tasto destro nei campi di testo (Taglia, Copia, Incolla…) e link di altri siti nella scheda YouTube. Dalla 0.23.1: video H.264 con fotogrammi molto piccoli, audio Opus dei WebM e AVI completi (audio MP3/AAC, H.264, XVID e DivX). Dalla 0.23.0: anteprime per tutti i file nel tema «Documento al centro», con il tasto **Play** per ascoltare l'audio e guardare il video.

Dalla 0.22.0: Cedro parla inglese (Impostazioni → Lingua) e l'installatore di Windows chiede la lingua. Dalla 0.21.3: il doppio clic sull'icona di Windows apre il pannello Conversioni.

Dalla 0.21.2: video dal web da pagine qualunque (come magnific.com) e messaggio di rete più preciso. Dalla 0.21.1: yt-dlp, deno e ffmpeg si aggiornano da soli.

Dalla 0.21.0: il Comune di Bondeno è un tema di Cedro (Impostazioni → Aspetto) e la nuova grafica con cinque aspetti a scelta.

Dalla 0.19.1 alla 0.20.1: edizione Comune di Bondeno, una versione a parte di Cedro con lo stemma ufficiale del Comune; dalla 0.21.0 è un tema della versione standard.

Dalla 0.19.0: Modalità Semplice, un'interfaccia essenziale da attivare in Impostazioni → Generale (le altre opzioni restano a un clic in «Altre opzioni»), scelta anche alla fine dell'installazione.

Dalla 0.18.5: versione di manutenzione con correzioni di robustezza e sicurezza trovate rileggendo il codice.

Dalla 0.18.4: guida iniziale rifatta in sei passi, Linux che si aggiorna da solo, riga rossa degli errori che sparisce da sola.

Dalla 0.18.3: su Linux le finestre restano chiare e leggibili anche col tema scuro di KDE o GNOME.

Dalla 0.18.2: trascrizione italiana molto più precisa e nuova scheda «Video dal web».

Dalla 0.18.0: versione nativa per Windows on ARM, con trascrizione ottimizzata (base, dotprod, i8mm e Vulkan per le schede Adreno), scheda Sistema con le istruzioni del processore, YouTube con strumenti ARM64 e la nuova firma col logo hcok.

L'installatore non è firmato: la prima volta Windows può dire «Windows ha protetto il PC». Clicca «Ulteriori informazioni», poi «Esegui comunque». Con Smart App Control acceso va spento dalle impostazioni di Windows.
