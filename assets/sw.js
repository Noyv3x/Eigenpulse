// Eigenpulse service worker: cache immutable-ish static assets only.
// Authenticated HTML must always go to the network so logout/session changes
// cannot show a stale SSR snapshot.
const CACHE = 'ep-v0.1.4';
const PRECACHE_ASSETS = [
  '/static/styles.css',
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
