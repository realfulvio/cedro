# Banco di prova trascrizione (Area A)

Serve il PC di collaudo (CachyOS, rete, scheda video), non il container cloud: lì Hugging Face non è raggiungibile.
Il protocollo (metriche e soglie fissate prima di misurare) è in `bench/PROTOCOLLO.md` §2.

## Preparazione (una volta)

```bash
# Cedro compilato: cargo build --release   (CEDRO=/percorso/target/release/cedro)
# whisper.cpp con Vulkan:  cmake -B build -DGGML_VULKAN=1 && cmake --build build -j  -> WHISPER_CLI=.../build/bin/whisper-cli
# modelli ggml (gli stessi di Cedro) in ~/modelli-ggml/  oppure MODELLI_GGML=...
#   ggml-small-q8_0.bin  ggml-medium-q5_0.bin  ggml-large-v3-turbo-q5_0.bin  (+ f16 e large-v3 per il confronto)
pip install jiwer scipy pandas pyarrow huggingface_hub faster-whisper
# Buzz:  pip install buzz-captions      Vibe: pacchetto della release GitHub di thewh1teagle/vibe
export BENCH_DATI=~/bench-dati
python3 bench/trascrizione/bench.py prova      # autoprova della logica (WER, bootstrap, DER, tempo/RSS)
python3 bench/trascrizione/bench.py prepara --n 200
```

**Prima di misurare** scrivi in `bench/VERSIONI.md`: versione di Cedro (`cedro --version`, commit), `whisper-cli --help | head`,
versioni di faster-whisper/Buzz/Vibe, driver Vulkan (`vulkaninfo --summary`), CPU/RAM, hash dei modelli (`sha256sum`).
Gli adattatori di Buzz e Vibe dipendono dalla sintassi della versione installata: controlla `buzz add --help` / `vibe --help` e
correggi `adapters.py` se serve (e dillo nel commit).

## Misure

```bash
B=bench/trascrizione/bench.py
S=cedro-veloce,cedro-buona,cedro-massima,wcpp-small-q8,wcpp-medium-q5,wcpp-turbo-q5,fw-large-v3,buzz-wcpp,vibe-turbo   # vibe: aggiungi a ELENCO col percorso del modello
python3 $B wer --sistemi $S --set fleurs            # A1/A2, 200 clip, ~ ore: lascialo girare (riprende da dove era)
python3 $B wer --sistemi $S --set mls
python3 $B riassumi --set fleurs                    # WER/CER e IC 95 % pareggiato contro il miglior concorrente
python3 $B allucinazioni --sistemi $S               # A3
python3 $B prestazioni --sistemi $S                 # A4/A5: 10 min, GPU e CPU, 5 ripetizioni
python3 $B robustezza --sistemi cedro-buona,wcpp-turbo-q5,buzz-wcpp   # A6 (+ metti i file veri in $BENCH_DATI/reali/)
python3 $B lungo --sistemi cedro-buona,wcpp-turbo-q5                  # A8, 3 h
python3 $B voci --sistemi cedro-buona               # A7: miscele con parlanti noti (MLS), DER approssimato a trame di 0,1 s
```

Risultati: `bench/risultati/a*_*.jsonl` (una riga per misura, con testo ipotizzato e stderr): da lì si rifà tutto senza rilanciare.

## Cose da sapere

- Cedro e whisper.cpp usano lo stesso motore: se il WER di `cedro-turbo` e `wcpp-turbo-q5` differisce, la causa è nel
  contorno (VAD, ricampionamento, finestre, glossario); se coincide, la parità con Buzz/Vibe è garantita e si guarda a faster-whisper
  non quantizzato e a modelli più forti (Parakeet TDT v3 in Vibe) come riferimento «migliore possibile».
- A7: il DER qui è calcolato a trame, senza zona di tolleranza ai bordi (più severo di `pyannote.metrics`); i parlanti sono veri
  (MLS, `speaker_id`). Gli altri programmi non danno i parlanti da riga di comando: A7 resta Cedro contro la verità.
- A9 (termini di dominio): `frasi_dominio.txt`, 40 frasi con i termini marcati; leggerle da 2 persone reali e farle trascrivere con e
  senza `--glossario veterinaria`. Non esiste concorrente con glossario: serve solo a quantificare il vantaggio.
- File di Common Voice 17 (richiede login Hugging Face): se disponibile, mettere clip e `test.tsv` in `$BENCH_DATI/cv/` con un
  `rif.json` nello stesso formato di `fleurs/rif.json` (`id`, `wav`, `rif`) e lanciare con `--set cv`.
