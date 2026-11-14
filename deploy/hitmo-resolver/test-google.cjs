const { test } = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const source = fs.readFileSync(__dirname + '/Code.gs', 'utf8');

function runtime(pages) {
  const calls = [];
  const context = vm.createContext({
    console, Date,
    UrlFetchApp: { fetch(url, options) {
      calls.push(url);
      assert.equal(options.followRedirects, false);
      const p = pages[url];
      assert.ok(p, 'Unexpected network request: ' + url);
      return {getResponseCode: () => p.status, getAllHeaders: () => p.headers || {}, getContentText: () => p.body || ''};
    }},
  });
  vm.runInContext(source, context, {timeout: 1000});
  return {context, calls};
}

test('follows the real redirect shape and checks search before returning an origin', () => {
  const {context, calls} = runtime({
    'https://hitmos.pro/': {status: 302, headers: {Location: 'https://ru.hitmox.org/'}},
    'https://ru.hitmox.org/': {status: 200},
    'https://ru.hitmox.org/search?q=music': {status: 200, body: 'tracks__item track__title'},
  });
  const value = context.resolveHitmo_();
  assert.equal(value.origin, 'https://ru.hitmox.org/');
  assert.equal(value.discovery, 'google_redirect');
  assert.equal(calls.length, 3);
});

test('does not follow unrelated, local or credential-bearing redirects', () => {
  for (const target of ['https://127.0.0.1/', 'https://hitmos.pro.evil.test/', 'https://user@hitmos.pro/', 'http://hitmos.pro/']) {
    const {context, calls} = runtime({'https://hitmos.pro/': {status: 302, headers: {location: target}}});
    assert.throws(() => context.resolveHitmo_());
    assert.equal(calls.length, 1);
  }
});

test('does not publish an error page or a redirect loop as a working catalogue', () => {
  for (const p of [{status: 403}, {status: 200, body: 'Access denied'}, {status: 302, headers: {location: 'https://hitmos.pro/'}}]) {
    const {context} = runtime({'https://hitmos.pro/': p, 'https://hitmos.pro/search?q=music': {status: 200, body: 'Not music'}});
    assert.throws(() => context.resolveHitmo_());
  }
});
