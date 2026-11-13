#!/bin/bash
# Giro di regressione di Cedro dalla riga di comando: ogni conversione deve finire e lasciare un file non vuoto.
# Uso: regressione.sh <cedro> <cartella-repo> <cartella-di-lavoro>
C="$1"; R="$2"; W="$3"
rm -rf "$W"; mkdir -p "$W"; cp -r "$R"/tests/fixtures/* "$W"/; cp -r "$R"/tests/data/firma "$W"/firma
cd "$W"
ok=0; ko=0
prova() { # descrizione, comando...
  local d="$1"; shift
  local before; before=$(find . -type f | wc -l)
  local out; out=$(timeout 300 "$@" 2>&1); local rc=$?
  local after; after=$(find . -type f | wc -l)
  if [ $rc -eq 0 ] && [ "$after" -gt "$before" ]; then ok=$((ok+1)); echo "OK   $d"; else ko=$((ko+1)); echo "ERR  $d (rc=$rc, file nuovi=$((after-before))): $(echo "$out" | tail -2 | tr '\n' ' ' | cut -c1-220)"; fi
}
# Converti: immagini
for f in png webp avif jxl tiff pdf ico gif bmp qoi; do prova "immagine jpg → $f" "$C" convert scan.jpg $f; done
prova "immagine strumenti: gray" "$C" tool scan.jpg gray
prova "immagine strumenti: rot90" "$C" tool scan.jpg rot90
prova "OCR su immagine" "$C" tool scan.jpg ocr
prova "OCR tabella" "$C" tool scan_tabella.jpg ocr
# Audio
for f in mp3 wav flac opus aiff; do prova "audio flac → $f" "$C" convert nota.flac $f; done
prova "audio m4a → mp3" "$C" convert nota.m4a mp3
prova "audio ogg → wav" "$C" convert nota.ogg wav
prova "audio aac → mp3" "$C" convert nota.aac mp3
# Video
prova "video mkv → mp4" "$C" convert clip.mkv mp4
prova "video mov → mp4" "$C" convert clip.mov mp4
prova "video webm vp9 → mp4" "$C" convert vp9.webm mp4
prova "video → mp3 (audio)" "$C" convert audio.mp4 mp3
# PDF
prova "PDF → testo" "$C" convert circolare.pdf txt
prova "PDF → Word" "$C" convert circolare.pdf docx
prova "PDF → PNG" "$C" convert circolare.pdf png
prova "PDF strumenti: compress" "$C" tool circolare.pdf compress
prova "PDF OCR" "$C" tool circolare.pdf ocr
# Labs
prova "Labs: pacchetto dati" "$C" pacchetto scan.jpg nota.flac
pk=$(ls -t *.zip 2>/dev/null | head -1); [ -n "$pk" ] && { timeout 60 "$C" tool "$pk" verify-pack >/dev/null 2>&1 && { ok=$((ok+1)); echo "OK   Labs: verifica pacchetto"; } || { ko=$((ko+1)); echo "ERR  Labs: verifica pacchetto"; }; }
prova "Labs: figura" "$C" figura scan.jpg scan_tabella.jpg
# Firma (verifica): deve dire valida per mario, non valida per cambiato
v=$(timeout 60 "$C" firma firma/mario.txt.p7m 2>&1); echo "$v" | grep -qi "valid\|integr" && { ok=$((ok+1)); echo "OK   Firma: verifica mario.p7m"; } || { ko=$((ko+1)); echo "ERR  Firma: verifica mario.p7m: $(echo "$v"|head -3|tr '\n' ' ')"; }
timeout 60 "$C" firma firma/cambiato.txt.p7m >/dev/null 2>&1; [ $? -eq 1 ] && { ok=$((ok+1)); echo "OK   Firma: file cambiato riconosciuto"; } || { ko=$((ko+1)); echo "ERR  Firma: file cambiato NON riconosciuto"; }
v=$(timeout 60 "$C" firma firma/determina_firmata.pdf 2>&1); [ -n "$v" ] && { ok=$((ok+1)); echo "OK   Firma: PDF firmato letto"; } || { ko=$((ko+1)); echo "ERR  Firma: PDF firmato"; }
echo "TOTALE: $ok ok, $ko errori"
