'use strict';
/*
 * Signage — minimale, gratis digitale-signage server.
 * Geen externe dependencies. Start met: node server.js
 * Beheer:  http://<server>:8100/
 * Player:  http://<server>:8100/player?k=<scherm-sleutel>
 */
const http = require('node:http');
const fs = require('node:fs');
const fsp = require('node:fs/promises');
const path = require('node:path');
const os = require('node:os');
const crypto = require('node:crypto');
const zlib = require('node:zlib');

const ROOT = __dirname;
const PUBLIC = path.join(ROOT, 'public');
const DATA = process.env.SIGNAGE_DATA ? path.resolve(process.env.SIGNAGE_DATA) : path.join(ROOT, 'data');
const MEDIA = path.join(DATA, 'media');
const DB_FILE = path.join(DATA, 'db.json');
const PORT = Number(process.env.PORT || 8100);
// fs.watch kan op Windows na verloop van tijd stilvallen. Naast de watch
// controleren we daarom ook regelmatig de mtime/size van db.json.
const DB_POLL_MS = Math.max(500, Number(process.env.SIGNAGE_DB_POLL_MS) || 2000);
const MAX_UPLOAD = 2 * 1024 * 1024 * 1024; // 2 GB
const APP_VERSION = '1.3.5'; // houd gelijk aan installer/Cargo.toml
const UPDATE_FEED =
  process.env.SIGNAGE_UPDATE_FEED || 'https://api.github.com/repos/DrIamlegion/signage/releases/latest';
const UPDATE_TTL = 6 * 60 * 60 * 1000;
const UPDATE_ERROR_TTL = 10 * 60 * 1000;
// Hoe vaak de zender de release-info ververst, zodat een nieuwe versie ook
// zonder herstart of bezoek aan het beheer bij de ontvangers terechtkomt.
const UPDATE_REFRESH = Math.max(
  60 * 1000,
  Number(process.env.SIGNAGE_UPDATE_REFRESH_MS) || 5 * 60 * 1000
);
let updateCache = { at: 0, data: null };

for (const dir of [DATA, MEDIA]) fs.mkdirSync(dir, { recursive: true });

/* ---------------------------------------------------------------- opslag */

const db = loadDb();

function loadDb() {
  let data = {};
  try {
    data = JSON.parse(fs.readFileSync(DB_FILE, 'utf8'));
  } catch {
    data = {};
  }
  const base = {
    salt: crypto.randomBytes(16).toString('hex'),
    password: '',
    screens: [],
    media: [],
    presentations: [],
    settings: { defaultDuration: 10, title: 'Signage' },
    ...data,
  };
  if (!Array.isArray(base.screens)) base.screens = [];
  if (!Array.isArray(base.media)) base.media = [];
  if (!Array.isArray(base.presentations)) base.presentations = [];
  base.settings = { defaultDuration: 10, title: 'Signage', ...(base.settings || {}) };

  // oude losse slides omzetten naar presentaties met één slide
  if (Array.isArray(base.slides) && base.slides.length) {
    const map = new Map();
    for (const old of base.slides) {
      const deck = {
        id: old.id,
        name: old.name || 'Presentatie',
        duration: 10,
        transition: 'fade',
        slides: [
          normalizeSlideSpec(
            {
              layout: 'title',
              title: old.title,
              subtitle: old.subtitle,
              footer: old.footer,
              bg: old.bg,
              fg: old.fg,
              accent: old.accent,
              imageId: old.imageId || null,
              shade: old.shade,
              align: old.align,
            },
            {}
          ),
        ],
        updatedAt: old.updatedAt || Date.now(),
      };
      base.presentations.push(deck);
      map.set(old.id, deck.id);
    }
    for (const screen of base.screens) {
      screen.items = (screen.items || [])
        .map((item) =>
          item.type === 'slide'
            ? { ...item, type: 'deck', presentationId: map.get(item.slideId) }
            : item
        )
        .filter((item) => item.type !== 'deck' || item.presentationId);
    }
    delete base.slides;
    try {
      fs.writeFileSync(DB_FILE, JSON.stringify(base, null, 2));
    } catch {
      /* de volgende save schrijft het bestand alsnog */
    }
  }

  return base;
}

/* ---- live herladen: de native editor schrijft rechtstreeks naar db.json ---- */

let dbWatchTimer = null;
let dbWatcher = null;
let dbPollTimer = null;
let dbFileStamp = null;

function dbFileSignature() {
  try {
    const stat = fs.statSync(DB_FILE);
    return `${stat.mtimeMs}:${stat.ctimeMs}:${stat.size}`;
  } catch {
    return '';
  }
}

function saveDb() {
  const tmp = DB_FILE + '.tmp';
  fs.writeFileSync(tmp, JSON.stringify(db, null, 2));
  fs.renameSync(tmp, DB_FILE);
  dbFileStamp = dbFileSignature();
}

function reloadDbFromDisk() {
  let next;
  try {
    next = JSON.parse(fs.readFileSync(DB_FILE, 'utf8'));
  } catch {
    return false; // half geschreven bestand; een volgende melding probeert opnieuw
  }
  if (!next || typeof next !== 'object' || Array.isArray(next)) return false;
  if (JSON.stringify(next) === JSON.stringify(db)) return true; // eigen saveDb

  if (!Array.isArray(next.screens)) next.screens = [];
  if (!Array.isArray(next.media)) next.media = [];
  if (!Array.isArray(next.presentations)) next.presentations = [];
  next.settings = { defaultDuration: 10, title: 'Signage', ...(next.settings || {}) };

  for (const key of Object.keys(db)) delete db[key];
  Object.assign(db, next);

  // spelers verversen op basis van updatedAt; een native wijziging raakt ook
  // presentaties die in een playlist zitten, dus alle schermen krijgen een tik.
  const now = Date.now();
  for (const screen of db.screens) screen.updatedAt = now;

  console.log('  Database live herladen van schijf.');
  return true;
}

function pollDbFile() {
  const stamp = dbFileSignature();
  if (!stamp || stamp === dbFileStamp) return;
  // Alleen de stempel bijwerken als het bestand volledig leesbaar was;
  // anders probeert de volgende ronde het opnieuw.
  if (reloadDbFromDisk()) dbFileStamp = stamp;
}

function watchDbFile() {
  dbFileStamp = dbFileSignature();
  try {
    dbWatcher = fs.watch(DATA, { persistent: false }, (_event, filename) => {
      if (!filename || String(filename).toLowerCase() !== 'db.json') return;
      const stamp = dbFileSignature();
      clearTimeout(dbWatchTimer);
      dbWatchTimer = setTimeout(() => {
        if (reloadDbFromDisk()) dbFileStamp = stamp;
      }, 120);
    });
    dbWatcher.on('error', (err) => {
      console.warn('  Live watch gestopt; pollingfallback neemt over:', err.message);
      try {
        dbWatcher.close();
      } catch {
        /* al gesloten */
      }
      dbWatcher = null;
    });
    console.log('  Live herladen van db.json is actief.');
  } catch (err) {
    console.warn('  Live watch niet beschikbaar; pollingfallback is actief:', err.message);
  }

  // fs.watch is de snelle route; deze timer vangt een stilgevallen of
  // ontbrekende watcher op. De interval is bewust klein genoeg voor signage,
  // maar leest db.json alleen als de mtime/size echt is veranderd.
  dbPollTimer = setInterval(pollDbFile, DB_POLL_MS);
  if (dbPollTimer.unref) dbPollTimer.unref();
}

watchDbFile();

function hashPassword(pw) {
  return crypto.scryptSync(String(pw), db.salt, 32).toString('hex');
}

if (!db.password && process.env.SIGNAGE_PASSWORD) {
  db.password = hashPassword(process.env.SIGNAGE_PASSWORD);
  saveDb();
  fs.writeFileSync(path.join(DATA, 'admin-password.txt'), process.env.SIGNAGE_PASSWORD + '\n');
  console.log('\n  Wachtwoord ingesteld via SIGNAGE_PASSWORD.\n');
}
if (!db.password) {
  console.log('\n  Nog geen beheerwachtwoord: open het beheer op deze computer om er een te kiezen.\n');
}

function sessionToken() {
  return crypto.createHash('sha256').update(db.password + db.salt).digest('hex');
}

function isLoopback(req) {
  const address = (req.socket && req.socket.remoteAddress) || '';
  return address === '127.0.0.1' || address === '::1' || address === '::ffff:127.0.0.1';
}

/* ---------------------------------------------------------------- helpers */

function json(res, status, body) {
  const payload = JSON.stringify(body);
  res.writeHead(status, {
    'Content-Type': 'application/json; charset=utf-8',
    'Cache-Control': 'no-store',
    'Content-Length': Buffer.byteLength(payload),
  });
  res.end(payload);
}

function text(res, status, body, type = 'text/plain; charset=utf-8') {
  res.writeHead(status, { 'Content-Type': type, 'Content-Length': Buffer.byteLength(body) });
  res.end(body);
}

async function readJson(req, limit = 1024 * 1024) {
  const chunks = [];
  let size = 0;
  for await (const chunk of req) {
    size += chunk.length;
    if (size > limit) throw new Error('te groot');
    chunks.push(chunk);
  }
  if (!chunks.length) return {};
  return JSON.parse(Buffer.concat(chunks).toString('utf8'));
}

function cookies(req) {
  const out = {};
  for (const part of (req.headers.cookie || '').split(';')) {
    const i = part.indexOf('=');
    if (i > 0) out[part.slice(0, i).trim()] = decodeURIComponent(part.slice(i + 1).trim());
  }
  return out;
}

function isAuthed(req) {
  return cookies(req).sig === sessionToken();
}

/* ------------------------------------------------------------- updates */

function versionParts(value) {
  const match = String(value || '')
    .trim()
    .replace(/^v/i, '')
    .match(/^(\d+)\.(\d+)\.(\d+)/);
  return match ? match.slice(1).map(Number) : null;
}

function isNewerVersion(latest, current) {
  const next = versionParts(latest);
  const have = versionParts(current);
  if (!next || !have) return false;
  for (let i = 0; i < 3; i += 1) {
    if (next[i] !== have[i]) return next[i] > have[i];
  }
  return false;
}

async function checkForUpdate(force = false) {
  const now = Date.now();
  const ttl = updateCache.data && updateCache.data.error ? UPDATE_ERROR_TTL : UPDATE_TTL;
  if (!force && updateCache.data && now - updateCache.at < ttl) return updateCache.data;

  const result = {
    current: APP_VERSION,
    latest: null,
    updateAvailable: false,
    url: 'https://github.com/DrIamlegion/signage/releases/latest',
    assetUrl: null,
    notes: '',
    publishedAt: null,
    error: null,
    checkedAt: new Date(now).toISOString(),
  };

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 4000);
  try {
    const response = await fetch(UPDATE_FEED, {
      signal: controller.signal,
      headers: { accept: 'application/vnd.github+json', 'user-agent': 'Signage/' + APP_VERSION },
    });
    if (!response.ok) throw new Error('HTTP ' + response.status);
    const release = await response.json();
    const tag = String(release.tag_name || release.tag || '');
    result.latest = tag.replace(/^v/i, '') || null;
    result.url = release.html_url || result.url;
    result.notes = typeof release.body === 'string' ? release.body.slice(0, 4000) : '';
    result.publishedAt = release.published_at || null;
    const asset = Array.isArray(release.assets)
      ? release.assets.find((item) => /\.exe$/i.test(item.name || ''))
      : null;
    result.assetUrl = asset ? asset.browser_download_url : null;
    result.updateAvailable = Boolean(result.latest && isNewerVersion(result.latest, APP_VERSION));
  } catch (err) {
    result.error = err && err.name === 'AbortError' ? 'timeout' : String((err && err.message) || err);
  } finally {
    clearTimeout(timer);
  }

  updateCache = { at: now, data: result };
  return result;
}

function lanUrls() {
  const urls = [];
  for (const addrs of Object.values(os.networkInterfaces())) {
    for (const a of addrs || []) {
      if (a.family === 'IPv4' && !a.internal) urls.push(`http://${a.address}:${PORT}`);
    }
  }
  return urls;
}

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.webmanifest': 'application/manifest+json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.gif': 'image/gif',
  '.webp': 'image/webp',
  '.avif': 'image/avif',
  '.bmp': 'image/bmp',
  '.mp4': 'video/mp4',
  '.webm': 'video/webm',
  '.mov': 'video/quicktime',
  '.m4v': 'video/x-m4v',
  '.mp3': 'audio/mpeg',
  '.wav': 'audio/wav',
  '.pdf': 'application/pdf',
  '.ico': 'image/x-icon',
  '.txt': 'text/plain; charset=utf-8',
};

const IMAGE_EXT = new Set(['.jpg', '.jpeg', '.png', '.gif', '.webp', '.avif', '.bmp', '.svg']);
const VIDEO_EXT = new Set(['.mp4', '.webm', '.mov', '.m4v']);

function mimeOf(file) {
  return MIME[path.extname(file).toLowerCase()] || 'application/octet-stream';
}

function validTime(value) {
  return typeof value === 'string' && /^([01]\d|2[0-3]):[0-5]\d$/.test(value) ? value : null;
}

function isSlideLayout(value) {
  return ['title', 'text', 'image-left', 'image-right', 'full', 'quote', 'free'].includes(value);
}

const SHAPES = ['rect', 'round', 'ellipse', 'triangle', 'line', 'arrow', 'star'];
const ELEMENT_ANIMS = ['none', 'fade', 'up', 'zoom'];
const FONTS = ['sans', 'serif', 'mono', 'display'];
const ASPECTS = ['16:9', '4:3', '9:16', '1:1'];
const SLIDE_TRANSITIONS = ['none', 'fade', 'push', 'wipe', 'zoom'];

function normalizeElements(list) {
  const clamp = (value, min, max) => Math.min(max, Math.max(min, Number(value) || 0));
  const color = (value, fallback) =>
    typeof value === 'string' && /^#[0-9a-f]{6}$/i.test(value) ? value : fallback;
  return (Array.isArray(list) ? list.slice(0, 60) : []).map((raw) => {
    const e = raw || {};
    const type = e.type === 'image' ? 'image' : e.type === 'shape' ? 'shape' : 'text';
    const w = clamp(e.w || 30, 2, 100);
    const h = clamp(e.h || 10, 2, 100);
    const paint = (value) =>
      value && value !== 'transparent' ? color(value, 'transparent') : 'transparent';
    const spacing = Number(e.letterSpacing);
    return {
      id: String(e.id || crypto.randomUUID().slice(0, 6)),
      type,
      x: clamp(e.x, 0, 100 - w),
      y: clamp(e.y, 0, 100 - h),
      w,
      h,
      shape: SHAPES.includes(e.shape) ? e.shape : 'rect',
      text: typeof e.text === 'string' ? e.text.slice(0, 400) : '',
      size: clamp(e.size || 6, 1, 40),
      weight: e.weight === 'bold' ? 'bold' : 'normal',
      italic: !!e.italic,
      underline: !!e.underline,
      bullet: !!e.bullet,
      align: ['left', 'center', 'right'].includes(e.align) ? e.align : 'left',
      valign: ['top', 'center', 'bottom'].includes(e.valign) ? e.valign : 'center',
      lineHeight: Math.min(2.4, Math.max(0.9, Number(e.lineHeight) || 1.28)),
      letterSpacing: Number.isFinite(spacing) ? Math.min(0.2, Math.max(-0.05, spacing)) : 0,
      color: color(e.color, '#f8fafc'),
      fill: paint(e.fill),
      stroke: paint(e.stroke),
      strokeWidth: clamp(e.strokeWidth, 0, 20),
      opacity: e.opacity === undefined ? 1 : clamp(e.opacity, 0.05, 1),
      background:
        e.background && e.background !== 'transparent' ? color(e.background, 'transparent') : 'transparent',
      radius: clamp(e.radius, 0, 24),
      imageId: e.imageId ? String(e.imageId) : null,
      fit: e.fit === 'contain' ? 'contain' : 'cover',
      anim: ELEMENT_ANIMS.includes(e.anim) ? e.anim : 'none',
      delay: clamp(e.delay, 0, 20),
      font: FONTS.includes(e.font) ? e.font : 'sans',
      rotation: Math.min(180, Math.max(-180, Number(e.rotation) || 0)),
      flipX: !!e.flipX,
      flipY: !!e.flipY,
      shadow: !!e.shadow,
    };
  });
}

function normalizeSlideSpec(source, base) {
  const s = source || {};
  const b = base || {};
  const color = (value, fallback) =>
    typeof value === 'string' && /^#[0-9a-f]{6}$/i.test(value) ? value : fallback;
  const str = (value, fallback, max) => (typeof value === 'string' ? value.slice(0, max) : fallback || '');
  const deg = (value, fallback) => {
    const number = Number(value);
    return Number.isFinite(number) ? Math.min(360, Math.max(0, number)) : fallback;
  };
  return {
    id: b.id || String(s.id || crypto.randomUUID().slice(0, 8)),
    layout: isSlideLayout(s.layout) ? s.layout : b.layout || 'title',
    title: str(s.title, b.title, 140),
    subtitle: str(s.subtitle, b.subtitle, 240),
    body: str(s.body, b.body, 800),
    footer: str(s.footer, b.footer, 140),
    bg: color(s.bg, b.bg || '#0f172a'),
    bg2:
      s.bg2 === undefined ? b.bg2 || null : s.bg2 === null || s.bg2 === '' ? null : color(s.bg2, null),
    bgAngle: s.bgAngle === undefined ? deg(b.bgAngle, 135) : deg(s.bgAngle, 135),
    fg: color(s.fg, b.fg || '#f8fafc'),
    accent: color(s.accent, b.accent || '#5b8cff'),
    imageId: 'imageId' in s ? (s.imageId ? String(s.imageId) : null) : b.imageId || null,
    shade:
      s.shade === undefined ? b.shade ?? 0.45 : Math.min(1, Math.max(0, Number(s.shade) || 0)),
    align: s.align === 'left' || s.align === 'center' ? s.align : b.align || 'center',
    scale: s.scale === undefined ? b.scale || 1 : Math.min(1.5, Math.max(0.7, Number(s.scale) || 1)),
    duration:
      s.duration === undefined
        ? b.duration || 0
        : Math.min(600, Math.max(1, Math.round(Number(s.duration) || 8))),
    notes: str(s.notes, b.notes, 2000),
    font: FONTS.includes(s.font) ? s.font : b.font || 'sans',
    transition:
      s.transition === undefined
        ? b.transition || ''
        : SLIDE_TRANSITIONS.includes(s.transition)
          ? s.transition
          : '',
    hidden: s.hidden === undefined ? !!b.hidden : !!s.hidden,
    elements: normalizeElements(s.elements === undefined ? b.elements : s.elements),
  };
}

function normalizeDeck(source, base) {
  const s = source || {};
  const b = base || {};
  const slides = Array.isArray(s.slides)
    ? s.slides.slice(0, 60).map((slide) => normalizeSlideSpec(slide, {}))
    : b.slides || [];
  return {
    id: b.id || crypto.randomUUID().slice(0, 8),
    name: (typeof s.name === 'string' ? s.name : b.name || 'Presentatie').slice(0, 80).trim() || 'Presentatie',
    aspect: s.aspect === undefined ? b.aspect || '16:9' : ASPECTS.includes(s.aspect) ? s.aspect : '16:9',
    duration: Math.min(600, Math.max(1, Math.round(Number(s.duration ?? b.duration) || 8))),
    transition: ['none', 'fade', 'push', 'wipe', 'zoom'].includes(s.transition)
      ? s.transition
      : b.transition || 'fade',
    slides,
    updatedAt: Date.now(),
  };
}

/* ------------------------------------------------------------ iconen (PNG) */

const CRC_TABLE = (() => {
  const table = new Int32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c;
  }
  return table;
})();

function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function pngChunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}

function iconPng(size) {
  const raw = Buffer.alloc(size * (size * 4 + 1));
  const cx = size / 2;
  const cy = size / 2;
  const r = size * 0.3;
  const tri = [
    [cx - r * 0.55, cy - r],
    [cx - r * 0.55, cy + r],
    [cx + r * 0.95, cy],
  ];
  const inside = (x, y) => {
    const sign = (p1, p2, p3) =>
      (p1[0] - p3[0]) * (p2[1] - p3[1]) - (p2[0] - p3[0]) * (p1[1] - p3[1]);
    const d1 = sign([x, y], tri[0], tri[1]);
    const d2 = sign([x, y], tri[1], tri[2]);
    const d3 = sign([x, y], tri[2], tri[0]);
    const neg = d1 < 0 || d2 < 0 || d3 < 0;
    const pos = d1 > 0 || d2 > 0 || d3 > 0;
    return !(neg && pos);
  };
  let p = 0;
  for (let y = 0; y < size; y++) {
    raw[p++] = 0;
    for (let x = 0; x < size; x++) {
      const isTri = inside(x + 0.5, y + 0.5);
      raw[p++] = isTri ? 0xf8 : 0x0f;
      raw[p++] = isTri ? 0xfa : 0x17;
      raw[p++] = isTri ? 0xfc : 0x2a;
      raw[p++] = 255;
    }
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    pngChunk('IHDR', ihdr),
    pngChunk('IDAT', zlib.deflateSync(raw, { level: 9 })),
    pngChunk('IEND', Buffer.alloc(0)),
  ]);
}

const ICONS = { 192: iconPng(192), 512: iconPng(512), 180: iconPng(180) };

/* ------------------------------------------------------------------ routes */

function screenByKey(key) {
  return db.screens.find((s) => s.key === key) || null;
}

function mediaUrl(id) {
  const media = id ? db.media.find((m) => m.id === id) : null;
  return media ? '/media/' + encodeURIComponent(media.file) : null;
}

function itemPublic(item) {
  if (item.type === 'media') {
    const media = db.media.find((m) => m.id === item.mediaId);
    if (!media) return null;
    return {
      id: item.id,
      type: media.kind === 'video' ? 'video' : media.kind === 'image' ? 'image' : 'file',
      url: '/media/' + encodeURIComponent(media.file),
      duration: item.duration,
      start: item.start || null,
      end: item.end || null,
      days: item.days || [],
      timeStart: item.timeStart || null,
      timeEnd: item.timeEnd || null,
      name: media.name,
    };
  }
  return {
    id: item.id,
    type: 'url',
    url: item.url,
    duration: item.duration,
    start: item.start || null,
    end: item.end || null,
    days: item.days || [],
    timeStart: item.timeStart || null,
    timeEnd: item.timeEnd || null,
    name: item.url,
  };
}

function playerPayload(screen) {
  const items = [];
  for (const item of screen.items || []) {
    if (item.type === 'deck') {
      const deck = db.presentations.find((d) => d.id === item.presentationId);
      if (!deck) continue;
      for (const slide of deck.slides || []) {
        if (slide.hidden) continue;
        items.push({
          id: `${item.id}:${slide.id}`,
          type: 'slide',
          spec: {
            ...slide,
            imageUrl: mediaUrl(slide.imageId),
            elements: (slide.elements || []).map((element) => ({
              ...element,
              imageUrl: mediaUrl(element.imageId),
            })),
          },
          duration: slide.duration || deck.duration || item.duration || 8,
          transition: slide.transition || deck.transition || 'fade',
          start: item.start || null,
          end: item.end || null,
          days: item.days || [],
          timeStart: item.timeStart || null,
          timeEnd: item.timeEnd || null,
          name: deck.name,
        });
      }
      continue;
    }
    const entry = itemPublic(item);
    if (entry) items.push(entry);
  }

  return {
    version: `${screen.updatedAt}-${items.length}`,
    screen: { id: screen.id, name: screen.name, paused: !!screen.paused, fit: screen.fit || 'contain' },
    items,
  };
}

function screenSummary(s) {
  const items = (s.items || []).map(itemPublic).filter(Boolean);
  return {
    id: s.id,
    name: s.name,
    key: s.key,
    paused: !!s.paused,
    fit: s.fit || 'contain',
    items: s.items || [],
    itemCount: items.length,
    updatedAt: s.updatedAt,
    lastSeen: s.lastSeen || 0,
    lastItem: s.lastItem ?? null,
    createdAt: s.createdAt,
  };
}

function touch(screen) {
  screen.updatedAt = Date.now();
}

function publicState() {
  return {
    settings: db.settings,
    media: db.media,
    presentations: db.presentations,
    screens: db.screens.map(screenSummary),
  };
}

async function assetVersion() {
  let version = 0;
  for (const name of ['admin.js', 'admin.css', 'player.js', 'player.css', 'slide-render.js', 'slide-render.css']) {
    try {
      version = Math.max(version, Math.round((await fsp.stat(path.join(PUBLIC, name))).mtimeMs));
    } catch {
      /* bestand bestaat nog niet */
    }
  }
  return String(version || 1);
}

async function serveStatic(req, res, urlPath) {
  const rel = urlPath === '/' ? 'admin.html' : urlPath.replace(/^\/+/, '');
  const file = path.join(PUBLIC, path.normalize(rel));
  if (!file.startsWith(PUBLIC)) return text(res, 403, 'Verboden');
  try {
    if (/\.html$/i.test(rel)) {
      // versienummer in de <script>/<link>-tags voorkomt een oude admin.js uit de browsercache
      const html = (await fsp.readFile(file, 'utf8')).replace(/\{\{v\}\}/g, await assetVersion());
      res.writeHead(200, {
        'Content-Type': 'text/html; charset=utf-8',
        'Content-Length': Buffer.byteLength(html),
        'Cache-Control': 'no-store',
      });
      return res.end(html);
    }
    const stat = await fsp.stat(file);
    if (!stat.isFile()) throw new Error('geen bestand');
    res.writeHead(200, {
      'Content-Type': mimeOf(file),
      'Content-Length': stat.size,
      'Cache-Control': /\.(js|css)$/i.test(rel) ? 'no-cache' : 'public, max-age=3600',
    });
    fs.createReadStream(file).pipe(res);
  } catch {
    text(res, 404, 'Niet gevonden');
  }
}

async function serveMedia(req, res, name) {
  const safe = path.basename(decodeURIComponent(name));
  const file = path.join(MEDIA, safe);
  let stat;
  try {
    stat = await fsp.stat(file);
  } catch {
    return text(res, 404, 'Niet gevonden');
  }
  const headers = {
    'Content-Type': mimeOf(file),
    'Cache-Control': 'public, max-age=31536000, immutable',
    'Accept-Ranges': 'bytes',
  };
  const range = req.headers.range;
  if (range) {
    const m = /bytes=(\d*)-(\d*)/.exec(range);
    if (m) {
      const start = m[1] ? Number(m[1]) : 0;
      const end = m[2] ? Number(m[2]) : stat.size - 1;
      if (start >= stat.size || end >= stat.size || start > end) {
        res.writeHead(416, { 'Content-Range': `bytes */${stat.size}` });
        return res.end();
      }
      res.writeHead(206, {
        ...headers,
        'Content-Range': `bytes ${start}-${end}/${stat.size}`,
        'Content-Length': end - start + 1,
      });
      return fs.createReadStream(file, { start, end }).pipe(res);
    }
  }
  res.writeHead(200, { ...headers, 'Content-Length': stat.size });
  fs.createReadStream(file).pipe(res);
}

async function handle(req, res) {
  const url = new URL(req.url, 'http://localhost');
  const p = url.pathname;
  const method = req.method || 'GET';

  /* ---- publiek ---- */
  if (p === '/icon.svg' || p === '/favicon.ico') return serveStatic(req, res, '/icon.svg');
  if (p === '/icon-180.png') return sendPng(res, ICONS[180]);
  if (p === '/icon-192.png') return sendPng(res, ICONS[192]);
  if (p === '/icon-512.png') return sendPng(res, ICONS[512]);

  if (p === '/manifest.webmanifest') {
    const key = url.searchParams.get('k') || '';
    const startUrl = key ? `/player?k=${encodeURIComponent(key)}` : '/player';
    return json(res, 200, {
      name: 'Signage',
      short_name: 'Signage',
      start_url: startUrl,
      scope: '/',
      display: 'fullscreen',
      background_color: '#0b1020',
      theme_color: '#0b1020',
      icons: [
        { src: '/icon-192.png', sizes: '192x192', type: 'image/png', purpose: 'any maskable' },
        { src: '/icon-512.png', sizes: '512x512', type: 'image/png', purpose: 'any maskable' },
      ],
    });
  }

  if (p.startsWith('/media/')) return serveMedia(req, res, p.slice('/media/'.length));

  /* ---- player (sleutel-gebaseerd, geen login) ---- */
  if (p === '/api/player' && method === 'GET') {
    const screen = screenByKey(url.searchParams.get('k'));
    if (!screen) return json(res, 404, { error: 'Scherm niet gevonden' });
    return json(res, 200, playerPayload(screen));
  }

  if (p === '/api/player/heartbeat' && method === 'POST') {
    let body = {};
    try {
      body = await readJson(req, 64 * 1024);
    } catch {
      /* negeren */
    }
    const screen = screenByKey(body.k || url.searchParams.get('k'));
    if (!screen) return json(res, 404, { error: 'Scherm niet gevonden' });
    screen.lastSeen = Date.now();
    if (typeof body.item === 'number') screen.lastItem = body.item;
    if (typeof body.count === 'number') screen.itemCount = body.count;
    saveDb();
    return json(res, 200, { ok: true, version: touchVersion(screen) });
  }

  if ((p === '/player' || p === '/player/' || p === '/play') && method === 'GET') {
    return serveStatic(req, res, '/player.html');
  }

  /* Alleen versienummers, zodat ook de ontvanger (zonder login) een korte
     melding kan tonen; de volledige release-info blijft achter de login. */
  if (p === '/api/update/summary' && method === 'GET') {
    const info = await checkForUpdate();
    res.setHeader('Cache-Control', 'no-store');
    return json(res, 200, {
      current: info.current,
      latest: info.latest,
      available: Boolean(info.updateAvailable),
    });
  }

  if (p === '/sw.js' || p === '/player.js' || p === '/player.css' || p === '/admin.js' || p === '/admin.css') {
    return serveStatic(req, res, p);
  }

  /* ---- eerste keer: wachtwoord kiezen, alleen op deze computer ---- */
  if (p === '/api/setup' && method === 'GET') {
    return json(res, 200, { needed: !db.password, local: isLoopback(req) });
  }

  if (p === '/api/setup' && method === 'POST') {
    if (db.password) return json(res, 409, { error: 'Er is al een wachtwoord ingesteld' });
    if (!isLoopback(req)) {
      return json(res, 403, { error: 'Stel het wachtwoord in op de computer waar Signage draait' });
    }
    const body = await readJson(req).catch(() => ({}));
    const password = String(body.password || '');
    if (password.length < 6) {
      return json(res, 400, { error: 'Kies een wachtwoord van minstens 6 tekens' });
    }
    db.password = hashPassword(password);
    saveDb();
    fs.writeFileSync(path.join(DATA, 'admin-password.txt'), password + '\n');
    res.setHeader(
      'Set-Cookie',
      `sig=${sessionToken()}; Path=/; HttpOnly; SameSite=Lax; Max-Age=2592000`
    );
    return json(res, 200, { ok: true });
  }

  /* ---- login ---- */
  if (p === '/api/login' && method === 'POST') {
    let body;
    try {
      body = await readJson(req);
    } catch {
      return json(res, 400, { error: 'Ongeldige aanvraag' });
    }
    if (!db.password) {
      return json(res, 409, { error: 'Stel eerst een wachtwoord in voor het beheer' });
    }
    if (!body.password || hashPassword(body.password) !== db.password) {
      return json(res, 401, { error: 'Onjuist wachtwoord' });
    }
    res.setHeader(
      'Set-Cookie',
      `sig=${sessionToken()}; Path=/; HttpOnly; SameSite=Lax; Max-Age=2592000`
    );
    return json(res, 200, { ok: true });
  }

  if (p === '/api/logout' && method === 'POST') {
    res.setHeader('Set-Cookie', 'sig=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0');
    return json(res, 200, { ok: true });
  }

  /* ---- beheer (login vereist) ---- */
  if (p.startsWith('/api/') && !isAuthed(req)) {
    return json(res, 401, { error: 'Niet ingelogd' });
  }

  if (p === '/api/state' && method === 'GET') return json(res, 200, publicState());

  if (p === '/api/update' && method === 'GET') {
    return json(res, 200, await checkForUpdate(url.searchParams.get('force') === '1'));
  }

  if (p === '/api/password' && method === 'POST') {
    const body = await readJson(req);
    if (!body.password || String(body.password).length < 6) {
      return json(res, 400, { error: 'Wachtwoord moet minstens 6 tekens zijn' });
    }
    db.password = hashPassword(body.password);
    saveDb();
    fs.writeFileSync(path.join(DATA, 'admin-password.txt'), String(body.password) + '\n');
    res.setHeader(
      'Set-Cookie',
      `sig=${sessionToken()}; Path=/; HttpOnly; SameSite=Lax; Max-Age=2592000`
    );
    return json(res, 200, { ok: true });
  }

  if (p === '/api/settings' && method === 'PATCH') {
    const body = await readJson(req);
    if (typeof body.defaultDuration === 'number') {
      db.settings.defaultDuration = Math.min(86400, Math.max(1, Math.round(body.defaultDuration)));
    }
    if (typeof body.title === 'string') db.settings.title = body.title.slice(0, 80);
    saveDb();
    return json(res, 200, db.settings);
  }

  if (p === '/api/screens' && method === 'POST') {
    const body = await readJson(req).catch(() => ({}));
    const screen = {
      id: crypto.randomUUID(),
      name: (body.name || `Scherm ${db.screens.length + 1}`).slice(0, 80),
      key: crypto.randomBytes(12).toString('base64url'),
      items: [],
      fit: 'contain',
      paused: false,
      createdAt: Date.now(),
      updatedAt: Date.now(),
      lastSeen: 0,
    };
    db.screens.push(screen);
    saveDb();
    return json(res, 201, screenSummary(screen));
  }

  const screenMatch = /^\/api\/screens\/([\w-]+)$/.exec(p);
  if (screenMatch) {
    const screen = db.screens.find((s) => s.id === screenMatch[1]);
    if (!screen) return json(res, 404, { error: 'Scherm niet gevonden' });

    if (method === 'PATCH') {
      const body = await readJson(req);
      if (typeof body.name === 'string') screen.name = body.name.slice(0, 80);
      if (body.fit === 'contain' || body.fit === 'cover') screen.fit = body.fit;
      if (typeof body.paused === 'boolean') screen.paused = body.paused;
      if (Array.isArray(body.items)) {
        screen.items = body.items
          .filter((it) => it && ['url', 'media', 'deck'].includes(it.type))
          .map((it) => ({
            id: it.id || crypto.randomUUID().slice(0, 8),
            type: it.type,
            mediaId: it.type === 'media' ? String(it.mediaId || '') : undefined,
            presentationId: it.type === 'deck' ? String(it.presentationId || '') : undefined,
            url: it.type === 'url' ? String(it.url || '').slice(0, 2000) : undefined,
            duration: Math.min(86400, Math.max(1, Math.round(Number(it.duration) || 10))),
            start: it.start ? new Date(it.start).getTime() : null,
            end: it.end ? new Date(it.end).getTime() : null,
            days: Array.isArray(it.days)
              ? [...new Set(it.days.map(Number).filter((d) => Number.isInteger(d) && d >= 0 && d <= 6))]
              : [],
            timeStart: validTime(it.timeStart),
            timeEnd: validTime(it.timeEnd),
          }))
          .filter((it) =>
            it.type === 'media'
              ? !!it.mediaId
              : it.type === 'deck'
                ? !!it.presentationId
                : /^https?:\/\//i.test(it.url)
          );
      }
      touch(screen);
      saveDb();
      return json(res, 200, screenSummary(screen));
    }

    if (method === 'DELETE') {
      db.screens = db.screens.filter((s) => s.id !== screen.id);
      saveDb();
      return json(res, 200, { ok: true });
    }
  }

  const rotateMatch = /^\/api\/screens\/([\w-]+)\/key$/.exec(p);
  if (rotateMatch && method === 'POST') {
    const screen = db.screens.find((s) => s.id === rotateMatch[1]);
    if (!screen) return json(res, 404, { error: 'Scherm niet gevonden' });
    screen.key = crypto.randomBytes(12).toString('base64url');
    saveDb();
    return json(res, 200, screenSummary(screen));
  }

  /* ---- uploads ---- */
  if (p === '/api/media' && method === 'POST') {
    const name = (url.searchParams.get('name') || 'bestand').slice(0, 200);
    const ext = path.extname(name).toLowerCase();
    const length = Number(req.headers['content-length'] || 0);
    if (length > MAX_UPLOAD) return json(res, 413, { error: 'Bestand te groot (max 2 GB)' });
    const id = crypto.randomUUID().slice(0, 8);
    const file = id + (ext ? ext.replace(/[^.a-z0-9]/g, '') : '');
    const dest = path.join(MEDIA, file);
    let bytes = 0;
    let aborted = false;
    const ws = fs.createWriteStream(dest);
    req.on('data', (c) => {
      bytes += c.length;
      if (bytes > MAX_UPLOAD && !aborted) {
        aborted = true;
        req.destroy();
        ws.destroy();
        fs.rm(dest, { force: true }, () => {});
      }
    });
    await new Promise((resolve) => {
      req.pipe(ws);
      ws.on('finish', resolve);
      ws.on('error', resolve);
      req.on('error', resolve);
    });
    if (aborted || !bytes) {
      await fsp.rm(dest, { force: true });
      return json(res, 400, { error: 'Upload mislukt' });
    }
    const kind = IMAGE_EXT.has(ext) ? 'image' : VIDEO_EXT.has(ext) ? 'video' : 'file';
    const media = {
      id,
      name,
      file,
      ext,
      kind,
      mime: mimeOf(file),
      size: bytes,
      uploadedAt: Date.now(),
    };
    db.media.push(media);
    saveDb();
    return json(res, 201, media);
  }

  const mediaMatch = /^\/api\/media\/([\w-]+)$/.exec(p);
  if (mediaMatch && method === 'DELETE') {
    const media = db.media.find((m) => m.id === mediaMatch[1]);
    if (!media) return json(res, 404, { error: 'Media niet gevonden' });
    db.media = db.media.filter((m) => m.id !== media.id);
    for (const deck of db.presentations) {
      for (const slide of deck.slides || []) {
        if (slide.imageId === media.id) slide.imageId = null;
      }
    }
    for (const s of db.screens) {
      s.items = (s.items || []).filter((it) => it.mediaId !== media.id);
      touch(s);
    }
    saveDb();
    await fsp.rm(path.join(MEDIA, media.file), { force: true });
    return json(res, 200, { ok: true });
  }

  /* ---- presentaties (zender) ---- */
  if (p === '/api/presentations' && method === 'POST') {
    const body = await readJson(req).catch(() => ({}));
    const slides = Array.isArray(body.slides) && body.slides.length ? body.slides : [{}];
    const deck = normalizeDeck({ ...body, slides }, { id: crypto.randomUUID().slice(0, 8) });
    db.presentations.push(deck);
    saveDb();
    return json(res, 201, deck);
  }

  const deckMatch = /^\/api\/presentations\/([\w-]+)$/.exec(p);
  if (deckMatch) {
    const deck = db.presentations.find((d) => d.id === deckMatch[1]);
    if (!deck) return json(res, 404, { error: 'Presentatie niet gevonden' });

    if (method === 'PATCH') {
      const body = await readJson(req);
      Object.assign(deck, normalizeDeck(body, deck));
      if (!deck.slides.length) deck.slides = normalizeDeck({ slides: [{}] }, {}).slides;
      for (const s of db.screens) {
        if ((s.items || []).some((it) => it.presentationId === deck.id)) touch(s);
      }
      saveDb();
      return json(res, 200, deck);
    }

    if (method === 'DELETE') {
      db.presentations = db.presentations.filter((d) => d.id !== deck.id);
      for (const s of db.screens) {
        s.items = (s.items || []).filter((it) => it.presentationId !== deck.id);
        touch(s);
      }
      saveDb();
      return json(res, 200, { ok: true });
    }
  }

  /* ---- statisch ---- */
  if (!p.startsWith('/api/')) return serveStatic(req, res, p);

  text(res, 404, 'Niet gevonden');
}

function touchVersion(screen) {
  return `${screen.updatedAt}-${(screen.items || []).length}`;
}

function sendPng(res, buf) {
  res.writeHead(200, {
    'Content-Type': 'image/png',
    'Content-Length': buf.length,
    'Cache-Control': 'public, max-age=31536000, immutable',
  });
  res.end(buf);
}

const server = http.createServer((req, res) => {
  handle(req, res).catch((err) => {
    console.error(err);
    if (!res.headersSent) json(res, 500, { error: 'Serverfout' });
    else res.end();
  });
});

server.listen(PORT, '0.0.0.0', () => {
  const urls = lanUrls();
  console.log('\n  Signage draait op:');
  console.log(`    lokaal     http://localhost:${PORT}`);
  for (const u of urls) console.log(`    netwerk    ${u}`);
  console.log('\n  Beheer:  ' + (urls[0] || `http://localhost:${PORT}`) + '/');
  console.log('  Stop met Ctrl+C\n');

  checkForUpdate()
    .then((info) => {
      if (info.updateAvailable) {
        console.log(`  Nieuwe versie beschikbaar: ${info.latest} (nu ${APP_VERSION})`);
        console.log(`  ${info.url}\n`);
      }
    })
    .catch(() => {});

  const updateTimer = setInterval(() => {
    checkForUpdate(true).catch(() => {});
  }, UPDATE_REFRESH);
  if (updateTimer.unref) updateTimer.unref();
});
