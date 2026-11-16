## Scarica questo

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** (quasi tutti i PC) | [**Cedro-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Setup-{versione}.exe) |
| **Windows portable** (x64, senza installazione) | [**Cedro-Portable-{versione}-x64.zip**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Portable-{versione}-x64.zip) |
| **Linux** (Wayland con KDE Plasma o GNOME) | [**cedro-{versione}-linux-x64.tar.gz**](https://github.com/{repo}/releases/download/v{versione}/cedro-{versione}-linux-x64.tar.gz) |

Scarica il file e fai doppio clic: si installa sopra la versione che hai (su Linux: estrai il tar.gz e lancia `./install.sh`, senza permessi di amministratore). Se hai già Cedro non serve nemmeno questo: si aggiorna da solo («Controlla aggiornamenti» nel menu dell'icona).
I motori della trascrizione non vanno scaricati a mano: Cedro li prende da solo la prima volta che trascrivi.

## Novità della {versione}

**Correzioni dal banco di prova contro i programmi del settore.**

- **Anonimizzazione DICOM:** ora segue il profilo base dello standard anche per gli orari (spostati di un valore fisso per paziente, così l'ordine nel tempo resta), il fuso orario, il riempimento, il mezzo di contrasto e i dati della sperimentazione.
- **Firma digitale:** una firma con SHA-1 dà un avviso e non più il via libera pieno; un PDF cambiato dopo la firma (impostazioni, metadati, script) viene segnalato.
- **PDF:** «Dividi» e «Togli pagine» tengono titolo e metadati e dicono che gli allegati restano solo nel file originale; «Togli dati nascosti» vede anche gli allegati dentro i nomi.
- **Trascrizione:** niente più parole inventate su silenzio, ronzii, soffi e toni: Cedro dice «nessun parlato riconosciuto».
- **Word in PDF:** se il testo è in arabo o ebraico, Cedro avvisa che l'ordine e la forma delle lettere non sono ancora resi bene.
- **File .doc rovinati:** un .doc con i conteggi sbagliati non riempie più la memoria (prima fino a 4,9 GB).
