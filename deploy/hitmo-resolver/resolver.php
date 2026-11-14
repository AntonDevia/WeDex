<?php
declare(strict_types=1);

// Fixed-source discovery. This is not an arbitrary URL proxy.
const HITMO_BOOTSTRAP = 'https://hitmos.pro/';
const HITMO_LIMIT = 2097152;

function mirror_origin(string $value): string {
    $u = parse_url(trim(html_entity_decode($value, ENT_QUOTES | ENT_HTML5, 'UTF-8')));
    if (!$u || ($u['scheme'] ?? '') !== 'https' || isset($u['user']) || isset($u['pass'])
        || isset($u['port']) && $u['port'] !== 443
        || !preg_match('/^(?:[a-z0-9-]+\.)*hitmo[a-z0-9-]*\.[a-z]{2,15}$/D', $u['host'] ?? '')) {
        throw new RuntimeException('Invalid Hitmo HTTPS host');
    }
    return 'https://' . $u['host'] . '/';
}

function mirror_request(string $url): array {
    $origin = mirror_origin($url);
    $host = parse_url($origin, PHP_URL_HOST);
    $ips = gethostbynamel($host) ?: [];
    if (!$ips) throw new RuntimeException('DNS unavailable');
    foreach ($ips as $ip) {
        if (!filter_var($ip, FILTER_VALIDATE_IP, FILTER_FLAG_IPV4 | FILTER_FLAG_GLOBAL_RANGE)) {
            throw new RuntimeException('Non-public destination');
        }
    }
    $body = ''; $location = ''; $refresh = '';
    $c = curl_init($url);
    curl_setopt_array($c, [
        CURLOPT_FOLLOWLOCATION => false, CURLOPT_CONNECTTIMEOUT => 5, CURLOPT_TIMEOUT => 15,
        CURLOPT_PROTOCOLS => CURLPROTO_HTTPS, CURLOPT_USERAGENT => 'Mozilla/5.0 (compatible; WeDexMirror/1.0)',
        CURLOPT_RESOLVE => [$host . ':443:' . $ips[0]],
        CURLOPT_HEADERFUNCTION => function ($c, $line) use (&$location, &$refresh) {
            if (stripos($line, 'Location:') === 0) $location = trim(substr($line, 9));
            if (stripos($line, 'Refresh:') === 0) $refresh = trim(substr($line, 8));
            return strlen($line);
        },
        CURLOPT_WRITEFUNCTION => function ($c, $part) use (&$body) {
            if (strlen($body) + strlen($part) > HITMO_LIMIT) return 0;
            $body .= $part; return strlen($part);
        },
    ]);
    $ok = curl_exec($c); $status = curl_getinfo($c, CURLINFO_HTTP_CODE);
    $error = curl_error($c); curl_close($c);
    if ($ok === false) throw new RuntimeException('Hitmo transport: ' . $error);
    if ($status < 200 || $status >= 400) throw new RuntimeException('Hitmo HTTP ' . $status);
    return compact('status', 'location', 'refresh', 'body');
}

function mirror_target(string $base, string $value): string {
    $value = html_entity_decode(trim($value), ENT_QUOTES | ENT_HTML5, 'UTF-8');
    if (str_starts_with($value, '//')) $value = 'https:' . $value;
    elseif (str_starts_with($value, '/')) $value = rtrim(mirror_origin($base), '/') . $value;
    mirror_origin($value); // Check every redirect, including the first hop.
    return $value;
}

function mirror_next(string $url, array $page): ?string {
    if ($page['location'] !== '') return mirror_target($url, $page['location']);
    if (preg_match('/url\s*=\s*["\']?([^"\'\s>]+)/i', $page['refresh'], $m)) return mirror_target($url, $m[1]);
    $doc = new DOMDocument();
    @$doc->loadHTML('<?xml encoding="UTF-8">' . $page['body'], LIBXML_NONET);
    foreach ($doc->getElementsByTagName('meta') as $meta) {
        if (strtolower($meta->getAttribute('http-equiv')) === 'refresh'
            && preg_match('/url\s*=\s*["\']?([^"\'\s>]+)/i', $meta->getAttribute('content'), $m)) {
            return mirror_target($url, $m[1]);
        }
    }
    // Some foreign landing pages expose a regular link to the Russian mirror.
    foreach ($doc->getElementsByTagName('a') as $a) {
        try {
            $target = mirror_target($url, $a->getAttribute('href'));
            if (mirror_origin($target) !== mirror_origin($url)) return $target;
        } catch (RuntimeException $e) { /* Ignore unrelated links. */ }
    }
    return null;
}

function mirror_verify(string $origin): void {
    $page = mirror_request(mirror_origin($origin) . 'search?q=music');
    if ($page['status'] !== 200 || $page['location'] !== ''
        || !(str_contains($page['body'], 'tracks__item')
            || str_contains($page['body'], 'self.__next_f.push(') && str_contains($page['body'], 'play'))) {
        throw new RuntimeException('Search catalogue not confirmed');
    }
}

function mirror_resolve(?array $previous = null): array {
    $origin = null; $discovery = 'redirect'; $warning = null;
    try {
        $url = HITMO_BOOTSTRAP; $seen = [];
        for ($i = 0; $i < 6; $i++) {
            if (isset($seen[$url])) throw new RuntimeException('Redirect loop');
            $seen[$url] = true;
            $page = mirror_request($url);
            $next = mirror_next($url, $page);
            if ($next === null) {
                $origin = mirror_origin($url); break;
            }
            $url = $next;
        }
        if ($origin === null) throw new RuntimeException('Too many redirects');
        mirror_verify($origin);
    } catch (RuntimeException $e) {
        $warning = $e->getMessage();
        if (!$previous || ($previous['service'] ?? '') !== 'wedex-hitmo-mirror') throw $e;
        $origin = mirror_origin($previous['origin'] ?? '');
        try { mirror_verify($origin); } // Never refresh a cached address without checking it.
        catch (RuntimeException $cached) {
            throw new RuntimeException('Discovery: ' . $warning . '; cached mirror: ' . $cached->getMessage());
        }
        $discovery = 'last_verified';
    }
    return [
        'service' => 'wedex-hitmo-mirror', 'schema' => 1, 'source' => HITMO_BOOTSTRAP,
        'origin' => $origin, 'checked_at_ms' => (int) floor(microtime(true) * 1000),
        'discovery' => $discovery, 'discovery_warning' => $warning,
    ];
}

function mirror_main(): int {
    $previous = null;
    if (PHP_SAPI === 'cli' && isset($_SERVER['argv'][1])) {
        $previous = json_decode(file_get_contents($_SERVER['argv'][1]), true, 16, JSON_THROW_ON_ERROR);
    }
    try { $result = mirror_resolve($previous); $code = 0; }
    catch (Throwable $e) {
        $result = ['service' => 'wedex-hitmo-mirror', 'schema' => 1, 'source' => HITMO_BOOTSTRAP,
            'error' => $e->getMessage(), 'checked_at_ms' => (int) floor(microtime(true) * 1000)];
        $code = 1;
    }
    if (PHP_SAPI !== 'cli') {
        header('Content-Type: application/json; charset=utf-8');
        header('Cache-Control: no-store'); header('X-Content-Type-Options: nosniff');
        if ($code) http_response_code(503);
    }
    echo json_encode($result, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_THROW_ON_ERROR) . "\n";
    return $code;
}

if (realpath($_SERVER['SCRIPT_FILENAME'] ?? '') === __FILE__) exit(mirror_main());
