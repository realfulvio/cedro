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

**Menu più chiari e colorati, foto ripulite senza perdere qualità, interfaccia più ordinata e viva.**

- Il menu «Formato» ha un'iconcina colorata per ogni voce e nomi che si capiscono: «Ruota di 90° a destra», «Togli GPS e dati nascosti» al posto di «Strumento · …», con una breve spiegazione a fianco e le voci divise fra «Converti in» e gli strumenti.
- Anche i menu delle impostazioni di conversione (dimensioni, pagina, qualità audio, video…) hanno l'iconcina e una spiegazione per ogni scelta.
- «Togli GPS e dati nascosti» funziona come Scrambled Exif: su JPEG, PNG e WebP toglie posizione, fotocamera, data di scatto, miniatura, XMP e commenti senza ricomprimere la foto (qualità identica, file più leggero), tiene il verso giusto e i colori, e salva una copia «-pulito» dicendo cosa ha tolto.
- La ruota al clic destro ora è spenta di default: si accende con «Usa la ruota» in Impostazioni > Ruota. Spenta, su Windows Cedro non tiene più agganci a tastiera e mouse; su Linux il menu del file ha la sola voce «Apri in Cedro».
- Scritte e controlli tutti in riga, etichette delle opzioni in semigrassetto e più spazio fra un'opzione e l'altra.
- Animazioni leggere: pulsanti che sfumano e si premono, icone che si muovono, scheda scelta che scivola, menu in dissolvenza. Si fermano subito a movimento finito e spariscono se nel sistema hai spento gli effetti di animazione.
