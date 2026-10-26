## Scarica questo

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** (quasi tutti i PC) | [**Cedro-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Setup-{versione}.exe) |
| **Windows on ARM** (Snapdragon X e simili) | [**Cedro-ARM64-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-ARM64-Setup-{versione}.exe) |

Scarica il file e fai doppio clic: si installa sopra la versione che hai. Se hai già Cedro non serve nemmeno questo: si aggiorna da solo («Controlla aggiornamenti» nel menu dell'icona).
Non sai quale ti serve? *Impostazioni di Windows → Sistema → Informazioni*: se in «Tipo di sistema» c'è «basato su ARM», prendi quello ARM; altrimenti l'altro.
I motori della trascrizione non vanno scaricati a mano: Cedro li prende da solo la prima volta che trascrivi.

## Novità della {versione}

**«Controlla aggiornamenti» di nuovo a posto**: con la 0.18.0, aggiornando da una 0.17 su un PC Windows normale, Cedro prendeva l'installatore per ARM e Windows rispondeva «Questo programma non supporta la versione di Windows installata». Ora l'installatore per ARM ha un nome diverso (**Cedro-ARM64-Setup**) e ogni versione prende quello giusto. L'aggiornamento da una 0.17 vera è provato in automatico prima e dopo ogni pubblicazione.

**Pagina più chiara**: qui sopra ci sono solo i file da scaricare. I motori di trascrizione stanno in una pagina a parte.

Dalla 0.18.0: versione nativa per Windows on ARM, con trascrizione ottimizzata (base, dotprod, i8mm e Vulkan per le schede Adreno), scheda Sistema con le istruzioni del processore, YouTube con strumenti ARM64 e la nuova firma col logo hcok.

L'installatore non è firmato: la prima volta Windows può dire «Windows ha protetto il PC». Clicca «Ulteriori informazioni», poi «Esegui comunque». Con Smart App Control acceso va spento dalle impostazioni di Windows.
