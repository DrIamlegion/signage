'use strict';

const assert = require('node:assert/strict');
const { spawn } = require('node:child_process');
const fs = require('node:fs');
const net = require('node:net');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');

const ROOT = path.resolve(__dirname, '..');
const NO_WATCH_SERVER = path.join(__dirname, 'fixtures', 'server-no-watch.js');
const SCREEN_KEY = 'watcher-smoke-key';

const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function freePort() {
  const probe = net.createServer();
  await new Promise((resolve, reject) => {
    probe.once('error', reject);
    probe.listen(0, '127.0.0.1', resolve);
  });
  const { port } = probe.address();
  await new Promise((resolve) => probe.close(resolve));
  return port;
}

function writeTestDb(file) {
  const db = {
    salt: 'watcher-smoke-salt',
    password: '',
    screens: [
      {
        id: 'watcher-smoke-screen',
        name: 'Polling test',
        key: SCREEN_KEY,
        items: [
          {
            id: 'watcher-smoke-item',
            type: 'url',
            url: 'https://example.com/watcher-smoke',
            duration: 10,
            start: null,
            end: null,
            days: [],
            timeStart: null,
            timeEnd: null,
          },
        ],
        fit: 'contain',
        paused: false,
        createdAt: 1,
        updatedAt: 123,
        lastSeen: 0,
      },
    ],
    media: [],
    presentations: [],
    settings: { defaultDuration: 10, title: 'Signage' },
  };
  fs.writeFileSync(file, JSON.stringify(db, null, 2));
}

async function fetchJson(url) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  return response.json();
}

async function waitFor(check, { timeout = 8000, interval = 100, label = 'voorwaarde' } = {}) {
  const deadline = Date.now() + timeout;
  let lastError = null;
  while (Date.now() < deadline) {
    try {
      const result = await check();
      if (result) return result;
    } catch (err) {
      lastError = err;
    }
    await delay(interval);
  }
  const detail = lastError ? ` (${lastError.message})` : '';
  throw new Error(`Timeout: ${label}${detail}`);
}

function waitForExit(child, timeout) {
  return new Promise((resolve) => {
    if (child.exitCode !== null) return resolve();
    const timer = setTimeout(resolve, timeout);
    child.once('exit', () => {
      clearTimeout(timer);
      resolve();
    });
  });
}

async function stopChild(child) {
  if (!child || child.exitCode !== null) return;
  child.kill();
  await waitForExit(child, 2000);
  if (child.exitCode === null) {
    child.kill('SIGKILL');
    await waitForExit(child, 2000);
  }
}

test(
  'zender herlaadt db.json via polling als fs.watch uitvalt',
  { timeout: 20000 },
  async () => {
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'signage-watcher-smoke-'));
    const dataDir = path.join(tempDir, 'data');
    const dbFile = path.join(dataDir, 'db.json');
    let child = null;

    try {
      fs.mkdirSync(dataDir, { recursive: true });
      writeTestDb(dbFile);

      const port = await freePort();
      let output = '';
      child = spawn(process.execPath, [NO_WATCH_SERVER], {
        cwd: ROOT,
        env: {
          ...process.env,
          NODE_ENV: 'test',
          SIGNAGE_DATA: dataDir,
          PORT: String(port),
          SIGNAGE_DB_POLL_MS: '500',
          SIGNAGE_UPDATE_FEED: 'http://127.0.0.1:9/latest',
          SIGNAGE_UPDATE_REFRESH_MS: '600000',
        },
        stdio: ['ignore', 'pipe', 'pipe'],
      });
      child.stdout.on('data', (chunk) => {
        output += chunk.toString();
      });
      child.stderr.on('data', (chunk) => {
        output += chunk.toString();
      });

      const playerUrl = `http://127.0.0.1:${port}/api/player?k=${encodeURIComponent(SCREEN_KEY)}`;
      const before = await waitFor(
        async () => {
          const body = await fetchJson(playerUrl);
          return body.screen && body.screen.name === 'Polling test' ? body : null;
        },
        { label: 'zender en beginscherm' }
      );
      await waitFor(() => /pollingfallback is actief/.test(output), {
        timeout: 5000,
        label: 'pollingfallback-log',
      });

      const disk = JSON.parse(fs.readFileSync(dbFile, 'utf8'));
      disk.screens[0].name = 'Polling fallback test';
      fs.writeFileSync(dbFile, JSON.stringify(disk, null, 2));

      const after = await waitFor(
        async () => {
          const body = await fetchJson(playerUrl);
          return body.screen && body.screen.name === 'Polling fallback test' ? body : null;
        },
        { label: 'db-wijziging via polling' }
      );

      assert.notEqual(after.version, before.version);
      assert.equal(after.items.length, before.items.length);
      await waitFor(() => /Database live herladen van schijf/.test(output), {
        timeout: 5000,
        label: 'herlaadlog',
      });
    } finally {
      await stopChild(child);
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  }
);
