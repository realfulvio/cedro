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

**PDF → Excel legge anche le tabelle dei PDF scansionati.**

- Con «Leggi le scansioni (OCR)» acceso, le pagine che sono foto o scansioni danno tabelle vere di Excel: Cedro raddrizza la pagina storta, ritrova le righe e le colonne (con o senza linee), tiene unite le celle scritte su due righe e unisce le tabelle che continuano sulla pagina dopo.
- Numeri e date vengono riconosciuti; i decimali che l'OCR legge con il punto («80.00») tornano con la virgola («80,00»); la prima riga è l'intestazione quando si capisce.
- Il testo fuori dalle tabelle va nel foglio del testo, riga per riga. Se le scansioni non vengono lette (opzione spenta), Cedro lo dice.
- Su Linux, senza i dati italiani di Tesseract si usa quelli inglesi, che leggono molto meglio del modello incluso.
- Corretto «1 pagine» nei messaggi quando la pagina è una sola.

**Caratteri più grandi e un carattere per ogni tema.**

- Tutti i testi dell'interfaccia crescono di 2 punti, per leggere meglio.
- Ogni tema ha il suo carattere: Windows essenziale quello di Windows, Percorso guidato Atkinson Hyperlegible, Cedro compatto Figtree, Banco di lavoro IBM Plex Sans, Documento al centro Source Sans 3, Comune di Bondeno Titillium Web.
- Ho allargato i punti dove il testo più grande non ci stava: «Aggiungi file», la barra in basso, le schede di Aspetto, la ruota e le scelte «Un foglio per tabella».
