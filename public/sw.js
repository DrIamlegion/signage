/* Service worker: spelers blijven werken als de server even wegvalt. */
const CACHE = 'signage-v2';
const SHELL = [
  '/player',
  '/player.css',
  '/player.js',
  '/slide-render.css',
  '/slide-render.js',
  '/icon.svg',
];

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      .then((cache) => cache.addAll(SHELL))
      .then(() => self.skipWaiting())
  );
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim())
  );
});

self.addEventListener('fetch', (event) => {
  const request = event.request;
  if (request.method !== 'GET') return;

  const url = new URL(request.url);
  if (url.origin !== location.origin) return;

  // Media: eerst uit de cache (afbeeldingen en PDF's; video's blijven online).
  if (url.pathname.startsWith('/media/')) {
    event.respondWith(
      (async () => {
        const cache = await caches.open(CACHE);
        const hit = await cache.match(request, { ignoreVary: true });
        if (hit) return hit;
        try {
          const response = await fetch(request);
          const type = response.headers.get('content-type') || '';
          if (response.status === 200 && (type.startsWith('image/') || type.startsWith('application/pdf'))) {
            cache.put(request, response.clone()).catch(() => {});
          }
          return response;
        } catch (err) {
          return hit || Response.error();
        }
      })()
    );
    return;
  }

  // Player-pagina en scripts: netwerk eerst, cache als terugval.
  if (
    url.pathname === '/player' ||
    url.pathname === '/player.js' ||
    url.pathname === '/player.css' ||
    url.pathname === '/slide-render.js' ||
    url.pathname === '/slide-render.css'
  ) {
    event.respondWith(
      (async () => {
        const cache = await caches.open(CACHE);
        try {
          const response = await fetch(request);
          if (response.ok) cache.put(request, response.clone()).catch(() => {});
          return response;
        } catch (err) {
          const hit = await cache.match(request, { ignoreVary: true });
          return hit || Response.error();
        }
      })()
    );
  }
});
