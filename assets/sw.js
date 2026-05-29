// Eigenpulse service worker: cache immutable-ish static assets only.
// Authenticated HTML must always go to the network so logout/session changes
// cannot show a stale SSR snapshot.
//
// The cache version is templated at request time from CARGO_PKG_VERSION by the
// `/sw.js` handler (it replaces the __EP_SW_VERSION__ token below), so the SW
// cache key always tracks the running binary's crate version. If this file is
// ever served raw (e.g. via the generic static handler), the literal token is a
// valid—if version-less—cache name, so caching still works.
const CACHE = 'ep-__EP_SW_VERSION__';
const PRECACHE_ASSETS = [
  '/static/favicon.svg',
  '/static/manifest.webmanifest',
];

self.addEventListener('install', (event) => {
  event.waitUntil(caches.open(CACHE).then((c) => c.addAll(PRECACHE_ASSETS)).then(() => self.skipWaiting()));
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches.keys().then((keys) =>
      Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k)))
    ).then(() => self.clients.claim())
  );
});

self.addEventListener('fetch', (event) => {
  const req = event.request;
  if (req.method !== 'GET') return;
  const url = new URL(req.url);
  if (url.origin !== self.location.origin) return;

  // Never cache API / events / server-fns.
  if (url.pathname.startsWith('/api/') || url.pathname.startsWith('/events/')) {
    return; // pass-through
  }
  // theme-init.js must track the current server bundle. It runs before
  // hydration to prevent FOUC, so stale cached code is worse than a network hit.
  if (url.pathname === '/static/theme-init.js') {
    return;
  }
  // CSS is embedded in the server binary and is part of the active UI bundle.
  // Use network-first so a deployment never renders one full visit with stale
  // layout rules from the previous service-worker cache.
  if (url.pathname === '/static/styles.css') {
    event.respondWith(
      fetch(req).then((res) => {
        if (res.ok) {
          const clone = res.clone();
          caches.open(CACHE).then((c) => c.put(req, clone));
        }
        return res;
      }).catch(() => caches.match(req))
    );
    return;
  }
  // Hydration assets use stable cargo-leptos filenames, so cache-first would
  // pin old JS/WASM across deployments until this service worker version
  // changes. Network-first keeps upgrades fresh while still allowing an
  // offline fallback to the last successful bundle.
  if (url.pathname.startsWith('/pkg/')) {
    event.respondWith(
      fetch(req).then((res) => {
        if (res.ok) {
          const clone = res.clone();
          caches.open(CACHE).then((c) => c.put(req, clone));
        }
        return res;
      }).catch(() => caches.match(req))
    );
    return;
  }
  // Stale-while-revalidate for static assets. This keeps home-screen launches
  // fast/offline-capable, but a normal online visit refreshes CSS/manifest even
  // if a release forgot to bump CACHE. Do not substitute unrelated fallbacks; a
  // failed asset request should fail visibly instead of receiving CSS bytes
  // with the wrong MIME type.
  if (url.pathname.startsWith('/static/')) {
    event.respondWith(
      caches.match(req).then((hit) => {
        const refresh = fetch(req).then((res) => {
          if (res.ok) {
            const clone = res.clone();
            caches.open(CACHE).then((c) => c.put(req, clone));
          }
          return res;
        });
        if (hit) {
          event.waitUntil(refresh.catch(() => undefined));
          return hit;
        }
        return refresh;
      })
    );
    return;
  }
  // Let navigation/HTML requests hit the network and server-side auth.
});
