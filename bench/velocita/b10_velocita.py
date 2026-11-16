#!/usr/bin/env python3
"""B10/D5/D9: velocità e memoria di Cedro contro i programmi di riferimento, 5 ripetizioni (mediana, minimo-massimo).
  PDF da 1/10/100/500 pagine (seme 42): testo, immagini, PDF→txt, PDF→png a 144 dpi (tutte le pagine), «più piccolo», PDF→Word;
  Word→PDF (DOCX da 1/10/100 pagine) contro LibreOffice.
  Vetrini: `cedro convert vetrino.<ext> png` (tempo, RSS) contro OpenSlide (miniatura e regione).
Si lancia sul PC di collaudo; ogni riga annota il carico della macchina (loadavg) perché il PC di collaudo ospita anche CI e una VM.
Uso: b10_velocita.py pdf | word | vetrini  [--rip 5] [--nfile 3]
Scrive bench/risultati/b10_velocita_<area>.json
"""
import json, os, random, shutil, statistics, sys, tempfile
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from common import CEDRO, esegui, SEED

PY = os.environ.get("PYFW", sys.executable)
AQUI = Path(__file__).resolve().parent
RIS = AQUI.parents[0] / "risultati"
REP = int(sys.argv[sys.argv.index("--rip") + 1]) if "--rip" in sys.argv else 5
NFILE = int(sys.argv[sys.argv.index("--nfile") + 1]) if "--nfile" in sys.argv else 3
DIM = [int(x) for x in sys.argv[sys.argv.index("--dim") + 1].split(",")] if "--dim" in sys.argv else [1, 10, 100, 500]
DATI = Path(os.environ.get("BENCH_DATI", str(Path.home() / "bench-dati")))
PAROLE = "Il preparato istologico mostra una proliferazione neoplastica epiteliale con atipie nucleari di grado intermedio, indice mitotico elevato e infiltrazione dello stroma adiacente.".split()

# piccoli script di riferimento (processo Python: il suo avvio e l'import sono nel tempo e nella memoria, e si riporta a parte la base)
REF = {
    "mupdf_txt": "import sys,pymupdf;d=pymupdf.open(sys.argv[1]);open(sys.argv[2],'w').write('\\n'.join(p.get_text() for p in d))",
    "mupdf_png": "import sys,pymupdf;d=pymupdf.open(sys.argv[1])\nfor i,p in enumerate(d): p.get_pixmap(dpi=144).save(f'{sys.argv[2]}-{i+1}.png')",
    "pikepdf_compress": "import sys,pikepdf;p=pikepdf.open(sys.argv[1]);p.save(sys.argv[2],object_stream_mode=pikepdf.ObjectStreamMode.generate,compress_streams=True,recompress_flate=True)",
    "pdf2docx": "import sys;from pdf2docx import Converter;c=Converter(sys.argv[1]);c.convert(sys.argv[2]);c.close()",
    "base_mupdf": "import pymupdf", "base_pikepdf": "import pikepdf", "base_pdf2docx": "from pdf2docx import Converter",
}


def mediana(v): return round(statistics.median(v), 3)


def stat(ms):
    ok = [m for m in ms if m["rc"] == 0]
    if not ok: return {"rc": [m["rc"] for m in ms], "err": ms[0]["err"][-200:]}
    t, r = [m["s"] for m in ok], [m["rss_mb"] for m in ok]
    return {"n": len(ok), "falliti": len(ms) - len(ok), "s_freddo": t[0], "s_mediana_caldo": mediana(t[1:] or t), "s_min": min(t), "s_max": max(t),
            "rss_mediana_mb": mediana(r), "rss_min_mb": min(r), "rss_max_mb": max(r), "carico": ms[0]["carico"]}


def ripeti(cmd_fun, pulisci, timeout=900):
    ms = []
    for _ in range(REP):
        pulisci()
        cmd, env = cmd_fun()
        m = esegui(cmd, timeout=timeout, env=env)
        m["carico"] = round(os.getloadavg()[0], 1)
        ms.append(m)
    return stat(ms)


def genera_pdf(dst, n, seed):
    import pymupdf
    rnd = random.Random(seed); d = pymupdf.open()
    for i in range(n):
        p = d.new_page()
        p.insert_textbox(pymupdf.Rect(50, 50, 545, 700), " ".join(rnd.choices(PAROLE, k=330)), fontsize=10)
        p.draw_rect(pymupdf.Rect(50, 720, 545, 780), color=(0.2, 0.2, 0.2))
        p.insert_text((60, 755), f"Pagina {i+1} di {n} · caso V-2026-{seed:04d}", fontsize=9)
        if i % 5 == 0:   # un'immagine ogni 5 pagine, come un referto con foto
            pm = pymupdf.Pixmap(pymupdf.csRGB, pymupdf.IRect(0, 0, 400, 150), False)
            pm.set_rect(pm.irect, (rnd.randint(60, 220), rnd.randint(60, 220), rnd.randint(60, 220)))
            p.insert_image(pymupdf.Rect(150, 600, 450, 712), pixmap=pm)
    d.save(str(dst), deflate=True); d.close()


def area_pdf():
    lav = Path(tempfile.mkdtemp(prefix="b10-")); righe = []
    out = lav / "o"; out.mkdir()
    def pulisci():
        for f in out.iterdir(): f.unlink()
    # base: costo del solo avvio dei riferimenti Python
    base = {}
    for k in ("base_mupdf", "base_pikepdf", "base_pdf2docx"):
        s = stat([esegui([PY, "-c", REF[k]]) | {"carico": round(os.getloadavg()[0], 1)} for _ in range(REP)])
        base[k] = s; print(k, s)
    for n in DIM:
        for k in range(NFILE if n < 500 else min(NFILE, 3)):
            f = lav / f"p{n}_{k}.pdf"; genera_pdf(f, n, SEED + n + k)
            ops = {
                "pdf_txt": {"cedro": lambda: ([CEDRO, "convert", str(f), "txt"], None), "pdftotext": lambda: (["pdftotext", str(f), str(out / "o.txt")], None),
                            "pymupdf": lambda: ([PY, "-c", REF["mupdf_txt"], str(f), str(out / "o.txt")], None)},
                "pdf_png144": {"cedro": lambda: ([CEDRO, "convert", str(f), "png"], None), "pdftoppm": lambda: (["pdftoppm", "-r", "144", "-png", str(f), str(out / "o")], None),
                              "pymupdf": lambda: ([PY, "-c", REF["mupdf_png"], str(f), str(out / "o")], None)},
                "pdf_compress": {"cedro": lambda: ([CEDRO, "tool", str(f), "compress"], None),
                                 "gs_ebook": lambda: (["gs", "-q", "-dNOPAUSE", "-dBATCH", "-sDEVICE=pdfwrite", "-dPDFSETTINGS=/ebook", f"-sOutputFile={out/'o.pdf'}", str(f)], None),
                                 "pikepdf_qpdf": lambda: ([PY, "-c", REF["pikepdf_compress"], str(f), str(out / "o.pdf")], None)},
                "pdf_docx": {"cedro": lambda: ([CEDRO, "convert", str(f), "docx"], None), "pdf2docx": lambda: ([PY, "-c", REF["pdf2docx"], str(f), str(out / "o.docx")], None)},
            }
            for op, tools in ops.items():
                for tool, cf in tools.items():
                    if tool == "pdf2docx" and n >= 500 and k > 0: continue     # pdf2docx a 500 pagine impiega ore: una sola misura
                    # i risultati di Cedro finiscono accanto al file: lavoriamo in una copia che si pulisce
                    def pul(f=f):
                        pulisci()
                        for g in f.parent.iterdir():
                            if g.is_file() and g.suffix != ".pdf" or "-compresso" in g.name: g.unlink()
                    r = ripeti(cf, pul, timeout=1800 if tool == "pdf2docx" else 900)
                    r.update({"op": op, "strumento": tool, "pagine": n, "file": f.name}); righe.append(r)
                    print(op, tool, n, k, r.get("s_mediana_caldo"), r.get("rss_mediana_mb"), "carico", r.get("carico"), flush=True)
            (RIS / "b10_velocita_pdf.json").write_text(json.dumps({"base_riferimenti_python": base, "righe": righe}, indent=1, ensure_ascii=False))
        for g in lav.glob(f"p{n}_*"): g.unlink()


def area_word():
    from docx import Document
    lav = Path(tempfile.mkdtemp(prefix="b10w-")); righe = []
    for n in (1, 10, 100):
        for k in range(NFILE):
            rnd = random.Random(SEED + n + k); d = Document()
            for _ in range(n * 4):
                d.add_paragraph(" ".join(rnd.choices(PAROLE, k=90)))
            f = lav / f"w{n}_{k}.docx"; d.save(f)
            tools = {"cedro": lambda: ([CEDRO, "convert", str(f), "pdf"], None),
                     "libreoffice": lambda: (["flatpak", "run", "--command=soffice", "org.libreoffice.LibreOffice", "--headless", "--convert-to", "pdf", "--outdir", str(lav / "lo"), str(f)], None)}
            for tool, cf in tools.items():
                def pul(f=f):
                    shutil.rmtree(lav / "lo", ignore_errors=True); (f.parent / (f.stem + ".pdf")).unlink(missing_ok=True)
                r = ripeti(cf, pul, 900); r.update({"op": "docx_pdf", "strumento": tool, "pagine_circa": n, "file": f.name}); righe.append(r)
                print(tool, n, k, r.get("s_mediana_caldo"), r.get("rss_mediana_mb"), flush=True)
    (RIS / "b10_velocita_word.json").write_text(json.dumps({"righe": righe}, indent=1, ensure_ascii=False))


def area_vetrini():
    import openslide
    righe = []; cart = DATI / "vetrini"
    for f in sorted(cart.rglob("*")):
        if f.suffix.lower() not in (".svs", ".ndpi", ".mrxs", ".scn", ".tif", ".tiff"): continue
        print(f.name)
        def pul(f=f):
            for g in f.parent.iterdir():
                if g.suffix.lower() in (".png", ".jpg") and g.stem.startswith(f.stem): g.unlink()
        r = ripeti(lambda: ([CEDRO, "convert", str(f), "png"], None), pul, 1800)
        r.update({"op": "convert_png", "strumento": "cedro", "file": f.name, "byte": f.stat().st_size}); righe.append(r); print(r)
        s = openslide.OpenSlide(str(f)); w, h = s.dimensions
        cod = f"import sys,openslide;s=openslide.OpenSlide(sys.argv[1]);w,h=s.dimensions;s.get_thumbnail((2048,2048)).save(sys.argv[2])"
        r = ripeti(lambda: ([PY, "-c", cod, str(f), str(f.parent / (f.stem + "-os.png"))], None), lambda: None, 600)
        r.update({"op": "miniatura_2048", "strumento": "openslide", "file": f.name, "dim": [w, h]}); righe.append(r); print(r)
        cod2 = "import sys,openslide;s=openslide.OpenSlide(sys.argv[1]);s.read_region((0,0),0,(4096,4096)).convert('RGB')"
        r = ripeti(lambda: ([PY, "-c", cod2, str(f)], None), lambda: None, 600)
        r.update({"op": "regione_4096_livello0", "strumento": "openslide", "file": f.name}); righe.append(r); print(r)
        (f.parent / (f.stem + "-os.png")).unlink(missing_ok=True)
    (RIS / "b10_velocita_vetrini.json").write_text(json.dumps({"righe": righe}, indent=1, ensure_ascii=False))


if __name__ == "__main__":
    {"pdf": area_pdf, "word": area_word, "vetrini": area_vetrini}[sys.argv[1]]()
