"""Parser/access boundary tests, no network. yt-dlp APIs are exercised by live CLI tests."""
import importlib.util
from pathlib import Path
import sys
import types
import unittest

base = Path(__file__).parent / 'yt_dlp_plugins' / 'extractor'
if not base.exists(): base = Path(__file__).parent
# Keep static parser tests independent from an installed yt-dlp/Python distribution.
for name in ('yt_dlp', 'yt_dlp.extractor', 'yt_dlp.extractor.common', 'yt_dlp.utils'):
    sys.modules[name] = types.ModuleType(name)
sys.modules['yt_dlp.extractor.common'].InfoExtractor = object
class ExtractorError(Exception):
    def __init__(self, msg, **kw): super().__init__(msg)
sys.modules['yt_dlp.utils'].ExtractorError = ExtractorError
sys.modules['yt_dlp.utils'].decode_packed_codes = lambda _: (_ for _ in ()).throw(ValueError())
sys.modules['yt_dlp.utils'].urlencode_postdata = lambda _: b''

def load(name):
    spec = importlib.util.spec_from_file_location(name, base / (name + '.py'))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod

turbo = load('cedro_turbovid').CedroTurbovidIE
euro = load('cedro_eurostream').CedroEurostreamIE

class Adapters(unittest.TestCase):
    def test_exact_host(self):
        for url in ('https://turbovid.me.evil.example/abcdefgh1234', 'https://evil.example/abcdefgh1234', 'https://turbovid.me/short'):
            self.assertIsNone(__import__('re').match(turbo._VALID_URL, url))
        self.assertIsNotNone(__import__('re').match(turbo._VALID_URL, 'https://turbovid.me/abcdefgh1234'))

    def test_public_source_preserves_signature(self):
        url = 'https://tu01.host-cdn.net/v/test/n.mp4?t=unaltered&s=123'
        self.assertEqual(turbo._media_url('sources:["' + url + '"]'), url)

    def test_reject_other_hosts_and_manifests(self):
        for url in ('https://host-cdn.net.evil.example/n.mp4', 'https://tu01.host-cdn.net/stream.m3u8', 'file:///tmp/video.mp4', 'https://user@evil.example/n.mp4'):
            self.assertIsNone(turbo._media_url('sources:["' + url + '"]'))

    def test_access_controls_stop(self):
        for html in ('<input name="captch5">', '<div class="g-recaptcha">', 'Enter the code shown', 'File has been removed'):
            with self.assertRaises(ExtractorError): turbo._check_access(html)

    def test_catalog_duplicate_numbers_keep_editions(self):
        def section(label, title):
            return f'<div class="su-spoiler"><div class="su-spoiler-title">{label}</div><div class="su-spoiler-content">16×01 {title} – <a href="https://clicka.cc/tv/one">Turbovid</a><br>16×01 {title} – <a href="https://clicka.cc/tv/one">Turbovid</a><br></div></div>'
        html = section('STAGIONE 16 (gennaio)', 'Titolo originale') + section('STAGIONE 16 (settembre)', 'Altro titolo')
        entries = euro.catalog(html, 'https://eurostream.fit/show/')
        self.assertEqual(len(entries), 2)
        self.assertEqual([x['title'] for x in entries], ['Titolo originale', 'Altro titolo'])
        self.assertNotEqual(entries[0]['season'], entries[1]['season'])

    def test_catalog_no_ads_and_prefer_supported_host(self):
        html = '<a href="https://ads.invalid">ad</a><div class="su-spoiler"><div class="su-spoiler-title">STAGIONE 4</div><div class="su-spoiler-content">4×01 Titolo – <a href="https://clicka.cc/delta/other">DeltaBit</a> – <a href="https://clicka.cc/tv/one">Turbovid</a><br></div></div>'
        entries = euro.catalog(html, 'https://eurostream.fit/show/')
        self.assertEqual(len(entries), 1)
        self.assertEqual(entries[0]['url'], 'https://clicka.cc/tv/one')

if __name__ == '__main__': unittest.main()
