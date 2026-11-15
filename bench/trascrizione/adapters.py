"""Adattatori dei programmi di trascrizione: ciascuno trasforma (file audio, opzioni) in un comando e dice dove trovare il testo.
Si lanciano SEMPRE su una macchina con rete (scaricano i modelli la prima volta) e si misurano con /usr/bin/time -v (RSS di picco).
Ogni adattatore: .comando(wav, outdir, gpu) -> lista argv;  .leggi(wav, outdir, stdout) -> testo trascritto.
Versioni di programmi e modelli vanno annotate in bench/VERSIONI.md PRIMA di misurare (§1.3 del protocollo)."""
import os, re, shlex
from pathlib import Path

CEDRO = os.environ.get("CEDRO", "cedro")
WHISPER_CLI = os.environ.get("WHISPER_CLI", "whisper-cli")                 # whisper.cpp compilato (Vulkan o CPU)
MODELLI = Path(os.environ.get("MODELLI_GGML", str(Path.home() / "modelli-ggml")))


class Cedro:
    def __init__(self, livello="buona"):
        self.livello = livello; self.nome = f"cedro-{livello}"
    def comando(self, wav, outdir, gpu):
        return [CEDRO, "trascrivi", str(wav), "--testo", "--precisione", self.livello, "--voci", "1"] + ([] if gpu else ["--cpu"])
    def leggi(self, wav, outdir, stdout):
        # «OK <ingresso> -> <uscita> (messaggio)»: il testo è nel file di uscita
        m = re.search(r"^OK .* -> (.+?) \(", stdout, re.M)
        return Path(m.group(1)).read_text(errors="replace") if m else ""


class WhisperCpp:
    """whisper.cpp nudo con LO STESSO modello di Cedro (per separare «motore» da «programma»)."""
    def __init__(self, modello):
        self.modello = modello; self.nome = f"whispercpp-{modello}"
    def comando(self, wav, outdir, gpu):
        return [WHISPER_CLI, "-m", str(MODELLI / self.modello), "-f", str(wav), "-l", "it", "-nt", "-otxt", "-of", str(outdir / Path(wav).stem)] + ([] if gpu else ["-ng"])
    def leggi(self, wav, outdir, stdout):
        p = outdir / (Path(wav).stem + ".txt"); return p.read_text(errors="replace") if p.exists() else stdout


class FasterWhisper:
    """faster-whisper (CTranslate2): è anche il motore «Faster Whisper» di Buzz. Riferimento forte non quantizzato."""
    def __init__(self, modello="large-v3", tipo="float16"):
        self.modello, self.tipo = modello, tipo; self.nome = f"faster-whisper-{modello}-{tipo}"
    def comando(self, wav, outdir, gpu):
        sc = Path(__file__).with_name("fw_run.py")
        return ["python3", str(sc), self.modello, self.tipo, "cuda" if gpu else "cpu", str(wav)]
    def leggi(self, wav, outdir, stdout): return stdout


class Buzz:
    """Buzz da riga di comando (pip install buzz-captions). La sintassi dipende dalla versione: verificare con `buzz add --help` e annotare in VERSIONI.md."""
    def __init__(self, tipo="Whisper.cpp", dim="large-v3-turbo"):
        self.tipo, self.dim = tipo, dim; self.nome = f"buzz-{tipo}-{dim}"
    def comando(self, wav, outdir, gpu):
        return ["buzz", "add", "--task", "transcribe", "--model-type", self.tipo, "--model-size", self.dim, "--language", "it", "--txt", "--output-directory", str(outdir), str(wav)]
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
    "fw-large-v3": lambda: FasterWhisper("large-v3", "float16"), "fw-large-v3-int8": lambda: FasterWhisper("large-v3", "int8"),
    "buzz-wcpp": lambda: Buzz("Whisper.cpp", "large-v3-turbo"), "buzz-fw": lambda: Buzz("Faster Whisper", "large-v3"),
}
