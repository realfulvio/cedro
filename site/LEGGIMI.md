# Sito di Cedro

Pagina unica (IT/EN) pubblicata su GitHub Pages: https://cedrolabs.app/

Dominio: https://cedrolabs.app/ (DNS su Cloudflare, solo DNS senza proxy: 4 record A verso GitHub Pages + CNAME www verso realfulvio.github.io; dominio impostato in Settings → Pages). Il vecchio indirizzo realfulvio.github.io/cedro/ reindirizza qui. Il repository F-Droid resta con URL …/cedro/fdroid/repo (GitHub reindirizza).

- Niente dipendenze né servizi esterni: HTML, CSS e JS in `index.html`, font Figtree in `fonts/` (licenza OFL), screenshot in `img/` (da `docs/v4`).
- Il deploy lo fa `.github/workflows/android-fdroid.yml`, che mette nello stesso artefatto di Pages il sito e il repository F-Droid (`/fdroid/repo`). Un deploy sostituisce l'altro, quindi non creare un workflow Pages separato.
- Da aggiornare a ogni release: numero di versione, nomi e dimensioni dei file nella sezione «Scarica», tabella «Cosa c'è già / Cosa manca» di Cedro 4 Labs.
- Il video promo non c'è ancora: serve una versione leggera (il promo v5 pesa 57 MB).
- `formati.html`: pagina dei formati (cosa legge Cedro e in cosa lo converte), con i dati dentro la pagina. Da aggiornare quando cambiano i formati in `src/engine.rs` e `src/conversioni.rs`.
- **Inglese e SEO**: l'inglese vive in `en/` (pagine statiche indicizzabili), generate da `tools/genera-en.js` (`NODE_PATH=$(npm root -g) node tools/genera-en.js`, serve `playwright`). **Va rifatto ogni volta che cambia il testo di `index.html` o `formati.html`**, e i file `en/` si committano. La lingua non cambia mai da sola: il selettore IT/EN è un link tra le due pagine. Titoli, description, canonical, hreflang, dati strutturati e Open Graph stanno nell'`<head>`; `sitemap.xml` e `robots.txt` in radice (la sitemap va inviata a Search Console). `img/og.png` è l'anteprima per i social.
- Il workflow aggiorna versione e dimensioni anche in `en/index.html` (`tools/aggiorna-sito.py`).
