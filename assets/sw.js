// Eigenpulse service worker — minimal app-shell + network-first for API/SSE.
const CACHE = 'ep-v0.1.0';
const SHELL = [
  '/static/styles.css',
  '/static/theme-init.js',
  '/static/favicon.svg',
  '/static/manifest.webmanifest',
];

self.addEventListener('install', (event) => {
  event.waitUntil(caches.open(CACHE).then((c) => c.addAll(SHELL)).then(() => self.skipWaiting()));
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

  // Never cache API / events / server-fns.
  if (url.pathname.startsWith('/api/') || url.pathname.startsWith('/events/')) {
    return; // pass-through
  }
  // Cache-first for static assets.
  if (url.pathname.startsWith('/static/') || url.pathname.startsWith('/pkg/')) {
    event.respondWith(
      caches.match(req).then((hit) => hit || fetch(req).then((res) => {
        const clone = res.clone();
        caches.open(CACHE).then((c) => c.put(req, clone));
        return res;
      }).catch(() => caches.match('/static/styles.css')))
    );
    return;
  }
  // App shell: stale-while-revalidate for HTML.
  if (req.headers.get('accept') && req.headers.get('accept').includes('text/html')) {
    event.respondWith(
      caches.match(req).then((hit) => {
        const fetchPromise = fetch(req).then((res) => {
          const clone = res.clone();
          caches.open(CACHE).then((c) => c.put(req, clone));
          return res;
        }).catch(() => hit);
        return hit || fetchPromise;
      })
    );
  }
});
