const { invoke } = window.__TAURI__.core;
const appWindow = window.__TAURI__.window.getCurrentWindow();
const $ = (id) => document.getElementById(id);

async function ask(payload) {
  const reply = JSON.parse(await invoke('request', { line: JSON.stringify(payload) }));
  if (reply.error) throw new Error(reply.error);
  return reply;
}

const command = (cmd, arg) => ask(arg === undefined ? { cmd } : { cmd, arg }).catch((e) => { $('error').textContent = e.message; });

const clock = (s) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, '0')}`;

// Only touch the page when something changed: every write means a repaint.
function setText(id, text) {
  const el = $(id);
  if (el.textContent !== text) el.textContent = text;
}

function setAttr(id, name, value) {
  const el = $(id);
  if (el.getAttribute(name) !== value) el.setAttribute(name, value);
}

function setFill(input) {
  const span = input.max - input.min;
  input.style.setProperty('--fill', `${((input.value - input.min) / span) * 100}%`);
}

// ---- Mode: the modern player, or classic .wsz skins as pixels.

function readMode() {
  try { return localStorage.getItem('waimea-mode') === 'classic' ? 'classic' : 'modern'; } catch { return 'modern'; }
}

function setMode(mode) {
  document.body.dataset.mode = mode;
  try { localStorage.setItem('waimea-mode', mode); } catch { /* the choice just lasts this session */ }
  if (mode === 'classic') drawFrame();
  else sizeWave();
}

// ---- Window shape: rounded corners, except when maximized.

async function syncMaximized() {
  document.body.classList.toggle('maximized', await appWindow.isMaximized());
}

appWindow.onResized(syncMaximized);
syncMaximized();

// ---- The card: the library is on the back of the player.

const card = document.querySelector('.card');
let flipId = 0;

function showFace(toBack) {
  document.body.classList.toggle('flipped', toBack);
  $('library').inert = !toBack;
  document.querySelector('.stage').inert = toBack;
  const toggle = $('library-toggle');
  toggle.setAttribute('aria-expanded', String(toBack));
  toggle.setAttribute('aria-label', toBack ? T.backToPlayer : T.showLibrary);
  $('toggle-icon').setAttribute('d', toBack ? 'M6 6l12 12M18 6L6 18' : 'M4 6h16M4 12h16M4 18h10');
  syncEqButton();
}

function syncEqButton() {
  $('eq-toggle').setAttribute('aria-expanded', String(isFlipped() && source === 'eq'));
}

function flip(toBack) {
  const id = ++flipId;
  if (matchMedia('(prefers-reduced-motion: reduce)').matches) {
    showFace(toBack);
    return;
  }
  card.className = 'card turning-out';
  setTimeout(() => {
    if (id !== flipId) return;
    showFace(toBack);
    card.className = 'card turning-in';
    card.getBoundingClientRect(); // apply the start of the second half
    card.className = 'card landing';
    setTimeout(() => { if (id === flipId) card.className = 'card'; }, 320);
  }, 240);
}

const isFlipped = () => document.body.classList.contains('flipped');

$('library-toggle').addEventListener('click', () => flip(!isFlipped()));
addEventListener('keydown', (e) => { if (e.key === 'Escape' && isFlipped()) flip(false); });

// ---- Artwork: the artist's photo or the station's logo on the front.

const artCache = new Map();
let artKey = '';

async function showArtwork({ title, artist }) {
  let station = '';
  let stream = '';
  let isStream = false;
  if (status.queue_index !== null && status.queue_index !== undefined) {
    const queue = await ask({ get: 'queue' }).catch(() => null);
    const item = queue?.items.find((entry) => entry.current);
    isStream = item?.source === 'url' && !status.duration;
    if (isStream) { station = item.title; stream = item.id; }
  }
  // Stream titles are usually "Artist - Title".
  let name = artist;
  if (!name && isStream && title.includes(' - ')) {
    [name] = title.split(' - ');
  }
  const key = `${name}|${station}`;
  if (key === artKey) return;
  artKey = key;
  if (!artCache.has(key)) {
    artCache.set(key, await invoke('artwork', { artist: name.trim(), station, stream }).catch(() => null));
  }
  if (key !== artKey) return;
  const art = artCache.get(key);
  $('art').hidden = !art;
  if (art) {
    $('art-image').src = art.url;
    $('art').classList.toggle('logo', art.logo);
    setText('art-credit', art.credit);
  }
}

// ---- Status: what plays, polled once a second.

let status = { status: 'stopped' };
let elapsedAt = 0;
let seeking = false;
let settingVolume = false;

// ponytail: some stations pack "title~artist~~year~..." into their stream title.
// Split that one pattern; a real parser belongs with Peer Radio's title matching.
function nowPlaying({ title, artist }) {
  const parts = (title || '').split('~');
  if (parts.length >= 3) return { title: parts[0], artist: artist || parts[1] };
  return { title: title || '', artist: artist || '' };
}

let shownTitle = '';
const PLAY = 'M8 5v14l11-7z';
const PAUSE = 'M7 5h3.6v14H7zM13.4 5H17v14h-3.6z';

function showStatus() {
  const playing = status.status === 'playing';
  const { title, artist } = nowPlaying(status);
  if (title !== shownTitle) {
    shownTitle = title;
    if (title) showArtwork({ title, artist });
    else { artKey = ''; $('art').hidden = true; }
  }
  setText('title', title || T.choose);
  setText('artist', artist);
  setText('error', status.last_error || '');
  const pageTitle = title ? `${title} - Waimea` : 'Waimea';
  if (document.title !== pageTitle) document.title = pageTitle;
  setAttr('play-icon', 'd', playing ? PAUSE : PLAY);
  setAttr('play', 'aria-label', playing ? T.pause : T.play);
  setAttr('shuffle', 'aria-pressed', String(!!status.shuffle));
  const repeat = status.repeat && status.repeat !== 'off';
  setAttr('repeat', 'aria-pressed', String(!!repeat));
  if ($('repeat-one').hidden !== (status.repeat !== 'one')) $('repeat-one').hidden = status.repeat !== 'one';
  if (!settingVolume && status.volume !== undefined && Number($('volume').value) !== status.volume) {
    $('volume').value = status.volume;
    setFill($('volume'));
  }
  const live = status.status !== 'stopped' && !status.duration;
  if ($('position').disabled !== !status.duration) $('position').disabled = !status.duration;
  setText('duration', status.duration ? clock(status.duration) : (live ? 'Live' : ''));
  showClock();
}

function showClock() {
  let elapsed = status.elapsed || 0;
  if (status.status === 'playing') elapsed += (performance.now() - elapsedAt) / 1000;
  if (status.duration) elapsed = Math.min(elapsed, status.duration);
  setText('elapsed', status.status === 'stopped' ? '0:00' : clock(elapsed));
  if (!seeking) {
    const position = Math.round(status.duration ? (elapsed / status.duration) * 1000 : 0);
    if (Number($('position').value) !== position) {
      $('position').value = position;
      setFill($('position'));
    }
  }
}

let shownQueue = '';

async function pollStatus() {
  try {
    const next = await ask({ get: 'status' });
    status = next;
    elapsedAt = performance.now();
    showStatus();
    const queueKey = `${next.queue_index}/${next.queue_length}`;
    if (queueKey !== shownQueue) {
      shownQueue = queueKey;
      if (source === 'queue') showQueue();
    }
  } catch (e) {
    console.error(e);
  }
  setTimeout(pollStatus, document.hidden ? 2000 : 1000);
}

setInterval(() => { if (!document.hidden && status.status === 'playing') showClock(); }, 250);

// ---- Controls

$('play').addEventListener('click', () => command(status.status === 'stopped' ? 'play' : 'toggle_pause'));
$('prev').addEventListener('click', () => command('prev'));
$('next').addEventListener('click', () => command('next'));
$('shuffle').addEventListener('click', () => command('toggle_shuffle'));
$('repeat').addEventListener('click', () => command('cycle_repeat'));
$('minimize').addEventListener('click', () => appWindow.minimize());
$('close').addEventListener('click', () => command('quit'));

$('position').addEventListener('input', (e) => {
  seeking = true;
  setFill(e.target);
  if (status.duration) $('elapsed').textContent = clock((e.target.value / 1000) * status.duration);
});
$('position').addEventListener('change', async (e) => {
  await command('seek_frac', e.target.value / 1000);
  seeking = false;
});

$('volume').addEventListener('input', (e) => {
  settingVolume = true;
  setFill(e.target);
  command('set_volume', e.target.value / 100);
});
$('volume').addEventListener('change', () => { settingVolume = false; });

addEventListener('keydown', (e) => {
  if (e.code === 'Space' && !['INPUT', 'BUTTON', 'SELECT'].includes(e.target.tagName)) {
    e.preventDefault();
    $('play').click();
  }
});

// ---- The wave: the spectrum as the surface of the sea.

const wave = $('wave');
const pen = wave.getContext('2d');
const reduceMotion = matchMedia('(prefers-reduced-motion: reduce)').matches;
const LAYERS = [
  { smooth: 0.14, scale: 0.6, shift: 0.0, level: 0.34, top: '#0e5a7d', bottom: '#0a3a5c' },
  { smooth: 0.24, scale: 0.8, shift: 0.9, level: 0.5, top: '#12a4c4', bottom: '#0b4f70' },
  { smooth: 0.42, scale: 1.0, shift: 1.9, level: 0.66, top: '#7ff5e3', bottom: '#0b4f70' },
];
const POINTS = 22;
let levels = LAYERS.map(() => []);
let spectrum = [];
let fills = [];

function sizeWave() {
  const ratio = devicePixelRatio || 1;
  const box = wave.getBoundingClientRect();
  wave.width = Math.max(1, Math.round(box.width * ratio));
  wave.height = Math.max(1, Math.round(box.height * ratio));
  fills = LAYERS.map((layer) => {
    const fill = pen.createLinearGradient(0, wave.height * 0.2, 0, wave.height);
    fill.addColorStop(0, layer.top);
    fill.addColorStop(1, layer.bottom);
    return fill;
  });
  paintWave(performance.now());
}

function surface(layer, values, time) {
  const w = wave.width;
  const h = wave.height;
  const moving = status.status === 'playing' && !reduceMotion;
  const drift = moving ? time / 1500 : 0;
  const points = [];
  for (let i = 0; i <= POINTS; i += 1) {
    const u = i / POINTS;
    const swell = Math.sin(u * 4.6 + drift + layer.shift) * h * 0.035;
    // The outer bands are mostly empty; use the middle so both edges keep moving.
    const index = 1 + u * Math.max(0, values.length * 0.8 - 2);
    const a = Math.floor(index);
    const value = values.length ? values[a] + (values[Math.min(a + 1, values.length - 1)] - values[a]) * (index - a) : 0;
    // The highs are quieter than the lows: lift them so the whole width moves.
    const lift = Math.min(Math.sqrt(value) * (0.55 + u * 0.7), 1) * h * 0.3 * layer.scale;
    points.push([u * w, h * layer.level + swell - lift]);
  }
  return points;
}

function drawLayer(n, points) {
  pen.fillStyle = fills[n];
  pen.beginPath();
  pen.moveTo(0, wave.height);
  pen.lineTo(points[0][0], points[0][1]);
  for (let i = 1; i < points.length - 1; i += 1) {
    pen.quadraticCurveTo(points[i][0], points[i][1], (points[i][0] + points[i + 1][0]) / 2, (points[i][1] + points[i + 1][1]) / 2);
  }
  const last = points[points.length - 1];
  pen.lineTo(last[0], last[1]);
  pen.lineTo(wave.width, wave.height);
  pen.closePath();
  pen.fill();
}

function paintWave(time) {
  pen.clearRect(0, 0, wave.width, wave.height);
  LAYERS.forEach((layer, n) => {
    if (levels[n].length !== spectrum.length) levels[n] = new Array(spectrum.length).fill(0);
    levels[n] = levels[n].map((v, i) => v + (spectrum[i] - v) * layer.smooth);
    drawLayer(n, surface(layer, levels[n], time));
  });
}

// 30 frames a second is plenty for water; the engine is asked half as often.
const FRAME_MS = 33;
let frameCount = 0;

async function waveLoop() {
  const started = performance.now();
  if (document.body.dataset.mode === 'modern' && !document.hidden) {
    const playing = status.status === 'playing';
    if (playing) {
      frameCount += 1;
      if (frameCount % 2 === 0) spectrum = Array.from(new Float32Array(await invoke('spectrum')));
      paintWave(started);
    } else if (levels.some((layer) => layer.some((v) => v > 0.002))) {
      spectrum = spectrum.map(() => 0);
      paintWave(started);
    }
  }
  const idle = status.status !== 'playing' && !levels.some((layer) => layer.some((v) => v > 0.002));
  setTimeout(waveLoop, idle ? 250 : Math.max(5, FRAME_MS - (performance.now() - started)));
}

// While the stage resizes, CSS stretches the canvas; redraw sharply when it settles.
let waveResize;
new ResizeObserver(() => {
  clearTimeout(waveResize);
  waveResize = setTimeout(sizeWave, 120);
}).observe(wave);

// ---- Equalizer panel

const BANDS = ['60', '170', '310', '600', '1k', '3k', '6k', '12k', '14k', '16k'];

function slider(label, value, onInput, extra = '') {
  const band = document.createElement('div');
  band.className = `band ${extra}`;
  const rail = document.createElement('div');
  rail.className = 'rail';
  rail.tabIndex = 0;
  rail.setAttribute('role', 'slider');
  rail.setAttribute('aria-label', label === 'Pre' ? T.preamp : `${label} Hz`);
  rail.setAttribute('aria-valuemin', '-12');
  rail.setAttribute('aria-valuemax', '12');
  const fill = document.createElement('span');
  fill.className = 'fill';
  const thumb = document.createElement('span');
  thumb.className = 'thumb';
  rail.append(fill, thumb);
  let db = value;
  const show = () => {
    rail.style.setProperty('--frac', String((db + 12) / 24));
    rail.setAttribute('aria-valuenow', String(db));
  };
  const change = (next) => {
    const stepped = Math.max(-12, Math.min(12, Math.round(next * 2) / 2));
    if (stepped === db) return;
    db = stepped;
    show();
    onInput(db);
  };
  const fromPointer = (event) => {
    const box = rail.getBoundingClientRect();
    change(((box.bottom - event.clientY) / box.height) * 24 - 12);
  };
  rail.addEventListener('pointerdown', (event) => {
    rail.setPointerCapture(event.pointerId);
    rail.dataset.dragging = 'true';
    fromPointer(event);
  });
  rail.addEventListener('pointermove', (event) => { if (rail.dataset.dragging) fromPointer(event); });
  const stop = () => { delete rail.dataset.dragging; };
  rail.addEventListener('pointerup', stop);
  rail.addEventListener('pointercancel', stop);
  rail.addEventListener('keydown', (event) => {
    const steps = { ArrowUp: 0.5, ArrowDown: -0.5, PageUp: 3, PageDown: -3 };
    if (event.key in steps) {
      event.preventDefault();
      change(db + steps[event.key]);
    } else if (event.key === 'Home') {
      event.preventDefault();
      change(0);
    }
  });
  show();
  band.append(rail, label);
  return band;
}

async function showEq() {
  const { eq, presets } = JSON.parse(await invoke('eq'));
  $('eq-on').checked = eq.enabled;
  const select = $('eq-preset');
  if (!select.options.length) {
    select.append(new Option(T.preset, ''), ...presets.map((name) => new Option(name, name)));
  }
  select.value = '';
  $('eq-bands').replaceChildren(
    slider('Pre', eq.preamp_db, (db) => command('set_preamp', db), 'pre'),
    ...eq.bands_db.map((db, band) => slider(BANDS[band], db, (value) => command('set_eq_band', { band, db: value }))),
  );
}

$('eq-toggle').addEventListener('click', () => {
  if (isFlipped() && source === 'eq') {
    flip(false);
    return;
  }
  show('eq');
  if (!isFlipped()) flip(true);
});
$('eq-on').addEventListener('change', () => command('toggle_eq'));
$('eq-preset').addEventListener('change', async (e) => {
  if (!e.target.value) return;
  await command('eq_preset', e.target.value);
  showEq();
});

// ---- Classic skins: the engine draws the pixels, this only shows them.

const canvas = $('winamp');
const paint = canvas.getContext('2d');
const WIDTH = 275;
let wake = null;
let frameRunning = false;

async function drawFrame() {
  if (frameRunning) return;
  frameRunning = true;
  while (document.body.dataset.mode === 'classic') {
    let delay = 500;
    try {
      const bytes = new Uint8Array(await invoke('frame'));
      if (bytes.length > 3) {
        const height = bytes[1] | (bytes[2] << 8);
        if (canvas.height !== height) {
          canvas.height = height;
          fitClassic();
        }
        const pixels = new Uint8ClampedArray(bytes.buffer, 3, WIDTH * height * 4);
        paint.putImageData(new ImageData(pixels, WIDTH, height), 0, 0);
        $('loading').hidden = true;
        canvas.classList.remove('waiting');
      } else if (bytes.length === 0) {
        canvas.classList.add('waiting');
      }
      if (bytes.length > 0) delay = bytes[0] & 1 ? 50 : 250;
    } catch (e) {
      console.error(e);
    }
    await new Promise((resolve) => {
      const timer = setTimeout(resolve, delay);
      wake = () => { clearTimeout(timer); resolve(); };
    });
  }
  frameRunning = false;
}

function fitClassic() {
  const box = document.querySelector('.stage').getBoundingClientRect();
  const scale = Math.min(box.width / WIDTH, box.height / canvas.height);
  canvas.style.width = `${WIDTH * scale}px`;
  canvas.style.height = `${canvas.height * scale}px`;
}

new ResizeObserver(fitClassic).observe(document.querySelector('.stage'));

function pixelAt(event) {
  const box = canvas.getBoundingClientRect();
  const clamp = (v, max) => Math.max(0, Math.min(max - 1, Math.floor(v)));
  return {
    x: clamp((event.clientX - box.left) * canvas.width / box.width, canvas.width),
    y: clamp((event.clientY - box.top) * canvas.height / box.height, canvas.height),
  };
}

async function send(kind, { x, y }) {
  const openFiles = await invoke('mouse', { kind, x, y });
  if (wake) wake();
  if (openFiles) addMusic(false);
}

let pressed = false;

canvas.addEventListener('pointerdown', (event) => {
  const at = pixelAt(event);
  // The main window's title bar moves the app; its buttons sit at the edges.
  if (at.y < 14 && at.x >= 16 && at.x < 244) {
    appWindow.startDragging();
    return;
  }
  if (at.y < 14 && at.x >= 244 && at.x < 254) {
    appWindow.minimize();
    return;
  }
  pressed = true;
  canvas.setPointerCapture(event.pointerId);
  send('down', at);
});

canvas.addEventListener('pointermove', (event) => {
  if (pressed) send('drag', pixelAt(event));
});

canvas.addEventListener('pointerup', (event) => {
  if (!pressed) return;
  pressed = false;
  send('up', pixelAt(event));
});

canvas.addEventListener('wheel', (event) => {
  event.preventDefault();
  send(event.deltaY < 0 ? 'wheel-up' : 'wheel-down', pixelAt(event));
}, { passive: false });

// ---- Library

const tabs = [...document.querySelectorAll('[role=tab]')];
const query = $('query');
const chips = $('chips');
const add = $('add');
const results = $('results');
const message = $('message');
const form = $('search');

const SOURCES = {
  local: { placeholder: T.searchMusic, live: true, empty: T.noMusic },
  radio: { placeholder: T.searchRadio, start: T.chips[0], chips: T.chips, empty: T.noStations },
};

let source = 'local';
let searchId = 0;

function say(text, isError = false) {
  message.textContent = text;
  message.classList.toggle('error', isError);
}

function row({ title, detail, time, current }, onClick) {
  const item = document.createElement('li');
  const button = document.createElement('button');
  button.type = 'button';
  if (current) button.setAttribute('aria-current', 'true');
  const name = document.createElement('span');
  name.className = 'title';
  name.textContent = title;
  button.append(name);
  if (detail) {
    const sub = document.createElement('span');
    sub.className = 'sub';
    sub.textContent = detail;
    button.append(sub);
  }
  if (time) {
    const span = document.createElement('span');
    span.className = 'time';
    span.textContent = clock(time);
    button.append(span);
  }
  button.addEventListener('click', onClick);
  item.append(button);
  return item;
}

const details = (hit) => [hit.artist, hit.album].filter(Boolean).join(' - ') || hit.subtitle;

async function search(text) {
  const id = ++searchId;
  const from = source;
  say(T.searching);
  try {
    const limit = from === 'local' ? 500 : 30;
    const reply = await ask({ search: { source: from, query: text, limit, then: 'show' } });
    if (id !== searchId) return;
    results.replaceChildren(...reply.results.map((hit) => row(
      { title: hit.title, detail: details(hit), time: hit.duration },
      () => play(hit, text, from, limit),
    )));
    say(reply.results.length ? '' : SOURCES[from].empty);
  } catch (e) {
    if (id === searchId) say(e.message, true);
  }
}

async function play(hit, text, from, limit) {
  say(T.starting(hit.title));
  try {
    // ponytail: repeats the search, as `search --play N` does; results that
    // changed in between start another row. Queue by id if that ever bites.
    await ask({ search: { source: from, query: text, limit, then: { play: hit.index } } });
    say('');
    flip(false);
  } catch (e) {
    say(e.message, true);
  }
}

async function showQueue() {
  const { items } = await ask({ get: 'queue' });
  results.replaceChildren(...items.map((item) => row(
    { title: item.title, detail: item.artist, time: item.duration, current: item.current },
    () => { command('play_index', item.index - 1); flip(false); },
  )));
  say(items.length ? '' : T.queueEmpty);
}

async function showLooks() {
  say('');
  const names = await invoke('skins');
  results.replaceChildren(
    row({ title: 'Waimea', detail: T.modernPlayer, current: document.body.dataset.mode === 'modern' }, () => {
      setMode('modern');
      showLooks();
      flip(false);
    }),
    ...names.map((name) => row({ title: name, detail: T.classicSkin }, async () => {
      try {
        await ask({ skin: name });
        setMode('classic');
        say(T.skinChosen(name));
        showLooks();
        flip(false);
      } catch (e) {
        say(e.message, true);
      }
    })),
  );
}

function show(next) {
  source = next;
  const config = SOURCES[next];
  tabs.forEach((tab) => tab.setAttribute('aria-selected', String(tab.dataset.source === next)));
  form.hidden = !config;
  add.hidden = next !== 'local';
  chips.hidden = !config || !config.chips;
  results.replaceChildren();
  results.hidden = next === 'eq';
  $('eq').hidden = next !== 'eq';
  say('');
  syncEqButton();
  if (next === 'eq') { showEq(); return; }
  if (next === 'queue') { showQueue(); return; }
  if (next === 'skins') { showLooks(); return; }
  query.placeholder = config.placeholder;
  query.value = '';
  chips.replaceChildren(...(config.chips || []).map((label) => {
    const chip = document.createElement('button');
    chip.type = 'button';
    chip.textContent = label;
    chip.addEventListener('click', () => { query.value = label; search(label); });
    return chip;
  }));
  search(next === 'local' ? '' : config.start);
}

async function addMusic(folder) {
  try {
    const count = await invoke('add_music', { folder, title: T.addMusicTitle });
    if (count) say(T.added);
  } catch (e) {
    say(String(e), true);
  }
}

tabs.forEach((tab) => tab.addEventListener('click', () => show(tab.dataset.source)));
$('add-files').addEventListener('click', () => addMusic(false));
$('add-folder').addEventListener('click', () => addMusic(true));

form.addEventListener('submit', (event) => {
  event.preventDefault();
  search(query.value.trim());
});

let typing;
query.addEventListener('input', () => {
  if (!SOURCES[source]?.live) return;
  clearTimeout(typing);
  typing = setTimeout(() => search(query.value.trim()), 150);
});

$('library').inert = true;
setMode(readMode());
show('local');
pollStatus();
waveLoop();
