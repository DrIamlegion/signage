'use strict';

const $ = (sel, root = document) => root.querySelector(sel);
const $$ = (sel, root = document) => [...root.querySelectorAll(sel)];

let state = { screens: [], media: [], settings: {} };
let selectedId = localStorage.getItem('sig_selected') || null;
let pending = Promise.resolve();

/* ------------------------------------------------------------------- api */

async function api(path, opts = {}) {
  const res = await fetch(path, {
    headers: { 'Content-Type': 'application/json' },
    ...opts,
  });
  if (res.status === 401) {
    showLogin();
    throw new Error('Niet ingelogd');
  }
  if (!res.ok) {
    let msg = 'Er ging iets mis';
    try {
      msg = (await res.json()).error || msg;
    } catch {}
    throw new Error(msg);
  }
  if (res.status === 204) return null;
  return res.json();
}

function toast(msg, isError = false) {
  const el = $('#toast');
  el.textContent = msg;
  el.classList.toggle('err', isError);
  el.classList.remove('hidden');
  clearTimeout(toast._t);
  toast._t = setTimeout(() => el.classList.add('hidden'), 2600);
}

/* -------------------------------------------------------------- auth */

function showLogin() {
  $('#login').classList.remove('hidden');
  $('#login-password').focus();
}

$('#login-form').addEventListener('submit', async (e) => {
  e.preventDefault();
  const err = $('#login-error');
  err.classList.add('hidden');
  try {
    await fetch('/api/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ password: $('#login-password').value }),
    }).then(async (r) => {
      if (!r.ok) throw new Error((await r.json()).error || 'Onjuist wachtwoord');
    });
    $('#login').classList.add('hidden');
    $('#login-password').value = '';
    await refresh();
  } catch (ex) {
    err.textContent = ex.message;
    err.classList.remove('hidden');
  }
});

$('#logout').addEventListener('click', async () => {
  await fetch('/api/logout', { method: 'POST' });
  showLogin();
});

/* --------------------------------------------------------------- state */

async function refresh() {
  state = await api('/api/state');
  if (selectedId && !state.screens.some((s) => s.id === selectedId)) selectedId = null;
  if (!selectedId && state.screens.length) selectedId = state.screens[0].id;
  render();
}

function saveScreen(patch) {
  const id = selectedId;
  if (!id) return Promise.resolve();
  const screen = state.screens.find((s) => s.id === id);
  if (screen) Object.assign(screen, patch);
  pending = pending
    .then(() => api('/api/screens/' + id, { method: 'PATCH', body: JSON.stringify(patch) }))
    .then((updated) => {
      const s = state.screens.find((x) => x.id === updated.id);
      if (s) Object.assign(s, updated);
    })
    .catch((e) => toast(e.message, true));
  return pending;
}

const fmtSize = (bytes) => {
  if (bytes < 1024) return bytes + ' B';
  if (bytes < 1048576) return (bytes / 1024).toFixed(0) + ' KB';
  if (bytes < 1073741824) return (bytes / 1048576).toFixed(1) + ' MB';
  return (bytes / 1073741824).toFixed(2) + ' GB';
};

const isOnline = (s) => Date.now() - (s.lastSeen || 0) < 90000;

const DAY_LABELS = [
  ['ma', 1],
  ['di', 2],
  ['wo', 3],
  ['do', 4],
  ['vr', 5],
  ['za', 6],
  ['zo', 0],
];
const DAY_FULL = { 0: 'zondag', 1: 'maandag', 2: 'dinsdag', 3: 'woensdag', 4: 'donderdag', 5: 'vrijdag', 6: 'zaterdag' };

function scheduleSummary(item) {
  const days = item.days || [];
  const parts = [];
  if (!days.length || days.length === 7) parts.push('elke dag');
  else parts.push(DAY_LABELS.filter(([, n]) => days.includes(n)).map(([l]) => l).join(' '));

  if (item.timeStart || item.timeEnd) {
    parts.push(`${item.timeStart || '00:00'}–${item.timeEnd || '24:00'}`);
  }
  if (item.start || item.end) {
    const fmt = (ms) => new Date(ms).toLocaleDateString('nl-NL', { day: '2-digit', month: 'short' });
    parts.push(`van ${item.start ? fmt(item.start) : '…'} t/m ${item.end ? fmt(item.end) : '…'}`);
  }
  return parts.join(' · ');
}

/* --------------------------------------------------------------- render */

function render() {
  renderScreenList();
  renderEditor();
  renderDeckList();
  renderSlideStrip();
  syncDeckForm();
  renderMedia();
  renderSettings();
  const online = state.screens.filter(isOnline).length;
  $('#online-count').textContent = `${online}/${state.screens.length} online`;
}

function renderScreenList() {
  const list = $('#screen-list');
  list.innerHTML = '';
  $('#no-screens').classList.toggle('hidden', state.screens.length > 0);
  for (const s of state.screens) {
    const li = document.createElement('li');
    li.className = s.id === selectedId ? 'active' : '';
    li.innerHTML = `<span class="led ${isOnline(s) ? 'on' : ''}"></span>
      <span class="name"></span><span class="count">${s.itemCount}</span>`;
    li.querySelector('.name').textContent = s.name;
    li.addEventListener('click', () => {
      selectedId = s.id;
      localStorage.setItem('sig_selected', s.id);
      render();
    });
    list.appendChild(li);
  }
}

function playerUrl(screen) {
  return `${location.origin}/player?k=${encodeURIComponent(screen.key)}`;
}

function renderEditor() {
  const screen = state.screens.find((s) => s.id === selectedId);
  $('#editor').classList.toggle('hidden', !screen);
  $('#no-selection').classList.toggle('hidden', !!screen);
  if (!screen) return;

  const nameInput = $('#screen-name');
  if (document.activeElement !== nameInput) nameInput.value = screen.name;

  const pill = $('#screen-status');
  pill.textContent = isOnline(screen)
    ? 'online · ' + new Date(screen.lastSeen).toLocaleTimeString('nl-NL')
    : screen.lastSeen
      ? 'offline · laatst gezien ' + new Date(screen.lastSeen).toLocaleString('nl-NL')
      : 'nog nooit verbonden';
  pill.classList.toggle('on', isOnline(screen));

  $('#player-url').value = playerUrl(screen);
  $('#screen-fit').value = screen.fit || 'contain';
  $('#screen-paused').checked = !!screen.paused;

  /* playlist */
  const ol = $('#playlist');
  ol.innerHTML = '';
  $('#empty-playlist').classList.toggle('hidden', screen.items.length > 0);
  screen.items.forEach((item, index) => {
    const media = item.type === 'media' ? state.media.find((m) => m.id === item.mediaId) : null;
    const deckItem = item.type === 'deck' ? state.presentations.find((d) => d.id === item.presentationId) : null;
    const li = document.createElement('li');

    const thumb = document.createElement('div');
    thumb.className = 'thumb';
    if (deckItem) {
      const mini = document.createElement('div');
      mini.className = 'mini';
      thumb.appendChild(mini);
      window.SlideRender.render(deckPoster(deckItem), mini);
    } else if (media && media.kind === 'image') {
      const img = document.createElement('img');
      img.src = '/media/' + encodeURIComponent(media.file);
      img.alt = '';
      thumb.appendChild(img);
    } else if (media && media.kind === 'video') {
      const v = document.createElement('video');
      v.src = '/media/' + encodeURIComponent(media.file) + '#t=0.5';
      v.muted = true;
      v.preload = 'metadata';
      thumb.appendChild(v);
    } else {
      thumb.textContent = item.type === 'url' ? '🌐' : '📄';
    }

    const meta = document.createElement('div');
    meta.className = 'meta';
    const n = document.createElement('span');
    n.className = 'n';
    n.textContent = media ? media.name : deckItem ? deckItem.name : item.url || '(verwijderd)';
    meta.appendChild(n);

    const chip = document.createElement('span');
    chip.className = 'chip';
    chip.title = 'Weekrooster van deze slide';
    meta.appendChild(chip);

    item.days = Array.isArray(item.days) ? item.days : [];
    function updateChip() {
      chip.textContent = scheduleSummary(item);
    }

    const dur = document.createElement('div');
    dur.className = 'dur';
    const durInput = document.createElement('input');
    durInput.type = 'number';
    durInput.min = 1;
    durInput.max = 3600;
    durInput.value = item.duration;
    durInput.addEventListener('change', () => {
      item.duration = Math.max(1, Math.round(Number(durInput.value) || 10));
      durInput.value = item.duration;
      saveScreen({ items: screen.items });
    });
    dur.append(durInput, document.createTextNode('sec'));

    const acts = document.createElement('div');
    acts.className = 'acts';
    const up = document.createElement('button');
    up.className = 'ghost';
    up.textContent = '↑';
    up.disabled = index === 0;
    up.addEventListener('click', () => moveItem(index, -1));
    const down = document.createElement('button');
    down.className = 'ghost';
    down.textContent = '↓';
    down.disabled = index === screen.items.length - 1;
    down.addEventListener('click', () => moveItem(index, 1));
    const del = document.createElement('button');
    del.className = 'ghost danger';
    del.textContent = '✕';
    del.title = 'Verwijderen';
    del.addEventListener('click', () => {
      screen.items.splice(index, 1);
      saveScreen({ items: screen.items });
      renderEditor();
    });
    acts.append(up, down, del);

    /* weekrooster: dagen + tijdvak + optionele periode */
    const sched = document.createElement('div');
    sched.className = 'sched';

    const daysWrap = document.createElement('div');
    daysWrap.className = 'days';
    for (const [label, num] of DAY_LABELS) {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'day' + (item.days.includes(num) ? ' on' : '');
      btn.textContent = label;
      btn.title = 'Alleen op ' + DAY_FULL[num] + ' (niets aan = elke dag)';
      btn.addEventListener('click', () => {
        const at = item.days.indexOf(num);
        if (at >= 0) item.days.splice(at, 1);
        else item.days.push(num);
        btn.classList.toggle('on');
        updateChip();
        saveScreen({ items: screen.items });
      });
      daysWrap.appendChild(btn);
    }

    const timeStart = document.createElement('input');
    timeStart.type = 'time';
    timeStart.step = '300';
    timeStart.value = item.timeStart || '';
    timeStart.title = 'Vanaf hoe laat (leeg = vanaf 00:00)';
    const timeEnd = document.createElement('input');
    timeEnd.type = 'time';
    timeEnd.step = '300';
    timeEnd.value = item.timeEnd || '';
    timeEnd.title = 'Tot hoe laat (leeg = tot 24:00; mag over middernacht gaan)';
    const timeSep = document.createElement('span');
    timeSep.className = 'dash';
    timeSep.textContent = '–';
    const onTime = () => {
      item.timeStart = /^([01]\d|2[0-3]):[0-5]\d$/.test(timeStart.value) ? timeStart.value : null;
      item.timeEnd = /^([01]\d|2[0-3]):[0-5]\d$/.test(timeEnd.value) ? timeEnd.value : null;
      updateChip();
      saveScreen({ items: screen.items });
    };
    timeStart.addEventListener('change', onTime);
    timeEnd.addEventListener('change', onTime);

    const period = document.createElement('div');
    period.className = 'period';
    const periodLabel = document.createElement('span');
    periodLabel.className = 'lbl';
    periodLabel.textContent = 'periode';
    const startInput = document.createElement('input');
    startInput.type = 'datetime-local';
    startInput.value = toLocalInput(item.start);
    startInput.title = 'Vanaf deze datum/tijd (optioneel)';
    const endInput = document.createElement('input');
    endInput.type = 'datetime-local';
    endInput.value = toLocalInput(item.end);
    endInput.title = 'Tot deze datum/tijd (optioneel)';
    startInput.addEventListener('change', () => {
      item.start = startInput.value ? new Date(startInput.value).getTime() : null;
      updateChip();
      saveScreen({ items: screen.items });
    });
    endInput.addEventListener('change', () => {
      item.end = endInput.value ? new Date(endInput.value).getTime() : null;
      updateChip();
      saveScreen({ items: screen.items });
    });
    period.append(periodLabel, startInput, endInput);

    sched.append(daysWrap, timeStart, timeSep, timeEnd, period);
    updateChip();

    const top = document.createElement('div');
    top.className = 'prow';
    top.append(thumb, meta, dur, acts);
    li.append(top, sched);
    ol.appendChild(li);
  });

  /* media picker */
  const picker = $('#media-picker');
  const current = picker.value;
  picker.innerHTML = '<option value="">— kies media —</option>';
  for (const m of state.media) {
    const opt = document.createElement('option');
    opt.value = m.id;
    opt.textContent = m.name;
    picker.appendChild(opt);
  }
  if (state.media.some((m) => m.id === current)) picker.value = current;

  /* presentatie-picker */
  const deckPicker = $('#deck-picker');
  if (deckPicker) {
    const chosenDeck = deckPicker.value;
    deckPicker.innerHTML = '<option value="">— kies presentatie —</option>';
    for (const deck of state.presentations) {
      const option = document.createElement('option');
      option.value = deck.id;
      option.textContent = deck.name;
      deckPicker.appendChild(option);
    }
    if (state.presentations.some((d) => d.id === chosenDeck)) deckPicker.value = chosenDeck;
  }
}

function toLocalInput(ms) {
  if (!ms) return '';
  const d = new Date(ms - new Date(ms).getTimezoneOffset() * 60000);
  return d.toISOString().slice(0, 16);
}

function moveItem(index, delta) {
  const screen = state.screens.find((s) => s.id === selectedId);
  const target = index + delta;
  if (!screen || target < 0 || target >= screen.items.length) return;
  const [item] = screen.items.splice(index, 1);
  screen.items.splice(target, 0, item);
  saveScreen({ items: screen.items });
  renderEditor();
}

function renderMedia() {
  const grid = $('#media-grid');
  grid.innerHTML = '';
  $('#no-media').classList.toggle('hidden', state.media.length > 0);
  for (const m of [...state.media].sort((a, b) => b.uploadedAt - a.uploadedAt)) {
    const card = document.createElement('div');
    card.className = 'media-card';

    const preview = document.createElement('div');
    preview.className = 'preview';
    if (m.kind === 'image') {
      const img = document.createElement('img');
      img.src = '/media/' + encodeURIComponent(m.file);
      img.loading = 'lazy';
      img.alt = m.name;
      preview.appendChild(img);
    } else if (m.kind === 'video') {
      const v = document.createElement('video');
      v.src = '/media/' + encodeURIComponent(m.file) + '#t=0.5';
      v.muted = true;
      v.preload = 'metadata';
      preview.appendChild(v);
    } else {
      const icon = document.createElement('div');
      icon.className = 'fileicon';
      icon.textContent = '📄';
      preview.appendChild(icon);
    }

    const body = document.createElement('div');
    body.className = 'body';
    const fname = document.createElement('div');
    fname.className = 'fname';
    fname.textContent = m.name;
    fname.title = m.name;
    const sub = document.createElement('div');
    sub.className = 'sub';
    sub.innerHTML = `<span>${m.kind}</span><span>${fmtSize(m.size)}</span>`;
    const acts = document.createElement('div');
    acts.className = 'acts';
    const add = document.createElement('button');
    add.className = 'primary';
    add.textContent = '+ Naar scherm';
    add.disabled = !selectedId;
    add.addEventListener('click', () => {
      const screen = state.screens.find((s) => s.id === selectedId);
      if (!screen) return toast('Selecteer eerst een scherm', true);
      screen.items.push({
        id: Math.random().toString(36).slice(2, 10),
        type: 'media',
        mediaId: m.id,
        duration: state.settings.defaultDuration || 10,
        start: null,
        end: null,
      });
      saveScreen({ items: screen.items }).then(() => {
        renderEditor();
        toast('Toegevoegd aan ' + screen.name);
      });
    });
    const del = document.createElement('button');
    del.className = 'ghost danger';
    del.textContent = 'Verwijderen';
    del.addEventListener('click', async () => {
      if (!confirm(`"${m.name}" verwijderen? Het bestand wordt van de server gehaald.`)) return;
      await api('/api/media/' + m.id, { method: 'DELETE' });
      await refresh();
      toast('Verwijderd');
    });
    acts.append(add, del);
    body.append(fname, sub, acts);
    card.append(preview, body);
    grid.appendChild(card);
  }
}

function renderSettings() {
  const input = $('#default-duration');
  if (document.activeElement !== input) input.value = state.settings.defaultDuration || 10;
}

/* -------------------------------------------------------------- uploads */

async function uploadFiles(files) {
  const list = [...files].filter(Boolean);
  if (!list.length) return;
  const bar = $('#upload-bar');
  const wrap = $('#upload-progress');
  wrap.classList.remove('hidden');
  let done = 0;
  for (const file of list) {
    try {
      await new Promise((resolve, reject) => {
        const xhr = new XMLHttpRequest();
        xhr.open('POST', '/api/media?name=' + encodeURIComponent(file.name));
        xhr.setRequestHeader('Content-Type', file.type || 'application/octet-stream');
        xhr.upload.onprogress = (e) => {
          if (e.lengthComputable) {
            bar.style.width = ((done + e.loaded / e.total) / list.length) * 100 + '%';
          }
        };
        xhr.onload = () => (xhr.status < 400 ? resolve() : reject(new Error('Upload mislukt')));
        xhr.onerror = () => reject(new Error('Upload mislukt'));
        xhr.send(file);
      });
      done++;
      bar.style.width = (done / list.length) * 100 + '%';
    } catch (e) {
      toast(`${file.name}: ${e.message}`, true);
    }
  }
  wrap.classList.add('hidden');
  bar.style.width = '0';
  await refresh();
  if (list.length === 1) toast('Geüpload');
  else toast(list.length + ' bestanden geüpload');
}

/* --------------------------------------------------------------- events */

$$('.tab').forEach((btn) =>
  btn.addEventListener('click', () => {
    $$('.tab').forEach((b) => b.classList.toggle('active', b === btn));
    $$('.tabpane').forEach((p) => p.classList.toggle('active', p.id === 'tab-' + btn.dataset.tab));
  })
);

$('#add-screen').addEventListener('click', async () => {
  const name = prompt('Naam van het nieuwe scherm:', `Scherm ${state.screens.length + 1}`);
  if (name === null) return;
  const screen = await api('/api/screens', { method: 'POST', body: JSON.stringify({ name: name || undefined }) });
  selectedId = screen.id;
  localStorage.setItem('sig_selected', screen.id);
  await refresh();
  toast('Scherm aangemaakt');
});

$('#screen-name').addEventListener('change', (e) => {
  saveScreen({ name: e.target.value.trim() || 'Naamloos' });
});

$('#screen-fit').addEventListener('change', (e) => saveScreen({ fit: e.target.value }));
$('#screen-paused').addEventListener('change', (e) => saveScreen({ paused: e.target.checked }));

$('#delete-screen').addEventListener('click', async () => {
  const screen = state.screens.find((s) => s.id === selectedId);
  if (!screen) return;
  if (!confirm(`Scherm "${screen.name}" verwijderen?`)) return;
  await api('/api/screens/' + screen.id, { method: 'DELETE' });
  selectedId = null;
  localStorage.removeItem('sig_selected');
  await refresh();
  toast('Scherm verwijderd');
});

$('#copy-url').addEventListener('click', async () => {
  const url = $('#player-url').value;
  try {
    await navigator.clipboard.writeText(url);
    toast('Link gekopieerd');
  } catch {
    $('#player-url').select();
    document.execCommand('copy');
    toast('Link gekopieerd');
  }
});

$('#open-player').addEventListener('click', () => window.open($('#player-url').value, '_blank'));

$('#rotate-key').addEventListener('click', async () => {
  if (!confirm('Nieuwe link maken? De oude link werkt daarna niet meer.')) return;
  await api(`/api/screens/${selectedId}/key`, { method: 'POST' });
  await refresh();
  toast('Nieuwe link gemaakt');
});

$('#add-media').addEventListener('click', () => {
  const id = $('#media-picker').value;
  const screen = state.screens.find((s) => s.id === selectedId);
  if (!id) return toast('Kies eerst media', true);
  if (!screen) return;
  screen.items.push({
    id: Math.random().toString(36).slice(2, 10),
    type: 'media',
    mediaId: id,
    duration: state.settings.defaultDuration || 10,
    start: null,
    end: null,
  });
  $('#media-picker').value = '';
  saveScreen({ items: screen.items }).then(renderEditor);
});

/** Google Presentaties: van een gewone deel-link een embed-loop maken. */
function normalizeUrl(raw) {
  const value = String(raw || '').trim();
  const match = /^https?:\/\/(?:docs|slides)\.google\.com\/presentation\/d\/([A-Za-z0-9_-]+)/i.exec(value);
  if (match) return `https://docs.google.com/presentation/d/${match[1]}/embed?start=true&loop=true&delayms=0`;
  return value;
}

$('#add-url').addEventListener('click', () => {
  const input = $('#url-input');
  const value = normalizeUrl(input.value);
  const screen = state.screens.find((s) => s.id === selectedId);
  if (!screen) return;
  if (!/^https?:\/\/\S+$/i.test(value)) return toast('Vul een volledige URL in (https://…)', true);
  screen.items.push({
    id: Math.random().toString(36).slice(2, 10),
    type: 'url',
    url: value,
    duration: state.settings.defaultDuration || 10,
    start: null,
    end: null,
  });
  input.value = '';
  saveScreen({ items: screen.items }).then(renderEditor);
});

$('#url-input').addEventListener('keydown', (e) => {
  if (e.key === 'Enter') $('#add-url').click();
});

$('#pick-files').addEventListener('click', () => $('#file-input').click());
$('#file-input').addEventListener('change', (e) => {
  uploadFiles(e.target.files);
  e.target.value = '';
});

const drop = $('#dropzone');
['dragenter', 'dragover'].forEach((ev) =>
  drop.addEventListener(ev, (e) => {
    e.preventDefault();
    drop.classList.add('over');
  })
);
['dragleave', 'drop'].forEach((ev) =>
  drop.addEventListener(ev, (e) => {
    e.preventDefault();
    drop.classList.remove('over');
  })
);
drop.addEventListener('drop', (e) => uploadFiles(e.dataTransfer.files));

$('#save-duration').addEventListener('click', async () => {
  const value = Number($('#default-duration').value) || 10;
  state.settings = await api('/api/settings', { method: 'PATCH', body: JSON.stringify({ defaultDuration: value }) });
  toast('Opgeslagen');
});

$('#save-password').addEventListener('click', async () => {
  const value = $('#new-password').value;
  if (value.length < 6) return toast('Minstens 6 tekens', true);
  await api('/api/password', { method: 'POST', body: JSON.stringify({ password: value }) });
  $('#new-password').value = '';
  toast('Wachtwoord gewijzigd');
});

/* ------------------------------------------------------------- presentaties */

let selectedDeck = localStorage.getItem('sig_deck') || null;
let deckSlideIndex = 0;
let formDeckId = null;
let formSlideKey = null;

function currentDeck() {
  return state.presentations.find((d) => d.id === selectedDeck) || null;
}

function currentDeckSlide() {
  const deck = currentDeck();
  if (!deck || !deck.slides.length) return null;
  const index = Math.min(Math.max(deckSlideIndex, 0), deck.slides.length - 1);
  return deck.slides[index] || null;
}

function slideSpec(slide) {
  const media = slide && slide.imageId ? state.media.find((m) => m.id === slide.imageId) : null;
  return Object.assign({}, slide, {
    imageUrl: media ? '/media/' + encodeURIComponent(media.file) : null,
  });
}

function deckPoster(deck) {
  return slideSpec((deck && deck.slides && deck.slides[0]) || {});
}

function renderDeckList() {
  const list = $('#deck-list');
  if (!list) return;
  if (selectedDeck && !state.presentations.some((d) => d.id === selectedDeck)) selectedDeck = null;
  list.innerHTML = '';
  $('#no-decks').classList.toggle('hidden', state.presentations.length > 0);
  for (const deck of state.presentations) {
    const li = document.createElement('li');
    li.className = deck.id === selectedDeck ? 'active' : '';
    const swatch = document.createElement('span');
    swatch.className = 'swatch';
    swatch.style.background = (deck.slides[0] && deck.slides[0].bg) || '#0f172a';
    const name = document.createElement('span');
    name.className = 'name';
    name.textContent = deck.name;
    const count = document.createElement('span');
    count.className = 'count';
    count.textContent = String(deck.slides.length);
    li.append(swatch, name, count);
    li.addEventListener('click', () => {
      selectedDeck = deck.id;
      deckSlideIndex = 0;
      localStorage.setItem('sig_deck', deck.id);
      formDeckId = null;
      formSlideKey = null;
      renderDeckList();
      renderSlideStrip();
      syncDeckForm();
    });
    list.appendChild(li);
  }
}

function renderSlideStrip() {
  const strip = $('#slide-strip');
  if (!strip) return;
  const deck = currentDeck();
  strip.innerHTML = '';
  if (!deck) return;

  deck.slides.forEach((slide, index) => {
    const cell = document.createElement('div');
    cell.className = 'strip-cell' + (index === deckSlideIndex ? ' active' : '');

    const thumb = document.createElement('div');
    thumb.className = 'strip-thumb';
    window.SlideRender.render(slideSpec(slide), thumb);

    const tools = document.createElement('div');
    tools.className = 'strip-tools';
    const label = document.createElement('span');
    label.className = 'num';
    label.textContent = String(index + 1);

    const left = document.createElement('button');
    left.type = 'button';
    left.className = 'ghost tiny';
    left.textContent = '←';
    left.title = 'Naar voren';
    left.disabled = index === 0;
    left.addEventListener('click', (event) => {
      event.stopPropagation();
      moveDeckSlide(index, -1);
    });

    const right = document.createElement('button');
    right.type = 'button';
    right.className = 'ghost tiny';
    right.textContent = '→';
    right.title = 'Naar achteren';
    right.disabled = index === deck.slides.length - 1;
    right.addEventListener('click', (event) => {
      event.stopPropagation();
      moveDeckSlide(index, 1);
    });

    const del = document.createElement('button');
    del.type = 'button';
    del.className = 'ghost tiny danger';
    del.textContent = '✕';
    del.title = 'Slide verwijderen';
    del.disabled = deck.slides.length === 1;
    del.addEventListener('click', (event) => {
      event.stopPropagation();
      deleteDeckSlide(index);
    });

    tools.append(label, left, right, del);
    cell.append(thumb, tools);
    cell.addEventListener('click', () => {
      deckSlideIndex = index;
      formSlideKey = null;
      renderSlideStrip();
      syncDeckForm();
    });
    strip.appendChild(cell);
  });

  const counter = $('#slide-counter');
  if (counter) {
    counter.textContent =
      'Slide ' + (Math.min(Math.max(deckSlideIndex, 0), deck.slides.length - 1) + 1) + ' van ' + deck.slides.length;
  }
  const meta = $('#deck-meta');
  if (meta) {
    const total = deck.slides.reduce((sum, slide) => sum + (slide.duration || deck.duration || 8), 0);
    const labels = { none: 'direct', fade: 'vervagen', push: 'opschuiven', wipe: 'vegen', zoom: 'inzoomen' };
    meta.textContent = '±' + Math.round(total) + ' s per ronde · overgang ' + (labels[deck.transition] || 'vervagen');
  }
}

function fillSlideForm(slide) {
  $('#slide-layout').value = slide.layout || 'title';
  $('#slide-title').value = slide.title || '';
  $('#slide-subtitle').value = slide.subtitle || '';
  $('#slide-body').value = slide.body || '';
  $('#slide-footer').value = slide.footer || '';
  $('#slide-bg').value = slide.bg || '#0f172a';
  $('#slide-fg').value = slide.fg || '#f8fafc';
  $('#slide-accent').value = slide.accent || '#5b8cff';
  $('#slide-shade').value = String(Math.round((slide.shade ?? 0.45) * 100));
  $('#slide-align').value = slide.align || 'center';
  $('#slide-scale').value = String(Math.round((slide.scale || 1) * 100));
  $('#slide-duration').value = slide.duration || '';
  $('#slide-bg-angle').value = String(slide.bgAngle === undefined ? 135 : slide.bgAngle);
  $('#slide-bg2-on').checked = !!slide.bg2;
  $('#slide-bg2').value = slide.bg2 || '#1d4ed8';
  $('#slide-notes').value = slide.notes || '';

  const imageSelect = $('#slide-image');
  const chosen = slide.imageId || '';
  imageSelect.innerHTML = '<option value="">— geen —</option>';
  for (const media of state.media) {
    if (media.kind !== 'image') continue;
    const option = document.createElement('option');
    option.value = media.id;
    option.textContent = media.name;
    imageSelect.appendChild(option);
  }
  imageSelect.value = state.media.some((m) => m.id === chosen) ? chosen : '';

  draftElements = JSON.parse(JSON.stringify(slide.elements || []));
  selectedElementId = null;

  formSlideKey = (selectedDeck || '') + ':' + (slide.id || '');
  setFreeMode(slide.layout === 'free', false);
}

function slideFormValues() {
  const duration = Number($('#slide-duration').value);
  return {
    layout: $('#slide-layout').value,
    title: $('#slide-title').value,
    subtitle: $('#slide-subtitle').value,
    body: $('#slide-body').value,
    footer: $('#slide-footer').value,
    bg: $('#slide-bg').value,
    fg: $('#slide-fg').value,
    accent: $('#slide-accent').value,
    bg2: $('#slide-bg2-on').checked ? $('#slide-bg2').value : null,
    bgAngle: Number($('#slide-bg-angle').value) || 135,
    notes: $('#slide-notes').value,
    imageId: $('#slide-image').value || null,
    shade: Number($('#slide-shade').value) / 100,
    align: $('#slide-align').value,
    scale: Number($('#slide-scale').value) / 100,
    duration: Number.isFinite(duration) && duration > 0 ? duration : 0,
    elements: draftElements.map((element) => Object.assign({}, element)),
  };
}

function updateSlidePreview() {
  const box = $('#slide-preview-box');
  if (!box) return;
  const values = slideFormValues();
  const media = values.imageId ? state.media.find((m) => m.id === values.imageId) : null;
  const spec = Object.assign({}, values, {
    imageUrl: media ? '/media/' + encodeURIComponent(media.file) : null,
  });
  window.SlideRender.render(spec, box);
  if (values.layout === 'free') {
    attachCanvas(box, spec);
    syncElementPos();
  }
}

function syncDeckForm() {
  const editor = $('#deck-editor');
  if (!editor) return;
  const deck = currentDeck();
  editor.classList.toggle('hidden', !deck);
  $('#no-deck-selection').classList.toggle('hidden', !!deck);
  if (!deck) return;

  if (formDeckId !== deck.id) {
    $('#deck-name').value = deck.name;
    $('#deck-duration').value = deck.duration;
    $('#deck-transition').value = deck.transition || 'fade';
    formDeckId = deck.id;
  }

  const slide = currentDeckSlide();
  if (!slide) return;
  const key = deck.id + ':' + slide.id;
  if (key !== formSlideKey) fillSlideForm(slide);
  else updateSlidePreview();
}

async function patchDeck(patch) {
  const deck = currentDeck();
  if (!deck) return null;
  const saved = await api('/api/presentations/' + deck.id, {
    method: 'PATCH',
    body: JSON.stringify(patch),
  });
  const local = state.presentations.find((d) => d.id === saved.id);
  if (local) Object.assign(local, saved);
  return saved;
}

async function applySlideForm() {
  const deck = currentDeck();
  const slide = currentDeckSlide();
  if (!deck || !slide) return;
  const values = slideFormValues();
  const slides = deck.slides.map((item, index) =>
    index === deckSlideIndex ? Object.assign({}, item, values, { id: item.id }) : item
  );
  try {
    await patchDeck({ slides });
    formSlideKey = null;
    await refresh();
    toast('Slide bijgewerkt');
  } catch (err) {
    toast(err.message, true);
  }
}

async function moveDeckSlide(index, delta) {
  const deck = currentDeck();
  if (!deck) return;
  const target = index + delta;
  if (target < 0 || target >= deck.slides.length) return;
  const slides = deck.slides.slice();
  const [moved] = slides.splice(index, 1);
  slides.splice(target, 0, moved);
  await patchDeck({ slides });
  deckSlideIndex = target;
  formSlideKey = null;
  await refresh();
}

async function deleteDeckSlide(index) {
  const deck = currentDeck();
  if (!deck || deck.slides.length <= 1) return;
  if (!confirm(`Slide ${index + 1} verwijderen?`)) return;
  const slides = deck.slides.filter((_, i) => i !== index);
  await patchDeck({ slides });
  deckSlideIndex = Math.max(0, Math.min(index, slides.length - 1));
  formSlideKey = null;
  await refresh();
  toast('Slide verwijderd');
}

async function addDeckSlide() {
  const deck = currentDeck();
  if (!deck) return;
  const last = deck.slides[deck.slides.length - 1] || {};
  const slides = deck.slides.concat([
    {
      layout: 'title',
      title: 'Nieuwe slide',
      subtitle: '',
      body: '',
      footer: last.footer || '',
      bg: last.bg || '#0f172a',
      fg: last.fg || '#f8fafc',
      accent: last.accent || '#5b8cff',
      imageId: null,
      shade: last.shade ?? 0.45,
      align: last.align || 'center',
      scale: last.scale || 1,
      duration: 0,
    },
  ]);
  await patchDeck({ slides });
  deckSlideIndex = slides.length - 1;
  formSlideKey = null;
  await refresh();
}

async function duplicateDeckSlide() {
  const deck = currentDeck();
  const slide = currentDeckSlide();
  if (!deck || !slide) return;
  const copy = Object.assign({}, slide, { id: undefined });
  const slides = deck.slides.slice();
  slides.splice(deckSlideIndex + 1, 0, copy);
  await patchDeck({ slides });
  deckSlideIndex += 1;
  formSlideKey = null;
  await refresh();
}

function addDeckToScreen(deckId) {
  const screen = state.screens.find((s) => s.id === selectedId);
  if (!screen) return toast('Selecteer eerst een scherm (tab Schermen)', true);
  const deck = state.presentations.find((d) => d.id === deckId);
  screen.items.push({
    id: Math.random().toString(36).slice(2, 10),
    type: 'deck',
    presentationId: deckId,
    duration: deck ? deck.duration : state.settings.defaultDuration || 8,
    start: null,
    end: null,
    days: [],
    timeStart: null,
    timeEnd: null,
  });
  return saveScreen({ items: screen.items }).then(() => {
    renderEditor();
    toast('Toegevoegd aan ' + screen.name);
  });
}

$('#new-deck').addEventListener('click', async () => {
  const deck = await api('/api/presentations', {
    method: 'POST',
    body: JSON.stringify({ name: 'Nieuwe presentatie' }),
  });
  selectedDeck = deck.id;
  deckSlideIndex = 0;
  localStorage.setItem('sig_deck', deck.id);
  formDeckId = null;
  formSlideKey = null;
  await refresh();
  toast('Presentatie aangemaakt');
});

$('#delete-deck').addEventListener('click', async () => {
  const deck = currentDeck();
  if (!deck) return;
  if (!confirm(`Presentatie "${deck.name}" verwijderen?`)) return;
  await api('/api/presentations/' + deck.id, { method: 'DELETE' });
  selectedDeck = null;
  localStorage.removeItem('sig_deck');
  formDeckId = null;
  formSlideKey = null;
  await refresh();
  toast('Presentatie verwijderd');
});

$('#deck-name').addEventListener('change', async () => {
  const name = $('#deck-name').value.trim() || 'Presentatie';
  await patchDeck({ name });
  await refresh();
});

$('#deck-duration').addEventListener('change', async () => {
  const duration = Math.max(1, Math.round(Number($('#deck-duration').value) || 8));
  await patchDeck({ duration });
});

$('#deck-transition').addEventListener('change', () => patchDeck({ transition: $('#deck-transition').value }));

$('#add-deck-slide').addEventListener('click', addDeckSlide);
$('#apply-slide').addEventListener('click', applySlideForm);
$('#duplicate-slide').addEventListener('click', duplicateDeckSlide);
$('#delete-slide').addEventListener('click', () => deleteDeckSlide(deckSlideIndex));

$('#add-deck').addEventListener('click', () => {
  const id = $('#deck-picker').value;
  if (!id) return toast('Kies eerst een presentatie', true);
  addDeckToScreen(id);
  $('#deck-picker').value = '';
});

[
  '#slide-layout',
  '#slide-title',
  '#slide-subtitle',
  '#slide-body',
  '#slide-footer',
  '#slide-bg',
  '#slide-bg2',
  '#slide-bg2-on',
  '#slide-bg-angle',
  '#slide-fg',
  '#slide-accent',
  '#slide-shade',
  '#slide-align',
  '#slide-scale',
  '#slide-image',
].forEach((selector) => {
  const element = $(selector);
  if (element) element.addEventListener('input', updateSlidePreview);
});

/* -------------------------------------------------------------- vrij canvas */

let draftElements = [];
let selectedElementId = null;

const clampPercent = (value, min, max) => Math.min(max, Math.max(min, value));

function selectedElement() {
  return draftElements.find((element) => element.id === selectedElementId) || null;
}

function syncElementPos() {
  const output = $('#el-pos');
  const element = selectedElement();
  if (!output || !element) return;
  output.textContent =
    Math.round(element.x) + ', ' + Math.round(element.y) + ' · ' + Math.round(element.w) + '×' + Math.round(element.h);
}

function setFreeMode(enabled, autoSeed) {
  const fields = $('.slide-fields');
  if (fields) fields.classList.toggle('free-mode', !!enabled);
  const tools = $('#free-tools');
  if (tools) tools.classList.toggle('hidden', !enabled);

  if (enabled && autoSeed && draftElements.length === 0) {
    const title = $('#slide-title').value.trim();
    if (title) {
      draftElements.push({
        id: Math.random().toString(36).slice(2, 8),
        type: 'text',
        x: 10, y: 32, w: 80, h: 22,
        text: title,
        size: 9,
        weight: 'bold',
        align: 'center',
        color: $('#slide-fg').value,
        background: 'transparent',
        radius: 0,
        imageId: null,
        fit: 'cover',
      });
      selectedElementId = draftElements[0].id;
    }
  }

  renderElementInspector();
  updateSlidePreview();
}

function renderElementInspector() {
  const inspector = $('#el-inspector');
  if (!inspector) return;
  const element = selectedElement();
  inspector.classList.toggle('hidden', !element);
  if (!element) {
    ['#rb-bold', '#rb-italic', '#rb-underline', '#rb-bullet'].forEach((selector) => {
      const button = $(selector);
      if (!button) return;
      button.classList.remove('on');
      button.setAttribute('aria-pressed', 'false');
    });
    if ($('#el-anim')) $('#el-anim').value = 'none';
    if ($('#el-delay')) $('#el-delay').value = '0';
    return;
  }

  $('#el-text').value = element.text || '';
  $('#el-size').value = String(element.size || 6);
  $('#el-color').value = element.color || '#f8fafc';
  const hasBackground = element.background && element.background !== 'transparent';
  $('#el-bg').value = hasBackground ? element.background : '#000000';
  $('#el-bg-on').checked = !!hasBackground;
  $('#el-align').value = element.align || 'left';
  $('#el-weight').value = element.weight || 'normal';
  $('#el-fit').value = element.fit || 'cover';
  $('#el-radius').value = String(element.radius || 0);
  $('#el-valign').value = element.valign || 'center';
  $('#el-opacity').value = String(Math.round((element.opacity === undefined ? 1 : element.opacity) * 100));
  $('#el-italic').checked = !!element.italic;
  $('#el-underline').checked = !!element.underline;
  $('#el-bullet').checked = !!element.bullet;
  $('#el-line-height').value = String(Math.round((element.lineHeight || 1.28) * 100));
  $('#el-letter-spacing').value = String(Math.round((element.letterSpacing || 0) * 100));
  $('#el-anim').value = element.anim || 'none';
  $('#el-delay').value = String(element.delay || 0);
  $('#el-shape').value = element.shape || 'rect';
  const hasFill = element.fill && element.fill !== 'transparent';
  $('#el-fill').value = hasFill ? element.fill : '#e8622c';
  $('#el-fill-on').checked = !!hasFill;
  const hasStroke = element.stroke && element.stroke !== 'transparent';
  $('#el-stroke').value = hasStroke ? element.stroke : '#f8fafc';
  $('#el-stroke-on').checked = !!hasStroke;
  $('#el-stroke-width').value = String(element.strokeWidth || 0);

  const imageSelect = $('#el-image');
  const chosen = element.imageId || '';
  imageSelect.innerHTML = '<option value="">— geen —</option>';
  for (const media of state.media) {
    if (media.kind !== 'image') continue;
    const option = document.createElement('option');
    option.value = media.id;
    option.textContent = media.name;
    imageSelect.appendChild(option);
  }
  imageSelect.value = state.media.some((m) => m.id === chosen) ? chosen : '';

  const pressed = (selector, on) => {
    const button = $(selector);
    if (!button) return;
    button.classList.toggle('on', !!on);
    button.setAttribute('aria-pressed', on ? 'true' : 'false');
  };
  pressed('#rb-bold', element.weight === 'bold');
  pressed('#rb-italic', element.italic);
  pressed('#rb-underline', element.underline);
  pressed('#rb-bullet', element.bullet);
  syncElementPos();
}

function attachCanvas(box, spec) {
  const nodes = [...box.querySelectorAll('.freeEl')];
  const layer = box.querySelector('.freeLayer');

  nodes.forEach((node) => {
    const id = node.getAttribute('data-el');
    if (!id) return;
    if (id === selectedElementId) {
      node.classList.add('selected');
      const handle = document.createElement('div');
      handle.className = 'el-handle';
      handle.addEventListener('pointerdown', (event) => startDrag(event, node, id, box, 'resize'));
      node.appendChild(handle);
    }
    node.addEventListener('pointerdown', (event) => startDrag(event, node, id, box, 'move'));
  });

  if (layer) {
    layer.addEventListener('pointerdown', (event) => {
      if (event.target === layer) {
        selectedElementId = null;
        renderElementInspector();
        updateSlidePreview();
      }
    });
  }
}

function startDrag(event, node, id, box, mode) {
  const element = draftElements.find((item) => item.id === id);
  if (!element) return;
  event.preventDefault();
  event.stopPropagation();
  selectedElementId = id;
  renderElementInspector();
  updateSlidePreview();

  const rect = box.getBoundingClientRect();
  const startX = event.clientX;
  const startY = event.clientY;
  const origin = { x: element.x, y: element.y, w: element.w, h: element.h };

  const onMove = (moveEvent) => {
    const dx = ((moveEvent.clientX - startX) / rect.width) * 100;
    const dy = ((moveEvent.clientY - startY) / rect.height) * 100;
    if (mode === 'move') {
      element.x = clampPercent(origin.x + dx, 0, 100 - element.w);
      element.y = clampPercent(origin.y + dy, 0, 100 - element.h);
    } else {
      element.w = clampPercent(origin.w + dx, 4, 100 - element.x);
      element.h = clampPercent(origin.h + dy, 4, 100 - element.y);
    }

    const live = box.querySelector('.freeEl[data-el="' + id + '"]');
    if (live) {
      live.style.left = element.x + '%';
      live.style.top = element.y + '%';
      live.style.width = element.w + '%';
      live.style.height = element.h + '%';
    }
    syncElementPos();
  };

  const onUp = () => {
    window.removeEventListener('pointermove', onMove);
    window.removeEventListener('pointerup', onUp);
    element.x = Math.round(element.x * 10) / 10;
    element.y = Math.round(element.y * 10) / 10;
    element.w = Math.round(element.w * 10) / 10;
    element.h = Math.round(element.h * 10) / 10;
    updateSlidePreview();
  };

  window.addEventListener('pointermove', onMove);
  window.addEventListener('pointerup', onUp);
}

function addFreeElement(type) {
  const layoutSelect = $('#slide-layout');
  if (layoutSelect && layoutSelect.value !== 'free') {
    layoutSelect.value = 'free';
    setFreeMode(true, true);
  }
  const id = Math.random().toString(36).slice(2, 8);
  const firstImage = state.media.find((media) => media.kind === 'image');
  const isShape = type === 'shape' || type === 'line';
  const element = {
    id,
    type: isShape ? 'shape' : type,
    x: type === 'image' ? 56 : 12,
    y: type === 'image' ? 18 : 40,
    w: type === 'image' ? 32 : 52,
    h: type === 'image' ? 56 : 14,
    shape: type === 'shape' ? 'rect' : 'line',
    text: type === 'image' || isShape ? '' : 'Nieuwe tekst',
    size: 7,
    weight: 'normal',
    italic: false,
    underline: false,
    bullet: false,
    align: type === 'image' ? 'center' : 'left',
    valign: 'center',
    lineHeight: 1.28,
    letterSpacing: 0,
    color: $('#slide-fg').value,
    fill: type === 'line' ? 'transparent' : $('#slide-accent').value,
    stroke: type === 'line' ? $('#slide-accent').value : '#f8fafc',
    strokeWidth: type === 'line' ? 3 : 2,
    opacity: 1,
    background: 'transparent',
    radius: 0,
    imageId: type === 'image' && firstImage ? firstImage.id : null,
    fit: 'cover',
    anim: 'none',
    delay: 0,
  };
  if (type === 'shape') {
    element.w = 30;
    element.h = 26;
  }
  if (type === 'line') {
    element.x = 20;
    element.y = 48;
    element.w = 60;
    element.h = 6;
  }
  draftElements.push(element);
  selectedElementId = id;
  renderElementInspector();
  updateSlidePreview();
}

$('#add-text-el').addEventListener('click', () => addFreeElement('text'));
$('#add-image-el').addEventListener('click', () => addFreeElement('image'));
$('#add-shape-el').addEventListener('click', () => addFreeElement('shape'));
$('#add-line-el').addEventListener('click', () => addFreeElement('line'));

$('#delete-el').addEventListener('click', () => {
  if (!selectedElementId) return toast('Selecteer eerst een element in de dia', true);
  draftElements = draftElements.filter((element) => element.id !== selectedElementId);
  selectedElementId = null;
  renderElementInspector();
  updateSlidePreview();
});

function reorderElement(delta) {
  const index = draftElements.findIndex((element) => element.id === selectedElementId);
  if (index < 0) return toast('Selecteer eerst een element in de dia', true);
  const target = index + delta;
  if (target < 0 || target >= draftElements.length) return;
  const [moved] = draftElements.splice(index, 1);
  draftElements.splice(target, 0, moved);
  updateSlidePreview();
}

$('#bring-front').addEventListener('click', () => reorderElement(1));
$('#send-back').addEventListener('click', () => reorderElement(-1));

function onElementInput(selector, event, apply) {
  const input = $(selector);
  if (!input) return;
  input.addEventListener(event, () => {
    const element = selectedElement();
    if (!element) return;
    apply(element, input);
    updateSlidePreview();
  });
}

onElementInput('#el-text', 'input', (element, input) => { element.text = input.value; });
 onElementInput('#el-size', 'input', (element, input) => { element.size = Number(input.value); });
 onElementInput('#el-color', 'input', (element, input) => { element.color = input.value; });
 onElementInput('#el-bg', 'input', (element, input) => {
  if (element.background && element.background !== 'transparent') element.background = input.value;
});
onElementInput('#el-align', 'change', (element, input) => { element.align = input.value; });
onElementInput('#el-weight', 'change', (element, input) => { element.weight = input.value; });
onElementInput('#el-fit', 'change', (element, input) => { element.fit = input.value; });
onElementInput('#el-radius', 'input', (element, input) => { element.radius = Number(input.value); });
onElementInput('#el-image', 'change', (element, input) => { element.imageId = input.value || null; });
onElementInput('#el-shape', 'change', (element, input) => { element.shape = input.value; });
onElementInput('#el-fill', 'input', (element, input) => {
  if (element.fill && element.fill !== 'transparent') element.fill = input.value;
});
onElementInput('#el-stroke', 'input', (element, input) => {
  if (element.stroke && element.stroke !== 'transparent') element.stroke = input.value;
});
onElementInput('#el-stroke-width', 'input', (element, input) => { element.strokeWidth = Number(input.value); });
onElementInput('#el-opacity', 'input', (element, input) => {
  element.opacity = Math.min(1, Math.max(0.05, Number(input.value) / 100));
});
onElementInput('#el-valign', 'change', (element, input) => { element.valign = input.value; });
onElementInput('#el-line-height', 'input', (element, input) => {
  element.lineHeight = Math.min(2.4, Math.max(0.9, Number(input.value) / 100));
});
onElementInput('#el-letter-spacing', 'input', (element, input) => {
  element.letterSpacing = Math.min(0.2, Math.max(-0.05, Number(input.value) / 100));
});
onElementInput('#el-anim', 'change', (element, input) => { element.anim = input.value; });
onElementInput('#el-delay', 'input', (element, input) => {
  element.delay = Math.min(20, Math.max(0, Number(input.value) || 0));
});

function bindToggle(selector, apply) {
  const input = $(selector);
  if (!input) return;
  input.addEventListener('change', () => {
    const element = selectedElement();
    if (!element) return;
    apply(element, input.checked);
    updateSlidePreview();
  });
}

bindToggle('#el-italic', (element, on) => { element.italic = on; });
bindToggle('#el-underline', (element, on) => { element.underline = on; });
bindToggle('#el-bullet', (element, on) => { element.bullet = on; });
bindToggle('#el-fill-on', (element, on) => {
  element.fill = on ? $('#el-fill').value : 'transparent';
});
bindToggle('#el-stroke-on', (element, on) => {
  element.stroke = on ? $('#el-stroke').value : 'transparent';
});

function alignElement(mode) {
  const element = selectedElement();
  if (!element) return toast('Selecteer eerst een element in de dia', true);
  if (mode === 'left') element.x = 0;
  if (mode === 'right') element.x = 100 - element.w;
  if (mode === 'center') element.x = (100 - element.w) / 2;
  if (mode === 'top') element.y = 0;
  if (mode === 'bottom') element.y = 100 - element.h;
  if (mode === 'middle') element.y = (100 - element.h) / 2;
  element.x = Math.round(element.x * 10) / 10;
  element.y = Math.round(element.y * 10) / 10;
  updateSlidePreview();
}

document.querySelectorAll('[data-el-align]').forEach((button) => {
  button.addEventListener('click', () => alignElement(button.getAttribute('data-el-align')));
});

const duplicateEl = $('#duplicate-el');
if (duplicateEl) {
  duplicateEl.addEventListener('click', () => {
    const element = selectedElement();
    if (!element) return toast('Selecteer eerst een element in de dia', true);
    const copy = Object.assign({}, element, {
      id: Math.random().toString(36).slice(2, 8),
      x: Math.min(100 - element.w, element.x + 3),
      y: Math.min(100 - element.h, element.y + 3),
    });
    draftElements.push(copy);
    selectedElementId = copy.id;
    renderElementInspector();
    updateSlidePreview();
  });
}

const bgToggle = $('#el-bg-on');
if (bgToggle) {
  bgToggle.addEventListener('change', () => {
    const element = selectedElement();
    if (!element) return;
    element.background = bgToggle.checked ? $('#el-bg').value : 'transparent';
    updateSlidePreview();
  });
}

const layoutSelect = $('#slide-layout');
if (layoutSelect) {
  layoutSelect.addEventListener('change', () => setFreeMode(layoutSelect.value === 'free', true));
}

/* ------------------------------------------------------------------- lint */

document.querySelectorAll('[data-ribbon-tab]').forEach((tab) => {
  tab.addEventListener('click', () => {
    const name = tab.getAttribute('data-ribbon-tab');
    document.querySelectorAll('[data-ribbon-tab]').forEach((other) => {
      const active = other === tab;
      other.classList.toggle('active', active);
      other.setAttribute('aria-selected', active ? 'true' : 'false');
    });
    document.querySelectorAll('[data-ribbon-panel]').forEach((panel) => {
      panel.classList.toggle('active', panel.getAttribute('data-ribbon-panel') === name);
    });
  });
});

function applyFormat(prop) {
  const element = selectedElement();
  if (!element) return toast('Selecteer eerst een element in de dia', true);
  if (prop === 'bold') element.weight = element.weight === 'bold' ? 'normal' : 'bold';
  else element[prop] = !element[prop];
  renderElementInspector();
  updateSlidePreview();
}

document.querySelectorAll('[data-format]').forEach((button) => {
  button.addEventListener('click', () => applyFormat(button.getAttribute('data-format')));
});

/* ---------------------------------------------------------------- start */

refresh().catch(() => showLogin());

setInterval(() => {
  if ($('#login').classList.contains('hidden')) refresh().catch(() => {});
}, 5000);
