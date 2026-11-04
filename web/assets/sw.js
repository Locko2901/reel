const SHELL = `reel-shell-${new URL(location.href).searchParams.get('v') || 'dev'}`;
let shared = [];
let handed = () => {};

self.addEventListener('install', () => self.skipWaiting());

self.addEventListener('activate', (e) => {
  e.waitUntil((async () => {
    for (const k of await caches.keys()) if (k !== SHELL) await caches.delete(k);
    await self.clients.claim();
  })());
});

const isShell = (url) => url.origin === location.origin && url.pathname.startsWith('/assets/')
  && /\.(css|js|woff2|svg|png|webmanifest)$/.test(url.pathname) && url.pathname !== '/assets/sw.js';

self.addEventListener('fetch', (e) => {
  const url = new URL(e.request.url);
  if (e.request.method === 'POST' && url.origin === location.origin && url.pathname === '/share-target') {
    e.respondWith((async () => {
      const form = await e.request.formData();
      shared = form.getAll('clip').filter((f) => f instanceof File && f.size > 0);
      return Response.redirect('/share-target', 303);
    })());
    e.waitUntil(new Promise((done) => { handed = done; setTimeout(done, 60000); }));
    return;
  }
  if (e.request.method !== 'GET' || !isShell(url)) return;
  e.respondWith((async () => {
    const cache = await caches.open(SHELL);
    try {
      const res = await fetch(e.request);
      if (res.ok) await cache.put(e.request, res.clone());
      return res;
    } catch (err) {
      const hit = await cache.match(e.request);
      if (hit) return hit;
      throw err;
    }
  })());
});

self.addEventListener('message', (e) => {
  if (e.data !== 'shared-files') return;
  e.source.postMessage({ files: shared });
  shared = [];
  handed();
});
