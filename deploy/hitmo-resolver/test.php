<?php
declare(strict_types=1);
require __DIR__ . '/resolver.php';
function expect(bool $value): void { if (!$value) throw new RuntimeException('Assertion failed'); }
foreach (['http://hitmos.pro/', 'https://hitmos.pro.evil.test/', 'https://127.0.0.1/', 'https://user@hitmos.pro/', 'https://hitmos.pro:444/'] as $bad) {
    try { mirror_origin($bad); throw new LogicException('Unsafe host accepted'); }
    catch (RuntimeException $e) {}
}
expect(mirror_origin('https://ru.hitmox.net/song/12') === 'https://ru.hitmox.net/');
$page = ['location' => '', 'refresh' => '', 'body' => ''];
expect(mirror_next(HITMO_BOOTSTRAP, $page + []) === null);
$page['location'] = 'https://ru.hitmox.net/';
expect(mirror_next(HITMO_BOOTSTRAP, $page) === 'https://ru.hitmox.net/');
$page['location'] = ''; $page['body'] = '<meta http-equiv="refresh" content="0; url=https://ru.hitmox.net/">';
expect(mirror_next(HITMO_BOOTSTRAP, $page) === 'https://ru.hitmox.net/');
$page['body'] = '<a href="https://advert.example/">ad</a><a href="https://ru.hitmox.net/">Перейти</a>';
expect(mirror_next(HITMO_BOOTSTRAP, $page) === 'https://ru.hitmox.net/');
echo "Hitmo redirect and host checks passed\n";
