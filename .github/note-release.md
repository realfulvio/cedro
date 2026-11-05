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

**Riassunto AI più preciso su decisioni e votazioni.** Il modello resta Qwen3.5-4B (provati altri quattro modelli dello stesso peso, nessuno era migliore); sono cambiate le istruzioni.

- Nelle riunioni lunghe le decisioni non si perdono più: prima gli appunti di ogni parte si fermavano a metà e un consiglio comunale con cinque votazioni poteva finire in «Decisioni: Nessuna».
- In «Decisioni» ci sono anche gli emendamenti respinti e i punti rinviati, con l'esito del voto («all'unanimità» solo quando è stato detto, altrimenti a maggioranza con favorevoli, contrari e astenuti).
- Le dichiarazioni di voto dei gruppi e le proposte non vengono più scambiate per decisioni, e nelle riunioni senza votazioni non compaiono voti inventati.
- «Chi ha detto cosa» è più corto, una frase per persona, così il riassunto non si interrompe prima di decisioni e compiti.
