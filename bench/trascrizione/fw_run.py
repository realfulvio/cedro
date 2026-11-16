import subprocess, sys
import numpy as np


def carica_audio(wav):
    """16 kHz mono float32 con ffmpeg (stessa conversione di faster_whisper.decode_audio, senza PyAV)."""
    r = subprocess.run(["ffmpeg", "-v", "error", "-nostdin", "-i", wav, "-f", "f32le", "-ac", "1", "-ar", "16000", "-"], capture_output=True)
    if r.returncode: raise RuntimeError("ffmpeg: " + r.stderr.decode(errors="replace")[-300:])
    return np.frombuffer(r.stdout, np.float32)


if __name__ == "__main__":
    from faster_whisper import WhisperModel
    modello, tipo, dev, wav = sys.argv[1:5]
    m = WhisperModel(modello, device=dev, compute_type=tipo)
    seg, _ = m.transcribe(carica_audio(wav), language="it", vad_filter=False, beam_size=5)
    print(" ".join(s.text.strip() for s in seg))
