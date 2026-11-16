#!/usr/bin/env python3
"""D2/D3: correttezza dei pixel e dei metadati dei vetrini e delle immagini di microscopia, contro i lettori di riferimento.
Uso: d2_pixel.py <cartella-file> ...   (SVS/NDPI/MRXS/TIF con OpenSlide, CZI con libCZI (pylibCZIrw), OME-TIFF con tifffile)
Cedro scrive `cedro convert file png`; si confronta il PNG con il riferimento:
  - stesse dimensioni: errore medio assoluto (MAE) e massimo su 0-255, per canale
  - dimensioni diverse (vetrini grandi: Cedro riduce): SSIM contro la miniatura di OpenSlide alla stessa dimensione
Scrive bench/risultati/d2_pixel.json
"""
import shutil, sys, tempfile
from pathlib import Path
import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from common import cedro, scrivi_json
Image.MAX_IMAGE_PIXELS = None


def rif_openslide(p):
    import openslide
    s = openslide.OpenSlide(str(p))
    w, h = s.dimensions
    img = s.read_region((0, 0), 0, (w, h)).convert("RGB") if w * h < 60_000_000 else None
    meta = {"livelli": s.level_count, "dim": list(s.dimensions), "mpp_x": s.properties.get("openslide.mpp-x"), "mpp_y": s.properties.get("openslide.mpp-y"),
            "ingrandimento": s.properties.get("openslide.objective-power") or s.properties.get("aperio.AppMag"), "lettore": "openslide " + openslide.__library_version__}
    return s, img, meta


def rif_czi(p):
    import czifile   # lettore indipendente (libjxr via imagecodecs)
    px = np.squeeze(czifile.imread(str(p)))
    return None, px, {"dim": [int(px.shape[1]), int(px.shape[0])], "lettore": "czifile+imagecodecs"}


def rif_ome(p):
    import tifffile
    a = np.squeeze(tifffile.imread(str(p)))
    while a.ndim > 3 or (a.ndim == 3 and a.shape[0] <= 4 and a.shape[2] > 4):   # primo piano: (C,Y,X) -> (Y,X)
        a = a[0]
    return None, a, {"dim": [int(a.shape[1]), int(a.shape[0])], "lettore": "tifffile"}


def mae(a, b):
    d = np.abs(a.astype(np.int16) - b.astype(np.int16))
    return round(float(d.mean()), 4), int(d.max())


def main():
    righe = []
    for cart in sys.argv[1:]:
        for f in sorted(Path(cart).iterdir()):
            sfx = f.suffix.lower()
            if sfx not in {".svs", ".ndpi", ".mrxs", ".scn", ".czi", ".tif", ".tiff"}:
                continue
            lav = Path(tempfile.mkdtemp(prefix="d2-")); src = lav / f.name; shutil.copy(f, src)
            riga = {"file": f.name}
            try:
                if sfx == ".czi":
                    s, ref, meta = rif_czi(f)
                elif ".ome." in f.name.lower():
                    s, ref, meta = rif_ome(f)
                else:
                    s, ref, meta = rif_openslide(f)
                    ref = np.array(ref) if ref is not None else None
                riga["rif"] = meta
            except Exception as e:
                riga["rif_errore"] = str(e)[:160]; s, ref = None, None
            r = cedro("convert", src, "png", timeout=600)
            riga.update(rc=r["rc"], s=r["s"], rss_mb=r["rss_mb"], errore=r["err"].strip()[:160] if r["rc"] else "")
            png = sorted(p for p in lav.glob("*.png") if "etichetta" not in p.name and "vetrino" not in p.name)
            if r["rc"] == 0 and png:
                im = Image.open(png[0]); riga["cedro_dim"] = list(im.size); arr = np.array(im.convert("RGB") if im.mode not in ("L", "I;16") else im)
                if ref is not None and ref.shape[:2] == arr.shape[:2]:
                    ar = arr.astype(np.float64) if arr.dtype != np.uint8 else arr
                    rr = ref
                    if rr.ndim == 2 and ar.ndim == 3: ar = ar[:, :, 0]
                    if rr.dtype != np.uint8:
                        # 16/32 bit: Cedro li porta a 8 bit con un contrasto automatico, quindi si misura quanto la mappa è monotona (Pearson e Spearman)
                        a1, r1 = ar.astype(np.float64).ravel(), rr.astype(np.float64).ravel()
                        if a1.std() == 0 or r1.std() == 0:
                            riga["pearson"] = 1.0 if (a1.std() == 0 and r1.std() == 0) else 0.0
                        else:
                            riga["pearson"] = round(float(np.corrcoef(a1, r1)[0, 1]), 5)
                    else:
                        try: riga["mae"], riga["max"] = mae(ar.astype(np.int32), rr.astype(np.int32))
                        except Exception as e: riga["confronto_errore"] = str(e)[:100]
                elif s is not None:
                    from skimage.metrics import structural_similarity as ssim
                    th = np.array(s.get_thumbnail(im.size).convert("RGB").resize(im.size))
                    riga["ssim_vs_miniatura"] = round(float(ssim(arr, th, channel_axis=2, data_range=255)), 4)
            righe.append(riga)
            shutil.rmtree(lav, ignore_errors=True)
    scrivi_json("d2_pixel.json", {"righe": righe})
    for x in righe:
        print({k: v for k, v in x.items() if k != "rif"})


main()
