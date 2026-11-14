import unittest
import io
import json
from unittest.mock import patch
import read_hitmo_service as service


class Mirrors(unittest.TestCase):
    def test_two_independent_successes_survive_a_third_probe_timeout(self):
        rows = [{'probe': {'country': 'RU', 'asn': n, 'network': str(n)},
                 'result': {'statusCode': 200, 'rawBody': '/static/hitmo/', 'tls': {'authorized': True}}} for n in (1, 2)]
        rows.append({'probe': {'country': 'RU', 'asn': 3}, 'result': {'statusCode': None}})
        with patch.object(service, 'measurement', return_value=('test', {'results': rows})):
            report = service.russian_check('https://ru.hitmox.net/')
        self.assertTrue(report['ok'])

    def test_root_probe_omits_the_invalid_empty_query_parameter(self):
        requests = []
        def respond(req, **kwargs):
            if isinstance(req, service.urllib.request.Request):
                requests.append(json.loads(req.data))
                return io.BytesIO(b'{"id":"fixture"}')
            return io.BytesIO(b'{"status":"finished","results":[]}')
        with patch.object(service.urllib.request, 'urlopen', side_effect=respond), patch.object(service.time, 'sleep'):
            service.measurement('hitmos.pro', residential=False)
        self.assertNotIn('query', requests[0]['measurementOptions']['request'])

    def test_blocked_new_redirect_keeps_the_verified_russian_mirror(self):
        fresh = {'origin': 'https://ru.hitmox.org/', 'checked_at_ms': 10, 'discovery': 'google_redirect'}
        previous = {'origin': 'https://ru.hitmox.net/'}
        with patch.object(service, 'probe_discovery', return_value=fresh), patch.object(service, 'russian_check', return_value={'ok': True, 'probes': []}):
            v = service.resolve('unused', previous, True)
        self.assertEqual(v['origin'], previous['origin'])
        self.assertEqual(v['alternatives'], ['https://ru.hitmox.org/'])
        self.assertEqual(v['discovered_origin'], 'https://ru.hitmox.org/')

    def test_next_redirect_is_promoted_when_the_old_mirror_becomes_unavailable(self):
        fresh = {'origin': 'https://ru.hitmox.org/', 'checked_at_ms': 10, 'discovery': 'google_redirect'}
        with patch.object(service, 'probe_discovery', return_value=fresh), patch.object(service, 'russian_check', side_effect=[{'ok': False, 'probes': []}, {'ok': True, 'probes': []}]):
            v = service.resolve('unused', {'origin': 'https://ru.hitmox.net/'}, True)
        self.assertEqual(v['origin'], 'https://ru.hitmox.org/')
        self.assertEqual(v['alternatives'], ['https://ru.hitmox.net/'])

    def test_all_network_failures_prevent_publication(self):
        with patch.object(service, 'probe_discovery', return_value={'origin': 'https://ru.hitmox.net/', 'checked_at_ms': 10}), patch.object(service, 'russian_check', return_value={'ok': False, 'probes': []}):
            with self.assertRaises(RuntimeError):
                service.resolve('unused', verify_ru=True)

    def test_google_redirects_cannot_move_to_an_unrelated_host(self):
        for bad in ['http://script.google.com/', 'https://script.google.com.evil.test/', 'https://user@script.google.com/', 'https://localhost/']:
            with self.assertRaises(ValueError):
                service.google_url(bad)

    def test_google_is_not_required_for_normal_probe_discovery(self):
        fresh = {'origin': 'https://ru.hitmox.org/', 'checked_at_ms': 10, 'discovery': 'globalping_redirect'}
        with patch.object(service, 'read', side_effect=RuntimeError('temporary error')), patch.object(service, 'probe_discovery', return_value=fresh), patch.object(service, 'russian_check', return_value={'ok': True, 'probes': []}):
            v = service.resolve('unused', {'origin': 'https://ru.hitmox.net/'}, True)
        self.assertEqual(v['origin'], 'https://ru.hitmox.net/')
        self.assertEqual(v['discovery'], 'globalping_redirect_with_ru_validation')

    def test_probe_discovery_outage_uses_the_published_google_resolver(self):
        fresh = {'origin': 'https://ru.hitmox.org/', 'checked_at_ms': 10, 'discovery': 'google_redirect'}
        with patch.object(service, 'probe_discovery', side_effect=RuntimeError('temporary error')), patch.object(service, 'read', return_value=fresh), patch.object(service, 'russian_check', return_value={'ok': True, 'probes': []}):
            v = service.resolve('unused', {'origin': 'https://ru.hitmox.net/'}, True)
        self.assertEqual(v['origin'], 'https://ru.hitmox.net/')
        self.assertEqual(v['discovery'], 'google_redirect_with_ru_validation')


if __name__ == '__main__':
    unittest.main()
