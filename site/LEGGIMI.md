# Sito di Cedro

Pagina unica (IT/EN) pubblicata su GitHub Pages: https://realfulvio.github.io/cedro/

- Niente dipendenze né servizi esterni: HTML, CSS e JS in `index.html`, font Figtree in `fonts/` (licenza OFL), screenshot in `img/` (da `docs/v4`).
- Il deploy lo fa `.github/workflows/android-fdroid.yml`, che mette nello stesso artefatto di Pages il sito e il repository F-Droid (`/fdroid/repo`). Un deploy sostituisce l'altro, quindi non creare un workflow Pages separato.
- Da aggiornare a ogni release: numero di versione, nomi e dimensioni dei file nella sezione «Scarica», tabella «Cosa c'è già / Cosa manca» di Cedro 4 Labs.
- Il video promo non c'è ancora: serve una versione leggera (il promo v5 pesa 57 MB).
