## Scarica questo

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** (quasi tutti i PC) | [**Cedro-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Setup-{versione}.exe) |
| **Windows portable** (x64, senza installazione) | [**Cedro-Portable-{versione}-x64.zip**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Portable-{versione}-x64.zip) |
| **Linux** (Wayland con KDE Plasma o GNOME) | [**cedro-{versione}-linux-x64.tar.gz**](https://github.com/{repo}/releases/download/v{versione}/cedro-{versione}-linux-x64.tar.gz) |

Scarica il file e fai doppio clic: si installa sopra la versione che hai (su Linux: estrai il tar.gz e lancia `./install.sh`, senza permessi di amministratore). Se hai già Cedro non serve nemmeno questo: si aggiorna da solo («Controlla aggiornamenti» nel menu dell'icona).
I motori della trascrizione non vanno scaricati a mano: Cedro li prende da solo la prima volta che trascrivi.

## Novità della {versione}

**Componenti aggiornati, per sicurezza e manutenzione.** Nessun cambiamento nell'uso.

- Firma digitale: aggiornate le librerie di crittografia (RSA, curve P-256/P-384, SHA-1/SHA-2). Chiudono due avvisi di sicurezza (RUSTSEC-2023-0071 e RUSTSEC-2026-0387) sui tempi di calcolo; in Cedro non erano sfruttabili, perché le chiavi private restano nella chiavetta o nella smart card.
- Riconoscimento del testo (OCR): ocrs 0.14 e rten 0.28, le versioni ufficiali. Il testo letto è identico a prima e la velocità è la stessa. Resta l'interruttore per spegnere AVX-512 nelle Impostazioni.
- Video: decoder H.264, VP8 e MPEG-4 aggiornati con correzioni.
- Aggiornati i certificati radice usati per i download e la libreria ZIP.
