## Scarica questo

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** (quasi tutti i PC) | [**Cedro-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Setup-{versione}.exe) |
| **Windows portable** (x64, senza installazione) | [**Cedro-Portable-{versione}-x64.zip**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Portable-{versione}-x64.zip) |
| **Linux** (Wayland con KDE Plasma o GNOME) | [**cedro-{versione}-linux-x64.tar.gz**](https://github.com/{repo}/releases/download/v{versione}/cedro-{versione}-linux-x64.tar.gz) |

Scarica il file e fai doppio clic: si installa sopra la versione che hai (su Linux: estrai il tar.gz e lancia `./install.sh`, senza permessi di amministratore). Se hai già Cedro non serve nemmeno questo: si aggiorna da solo («Controlla aggiornamenti» nel menu dell'icona).
I motori della trascrizione non vanno scaricati a mano: Cedro li prende da solo la prima volta che trascrivi.

## Novità della {versione}

**I termini tecnici dei referti dettati escono giusti molto più spesso.**

- **Correzione dei termini dopo la trascrizione.** Con il glossario veterinario («Termini dei referti veterinari») whisper scrive bene quasi tutto, ma i termini rari li sbaglia spesso di una o due lettere: «mastocitooma», «vimblastina», «emangio sarcoma», «lagoaspirato». Ora Cedro confronta ogni parola con un dizionario di circa 2000 termini di veterinaria, anatomia patologica e laboratorio e, quando ci somiglia molto, la riscrive giusta tenendo la desinenza detta («mastocitomi» resta plurale); due o tre pezzi che insieme fanno un termine si uniscono.
- **Anche le parole aggiunte al glossario** (voce `glossario_termini` delle impostazioni, o `--glossario` da riga di comando) si correggono allo stesso modo, con o senza il glossario veterinario.
- **Le parole italiane vere non si toccano.** Circa 650 mila parole italiane (da Wikipedia) restano sempre com'erano anche se somigliano a un termine: «regressione» non diventa «progressione», «asportato» non diventa «aspartato». Senza glossario, o in un'altra lingua, il testo non cambia affatto.
- **Quanto migliora.** Su quattro referti dettati, con «Massima» i termini tecnici giusti passano dal 75 % all'87 % e gli errori sulle parole dal 12,2 % al 10,0 %. Su altri tre referti scritti dopo aver chiuso il dizionario, con termini mai visti prima: termini giusti dal 65 % all'80 % con «Massima» e dal 64 % al 75 % con «Veloce». Su 15 minuti di frasi italiane comuni il risultato non cambia. Le prove sono fatte con voci sintetiche: su dettati veri il guadagno può essere diverso.
- **Nessuna attesa in più**: la correzione richiede pochi millisecondi per frase.
