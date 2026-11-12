## Scarica questo

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** (quasi tutti i PC) | [**Cedro-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Setup-{versione}.exe) |
| **Windows portable** (x64, senza installazione) | [**Cedro-Portable-{versione}-x64.zip**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Portable-{versione}-x64.zip) |
| **Linux** (Wayland con KDE Plasma o GNOME) | [**cedro-{versione}-linux-x64.tar.gz**](https://github.com/{repo}/releases/download/v{versione}/cedro-{versione}-linux-x64.tar.gz) |

Scarica il file e fai doppio clic: si installa sopra la versione che hai (su Linux: estrai il tar.gz e lancia `./install.sh`, senza permessi di amministratore). Se hai già Cedro non serve nemmeno questo: si aggiorna da solo («Controlla aggiornamenti» nel menu dell'icona).
I motori della trascrizione non vanno scaricati a mano: Cedro li prende da solo la prima volta che trascrivi.

## Novità della {versione}

**Correzioni per Cedro 4 Labs, trovate con un controllo di qualità su file veri.**

- Figura per l'articolo: con un vetrino SVS la barra di scala ora viene dalla misura del vetrino. Prima usava la calibrazione della fotocamera e poteva sbagliare di circa tre volte. La legenda dice proiezione, contrasto usato e da dove viene la barra.
- Anonimizzazione DICOM più completa: vanno via anche i testi dei referti strutturati, telefoni, organizzazioni, scritte sovrapposte (overlay), miniature e tutti gli identificativi dell'esame. Nei file più vecchi (Implicit VR) restavano alcuni nomi e date.
- Le radiografie DICOM compresse senza perdita a più di 8 bit si aprono con i valori giusti.
- Togli i nomi da Word, Excel e PowerPoint: vanno via anche autori e iniziali dei commenti di PowerPoint, i nomi utente di Excel, il percorso del modello collegato e la posizione GPS delle foto dentro il documento. Nei PDF i campi dei moduli restano funzionanti e la firma digitale va via con il certificato.
- Microscopia: si aprono i TIFF a piani separati e quelli a 32 bit, che prima davano errore o uscivano neri. Il contrasto automatico è lo stesso per tutti gli istanti dello stesso canale, così le differenze di intensità restano visibili. Il TIFF a 16 bit tiene la calibrazione in µm (Fiji la legge).
- Si leggono anche le acquisizioni ND2 interrotte, i progetti LIF con una serie rovinata e gli OME-TIFF divisi in più file.
- Vetrini: la copia anonima e le tessere non portano più il nome del file originale. Le tessere AI non vengono ingrandite oltre la scansione.
- Tabelle: «1,234» nei CSV americani non diventa più 1,234. Si tengono le durate oltre le 24 ore, le cifre decimali scritte (1,50) e le celle molto lunghe. Da Excel a CSV il separatore scelto è rispettato.
- Pacchetto di dati: un file rovinato dentro lo ZIP viene segnalato come cambiato invece di fermare la verifica; si controlla anche l'elenco delle impronte stesso.
- Cedro 4 Labs: per un Excel in coda la scheda propone il CSV, per uno ZIP la verifica del pacchetto.
- Nessun blocco del programma con file DICOM, JPEG o vetrini rovinati.
- Radiografie per il proprietario: Cedro non mette mai nello stesso PDF le lastre di animali diversi e salta i DICOM senza immagini (piani di cura, referti) invece di fermarsi.
