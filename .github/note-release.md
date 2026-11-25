## Scarica questo

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** (quasi tutti i PC) | [**Cedro-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Setup-{versione}.exe) |
| **Windows portable** (x64, senza installazione) | [**Cedro-Portable-{versione}-x64.zip**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Portable-{versione}-x64.zip) |
| **Linux** (Wayland con KDE Plasma o GNOME) | [**cedro-{versione}-linux-x64.tar.gz**](https://github.com/{repo}/releases/download/v{versione}/cedro-{versione}-linux-x64.tar.gz) |

Scarica il file e fai doppio clic: si installa sopra la versione che hai (su Linux: estrai il tar.gz e lancia `./install.sh`, senza permessi di amministratore). Se hai già Cedro non serve nemmeno questo: si aggiorna da solo («Controlla aggiornamenti» nel menu dell'icona).
I motori della trascrizione non vanno scaricati a mano: Cedro li prende da solo la prima volta che trascrivi.

## Novità della {versione}

**Cedro 4 Labs legge i file esportati dagli strumenti di laboratorio: trascini l'export e vedi la piastra con i valori veri.**

- **Dove.** In «Converti dati e piastre» trascini il file esportato dallo strumento (CSV, TXT, Excel, XML, JSON): Cedro riconosce da solo lo strumento e al centro mostra la piastra colorata con i valori, oppure l'elenco delle misure e i dati dello strumento. Se sbaglia, «Non è questo? Cambia strumento».
- **34 formati di 5 famiglie di strumenti.** Lettori di piastre (Tecan Magellan, Molecular Devices SoftMax Pro, Agilent BioTek Gen5, BMG Labtech MARS e SMART Control, PerkinElmer EnVision, Revvity Kaleido, Thermo Fisher SkanIt), qPCR e PCR digitale (Applied Biosystems QuantStudio e Design & Analysis, Bio-Rad CFX Maestro, Qiagen QIAcuity, Thermo Fisher QuantStudio Absolute Q, Diomni), spettrofotometri e fluorimetri (NanoDrop One, Eight e 8000, Qubit 4 e Flex, Genesys 30 e On-Board, VISIONlite, Unchained Labs Lunatic & Stunner), contacellule (Beckman Coulter Vi-CELL BLU e XR, ChemoMetec NucleoView e NC-View, Roche Cedex HiRes, Revvity Matrix), multiplex (Bio-Rad Bio-Plex Manager, MSD Discovery Workbench e Methodical Mind, Luminex xPONENT e INTELLIFLEX). Ogni formato legge correttamente tutti i file d'esempio della libreria pubblica allotropy.
- **I ruoli dei pozzetti.** Clicchi o trascini sui pozzetti e li segni come bianco, standard, controllo o campione; Cedro mostra media, DS e CV della scelta e, se vuoi, i valori meno il bianco.
- **Esce un Excel pulito.** Mappa a colori della piastra, un foglio con una riga per pozzetto (o per campione), spettri e cinetiche, valori calcolati, analiti e i dati dello strumento. A richiesta anche CSV, **Allotrope (.asm.json)** per LIMS e archivi dati, con i calcoli di Cedro separati dai dati grezzi, e CSV piatto.
- **Si sa da dove vengono i dati.** Nei «Lavori» e nel testo per il quaderno di laboratorio restano lo strumento e l'impronta SHA-256 del file originale, che non viene mai toccato.
- **Anche dalla riga di comando**, per chi vuole convertire tanti export in automatico.
- **Da sapere.** Alcuni messaggi d'errore specifici dei singoli strumenti e i nomi dei fogli Excel restano in italiano anche con Cedro in un'altra lingua; i citofluorimetri (FCS) non sono ancora letti.
