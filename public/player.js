'use strict';

/* --------------------------------------------------------------------------
 * Ontvanger: speelt de playlist van de zender, maar bewaart alles lokaal.
 * Valt de zender weg (of de hele computer start opnieuw zonder netwerk), dan
 * speelt deze pagina door met de laatst ontvangen inhoud.
 * ------------------------------------------------------------------------ */

const params = new URLSearchParams(location.search);
let key = params.get('k') || localStorage.getItem('sig_key') || '';
if (params.get('k')) localStorage.setItem('sig_key', params.get('k'));

const stage = document.getElementById('stage');
const idle = document.getElementById('idle');
const idleTitle = document.getElementById('idle-title');
const idleText = document.getElementById('idle-text');
const conn = document.getElementById('conn');

let payload = null;
let items = [];
let index = -1;
let timer = null;
let currentEl = null;
let wakeLock = null;
let emptyShown = false;

/* ------------------------------------------------------------ head-opbouw */

(function setupHead() {
  const link = document.createElement('link');
  link.rel = 'manifest';
  link.href = '/manifest.webmanifest' + (key ? '?k=' + encodeURIComponent(key) : '');
  document.head.appendChild(link);
  const apple = document.createElement('link');
  apple.rel = 'apple-touch-icon';
  apple.href = '/icon-180.png';
  document.head.appendChild(apple);
})();

/* ------------------------------------------- lokale mediabuffer (IndexedDB) */

const mediaStore = (() => {
  let opening = null;

  function open() {
    if (opening) return opening;
    opening = new Promise((resolve, reject) => {
      const request = indexedDB.open('signage-media', 1);
      request.onupgradeneeded = () => {
        const db = request.result;
        if (!db.objectStoreNames.contains('files')) db.createObjectStore('files');
      };
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
    return opening;
  }

  return {
    async get(url) {
      try {
        const db = await open();
        return await new Promise((resolve, reject) => {
          const request = db.transaction('files', 'readonly').objectStore('files').get(url);
          request.onsuccess = () => resolve(request.result || null);
          request.onerror = () => reject(request.error);
        });
      } catch {
        return null;
      }
    },
    async put(url, blob) {
      try {
        const db = await open();
        await new Promise((resolve, reject) => {
          const tx = db.transaction('files', 'readwrite');
          tx.objectStore('files').put(blob, url);
          tx.oncomplete = () => resolve();
          tx.onerror = () => reject(tx.error);
        });
      } catch {
        /* opslag geweigerd of vol: dan gewoon online spelen */
      }
    },
  };
})();

const localUrls = new Map();

/** Geeft een lokale blob-URL als het bestand al binnen is, anders de netwerk-URL. */
async function sourceFor(url) {
  if (!url) return url;
  if (localUrls.has(url)) return localUrls.get(url);
  const blob = await mediaStore.get(url);
  if (blob) {
    const objectUrl = URL.createObjectURL(blob);
    localUrls.set(url, objectUrl);
    return objectUrl;
  }
  return url;
}

const PREFETCH_LIMIT = 200 * 1024 * 1024;
let prefetching = false;

/** Haalt alle media van de huidige playlist binnen zodat een storing niets uitmaakt. */
async function prefetchAll() {
  if (prefetching) return;
  prefetching = true;
  try {
    const urls = new Set();
    for (const item of items) {
      if (item.type === 'image' || item.type === 'video' || item.type === 'file') urls.add(item.url);
      if (item.type === 'slide' && item.spec) {
        if (item.spec.imageUrl) urls.add(item.spec.imageUrl);
        for (const element of item.spec.elements || []) {
          if (element.imageUrl) urls.add(element.imageUrl);
        }
      }
    }
    for (const url of urls) {
      if (localUrls.has(url) || (await mediaStore.get(url))) continue;
      try {
        const head = await fetch(url, { method: 'HEAD' });
        const size = Number(head.headers.get('content-length') || 0);
        if (size > PREFETCH_LIMIT) continue;
        const response = await fetch(url);
        if (!response.ok) continue;
        await mediaStore.put(url, await response.blob());
      } catch {
        /* volgende ronde opnieuw proberen */
      }
    }
  } finally {
    prefetching = false;
  }
}

/* ---------------------------------------------------------------- planning */

const toMinutes = (value) => {
  const [h, m] = String(value).split(':').map(Number);
  return h * 60 + m;
};

function inWindow(item, now = Date.now()) {
  if (item.start && now < item.start) return false;
  if (item.end && now > item.end) return false;

  const days = Array.isArray(item.days) ? item.days : [];
  const timeStart = item.timeStart || null;
  const timeEnd = item.timeEnd || null;
  if (!days.length && !timeStart && !timeEnd) return true;

  const date = new Date(now);
  const minutes = date.getHours() * 60 + date.getMinutes();
  const dayAllowed = (day) => !days.length || days.includes(day);

  if (timeStart && timeEnd) {
    const from = toMinutes(timeStart);
    const until = toMinutes(timeEnd);
    if (from === until) return dayAllowed(date.getDay());
    if (from < until) return dayAllowed(date.getDay()) && minutes >= from && minutes < until;
    if (minutes >= from) return dayAllowed(date.getDay());
    if (minutes < until) return dayAllowed(new Date(now - 86400000).getDay());
    return false;
  }

  if (timeStart) return dayAllowed(date.getDay()) && minutes >= toMinutes(timeStart);
  if (timeEnd) return dayAllowed(date.getDay()) && minutes < toMinutes(timeEnd);
  return dayAllowed(date.getDay());
}

function activeItems() {
  const now = Date.now();
  return items.filter((item) => inWindow(item, now));
}

/* ------------------------------------------------------------------- stage */

function showIdle(title, html) {
  idleTitle.textContent = title;
  idleText.innerHTML = html;
  idle.classList.remove('hidden');
}

function hideIdle() {
  idle.classList.add('hidden');
}

function clearTimer() {
  if (timer) {
    clearTimeout(timer);
    timer = null;
  }
}

function clearStage() {
  clearTimer();
  if (currentEl) {
    const el = currentEl;
    el.classList.remove('show');
    setTimeout(() => el.remove(), 500);
    currentEl = null;
  }
}

function emptyState() {
  if (emptyShown) return;
  emptyShown = true;
  clearStage();
  showIdle(
    payload && payload.screen ? payload.screen.name : 'Signage',
    'Geen inhoud om te tonen.<br>Voeg media, een slide of een URL toe in het beheer.'
  );
}

function next() {
  const list = activeItems();
  if (!list.length) return;
  index = (index + 1) % list.length;
  show(list, index);
}

async function show(list, i) {
  const item = list[i];
  if (!item) return;

  const slide = document.createElement('div');
  slide.className = 'slide';

  let advanced = false;
  const advance = () => {
    if (advanced) return;
    advanced = true;
    clearTimer();
    const current = activeItems();
    if (!current.length) return;
    const position = current.findIndex((x) => x.id === item.id);
    if (position >= 0 && current.length > 1) {
      index = (position + 1) % current.length;
      show(current, index);
    } else {
      next();
    }
  };

  const schedule = (seconds) => {
    clearTimer();
    timer = setTimeout(advance, Math.max(1, seconds) * 1000);
  };

  if (item.type === 'slide') {
    slide.classList.add('fill');
    const holder = document.createElement('div');
    holder.className = 'slideHolder';
    slide.appendChild(holder);

    const spec = Object.assign({}, item.spec || {});
    try {
      if (spec.imageUrl) spec.imageUrl = await sourceFor(spec.imageUrl);
      if (Array.isArray(spec.elements)) {
        spec.elements = await Promise.all(
          spec.elements.map(async (element) =>
            element && element.imageUrl
              ? Object.assign({}, element, { imageUrl: await sourceFor(element.imageUrl) })
              : element
          )
        );
      }
    } catch {
      /* zonder afbeelding verder */
    }
    window.SlideRender.render(spec, holder);
    hideIdle();
    schedule(item.duration);
  } else if (item.type === 'image') {
    const img = new Image();
    img.decoding = 'async';
    img.alt = '';
    img.onload = () => {
      hideIdle();
      schedule(item.duration);
    };
    img.onerror = () => {
      slide.innerHTML = '<div class="missing">Afbeelding niet gevonden<br>' + escapeHtml(item.name || '') + '</div>';
      schedule(6);
    };
    img.src = await sourceFor(item.url);
    slide.appendChild(img);
  } else if (item.type === 'video') {
    const video = document.createElement('video');
    video.src = await sourceFor(item.url);
    video.muted = true;
    video.playsInline = true;
    video.autoplay = true;
    video.preload = 'auto';
    video.addEventListener('ended', advance);
    video.addEventListener('canplay', () => {
      hideIdle();
      video.play().catch(() => {});
    });
    video.addEventListener('error', () => {
      slide.innerHTML = '<div class="missing">Video niet gevonden<br>' + escapeHtml(item.name || '') + '</div>';
      schedule(6);
    });
    timer = setTimeout(advance, 30 * 60 * 1000);
    slide.appendChild(video);
  } else {
    const frame = document.createElement('iframe');
    frame.src = item.url;
    frame.loading = 'eager';
    frame.referrerPolicy = 'no-referrer';
    frame.setAttribute('allow', 'autoplay; fullscreen');
    frame.addEventListener('load', hideIdle);
    slide.appendChild(frame);
    schedule(item.duration);
  }

  stage.appendChild(slide);
  stage.dataset.t = ['push', 'wipe', 'zoom'].includes(item.transition) ? item.transition : 'fade';
  stage.classList.toggle('nofade', item.transition === 'none');
  requestAnimationFrame(() => slide.classList.add('show'));

  const previous = currentEl;
  if (previous) {
    previous.classList.remove('show');
    previous.classList.add('leaving');
    setTimeout(() => previous.remove(), 700);
  }
  currentEl = slide;

  beat(i, list.length);
}

function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, (ch) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[ch]);
}

/* ------------------------------------------------------------------ status */

function beat(item, count) {
  fetch('/api/player/heartbeat', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ k: key, item, count }),
    keepalive: true,
  }).catch(() => {});
}

function setOffline(flag) {
  if (!conn) return;
  conn.classList.toggle('off', flag);
  conn.title = flag ? 'geen verbinding met de zender — lokale kopie speelt door' : 'verbonden met de zender';
}

/* --------------------------------------------------------------- synchroniseren */

async function applyPayload(data, first) {
  const changed = !payload || payload.version !== data.version;
  payload = data;
  try {
    localStorage.setItem('sig_cache_' + key, JSON.stringify(data));
  } catch {
    /* opslag vol: geen ramp */
  }

  if (data.screen && data.screen.paused) {
    clearStage();
    showIdle(data.screen.name, 'Dit scherm is gepauzeerd.');
    return;
  }

  items = data.items || [];
  stage.classList.toggle('cover', !!(data.screen && data.screen.fit === 'cover'));

  const list = activeItems();
  if (!list.length) {
    emptyState();
    return;
  }

  emptyShown = false;
  if (first || changed || index < 0 || !currentEl) {
    index = -1;
    next();
  }
  prefetchAll();
}

async function sync(first) {
  if (!key) {
    showIdle(
      'Signage',
      'Deze pagina heeft een scherm-link nodig.<br>Open de link uit het beheer, bijvoorbeeld <code>/player?k=…</code>'
    );
    return;
  }
  try {
    const response = await fetch('/api/player?k=' + encodeURIComponent(key), { cache: 'no-store' });
    if (!response.ok) throw new Error('status ' + response.status);
    setOffline(false);
    await applyPayload(await response.json(), first);
  } catch {
    setOffline(true);
    if (first) {
      const cached = localStorage.getItem('sig_cache_' + key);
      if (cached) {
        try {
          await applyPayload(JSON.parse(cached), true);
          return;
        } catch {
          /* onbruikbare cache */
        }
      }
      showIdle(
        'Signage',
        'Geen verbinding met de zender.<br>Zodra er inhoud lokaal is opgeslagen, blijft die doorspelen.'
      );
    }
  }
}

/* ---------------------------------------------------------------- volledig scherm */

function goFullscreen() {
  const el = document.documentElement;
  if (document.fullscreenElement) return;
  const request = el.requestFullscreen || el.webkitRequestFullscreen;
  if (request) request.call(el).catch(() => {});
}

async function keepAwake() {
  try {
    if ('wakeLock' in navigator) {
      wakeLock = await navigator.wakeLock.request('screen');
      wakeLock.addEventListener('release', () => {
        wakeLock = null;
      });
    }
  } catch {
    /* niet ondersteund */
  }
}

document.addEventListener('visibilitychange', () => {
  if (document.visibilityState === 'visible') {
    if (!wakeLock) keepAwake();
    sync(false);
  }
});

['click', 'keydown', 'touchstart'].forEach((event) =>
  document.addEventListener(event, goFullscreen, { passive: true })
);

/* -------------------------------------------------------------------- start */

async function start() {
  keepAwake();

  // Eerst de lokale kopie: de ontvanger speelt meteen, ook zonder zender.
  const cached = key ? localStorage.getItem('sig_cache_' + key) : null;
  if (cached) {
    try {
      await applyPayload(JSON.parse(cached), true);
    } catch {
      /* onbruikbare cache */
    }
  }

  await sync(!payload);

  setInterval(() => {
    if (document.visibilityState === 'visible') sync(false);
  }, 20000);

  setInterval(() => {
    if (currentEl) beat(index, activeItems().length);
  }, 15000);
}

if ('serviceWorker' in navigator) {
  navigator.serviceWorker.register('/sw.js').catch(() => {});
}

start();
