#!/usr/bin/env python3
"""Per CEDRO_YOUTUBE=1: prende da youtubedl-android (Maven Central) gli eseguibili per arm64 che
Android lascia partire solo dalla cartella delle librerie dell'APK. Degli archivi (60 e 140 MB) si
scaricano solo le voci che servono, con le richieste «Range»; ogni file ha il suo SHA-256.
Uso: youtube-libs.py CARTELLA"""
import hashlib, io, os, sys, time, urllib.error, urllib.request, zipfile

MAVEN = "https://repo1.maven.org/maven2/io/github/junkfood02/youtubedl-android"
VERSION = "0.18.1"
FILES = {
    "library": {
        "libpython.so": "58ba29cfc5a6d88281c07b2b480fb1d8cce1b78f280b002347f792cfd9cf5901",
        "libqjs.so": "a0fe160a8cd76eec34132f945664856e96687d701e3b1ff28dff44ea1fad13bc",
    },
    "ffmpeg": {
        "libffmpeg.so": "1b8d5c371ea79e6e7a1930b31f2ad6a2731e48d75ceddc9c36dffda6e0092401",
        "libffprobe.so": "a4d4a6d1562aa87934e99ee1a43f8090ec1040659cedb667d6edf3aa08408c5e",
    },
}


class Remote(io.RawIOBase):
    """Un file remoto letto a pezzi."""

    def __init__(self, url):
        self.url, self.pos, self.buf, self.at = url, 0, b"", 0
        with urllib.request.urlopen(urllib.request.Request(url, method="HEAD")) as r:
            self.len = int(r.headers["Content-Length"])

    def seekable(self):
        return True

    def readable(self):
        return True

    def seek(self, off, whence=0):
        self.pos = {0: off, 1: self.pos + off, 2: self.len + off}[whence]
        return self.pos

    def tell(self):
        return self.pos

    def readinto(self, b):
        if self.pos >= self.len or not len(b):
            return 0
        # Pezzi da 4 MB: zipfile legge a piccoli morsi, e una richiesta per morso sarebbe troppo.
        if not (self.at <= self.pos < self.at + len(self.buf)):
            end = min(self.pos + max(len(b), 4 << 20), self.len) - 1
            req = urllib.request.Request(self.url, headers={"Range": f"bytes={self.pos}-{end}"})
            for attempt in range(5):
                try:
                    with urllib.request.urlopen(req) as r:
                        if r.status != 206:
                            raise OSError("il server non manda pezzi del file")
                        self.buf, self.at = r.read(), self.pos
                    break
                except urllib.error.HTTPError as e:
                    # Maven Central a volte risponde «troppe richieste»: si riprova con calma.
                    if e.code != 429 or attempt == 4:
                        raise
                    time.sleep(5 * (attempt + 1))
        data = self.buf[self.pos - self.at : self.pos - self.at + len(b)]
        b[: len(data)] = data
        self.pos += len(data)
        return len(data)

def main(dest):
    os.makedirs(dest, exist_ok=True)
    for aar, files in FILES.items():
        url = f"{MAVEN}/{aar}/{VERSION}/{aar}-{VERSION}.aar"
        with zipfile.ZipFile(Remote(url)) as z:
            for name, sha in files.items():
                data = z.read(f"jni/arm64-v8a/{name}")
                if hashlib.sha256(data).hexdigest() != sha:
                    sys.exit(f"{name}: SHA-256 diverso da quello atteso")
                with open(os.path.join(dest, name), "wb") as f:
                    f.write(data)
                print(f"{name} ({len(data)} byte)")


if __name__ == "__main__":
    main(sys.argv[1])
