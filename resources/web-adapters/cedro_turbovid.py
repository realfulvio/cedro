"""Cedro: public Turbovid player adapter. No CAPTCHA, login or DRM bypass."""
import re

from yt_dlp.extractor.common import InfoExtractor
from yt_dlp.utils import ExtractorError, decode_packed_codes, urlencode_postdata


class CedroTurbovidIE(InfoExtractor):
    IE_NAME = 'cedro:turbovid'
    _VALID_URL = r'https?://(?:www\.)?turbovid\.me/(?:embed-)?(?P<id>[a-zA-Z0-9]{12})(?:\.html)?(?:[/?#]|$)'
    _TESTS = [{
        'url': 'https://turbovid.me/dvbd6ts0w9v0',
        'only_matching': True,
    }]

    @staticmethod
    def _check_access(webpage):
        if re.search(r'(?:g-recaptcha|h-captcha|cf-turnstile|name=[\"\'](?:captcha|captch\d*)[\"\']|enter the code shown)', webpage, re.I):
            raise ExtractorError('Il sito richiede un CAPTCHA: completalo nel browser. Cedro non lo aggira.', expected=True)
        if re.search(r'(?:file (?:was |has been )?(?:deleted|removed)|file not found|video has been removed)', webpage, re.I):
            raise ExtractorError('Il video è stato rimosso o non è disponibile.', expected=True)

    @staticmethod
    def _media_url(webpage):
        # Public player configuration: unpack text only, never execute site JavaScript.
        scripts = [webpage]
        for packed in re.findall(r'<script\b[^>]*>(.*?)</script\s*>', webpage, re.I | re.S):
            if packed.lstrip().startswith('eval(function(p,a,c,k,e,'):
                try:
                    scripts.append(decode_packed_codes(packed))
                except Exception:
                    continue
        for script in scripts:
            source = re.search(r'\bsources\s*:\s*\[\s*[\"\'](https?://[^\"\']+)[\"\']', script)
            if not source:
                source = re.search(r'<(?:video|source)\b[^>]*\bsrc=[\"\'](https?://[^\"\']+)[\"\']', script, re.I)
            if source:
                url = source.group(1).replace('\\/', '/').replace('&amp;', '&')
                # Query signatures supplied by the player are preserved, never generated or altered.
                if re.match(r'https?://(?:[a-zA-Z0-9-]+\.)*host-cdn\.net/[^\s\"\'<>]+\.mp4(?:\?[^\s\"\'<>]*)?$', url):
                    return url
        return None

    def _real_extract(self, url):
        video_id = self._match_id(url)
        page_url = f'https://turbovid.me/{video_id}'
        webpage = self._download_webpage(page_url, video_id)
        self._check_access(webpage)
        fields = self._hidden_inputs(webpage)
        title = self._html_extract_title(webpage, default=video_id)
        if fields.get('op') == 'download1':
            if fields.get('id') != video_id:
                raise ExtractorError('Il modulo del sito non corrisponde al video richiesto.', expected=True)
            # Match the normal public form and its countdown; never submit login/premium forms.
            form = re.search(r'<form\b([^>]*)>.*?</form\s*>', webpage, re.I | re.S)
            if not form or not re.search(r'method\s*=\s*[\"\']?post\b', form.group(1), re.I):
                raise ExtractorError('Il modulo pubblico del sito è cambiato.', expected=True)
            action = re.search(r'action\s*=\s*[\"\']([^\"\']*)[\"\']', form.group(1), re.I)
            if action and action.group(1) not in ('', f'/{video_id}', page_url):
                raise ExtractorError('Il modulo rimanda a una destinazione inattesa.', expected=True)
            wait = self._search_regex(r'id=[\"\']cxc[\"\'][^>]*>\s*(\d+)', webpage, 'countdown', default='5')
            if int(wait) > 60:
                raise ExtractorError('Il sito richiede una lunga attesa: riprova dal browser.', expected=True)
            self._sleep(max(5, int(wait)), video_id)
            fields['imhuman'] = 'Proceed to video'
            webpage = self._download_webpage(page_url, video_id, note='Apro il player pubblico', data=urlencode_postdata(fields), headers={'Referer': page_url})
            self._check_access(webpage)
        media_url = self._media_url(webpage)
        if not media_url:
            raise ExtractorError('Nessun MP4 pubblico riconosciuto: il sito può richiedere accesso o aver cambiato player.', expected=True)
        title = self._html_search_regex(r'<h1\b[^>]*>\s*Watch\s*(.*?)</h1>', webpage, 'title', default=title)
        title = re.sub(r'^Watch\s+', '', title).strip()
        return {'id': video_id, 'title': title, 'url': media_url, 'ext': 'mp4', 'http_headers': {'Referer': page_url}}
