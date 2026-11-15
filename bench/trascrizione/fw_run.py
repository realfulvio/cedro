import sys
from faster_whisper import WhisperModel
modello, tipo, dev, wav = sys.argv[1:5]
m = WhisperModel(modello, device=dev, compute_type=tipo)
seg, _ = m.transcribe(wav, language="it", vad_filter=False, beam_size=5)
print(" ".join(s.text.strip() for s in seg))
