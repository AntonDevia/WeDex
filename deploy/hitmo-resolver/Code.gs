// Only script.external_request is required. No Drive, mail or account tokens.
function hitmoUrl_(value, base) {
  value = String(value).trim().replace(/&amp;/g, '&');
  if (value.indexOf('//') === 0) value = 'https:' + value;
  if (value.indexOf('/') === 0 && base) value = hitmoOrigin_(base) + value.substring(1);
  var m = /^https:\/\/([a-z0-9.-]+)(?::443)?(\/[^\s]*)?$/i.exec(value);
  if (!m || !/^(?:[a-z0-9-]+\.)*hitmo[a-z0-9-]*\.[a-z]{2,15}$/.test(m[1].toLowerCase())) {
    throw new Error('Invalid Hitmo HTTPS address');
  }
  return 'https://' + m[1].toLowerCase() + (m[2] || '/').split('#')[0];
}
function hitmoOrigin_(url) {
  return hitmoUrl_(url).match(/^https:\/\/[^/]+\//)[0];
}
function hitmoFetch_(url) {
  url = hitmoUrl_(url);
  var r = UrlFetchApp.fetch(url, {followRedirects: false, muteHttpExceptions: true});
  var status = r.getResponseCode();
  if (status < 200 || status >= 400) throw new Error('Hitmo HTTP ' + status);
  var headers = {};
  var raw = r.getAllHeaders();
  Object.keys(raw).forEach(function(k) { headers[k.toLowerCase()] = String(raw[k]); });
  var body = r.getContentText();
  if (body.length > 2097152) throw new Error('Hitmo response too large');
  return {status: status, headers: headers, body: body};
}
function hitmoCatalogue_(body) {
  return body.indexOf('tracks__item') >= 0 && body.indexOf('track__title') >= 0
    || body.indexOf('self.__next_f.push(') >= 0 && body.indexOf('play') >= 0;
}
function hitmoNext_(url, page) {
  if (page.headers.location) return hitmoUrl_(page.headers.location, url);
  var refresh = /url\s*=\s*["']?([^"'\s>]+)/i.exec(page.headers.refresh || '');
  if (refresh) return hitmoUrl_(refresh[1], url);
  var metas = page.body.match(/<meta\b[^>]*>/gi) || [];
  for (var i = 0; i < metas.length; i++) {
    if (!/http-equiv\s*=\s*["']refresh["']/i.test(metas[i])) continue;
    var content = /content\s*=\s*(?:"([^"]*)"|'([^']*)')/i.exec(metas[i]);
    var target = content && /url\s*=\s*([^\s]+)/i.exec(content[1] || content[2]);
    if (target) return hitmoUrl_(target[1].replace(/^["']|["']$/g, ''), url);
  }
  if (hitmoCatalogue_(page.body)) return null;
  var links = page.body.match(/<a\b[^>]*\bhref\s*=\s*(?:"[^"]*"|'[^']*')[^>]*>/gi) || [];
  for (var j = 0; j < links.length; j++) {
    var href = /\bhref\s*=\s*(?:"([^"]*)"|'([^']*)')/i.exec(links[j]);
    try {
      var next = hitmoUrl_(href[1] || href[2], url);
      if (hitmoOrigin_(next) !== hitmoOrigin_(url)) return next;
    } catch (e) { /* Unrelated links are not fetched. */ }
  }
  return null;
}
function resolveHitmo_() {
  var source = 'https://hitmos.pro/';
  var url = source;
  var seen = {};
  for (var i = 0; i < 6; i++) {
    if (seen[url]) throw new Error('Hitmo redirect loop');
    seen[url] = true;
    var page = hitmoFetch_(url);
    var next = hitmoNext_(url, page);
    if (next) { url = next; continue; }
    if (page.status !== 200) throw new Error('Redirect has no target');
    var origin = hitmoOrigin_(url);
    var search = hitmoFetch_(origin + 'search?q=music');
    if (search.status !== 200 || search.headers.location || !hitmoCatalogue_(search.body)) {
      throw new Error('Hitmo search catalogue not confirmed');
    }
    return {service: 'wedex-hitmo-mirror', schema: 1, source: source, origin: origin,
      checked_at_ms: Date.now(), discovery: 'google_redirect', hops: Object.keys(seen).length};
  }
  throw new Error('Too many Hitmo redirects');
}
function probeHitmo() {
  console.log(JSON.stringify(resolveHitmo_()));
}
function doGet() {
  var cache = CacheService.getScriptCache();
  var json = cache.get('hitmo-v1');
  if (!json) {
    var lock = LockService.getScriptLock();
    if (!lock.tryLock(1000)) {
      json = JSON.stringify({service: 'wedex-hitmo-mirror', error: 'Discovery busy'});
    } else {
      try {
        json = cache.get('hitmo-v1');
        if (!json) {
          try { json = JSON.stringify(resolveHitmo_()); cache.put('hitmo-v1', json, 600); }
          catch (e) {
            json = JSON.stringify({service: 'wedex-hitmo-mirror', error: String(e.message || e)});
            cache.put('hitmo-v1', json, 60);
          }
        }
      } finally { lock.releaseLock(); }
    }
  }
  return ContentService.createTextOutput(json).setMimeType(ContentService.MimeType.JSON);
}
