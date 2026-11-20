## Scarica questo

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** (quasi tutti i PC) | [**Cedro-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Setup-{versione}.exe) |
| **Windows portable** (x64, senza installazione) | [**Cedro-Portable-{versione}-x64.zip**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Portable-{versione}-x64.zip) |
| **Linux** (Wayland con KDE Plasma o GNOME) | [**cedro-{versione}-linux-x64.tar.gz**](https://github.com/{repo}/releases/download/v{versione}/cedro-{versione}-linux-x64.tar.gz) |

Scarica il file e fai doppio clic: si installa sopra la versione che hai (su Linux: estrai il tar.gz e lancia `./install.sh`, senza permessi di amministratore). Se hai già Cedro non serve nemmeno questo: si aggiorna da solo («Controlla aggiornamenti» nel menu dell'icona).
I motori della trascrizione non vanno scaricati a mano: Cedro li prende da solo la prima volta che trascrivi.

## Novità della {versione}

**I termini dei referti dettati escono giusti ancora più spesso, anche quando whisper li spezza, li attacca o ne regala una lettera all'articolo.**

- **Termini spezzati e attaccati.** Tornano giusti anche quando whisper attacca all'inizio del termine una parolina o divide due termini nel punto sbagliato: «con angiocarcinoma» → «colangiocarcinoma», «di fenidamina» → «difenidramina», «per ecogeno» → «iperecogeno», «tubulo papillare» e «tubulo-papillare» → «tubulopapillare», «emangio sarcomasplenico» → «emangiosarcoma splenico», «toxocaracati» → «Toxocara cati».
- **Articoli che si prendono una lettera del termine.** «una denoma» → «un adenoma», «una tita» → «un'otite», «dell'alipasi» → «della lipasi», «l'acitometria» → «la citometria», «dell'infonodo» → «del linfonodo».
- **Nomi propri e intestazioni.** Dopo «test di», «colorazione di», «secondo» i nomi tornano giusti anche se detti corti o scritti come sigla: «test di COMS» → «test di Coombs», «secondo Patnek» → «secondo Patnaik». In testa a un referto «Effetto citologico», «Perfetto ematologico», «Per effetto istologico» diventano «Referto …»; «esame culturale» diventa «esame colturale».
- **Desinenze che un termine non ha.** «prednisolona» → «prednisolone», «coleciste» → «colecisti».
- **Meno correzioni perse.** Il controllo delle parole italiane vere scambia per vera una parola storpiata una volta su duemila invece che una su cento, e ora conosce anche le parole comuni di 4 e 5 lettere («una mina» resta «una mina»).
- **Quanto migliora.** Su tre referti nuovi, scritti prima di queste modifiche e mai usati per tararle: con «Massima» gli errori sulle parole scendono dall'11,0 % al 9,8 % (erano il 14,4 % senza correzione) e i termini giusti salgono dal 79 % all'81 %; con «Buona» dall'11,6 % al 10,1 % e dal 79 % all'82 %; con «Veloce» dal 14,1 % al 12,3 % e dal 76 % all'80 %. Su 15 minuti di frasi italiane comuni nessuna parola cambia. Le prove sono fatte con voci sintetiche: su dettati veri il guadagno può essere diverso.
