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

**Correzioni di Rivedi trascrizione e PDF → Excel.**

- **Rivedi**: Play e il clic sul tempo di una frase fanno sentire davvero l'audio (prima restavano muti e fermi a 00:00); anche il clic sull'onda porta l'ascolto in quel punto.
- **Rivedi** si riapre anche dopo aver chiuso Cedro: rimesso il file in coda, il pulsante «Rivedi» riporta la revisione salvata, con le correzioni fatte.
- **PDF → Excel**: i numeri restano scritti come nel PDF, senza aggiungere il punto delle migliaia («44012» non diventa più «44.012», «2026» non diventa «2.026»), e le colonne di codici (CAP, protocollo, matricola, telefono, partita IVA…) restano testo.
