"""Lancia un comando e stampa un JSON con rc, tempo vero e RSS di picco DI QUEL processo.
Esiste perché ru_maxrss di un figlio parte dal picco del genitore al momento del fork/vfork+exec (Linux conta il mm vecchio):
se il genitore è un Python con numpy/pymupdf/jiwer già caricati, ogni processo piccolo risulterebbe «grande quanto il genitore».
Questo lanciatore è piccolo (python -I -S, ~9 MB) e rende il pavimento trascurabile.
Uso: python3 -I -S rss_helper.py '<json: {"cmd": [...], "cwd": null, "env": null, "timeout": 300}>'"""
import json, os, subprocess, sys, tempfile, time
a = json.loads(sys.argv[1])
fo, fe = tempfile.TemporaryFile(), tempfile.TemporaryFile()
t0 = time.perf_counter()
p = subprocess.Popen(a["cmd"], stdout=fo, stderr=fe, cwd=a.get("cwd"), env=a.get("env"))
fine = time.time() + a.get("timeout", 300); rc = None
while True:
    r = os.wait4(p.pid, os.WNOHANG)
    if r[0]:
        rc, ru = os.waitstatus_to_exitcode(r[1]), r[2]; break
    if time.time() > fine:
        p.kill(); r = os.wait4(p.pid, 0); rc, ru = 124, r[2]; break
    time.sleep(0.005)
s = time.perf_counter() - t0
def leggi(f, tronca=400000):
    f.seek(0); b = f.read(); return b[-tronca:].decode(errors="replace") + ("TIMEOUT" if rc == 124 and f is fe else "")
print(json.dumps({"rc": rc, "s": round(s, 3), "rss_mb": round(ru.ru_maxrss / 1024, 1), "out": leggi(fo), "err": leggi(fe)}))
