"""Adattatori dei programmi di trascrizione: ciascuno trasforma (file audio, opzioni) in un comando e dice dove trovare il testo.
Si lanciano SEMPRE su una macchina con rete (scaricano i modelli la prima volta) e si misurano con /usr/bin/time -v (RSS di picco).
Ogni adattatore: .comando(wav, outdir, gpu) -> lista argv;  .leggi(wav, outdir, stdout) -> testo trascritto.
Versioni di programmi e modelli vanno annotate in bench/VERSIONI.md PRIMA di misurare (§1.3 del protocollo)."""
import os, re, shlex
from pathlib import Path

CEDRO = os.environ.get("CEDRO", "cedro")
WHISPER_CLI = os.environ.get("WHISPER_CLI", "whisper-cli")                 # whisper.cpp compilato (Vulkan o CPU)
BUZZ = os.environ.get("BUZZ", "buzz")
MODELLI = Path(os.environ.get("MODELLI_GGML", str(Path.home() / "modelli-ggml")))


def ripulisci_cedro(t):
    """Toglie da un file di Cedro (`--testo`) titolo, riga «Trascrizione del … di audio», marche di tempo e «Interlocutore N:»: resta il parlato.
    Senza questo il WER di Cedro include parole che non sono trascrizione (intestazione) e le allucinazioni (A3) contano la riga di intestazione.
    Idempotente: su testo già pulito non cambia nulla."""
    righe = t.splitlines()
    for i, r in enumerate(righe[:4]):
        if re.match(r"^Trascrizione del .* di audio", r):
            righe = righe[i + 1:]; break
    return "\n".join(re.sub(r"^\[\d+:\d\d(?::\d\d)?\]\s*(?:Interlocutore \d+:\s*)?", "", r) for r in righe).strip()


class Cedro:
    def __init__(self, livello="buona"):
        self.livello = livello; self.nome = f"cedro-{livello}"
    def comando(self, wav, outdir, gpu):
        return [CEDRO, "trascrivi", str(wav), "--testo", "--precisione", self.livello, "--voci", "1"] + ([] if gpu else ["--cpu"])
    def grezzo(self, wav, outdir, stdout):
        # «OK <ingresso> -> <uscita> (messaggio)»: il testo è nel file di uscita (con titolo, intestazione e marche di tempo)
        m = re.search(r"^OK .* -> (.+?) \(", stdout, re.M)
        return Path(m.group(1)).read_text(errors="replace") if m else ""
    def leggi(self, wav, outdir, stdout):
        return ripulisci_cedro(self.grezzo(wav, outdir, stdout))


class WhisperCpp:
    """whisper.cpp nudo con LO STESSO modello di Cedro (per separare «motore» da «programma»)."""
    def __init__(self, modello):
        self.modello = modello; self.nome = f"whispercpp-{modello}"
    def comando(self, wav, outdir, gpu):
        return [WHISPER_CLI, "-m", str(MODELLI / self.modello), "-f", str(wav), "-l", "it", "-nt", "-otxt", "-of", str(outdir / Path(wav).stem)] + ([] if gpu else ["-ng"])
    def leggi(self, wav, outdir, stdout):
        p = outdir / (Path(wav).stem + ".txt"); return p.read_text(errors="replace") if p.exists() else stdout


class FasterWhisper:
    """faster-whisper (CTranslate2): è anche il motore «Faster Whisper» di Buzz. Riferimento forte non quantizzato.
    Decodifica l'audio con ffmpeg (il PyAV di faster-whisper 1.2.1 su Python 3.14 non parte: `open() got an unexpected keyword argument 'metadata_errors'`)."""
    def __init__(self, modello="large-v3", tipo="float16"):
        self.modello, self.tipo = modello, tipo; self.nome = f"faster-whisper-{modello}-{tipo}"
    def comando(self, wav, outdir, gpu):
        sc = Path(__file__).with_name("fw_run.py")
        return [os.environ.get("PYFW", "python3"), str(sc), self.modello, self.tipo, "cpu", str(wav)]   # AMD: niente CUDA, solo CPU
    def leggi(self, wav, outdir, stdout): return stdout
    def batch(self, wavs):
        """Per la sola misura di WER: il modello si carica una volta sola. Ritorna [(testo, secondi di decodifica+trascrizione, errore)]."""
        import time, sys
        sys.path.insert(0, str(Path(__file__).parent))
        from fw_run import carica_audio
        from faster_whisper import WhisperModel
        m = WhisperModel(self.modello, device="cpu", compute_type=self.tipo)
        for w in wavs:
            t0 = time.time()
            try:
                seg, _ = m.transcribe(carica_audio(str(w)), language="it", vad_filter=False, beam_size=5)
                yield " ".join(x.text.strip() for x in seg), time.time() - t0, ""
            except Exception as e:   # un file che non si legge non deve fermare le altre 199 clip
                yield "", time.time() - t0, repr(e)[:300]


class Buzz:
    """Buzz da riga di comando (pip install buzz-captions). La sintassi dipende dalla versione: verificare con `buzz add --help` e annotare in VERSIONI.md."""
    def __init__(self, tipo="Whisper.cpp", dim="large-v3-turbo"):
        self.tipo, self.dim = tipo, dim; self.nome = f"buzz-{tipo}-{dim}"
    def comando(self, wav, outdir, gpu):
        # Buzz 1.4.5: opzioni Qt -t/-m/-s/-l/-d; --model-type vale whisper|whispercpp|fasterwhisper (non «Whisper.cpp»); serve QT offscreen senza display
        tipo = {"Whisper.cpp": "whispercpp", "Faster Whisper": "fasterwhisper"}.get(self.tipo, self.tipo)
        return ["env", "QT_QPA_PLATFORM=offscreen", BUZZ, "add", "-t", "transcribe", "-m", tipo, "-s", self.dim, "-l", "it", "--txt", "--hide-gui", "-d", str(outdir), str(wav)]
    def leggi(self, wav, outdir, stdout):
        c = sorted(outdir.glob(Path(wav).stem + "*.txt")); return c[-1].read_text(errors="replace") if c else stdout


class Vibe:
    """Vibe (thewh1teagle/vibe) da riga di comando; `vibe --help` per la sintassi della versione installata."""
    def __init__(self, modello): self.modello = modello; self.nome = f"vibe-{Path(modello).stem}"
    def comando(self, wav, outdir, gpu):
        return ["vibe", "--model", self.modello, "--file", str(wav), "--language", "it", "--format", "txt", "--output", str(outdir / (Path(wav).stem + ".txt"))]
    def leggi(self, wav, outdir, stdout):
        p = outdir / (Path(wav).stem + ".txt"); return p.read_text(errors="replace") if p.exists() else stdout


ELENCO = {
    "cedro-veloce": lambda: Cedro("veloce"), "cedro-buona": lambda: Cedro("buona"), "cedro-massima": lambda: Cedro("massima"),
    "wcpp-small-q8": lambda: WhisperCpp("ggml-small-q8_0.bin"), "wcpp-medium-q5": lambda: WhisperCpp("ggml-medium-q5_0.bin"),
    "wcpp-turbo-q5": lambda: WhisperCpp("ggml-large-v3-turbo-q5_0.bin"), "wcpp-turbo-f16": lambda: WhisperCpp("ggml-large-v3-turbo.bin"),
    "wcpp-large-v3": lambda: WhisperCpp("ggml-large-v3.bin"),
    "fw-large-v3": lambda: FasterWhisper("large-v3", "int8"), "fw-large-v3-int8": lambda: FasterWhisper("large-v3", "int8"),
    "buzz-wcpp": lambda: Buzz("Whisper.cpp", "large-v3-turbo"), "buzz-fw": lambda: Buzz("Faster Whisper", "large-v3"),
}
