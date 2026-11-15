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

## Da compilare sul PC di collaudo prima di lanciare la trascrizione (Area A) e le misure di velocità

| Cosa | Versione |
| --- | --- |
| Cedro (`cedro --version`, commit) | |
| whisper.cpp (commit, backend Vulkan o CPU) | |
| faster-whisper / CTranslate2 | |
| Buzz | |
| Vibe | |
| Modelli ggml e sha256 | |
| Driver Vulkan (`vulkaninfo --summary`) | |
| CPU / RAM / scheda video | Ryzen 7 5700U, Radeon iGPU (PC di collaudo) |
