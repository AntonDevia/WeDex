"""Read the fixed Google Apps Script deployment; never proxy arbitrary URLs."""
import json
import re
import argparse
import time
import urllib.parse
import urllib.request
from pathlib import Path


def google_url(url):
    u = urllib.parse.urlsplit(url)
    if (u.scheme != 'https' or u.hostname not in ('script.google.com', 'script.googleusercontent.com')
            or u.username or u.password or u.port not in (None, 443)):
        raise ValueError('Expected the Google Apps Script HTTPS deployment')
    return url


class GoogleRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        google_url(newurl)
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def read(url):
    with urllib.request.build_opener(GoogleRedirect()).open(google_url(url), timeout=90) as r:
        body = r.read(8193)
    if len(body) > 8192:
        raise ValueError('Resolver record is too large')
    value = json.loads(body)
    if value.get('error'):
        raise RuntimeError('Google resolver: ' + str(value['error'])[:200])
    if value.get('service') != 'wedex-hitmo-mirror' or value.get('schema') != 1:
        raise ValueError('Unexpected resolver record')
    assert value.get('source') == 'https://hitmos.pro/' and 'error' not in value
    assert re.fullmatch(r'https://(?:[a-z0-9-]+\.)*hitmo[a-z0-9-]*\.[a-z]{2,15}/', value['origin'])
    now = int(time.time() * 1000)
    assert now - 1800000 < value['checked_at_ms'] < now + 300000
    assert value['discovery'] == 'google_redirect'
    return value


def valid_origin(value):
    return isinstance(value, str) and len(value) <= 255 and re.fullmatch(
        r'https://(?:[a-z0-9-]+\.)*hitmo[a-z0-9-]*\.[a-z]{2,15}/', value) is not None


def measurement(host, path='/', query='', residential=True):
    """Normal HTTPS GET from two public Russian probes, without account tokens."""
    api = 'https://api.globalping.io/v1/measurements'
    location = {'country': 'RU'}
    if residential:
        location['tags'] = ['eyeball-network']
    payload = {'type': 'http', 'target': host,
               'locations': [location], 'limit': 2,
               'measurementOptions': {'protocol': 'HTTPS',
                                      'request': {'method': 'GET', 'path': path, 'query': query}}}
    req = urllib.request.Request(api, data=json.dumps(payload).encode(),
                                 headers={'Content-Type': 'application/json'})
    with urllib.request.urlopen(req, timeout=20) as r:
        measurement = json.load(r)['id']
    assert re.fullmatch(r'[A-Za-z0-9_-]{1,80}', measurement)
    deadline = time.monotonic() + 65
    while time.monotonic() < deadline:
        time.sleep(2)
        with urllib.request.urlopen(api + '/' + measurement, timeout=20) as r:
            body = r.read(262145)
        if len(body) > 262144:
            raise ValueError('Probe result is too large')
        result = json.loads(body)
        if result.get('status') == 'finished':
            break
    else:
        raise TimeoutError('Russian availability check timed out')
    return measurement, result


def probe_discovery():
    """A separate source path when the Google deployment is temporarily unavailable."""
    mid, result = measurement('hitmos.pro', residential=False)
    origins = []
    for item in result.get('results', []):
        p, v = item.get('probe', {}), item.get('result', {})
        if p.get('country') != 'RU' or not v.get('tls', {}).get('authorized'):
            continue
        if v.get('statusCode') not in (301, 302, 303, 307, 308):
            continue
        location = v.get('headers', {}).get('location', '')
        if not isinstance(location, str):
            continue
        u = urllib.parse.urlsplit(location)
        if u.scheme != 'https' or u.username or u.password or u.port not in (None, 443):
            continue
        origin = f'https://{u.hostname}/'
        if valid_origin(origin) and origin not in origins:
            origins.append(origin)
    if not origins:
        raise RuntimeError('No verified bootstrap redirect from the Russian probes')
    return {'service': 'wedex-hitmo-mirror', 'schema': 1, 'source': 'https://hitmos.pro/',
            'origin': origins[0], 'alternatives': origins[1:], 'checked_at_ms': int(time.time() * 1000),
            'discovery': 'globalping_redirect', 'source_measurement': mid}


def russian_check(origin):
    assert valid_origin(origin)
    mid, result = measurement(urllib.parse.urlsplit(origin).hostname, '/search', 'q=music')
    reports = []
    for item in result.get('results', []):
        p, v = item.get('probe', {}), item.get('result', {})
        body = (v.get('rawBody') or '').lower()
        catalogue = '/static/hitmo/' in body or '__next' in body and 'hitmo' in body
        ok = (p.get('country') == 'RU' and v.get('statusCode') == 200
              and v.get('tls', {}).get('authorized') is True and catalogue
              and 'captcha' not in body and 'access denied' not in body)
        # Do not publish raw headers, cookies, probe IPs or complete response bodies.
        reports.append({'country': p.get('country'), 'network': p.get('network'),
                        'asn': p.get('asn'), 'http_status': v.get('statusCode'), 'ok': ok})
    return {'measurement': mid, 'probes': reports,
            'ok': len(reports) >= 2 and all(p['ok'] for p in reports)}


def resolve(url, previous=None, verify_ru=False):
    try:
        value = read(url) if url else probe_discovery()
    except Exception as e:
        print('Google discovery unavailable:', type(e).__name__, str(e)[:200])
        value = probe_discovery()
    discovered = value['origin']
    old = [] if not previous else [previous.get('origin')] + previous.get('alternatives', [])
    # Prefer the last working Russian mirror; still discover new alternatives each run.
    candidates = list(dict.fromkeys(o for o in old[:1] + [discovered] + value.get('alternatives', []) + old[1:] if valid_origin(o)))[:5]
    if verify_ru:
        checks = []
        for origin in candidates:
            try:
                report = russian_check(origin)
                checks.append({'origin': origin, **report})
                if report['ok']:
                    value['origin'] = origin
                    value['discovered_origin'] = discovered
                    value['discovered_at_ms'] = value['checked_at_ms']
                    value['checked_at_ms'] = int(time.time() * 1000)
                    value['discovery'] += '_with_ru_validation'
                    value['ru_checks'] = checks
                    break
            except Exception as e:
                print('Availability check failed:', origin, type(e).__name__)
        else:
            raise RuntimeError('No mirror passed both Russian network checks; public record unchanged')
    value['alternatives'] = [o for o in candidates if o != value['origin']]
    return value


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('url')
    parser.add_argument('out')
    parser.add_argument('--previous')
    parser.add_argument('--verify-ru', action='store_true')
    args = parser.parse_args()
    previous = json.loads(Path(args.previous).read_text(encoding='utf-8')) if args.previous else None
    value = resolve(args.url, previous, args.verify_ru)
    Path(args.out).write_text(json.dumps(value, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')
    print('Automatically discovered:', value['origin'])
