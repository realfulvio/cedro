#!/usr/bin/env node
// Genera site/en/ (pagine inglesi statiche, indicizzabili) dalle pagine italiane di site/.
// Le traduzioni stanno già dentro index.html e formati.html (dizionario EN / data-en): qui si apre ogni pagina
// con Chromium, si chiama cedroSet('en') e si salva il DOM risultante con percorsi, canonical e meta in inglese.
// Uso (da riga di comando, serve `playwright`): node tools/genera-en.js     -> rifare ogni volta che cambia il testo del sito.
const { chromium } = require('playwright');
const fs = require('fs'), path = require('path');
const SITO = path.resolve(__dirname, '..', 'site');
const B = 'https://cedrolabs.app/';
const META = {
  'index.html': {
    title: 'Cedro: free file converter, PDF tools and transcription',
    desc: 'Cedro converts photos, audio, video and PDFs, transcribes meetings to Word and signs with a smart card. Free, open source, runs offline on Windows and Linux.',
    ogt: 'Cedro: convert, transcribe and sign your files', ogd: 'Free, open source, everything on your computer. Windows 10 and 11 and Linux.',
    url: B + 'en/', it: B, en: B + 'en/', itl: '../', enl: './',
  },
  'formati.html': {
    title: 'Cedro formats: what it reads and what it converts to',
    desc: 'Every format Cedro reads and converts: images, documents, PDF, audio, video, DICOM and digital slides. All on your own computer.',
    ogt: 'Cedro formats: what it reads and what it converts to', ogd: 'Search your file and see what Cedro turns it into. Free, open source, all on your computer.',
    url: B + 'en/formati.html', it: B + 'formati.html', en: B + 'en/formati.html', itl: '../formati.html', enl: 'formati.html',
  },
};
const ALT = [
  ['alt="La finestra di Cedro sulla pagina Converti: il file al centro e i formati a destra"', 'alt="The Cedro window on the Convert page: the file in the middle and the formats on the right"'],
  ['alt="Cedro 4 Labs: la finestra con le attività di laboratorio"', 'alt="Cedro 4 Labs: the window with the laboratory activities"'],
  ['content="La finestra di Cedro sulla pagina Converti"', 'content="The Cedro window on the Convert page"'],
];
const rx = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
(async () => {
  const b = await chromium.launch();
  fs.mkdirSync(path.join(SITO, 'en'), { recursive: true });
  for (const [file, m] of Object.entries(META)) {
    const p = await b.newPage();
    await p.goto('file://' + path.join(SITO, file));
    await p.evaluate(() => window.cedroSet('en'));
    let h = '<!doctype html>\n' + (await p.evaluate(() => document.documentElement.outerHTML));
    await p.close();
    h = h.replace('<html lang="en"', '<html lang="en" data-static="en"');
    h = h.replace(/<title>[^<]*<\/title>/, `<title>${m.title}</title>`);
    h = h.replace(/(<meta name="description" content=")[^"]*"/, `$1${m.desc}"`);
    h = h.replace(/(<link rel="canonical" href=")[^"]*"/, `$1${m.url}"`);
    h = h.replace(/(<meta property="og:url" content=")[^"]*"/, `$1${m.url}"`);
    h = h.replace(/(<meta property="og:title" content=")[^"]*"/, `$1${m.ogt}"`);
    h = h.replace(/(<meta property="og:description" content=")[^"]*"/, `$1${m.ogd}"`);
    h = h.replace('og:locale" content="it_IT"', 'og:locale" content="en_US"').replace('og:locale:alternate" content="en_US"', 'og:locale:alternate" content="it_IT"');
    // dati strutturati: stessa struttura, testi in inglese (la FAQ italiana non si ripete)
    h = h.replace(/<script type="application\/ld\+json">([\s\S]*?)<\/script>/, (_, j) => {
      const d = JSON.parse(j);
      d['@graph'] = d['@graph'].filter((n) => n['@type'] !== 'FAQPage');
      for (const n of d['@graph']) {
        if (n.inLanguage === 'it') n.inLanguage = 'en';
        if (n['@type'] === 'WebSite') n.url = B + 'en/';
        if (n['@type'] === 'WebPage') { n.url = m.url; n['@id'] = m.url; n.name = 'Cedro formats'; }
        if (n['@type'] === 'SoftwareApplication') { n.url = B + 'en/'; n.description = 'Cedro converts photos, audio, video and documents, works on PDFs, transcribes meetings and signs with a smart card. Free, open source, everything on your computer.'; }
        if (n['@type'] === 'BreadcrumbList') { n.itemListElement[0].item = B + 'en/'; n.itemListElement[1].name = 'Formats'; n.itemListElement[1].item = m.url; }
      }
      return '<script type="application/ld+json">' + JSON.stringify(d) + '</script>';
    });
    for (const [a, e] of ALT) h = h.split(a).join(e);
    // selettore lingua
    h = h.replace(/<div class="lang"[^>]*>[\s\S]*?<\/div>/, `<div class="lang" role="group" aria-label="Language"><a data-l="it" href="${m.itl}" hreflang="it" lang="it">IT</a><a data-l="en" href="${m.enl}" hreflang="en" lang="en" aria-current="true">EN</a></div>`);
    // percorsi: la pagina sta in en/, risorse e pagina italiana un livello sopra
    h = h.replace(/(["'(])(img\/|fonts\/)/g, '$1../$2');
    fs.writeFileSync(path.join(SITO, 'en', file), h);
    console.log('scritto en/' + file, h.length, 'byte');
  }
  await b.close();
})();
