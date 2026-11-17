## Scarica questo

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** (quasi tutti i PC) | [**Cedro-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Setup-{versione}.exe) |
| **Windows portable** (x64, senza installazione) | [**Cedro-Portable-{versione}-x64.zip**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Portable-{versione}-x64.zip) |
| **Linux** (Wayland con KDE Plasma o GNOME) | [**cedro-{versione}-linux-x64.tar.gz**](https://github.com/{repo}/releases/download/v{versione}/cedro-{versione}-linux-x64.tar.gz) |

Scarica il file e fai doppio clic: si installa sopra la versione che hai (su Linux: estrai il tar.gz e lancia `./install.sh`, senza permessi di amministratore). Se hai già Cedro non serve nemmeno questo: si aggiorna da solo («Controlla aggiornamenti» nel menu dell'icona).
I motori della trascrizione non vanno scaricati a mano: Cedro li prende da solo la prima volta che trascrivi.

## Novità della {versione}

**Sei novità per Cedro 4 Labs.**

- **Vetrini Olympus VSI:** Cedro apre i file `.vsi` di cellSens con la loro cartella `_nome_`: campo chiaro come vetrino (livelli, ingrandimento, µm per pixel), fluorescenza come pila con i canali per nome (DAPI, FITC, Cy3…), piani Z e istanti. Anche in copia anonima (etichetta, foto del vetrino e nome dell'operatore tolti) e nel pacchetto per il consulto.
- **CD e chiavette DICOM:** trascinando la cartella del CD Cedro legge il DICOMDIR, prende tutte le immagini nell'ordine paziente, studio, serie e lascia fuori il programma di visualizzazione del CD. Anonimizzi o converti tutto in un colpo.
- **Referti dettati già impaginati:** con «Referti veterinari» la dettatura esce come un referto del laboratorio: logo e nome della clinica in testa, caso, specie, campione e data, le parti dette («Descrizione macroscopica», «Diagnosi», «Commento»…) con il loro titolo, «punto e a capo» rispettato. Finché i punti da riascoltare non sono confermati c'è «BOZZA · da non firmare» in testa a ogni pagina; il referto rivisto ha la riga della firma.
- **Mappa delle piastre a colori:** l'Excel delle piastre ha il foglio «Mappa» con i valori colorati dal blu al rosso, minimo e massimo, pronto da stampare su una pagina.
- **Registro dei lavori in Excel:** nella pagina Lavori il bottone «Registro in Excel» salva tutti i lavori ricordati, una riga per file uscito, con data, ora, attività, caso e nota, con i filtri già attivi.
- **Copia per il quaderno subito:** a lavoro finito il pannello del risultato ha «Copia per il quaderno», senza passare dalla pagina Lavori.
