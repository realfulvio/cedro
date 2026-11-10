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

**Versione di sola correzione: ho riguardato tutto il codice e ho sistemato errori e punti deboli, senza funzioni nuove.**

- Firma digitale: un PDF costruito apposta con oggetti nascosti nello spazio della firma non risulta più «firma valida»; PDF e certificati rovinati non fanno più chiudere la verifica; il PIN resta in memoria solo se serve.
- Video: AVI con misure dispari, MKV di registrazioni interrotte e audio PCM dei MOV delle fotocamere ora si convertono; un pacchetto audio rovinato non fa più andare l'audio avanti al video.
- Trascrizione e riassunto: se la scheda video non parte si prosegue col processore; se il download di un modello nuovo non riesce si usa quello già presente; i file di testo a metà vengono tolti.
- Aggiornamenti su Linux più sicuri: se si spegne il PC a metà l'installazione non lascia un programma rotto.
- Linux: aprire un file con il nome in codifica non UTF-8 non chiude più Cedro.
- Interfaccia: niente più «caricamento» infinito o finestra che si ridisegna da ferma quando un'anteprima, un'onda audio o una verifica va male; meno memoria usata dalle immagini preparate; il lettore non si blocca se l'audio è rovinato.
- Windows: le finestre di Conversioni, Verifica, Guida e Impostazioni non restano invisibili se Windows non riesce ad aprirle.
