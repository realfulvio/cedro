# Versioni usate nel banco (§1.3 del protocollo)

## Misure fatte nel container cloud (2026-11-15), Linux 6.18, 4 core, 15 GB, senza scheda video

| Cosa | Versione |
| --- | --- |
| Cedro | commit e909577 (ramo claude/project-thread-lu27xc, uguale a main + protocollo), `target/release/cedro` compilato con rustc 1.100 |
| qpdf | 11.9.0 |
| veraPDF | 1.28.2 (`GreenfieldCliWrapper`, `-f 2b --format xml`) |
| EU DSS | 6.5.1 (Java, Maven) |
| pyHanko | 0.38.1 |
| OpenSSL | 3.0.13 |
| Poppler pdfsig | 24.02.0 |
| LibreOffice | 24.2.7.2 (riferimento per Word/Excel → PDF) |
| Tesseract | 5.3.4 con `ita` |
| Apache POI / XmlBeans | schemi ECMA-376 per la validazione XSD (vedi `bench/pdf/java/Ooxml.java`) |
| pikepdf / pdf2docx / python-docx / openpyxl | 10.18.1 / 0.5.13 / 1.2.0 / 3.1.5 |
| PyMuPDF / jiwer | 2.1 / 4.0.0 |
| cryptography / asn1crypto | 50.0.1 / 1.5.1 |
| tifffile / czifile / openslide-python / pydicom | 2026.11.12 / 2026.11.12 / 1.4.7 / 3.0.2 |

## Misure fatte sul PC di collaudo (2026-11-16), CachyOS, kernel 7.3.4-1-cachyos-deckify

Hardware: AMD Ryzen 7 5700U (8 core/16 thread), 30 GB RAM, Radeon Renoir integrata (UMA, Vulkan RADV). **Carico**: il PC di collaudo ospita anche i runner CI,
la VM Windows `win11-test` (qemu, 14 GB) e altri lavori; ogni riga di tempo dei file `b10_*` e `a4_*` riporta il loadavg al momento della misura.

| Cosa | Versione |
| --- | --- |
| Cedro | commit 5b2b999 (ramo claude/project-thread-lu27xc = main 520475f + banco), `Cargo.toml` 0.50.0, `cargo build --release` con rustc 1.99.0; `cedro --version` non esiste (stampa l'aiuto) |
| Motore di trascrizione di Cedro | `cedro-trascrivi-vulkan` 0.50.0 (scaricato da Cedro alla prima volta), sha256 20dc4bf2…9228c; modelli di Cedro = quelli ggml sotto (stessi sha256) |
| whisper.cpp | commit 5922450 (master del 2026-11-16), `cmake -DGGML_VULKAN=1 -DCMAKE_BUILD_TYPE=Release`, `whisper-cli`; formati letti da solo: flac, mp3, ogg, wav |
| faster-whisper / CTranslate2 | 1.2.1 / 4.9.0, Python 3.14, `large-v3` int8 su CPU (il PC di collaudo ha una scheda AMD: niente CUDA, quindi il riferimento è CPU) |
| Buzz | 1.4.5 (`buzz-captions` da PyPI in venv Python 3.12), motore Whisper.cpp con il suo `whisper-cli` Vulkan, modello `large-v3-turbo` non quantizzato (f16); manda statistiche a us.i.posthog.com (visto nel log) |
| Vibe | **non misurato**: il binario del pacchetto GitHub v3.3.5 non è stato eseguito (permesso negato dal controllo automatico di questa sessione) |
| Driver Vulkan | Mesa 26.2.6 (RADV, API 1.4.354), `vulkan-radeon` 26.2.6 |
| Modello ggml small q8_0 | 49c8fb02b65e6049d5fa6c04f81f53b867b5ec9540406812c643f177317f779f |
| Modello ggml medium q5_0 | 19fea4b380c3a618ec4723c3eef2eb785ffba0d0538cf43f8f235e7b3b34220f |
| Modello ggml large-v3-turbo q5_0 | 394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2 |
| Modello ggml large-v3-turbo f16 (Buzz) | 1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69 |
| Python (strumenti) | 3.14.10: jiwer 4.0.0, pymupdf 2.1, pdf2docx 0.5.13, pikepdf 10.18.1 (libqpdf), python-docx 1.2.0, openslide-python 1.4.7 + openslide-bin 4.0.1.2, tifffile 2026.11.12 |
| Ghostscript / Poppler / ffmpeg | 10.08.0 / 26.10.0 / 9.0.2 |
| LibreOffice | Flatpak org.libreoffice.LibreOffice 26.8.3.1 (per Word→PDF); MuPDF `mutool` e qpdf non sono installati: il riferimento MuPDF è PyMuPDF 2.1, quello qpdf è pikepdf |
| Dati | FLEURS it_it test, 200 clip seme 42 (`google/fleurs`); MLS italiano test, 200 clip seme 42 (`facebook/multilingual_librispeech`), 10 parlanti con ≥4 clip; vetrini: openslide-testdata (Aperio/CMU-1.svs, Hamamatsu/CMU-1.ndpi, Mirax/Mirax2.2-1.zip) |
| Misura di RSS | `bench/lib/rss_helper.py` (python -I -S): il valore minimo misurabile è ~14 MB. Il metodo precedente (wait4 dal processo del banco) restituiva come minimo il picco del processo genitore (es. 58 MB per `true`) |
