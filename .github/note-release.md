## Scarica questo

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** (quasi tutti i PC) | [**Cedro-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Setup-{versione}.exe) |
| **Windows portable** (x64, senza installazione) | [**Cedro-Portable-{versione}-x64.zip**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Portable-{versione}-x64.zip) |
| **Linux** (Wayland con KDE Plasma o GNOME) | [**cedro-{versione}-linux-x64.tar.gz**](https://github.com/{repo}/releases/download/v{versione}/cedro-{versione}-linux-x64.tar.gz) |

Scarica il file e fai doppio clic: si installa sopra la versione che hai (su Linux: estrai il tar.gz e lancia `./install.sh`, senza permessi di amministratore). Se hai già Cedro non serve nemmeno questo: si aggiorna da solo («Controlla aggiornamenti» nel menu dell'icona).
I motori della trascrizione non vanno scaricati a mano: Cedro li prende da solo la prima volta che trascrivi.

## Novità della {versione}

**Cedro 4 Labs conosce il caso su cui lavori e tiene l'elenco di quello che hai fatto.**

- In alto, accanto al nome, «+ Caso o campione»: scrivi il codice (per esempio V-2026-0457) e, se vuoi, una nota per te. Finché non lo togli, ogni file che Cedro scrive si chiama «data, caso, nome»: `2026-11-13_V-2026-0457_vetrino`. Senza caso i nomi restano quelli di sempre. La nota non entra mai nei nomi.
- Il caso resta anche dopo aver chiuso Cedro, finché non lo togli.
- «Lavori», in alto a destra, apre l'elenco di quello che è uscito oggi: ora, cosa hai fatto, caso e nomi dei file. «Copia per il quaderno» mette negli appunti il caso e i nomi, uno per riga, pronti da incollare. L'elenco sta solo su questo computer.
- Nel modello dei nomi si può scrivere anche `%caso%`.
- Anche da riga di comando: `cedro --caso V-2026-0457 convert vetrino.svs png`. Senza `--caso` vale il caso scelto nell'app, con `--senza-caso` i nomi restano quelli di sempre.
- Nell'elenco dei lavori l'ora è giusta anche per i lavori fatti prima del cambio tra ora solare e legale (su Windows).
