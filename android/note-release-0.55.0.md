APK per Android (arm64, Android 8 o successivo).

## Novità della 0.55.0

Una versione di sole correzioni: più stabile, più fluida, più leggera. L'interfaccia è quella della 0.54.0, identica pixel per pixel.

- Lasciato aperto per giorni, anche in secondo piano, Cedro non si chiude più da solo: ogni attesa del ciclo principale lasciava indietro qualche riferimento interno ad Android, finché la tabella si riempiva (circa quattro giorni dopo).
- In secondo piano Cedro si sveglia una volta al secondo invece di sette (sul Mi 9T il processore usato a riposo scende del 70 %), e se lo lasci a metà di un'animazione (per esempio aprendo il selettore dei file) non tiene più occupato un core: meno batteria e meno calore.
- I lavori brevissimi non fanno più chiudere l'app con l'errore del servizio in primo piano.
- Scorrimento e animazioni più fluidi: ogni schermata si disegna dal 37 al 49 % più in fretta. Sul Mi 9T, per fotogramma: Inizio da 75 a 44 ms, Converti da 82 a 42, PDF da 78 a 41, Trascrivi da 75 a 39, Video da 87 a 54, Firma da 55 a 31. Lo scorrimento dell'Inizio passa da 14 a 22 fotogrammi al secondo.
- Durante una conversione l'avanzamento ridisegna lo schermo al massimo quattro volte al secondo e lascia più processore al lavoro.
- Meno memoria: la cache dei caratteri disegnati non cresce più senza limite per tutta la vita dell'app.
- Un documento firmato rovinato non lascia più girare all'infinito la rotella dell'anteprima.
- Gli errori interni ora finiscono nel registro di Android (logcat), così si possono diagnosticare.
- Dal motore comune della 0.55.0 (misurato sul computer, vale anche qui): foto ridimensionate, Foto → PDF, PDF → JPG e WebM → MP4 più veloci e con meno memoria, caratteri di sistema letti solo per la parte che serve.
- Le miniature di BMP e DDS rovinati, i vetrini VSI con misure impossibili e i file Zeiss CZI rovinati non chiudono più Cedro.
