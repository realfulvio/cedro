"""Eurostream catalog and public Clicka links. Access controls are terminal errors."""
import re
from html.parser import HTMLParser
from urllib.parse import urljoin, urlsplit

from yt_dlp.extractor.common import InfoExtractor
from yt_dlp.utils import ExtractorError


class _Node:
    def __init__(self, tag='', attrs=()):
        self.tag, self.attrs, self.children = tag, dict(attrs), []

    def text(self):
        return ''.join(c.text() if isinstance(c, _Node) else c for c in self.children)

    def walk(self):
        yield self
        for c in self.children:
            if isinstance(c, _Node):
                yield from c.walk()


class _Tree(HTMLParser):
    def __init__(self, html):
        super().__init__(convert_charrefs=True)
        self.root = _Node()
        self.stack = [self.root]
        self.feed(html)

    def handle_starttag(self, tag, attrs):
        n = _Node(tag, attrs)
        self.stack[-1].children.append(n)
        if tag not in ('br', 'img', 'meta', 'link', 'input', 'hr', 'source', 'area', 'base', 'embed', 'wbr'):
            self.stack.append(n)

    def handle_startendtag(self, tag, attrs):
        self.handle_starttag(tag, attrs)
        if self.stack[-1].tag == tag:
            self.stack.pop()

    def handle_endtag(self, tag):
        for i in range(len(self.stack) - 1, 0, -1):
            if self.stack[i].tag == tag:
                del self.stack[i:]
                break

    def handle_data(self, data):
        self.stack[-1].children.append(data)


class CedroEurostreamIE(InfoExtractor):
    IE_NAME = 'cedro:eurostream'
    _VALID_URL = r'https?://(?:www\.)?eurostream\.fit/(?P<id>matrimonio-a-prima-vista-italia)/?(?:[?#]|$)'

    @staticmethod
    def catalog(html, page_url):
        entries, seen = [], set()
        for node in _Tree(html).root.walk():
            if 'su-spoiler' not in node.attrs.get('class', '').split():
                continue
            title = next((n for n in node.children if isinstance(n, _Node) and 'su-spoiler-title' in n.attrs.get('class', '').split()), None)
            body = next((n for n in node.children if isinstance(n, _Node) and 'su-spoiler-content' in n.attrs.get('class', '').split()), None)
            if title is None or body is None:
                continue
            label = ' '.join(title.text().split())
            season = re.search(r'STAGIONE\s+(\d+)', label, re.I)
            if not season:
                continue
            lines, line = [], []
            for child in body.children:
                if isinstance(child, _Node) and child.tag == 'br':
                    lines.append(line)
                    line = []
                else:
                    line.append(child)
            lines.append(line)
            for line in lines:
                text = ' '.join(''.join(c.text() if isinstance(c, _Node) else c for c in line).split())
                match = re.match(r'(\d+)\s*[×xX]\s*(\d+)\s+(.+)', text)
                if not match:
                    continue
                episode_title = re.split(r'\s+[–—]\s+', match[3], maxsplit=1)[0]
                key = (label, int(match[2]), episode_title)
                if key in seen:
                    continue
                links = [a for n in line if isinstance(n, _Node) for a in n.walk() if a.tag == 'a' and a.attrs.get('href')]
                # Prefer the host supported by Cedro. Other hosts remain explicit unsupported URLs.
                links.sort(key=lambda a: 'turbovid' not in a.text().lower())
                if not links:
                    continue
                target = urljoin(page_url, links[0].attrs['href'].strip())
                if urlsplit(target).scheme not in ('http', 'https'):
                    continue
                seen.add(key)
                entries.append({'_type': 'url', 'url': target, 'title': episode_title, 'season': label, 'season_number': int(season[1]), 'episode_number': int(match[2])})
        return entries

    def _real_extract(self, url):
        catalog_id = self._match_id(url)
        webpage = self._download_webpage(url, catalog_id)
        entries = self.catalog(webpage, url)
        if not entries:
            raise ExtractorError('Nessun episodio riconosciuto: il catalogo del sito può essere cambiato.', expected=True)
        return self.playlist_result(entries, catalog_id, 'Matrimonio a prima vista Italia')


class CedroClickaIE(InfoExtractor):
    IE_NAME = 'cedro:clicka'
    _VALID_URL = r'https?://(?:www\.)?clicka\.cc/(?P<id>(?:tv|tva)/[^/?#]+)(?:[?#]|$)'

    def _real_extract(self, url):
        link_id = self._match_id(url)
        visited = set()
        for _ in range(4):
            if url in visited:
                break
            visited.add(url)
            webpage, response = self._download_webpage_handle(url, link_id)
            current = response.url
            if re.search(r'(?:enter the code shown|g-recaptcha|h-captcha|cf-turnstile|captch\d)', webpage, re.I):
                raise ExtractorError('Questo collegamento richiede un CAPTCHA nel browser. Cedro non lo aggira: usa poi il link pubblico del player.', expected=True)
            host = urlsplit(current).hostname
            if host in ('turbovid.me', 'www.turbovid.me'):
                return self.url_result(current, ie='CedroTurbovid')
            # Follow only visible public continuation links to known hosts; no token decoding.
            tree = _Tree(webpage)
            targets = []
            for node in tree.root.walk():
                if node.tag == 'a' and 'proceed to video' in node.text().lower():
                    target = urljoin(current, node.attrs.get('href', ''))
                    if urlsplit(target).scheme == 'https' and urlsplit(target).hostname in ('clicka.cc', 'www.clicka.cc', 'turbovid.me', 'www.turbovid.me'):
                        targets.append(target)
            if not targets:
                raise ExtractorError('Nessun collegamento pubblico al player riconosciuto. Apri il link nel browser.', expected=True)
            url = targets[0]
        raise ExtractorError('Troppi passaggi o collegamento circolare: apri il link nel browser.', expected=True)
