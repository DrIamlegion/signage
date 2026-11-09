'use strict';
/*
 * Genereert installer/assets/Signage.ico uit hetzelfde ontwerp als
 * public/icon.svg: donker vierkant met afgeronde hoeken en een play-driehoek.
 * Geen dependencies: PNG's worden met zlib geschreven en in een ICO-Container
 * gezet (PNG-payloads, geldig vanaf Windows Vista).
 */
const fs = require('node:fs');
const path = require('node:path');
const zlib = require('node:zlib');

const BG = [0x0f, 0x17, 0x2a];
const FG = [0xf8, 0xfa, 0xfc];
const SIZES = [16, 24, 32, 48, 64, 128, 256];
const SAMPLES = 4;

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

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}

function inRoundedSquare(x, y, size) {
  const r = (size * 12) / 64;
  if (x < 0 || y < 0 || x > size || y > size) return false;
  const cx = Math.min(Math.max(x, r), size - r);
  const cy = Math.min(Math.max(y, r), size - r);
  const dx = x - cx;
  const dy = y - cy;
  return dx * dx + dy * dy <= r * r;
}

function inTriangle(x, y, size) {
  const s = size / 64;
  const tri = [
    [24 * s, 17 * s],
    [47 * s, 32 * s],
    [24 * s, 47 * s],
  ];
  const sign = (p1, p2, p3) =>
    (p1[0] - p3[0]) * (p2[1] - p3[1]) - (p2[0] - p3[0]) * (p1[1] - p3[1]);
  const d1 = sign([x, y], tri[0], tri[1]);
  const d2 = sign([x, y], tri[1], tri[2]);
  const d3 = sign([x, y], tri[2], tri[0]);
  const neg = d1 < 0 || d2 < 0 || d3 < 0;
  const pos = d1 > 0 || d2 > 0 || d3 > 0;
  return !(neg && pos);
}

function png(size) {
  const raw = Buffer.alloc(size * (size * 4 + 1));
  let p = 0;
  const step = 1 / SAMPLES;
  for (let y = 0; y < size; y++) {
    raw[p++] = 0;
    for (let x = 0; x < size; x++) {
      let box = 0;
      let tri = 0;
      for (let sy = 0; sy < SAMPLES; sy++) {
        for (let sx = 0; sx < SAMPLES; sx++) {
          const px = x + (sx + 0.5) * step;
          const py = y + (sy + 0.5) * step;
          if (inRoundedSquare(px, py, size)) {
            box++;
            if (inTriangle(px, py, size)) tri++;
          }
        }
      }
      const total = SAMPLES * SAMPLES;
      const alpha = box / total;
      const fg = box ? tri / box : 0;
      raw[p++] = Math.round(BG[0] * (1 - fg) + FG[0] * fg);
      raw[p++] = Math.round(BG[1] * (1 - fg) + FG[1] * fg);
      raw[p++] = Math.round(BG[2] * (1 - fg) + FG[2] * fg);
      raw[p++] = Math.round(alpha * 255);
    }
  }

  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', zlib.deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

function ico(images) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(images.length, 4);

  const entries = [];
  let offset = 6 + 16 * images.length;
  for (const image of images) {
    const entry = Buffer.alloc(16);
    entry[0] = image.size >= 256 ? 0 : image.size;
    entry[1] = image.size >= 256 ? 0 : image.size;
    entry[2] = 0;
    entry[3] = 0;
    entry.writeUInt16LE(1, 4);
    entry.writeUInt16LE(32, 6);
    entry.writeUInt32LE(image.data.length, 8);
    entry.writeUInt32LE(offset, 12);
    offset += image.data.length;
    entries.push(entry);
  }

  return Buffer.concat([header, ...entries, ...images.map((image) => image.data)]);
}

const images = SIZES.map((size) => ({ size, data: png(size) }));
const target = path.join(__dirname, '..', 'assets', 'Signage.ico');
fs.mkdirSync(path.dirname(target), { recursive: true });
fs.writeFileSync(target, ico(images));
console.log(`Signage.ico geschreven (${SIZES.join(', ')} px, ${fs.statSync(target).size} bytes)`);
