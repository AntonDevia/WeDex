import unittest
from unittest.mock import patch
import read_hitmo_service as service


class Mirrors(unittest.TestCase):
    def test_blocked_new_redirect_keeps_the_verified_russian_mirror(self):
        fresh = {'origin': 'https://ru.hitmox.org/', 'checked_at_ms': 10, 'discovery': 'google_redirect'}
        previous = {'origin': 'https://ru.hitmox.net/'}
        with patch.object(service, 'read', return_value=fresh), patch.object(service, 'russian_check', return_value={'ok': True}):
            v = service.resolve('unused', previous, True)
        self.assertEqual(v['origin'], previous['origin'])
        self.assertEqual(v['alternatives'], ['https://ru.hitmox.org/'])
        self.assertEqual(v['discovered_origin'], 'https://ru.hitmox.org/')

    def test_next_redirect_is_promoted_when_the_old_mirror_becomes_unavailable(self):
        fresh = {'origin': 'https://ru.hitmox.org/', 'checked_at_ms': 10, 'discovery': 'google_redirect'}
        with patch.object(service, 'read', return_value=fresh), patch.object(service, 'russian_check', side_effect=[{'ok': False}, {'ok': True}]):
            v = service.resolve('unused', {'origin': 'https://ru.hitmox.net/'}, True)
        self.assertEqual(v['origin'], 'https://ru.hitmox.org/')
        self.assertEqual(v['alternatives'], ['https://ru.hitmox.net/'])

    def test_all_network_failures_prevent_publication(self):
        with patch.object(service, 'read', return_value={'origin': 'https://ru.hitmox.net/', 'checked_at_ms': 10}), patch.object(service, 'russian_check', return_value={'ok': False}):
            with self.assertRaises(RuntimeError):
                service.resolve('unused', verify_ru=True)

    def test_google_redirects_cannot_move_to_an_unrelated_host(self):
        for bad in ['http://script.google.com/', 'https://script.google.com.evil.test/', 'https://user@script.google.com/', 'https://localhost/']:
            with self.assertRaises(ValueError):
                service.google_url(bad)

    def test_google_outage_uses_independent_probe_discovery(self):
        fresh = {'origin': 'https://ru.hitmox.org/', 'checked_at_ms': 10, 'discovery': 'globalping_redirect'}
        with patch.object(service, 'read', side_effect=RuntimeError('temporary error')), patch.object(service, 'probe_discovery', return_value=fresh), patch.object(service, 'russian_check', return_value={'ok': True}):
            v = service.resolve('unused', {'origin': 'https://ru.hitmox.net/'}, True)
        self.assertEqual(v['origin'], 'https://ru.hitmox.net/')
        self.assertEqual(v['discovery'], 'globalping_redirect_with_ru_validation')


if __name__ == '__main__':
    unittest.main()
