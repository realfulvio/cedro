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

**Firma digitale: apri e verifica i file firmati, come con Dike o ArubaSign.**

- Nuova sezione «Firma digitale»: trascini un .p7m, .p7s, .m7m, .tsd, .tsr o un PDF firmato e Cedro ti dice subito se le firme valgono, in verde, giallo o rosso, con una frase chiara.
- Si apre il visualizzatore: il documento al centro (PDF, Word, testo, immagini) e a destra chi ha firmato, quando, e i quattro controlli: documento non modificato, certificato qualificato nell'elenco europeo, revoca, data certa.
- Buste una dentro l'altra, controfirme, firme staccate (.p7s con l'originale accanto), marche temporali (.tsd, .m7m, .tsr) e firme nei PDF, anche più di una.
- PDF cambiato dopo la firma: lo dice e ti fa vedere la versione firmata, com'era al momento della firma, da salvare a parte.
- Verifica alla data: oggi, quando è stato firmato o un altro giorno.
- «Verifica una cartella» (per esempio gli allegati di una PEC): tutti i file firmati in un elenco, quelli da guardare in cima, con un rapporto unico in PDF ed estrazione di tutti i documenti.
- Rapporto di verifica in PDF per ogni file, da allegare agli atti.
- Doppio clic sui file firmati: si aprono in Cedro (si può spegnere; per i PDF firmati è una scelta a parte).
- L'elenco europeo dei certificatori si aggiorna da solo ogni giorno; la revoca si controlla online (OCSP/CRL), e si può spegnere.
- Da terminale: `cedro firma <file>` stampa l'esito.
