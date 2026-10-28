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

- **Meno memoria con gli audio lunghi**: anteprima dell'onda e conversione WAV standard lavorano a pacchetti. Anche la preparazione dell'audio per la trascrizione conserva solo i campioni necessari al filtro, senza cambiare precisione o modello.
- **Una coda comune per i lavori pesanti**: ruota e pannelli si coordinano; i lavori in attesa si possono annullare. Gli aggiornamenti silenziosi aspettano anche le conversioni dei pannelli e le trascrizioni.
- **iGPU e Vulkan restano attivi**: se la RAM libera è limitata Cedro può usare la CPU con lo stesso modello; se il motore GPU si interrompe riprova il file sulla CPU, mantenendo la precisione scelta.

- **Aggiornamenti silenziosi**: all'accensione del computer Cedro non disturba più. Aspetta due minuti, controlla e scarica a priorità minima, senza finestre né richieste; su Windows installa quando non c'è una conversione in corso e si riapre da solo, su Linux installa e parte aggiornato al prossimo avvio. Al massimo compare un avviso discreto «Cedro è stato aggiornato alla …». Dal menu «Controlla aggiornamenti» resta la domanda di sempre.

- **Tutti i temi controllati con le ultime voci** (Archivi, ZIP, nome da modello, spia CPU/GPU, «Apri risultato» e «Ricomincia», menu del tasto destro): sistemate le sovrapposizioni nella finestrina del Compatto, in Windows essenziale (Semplificata) e in Documento al centro, dove spia e «Ricomincia» non si vedevano.
- Archivi: l'intestazione dice ZIP → ZIP, i nomi delle schede nel Percorso guidato non si tagliano più, «1 archivio» e «1 immagine» al singolare.
- Inglese: tradotte le voci del menu del tasto destro e «Impostazioni archivi»; etichette accorciate perché non si taglino.
