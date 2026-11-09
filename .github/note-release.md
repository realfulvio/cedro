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

**Firma digitale: ora firmi anche, con la chiavetta USB o la smart card (CNS), come con ArubaSign o Dike.**

- «Firma documenti» in tre passi: scegli i file (anche tanti insieme, o una cartella), come firmarli, poi scrivi il PIN una volta sola e firmo tutto.
- Due formati: la busta .p7m (CAdES) per qualsiasi file, e il PDF firmato (PAdES) con la firma dentro il PDF. In automatico i PDF restano PDF e gli altri diventano .p7m; puoi anche scegliere tutto .p7m o tutto PDF (i file non PDF li trasformo prima in PDF).
- Firma visibile nel PDF: nello spazio per la firma se c'è, dove la trascini tu sulla pagina, o invisibile. Nel riquadro nome, data e ora, e se vuoi uno stemma o la firma autografa scansionata.
- File già firmati: aggiungi la tua firma accanto a quelle che ci sono, sopra come controfirma, o in una busta nuova. Sui PDF firmati la nuova firma si aggiunge senza rompere quelle di prima.
- I file firmati vanno accanto agli originali (che non vengono toccati) o nella cartella che scegli; alla fine li apri nel visualizzatore per controllarli.
- «Chiavetta e certificati»: la chiavetta o la carta collegata, i suoi certificati con la scadenza, e cambio del PIN, sblocco con il PUK e cambio del PUK.
- Ti avviso quando il certificato di firma sta per scadere.
- Funziona con le chiavette Aruba (bit4id), le CNS e tessere sanitarie (Athena, Incard) e con OpenSC; il driver si trova da solo o si sceglie a mano in Opzioni.
- Trascinando file sulla sezione: quelli già firmati si aprono e si verificano, gli altri si preparano per la firma.
