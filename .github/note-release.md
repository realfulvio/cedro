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

- **A lavoro finito il pulsante cambia**: dopo una conversione o una trascrizione il pulsante grande diventa **«Apri risultato»** (o **«Apri cartella»** se i file sono più di uno), con sotto il link **«Ricomincia»** che svuota la lista. Se aggiungi file nuovi o cambi le impostazioni torna «Converti» o «Trascrivi», e conta solo i file ancora da fare: un lavoro finito non si rifà per sbaglio. Uguale in tutte le schede e in tutti gli aspetti.
- **Una spia discreta durante il lavoro**: sotto il pulsante compare un puntino con quello che sta davvero accelerando il lavoro, per esempio «CPU · AVX-512 · 8 thread», «CPU · AVX2», «GPU · Vulkan» o «GPU · NVIDIA NVENC». Si vede solo quando CPU o scheda video sono in uso e sparisce a lavoro finito.

Dalla 0.25.0: nuovi formati, tutti in Rust e senza programmi esterni. Foto: AVIF e JPEG XL (anche in scrittura), SVG, ICO, TGA, QOI, PNM, HDR, EXR e DDS. Audio: FLAC, AIFF e Opus in uscita; CAF e ADPCM in lettura. Gli ZIP si convertono dentro, e il nome del file di uscita si può scrivere a modello (`%nome%_%data%`).

Dalla 0.24.0: tutta l'app desktop riletta (74 problemi corretti) e più veloce: video H.264 → MP4 −43% del tempo, PDF → Word −54%, programma più leggero del 22%. «Sostituisci» non cancella più il file vecchio se la conversione fallisce; la trascrizione si ferma davvero con **Annulla**; audio e video restano allineati anche dopo ore; ZIP e MP4 oltre 4 GB; AltGr funziona nei campi su Windows.
