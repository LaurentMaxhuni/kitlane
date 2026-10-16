"""Build offline Homebrew/Flathub associations. Python stdlib; never executes package scripts."""
import argparse
from collections import defaultdict
from datetime import date
import gzip
import hashlib
import io
import json
from pathlib import Path
import re
import tempfile
import unicodedata
from urllib.parse import urlsplit, urlunsplit
import xml.etree.ElementTree as ET

SOURCES = {'homebrew': 'https://formulae.brew.sh/api/cask.json',
           'flathub': 'https://dl.flathub.org/repo/appstream/x86_64/appstream.xml.gz'}
MAX_BYTES = 64 * 1024 * 1024


def text(value, limit=512):
    return (isinstance(value, str) and 0 < len(value.encode('utf-8')) <= limit
            and value == value.strip() and all(unicodedata.category(c) != 'Cc' for c in value))


def identifier(value):
    return text(value, 256) and re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._+-]*', value) is not None


def homepage(value):
    if not value:
        return None
    if not text(value, 2048) or not value.isascii() or '\\' in value:
        return None
    parsed = urlsplit(value)
    if parsed.username or parsed.password:
        raise ValueError('Credential-bearing homepage')
    if parsed.scheme != 'https' or not parsed.hostname or parsed.port:
        return None
    if not re.fullmatch(r'[A-Za-z0-9.-]+', parsed.hostname):
        return None
    return urlunsplit(('https', parsed.hostname.lower(), parsed.path.rstrip('/'), parsed.query, ''))


def homebrew(casks):
    if not isinstance(casks, list) or len(casks) > 30_000:
        raise ValueError('Invalid Homebrew catalog')
    result, seen, owners = [], set(), defaultdict(set)
    for cask in casks:
        token = cask['token']
        if not text(token, 256) or not re.fullmatch(r'[a-z0-9][a-z0-9@+._-]*', token) or token in seen:
            raise ValueError('Invalid or duplicate cask token')
        seen.add(token)
        if cask.get('disabled') or cask.get('deprecated'):
            continue
        names = cask['name']
        if not isinstance(names, list) or not names or len(names) > 64 or not all(text(n) for n in names):
            raise ValueError('Invalid cask names')
        bundles = set()
        for artifact in cask.get('artifacts', []):
            for uninstall in artifact.get('uninstall', []):
                if not isinstance(uninstall, dict):
                    continue
                values = uninstall.get('quit', [])
                if isinstance(values, str):
                    values = [values]
                if isinstance(values, list):
                    bundles.update(v for v in values if identifier(v) and '.' in v)
        for bundle in bundles:
            owners[bundle].add(token)
        result.append({'provider': 'homebrew', 'id': token, 'name': names[0],
                       'names': names, 'homepage': homepage(cask.get('homepage')),
                       'native_ids': sorted(bundles)})
    for package in result:
        package['native_ids'] = [i for i in package['native_ids'] if len(owners[i]) == 1]
    return result


def flathub(xml):
    if len(xml) > MAX_BYTES or b'<!DOCTYPE' in xml or b'<!ENTITY' in xml:
        raise ValueError('Unsafe or oversized AppStream document')
    root = ET.fromstring(xml)
    if root.tag != 'components' or len(root) > 30_000:
        raise ValueError('Invalid AppStream root or count')
    result, seen = [], set()
    for component in root:
        if component.get('type') not in ('desktop-application', 'desktop'):
            continue
        component_id = component.findtext('id')
        name = next((n.text for n in component.findall('name') if not n.get('{http://www.w3.org/XML/1998/namespace}lang')), None)
        stable = [b.text.split('/') for b in component.findall('bundle')
                  if b.get('type') == 'flatpak' and b.text and b.text.startswith('app/')
                  and b.text.endswith('/x86_64/stable')]
        if len(stable) != 1 or len(stable[0]) != 4:
            continue
        app_id = stable[0][1]
        if (not identifier(app_id) or not text(name) or app_id in seen
                or component_id not in (app_id, app_id + '.desktop')):
            raise ValueError('Invalid or duplicate Flatpak application')
        seen.add(app_id)
        result.append({'provider': 'flathub', 'id': app_id, 'name': name, 'names': [name],
                       'homepage': homepage(component.findtext("url[@type='homepage']")),
                       'native_ids': [app_id]})
    return result


def link_packages(packages, links):
    by_key, candidates = {}, defaultdict(lambda: defaultdict(set))
    for package in packages:
        key = package['provider'] + ':' + package['id']
        if key in by_key:
            raise ValueError('Duplicate provider package')
        package['canonical_id'] = links.get(key, key)
        by_key[key] = package
        qualified = package['provider'] == 'homebrew' and (
            '@' in package['id'] or re.search(r'(^|-)(alpha|beta|nightly|dev|canary|insiders?|preview|experimental)(-|$)', package['id']))
        if package['homepage'] and not qualified:
            candidates[(package['homepage'], package['name'].casefold())][package['provider']].add(key)
    if set(links) - by_key.keys() or any(not text(v, 256) for v in links.values()):
        raise ValueError('Unknown package or invalid canonical link')
    # Only reciprocal, unambiguous homepage+name matches. Never name-only matching.
    proposed = defaultdict(set)
    for owners in candidates.values():
        if set(owners) == {'homebrew', 'flathub'} and all(len(v) == 1 for v in owners.values()):
            mac, linux = next(iter(owners['homebrew'])), next(iter(owners['flathub']))
            proposed[mac].add(linux)
            proposed[linux].add(mac)
    for mac in sorted(by_key):
        if not mac.startswith('homebrew:') or len(proposed[mac]) != 1:
            continue
        linux = next(iter(proposed[mac]))
        if len(proposed[linux]) != 1 or mac in links or linux in links:
            continue
        by_key[linux]['canonical_id'] = mac
    return sorted(packages, key=lambda p: (p['provider'], p['id']))


def generate(brew_path, flatpak_path, links_path, output, generated_on):
    if date.fromisoformat(generated_on).year < 2020:
        raise ValueError('Invalid generation date')
    sources = {}
    for provider, path in [('homebrew', brew_path), ('flathub', flatpak_path)]:
        if path.stat().st_size > MAX_BYTES:
            raise ValueError('Source byte limit exceeded')
        sources[provider] = path.read_bytes()
    try:
        with gzip.GzipFile(fileobj=io.BytesIO(sources['flathub'])) as stream:
            xml = stream.read(MAX_BYTES + 1)
    except (OSError, EOFError) as error:
        raise ValueError('Invalid compressed AppStream source') from error
    packages = homebrew(json.loads(sources['homebrew'])) + flathub(xml)
    if not packages:
        raise ValueError('Empty provider catalog')
    links = json.loads(links_path.read_text(encoding='utf-8'))
    packages = link_packages(packages, links)
    metadata = {'schema_version': 1, 'generated_on': generated_on,
                'sources': [{'provider': p, 'url': SOURCES[p], 'sha256': hashlib.sha256(b).hexdigest()}
                            for p, b in sources.items()]}
    payload = (json.dumps(metadata, indent=2)[:-2] + ',\n  "packages": [\n'
               + ',\n'.join('    ' + json.dumps(p, ensure_ascii=False, separators=(',', ':')) for p in packages)
               + '\n  ]\n}\n').encode('utf-8')
    if len(payload) > 16 * 1024 * 1024:
        raise ValueError('Output byte limit exceeded')
    with tempfile.NamedTemporaryFile(dir=output.parent, delete=False, suffix='.tmp') as handle:
        temporary = Path(handle.name)
        try:
            handle.write(payload)
            handle.close()
            temporary.replace(output)
        finally:
            temporary.unlink(missing_ok=True)
    print(f'{len(packages)} provider packages; {len(payload)} bytes')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('homebrew', 'flathub', 'links', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--generated-on', required=True)
    args = parser.parse_args()
    generate(args.homebrew, args.flathub, args.links, args.output, args.generated_on)
