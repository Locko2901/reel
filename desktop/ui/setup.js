// The bundled OBS's setup, saved at Finish; OBS runs on the defaults meanwhile so the audio step has levels.
import { $, $$, invoke, listen, toast } from './shell.js';
import { isOn, setOn } from './assets/js/switch.js';
import { keycaps, recorder, recordersOffOnLeave } from './hotkeys.js';
import { noiseHelp, noiseOptions, ownMicWords } from './noise.js';
import { ensureOption, syncCustom, takeInto, takenWords } from './inobs.js';
import { bindObsBar, renderObsBar } from './obsbar.js';
import { bindAsk, renderAsk } from './ask.js';
import { audioPlan, meterIn, renderLevelRows, wordsInto } from './sources.js';

const GUIDED = [['length', 'Clip length'], ['quality', 'Quality'], ['encoder', 'Encoder'], ['capture', 'What to capture'], ['audio', 'Audio'], ['hotkeys', 'Hotkeys'], ['where', 'Where clips go'], ['done', 'Done']];
const steps = () => GUIDED;
const QUALITY = { light: 'Light: 1080p30', smooth: 'Smooth: 1080p60', sharp: 'Sharp: 1440p60', native: 'Native: your screen', custom: 'As set in OBS' };
const FORMAT = { hybrid_mp4: 'hybrid MP4', mkv: 'MKV' };
let settings = null;
let status = null;
let screens = [];
let encoders = null;
let suggested = null;
let step = 'choose';
let seen = new Set();
const setup = () => settings.obs.setup;

function get(obj, path) { return path.split('.').reduce((o, k) => (o == null ? undefined : o[k]), obj); }
function put(obj, path, value) {
  const keys = path.split('.');
  let o = obj;
  for (const k of keys.slice(0, -1)) { if (o[k] == null) o[k] = {}; o = o[k]; }
  o[keys[keys.length - 1]] = value;
}

function seconds(n) {
  if (n < 60) return `${n} s`;
  const m = Math.floor(n / 60), s = n % 60;
  return s ? `${m} min ${s} s` : `${m} min`;
}

function screenWords(s) {
  return s ? `${s.name}, ${s.width}x${s.height} at ${s.hz} Hz${s.main ? ', the main one' : ''}` : 'not read';
}

function pressed(seg, attr, value) {
  for (const b of $$('button', seg)) b.setAttribute('aria-pressed', String(b.dataset[attr] === String(value)));
}

function renderSteps() {
  const nav = $('#steps');
  nav.replaceChildren();
  if (step === 'choose') return;
  const at = steps().findIndex(([id]) => id === step);
  steps().forEach(([id, name], i) => {
    const a = document.createElement('a');
    a.href = `#${id}`;
    a.dataset.step = id;
    a.className = id === step ? 'now' : (seen.has(id) && i < at ? 'done' : '');
    const n = document.createElement('i');
    n.textContent = String(i + 1);
    const w = document.createElement('span');
    w.textContent = name;
    a.append(n, w);
    nav.append(a);
  });
}

async function video() {
  try { return await invoke('obs_video', { setup: setup() }); } catch { return null; }
}

function size(mb) {
  return mb >= 1000 ? `${(mb / 1024).toLocaleString('en', { maximumFractionDigits: 1 })} GB` : `${Math.round(mb)} MB`;
}

// OBS keeps the whole replay buffer in memory: past a quarter of the PC's, the game runs short.
let memoryMb;
async function warnings(v) {
  if (memoryMb === undefined) {
    try { memoryMb = await invoke('system_memory'); } catch (e) { memoryMb = null; }
  }
  const out = [];
  if (v && memoryMb && v.buffer_mb > memoryMb / 4) {
    out.push(`That is more than a quarter of this PC's ${size(memoryMb)} of memory, which the game wants too: a shorter clip or a lower quality leaves it more.`);
  }
  const limit = status && status.server_upload_max_bytes;
  const clipMb = v ? v.buffer_mb / 1.25 : 0;
  if (v && limit && settings.mode !== 'standalone' && clipMb * 1024 * 1024 > limit) {
    out.push(`A clip this long is about ${size(clipMb)}, more than the ${size(limit / 1024 / 1024)} your server takes (its Settings, Imports, Upload size limit).`);
  }
  return out;
}

async function showWarnings(el, v) {
  const w = await warnings(v);
  el.textContent = w.join(' ');
  el.hidden = !w.length;
}

async function renderLength() {
  const n = setup().clip_seconds;
  pressed($('#length-seg'), 'seconds', n);
  $('#length-n').value = n;
  const v = await video();
  $('#length-help').textContent = v ? `the replay buffer keeps the last ${seconds(n)}, about ${size(v.buffer_mb)} of memory at this quality` : `the replay buffer keeps the last ${seconds(n)}`;
  showWarnings($('#length-warn'), v);
}

async function renderQuality() {
  const s = setup();
  syncCustom($('#quality'), s);
  $('#quality').value = s.quality;
  const v = await video();
  $('#quality-help').textContent = `the screen it captures: ${screenWords(s.screen.width ? s.screen : null)}`;
  $('#quality-suggest').hidden = !suggested;
  $('#quality-suggest').textContent = suggested ? suggested.why : '';
  const kv = $('#quality-kv');
  kv.replaceChildren();
  const rows = v ? [
    ['Recorded at', `${v.output[0]}x${v.output[1]}, ${v.fps} frames a second`],
    ['From the screen', `${v.base[0]}x${v.base[1]}`],
    [s.rate === 'quality' ? 'Quality level' : 'Bitrate', s.rate === 'quality' ? `${s.level}, the encoder's constant quality` : `${(v.kbit / 1000).toLocaleString('en', { maximumFractionDigits: 1 })} Mbit/s`],
    ['Replay buffer', `about ${size(v.buffer_mb)} of memory for ${seconds(s.clip_seconds)}`],
    ['Format', FORMAT[s.format] || s.format],
  ] : [['Recorded at', QUALITY[s.quality]]];
  for (const [k, val] of rows) {
    const dt = document.createElement('dt'); dt.textContent = k;
    const dd = document.createElement('dd'); dd.textContent = val;
    kv.append(dt, dd);
  }
  $('#rate').value = s.rate;
  $('#kbit').value = s.kbit;
  $('#level').value = s.level;
  ensureOption($('#format'), s.format);
  $('#format').value = s.format;
  $('#kbit-row').hidden = s.rate !== 'bitrate';
  $('#level-row').hidden = s.rate !== 'quality';
  showWarnings($('#quality-warn'), v);
}

// The encoder test as the page sees it: its run, when it asked, which row is in test since when.
const encTest = { run: 0, testing: null, since: 0, done: 0, total: 0, phase: 'ffmpeg', took: null, tick: null };
const encRunning = () => encTest.run !== 0 && encTest.took == null;
const firstOk = () => (encoders.find((e) => e.ok) || {}).id || 'obs_x264';

function encoderCell(e, open) {
  const st = document.createElement('span');
  const why = document.createElement('span');
  why.className = 'faint small';
  if (e.state === 'waiting') {
    st.className = 'st-neutral';
    st.textContent = 'waiting';
  } else if (e.state === 'testing' || e.state === 'in_obs') {
    st.className = 'st-active';
    st.textContent = e.state === 'testing' ? 'testing' : 'trying in OBS';
    why.textContent = ` ${Math.floor((Date.now() - encTest.since) / 1000)} s so far`;
  } else if (e.state === 'works') {
    st.className = 'st-ok';
    st.textContent = 'works';
    const took = [[e.ms, 'test encode'], [e.obs_ms, 'OBS']].filter(([n]) => n != null).map(([n, w]) => `${w} ${(n / 1000).toFixed(1)} s`);
    why.textContent = ` ${e.line}${took.length ? ` (${took.join(', ')})` : ''}`;
  } else if (e.state === 'refused' && e.untried) {
    st.className = 'st-neutral';
    st.textContent = 'not tried in OBS';
    why.textContent = ` ${e.line.replace(/^not tried in OBS: /, '')}; Test again tries it`;
  } else if (e.state === 'refused') {
    st.className = 'st-warn';
    st.textContent = 'OBS cannot use it';
    why.textContent = ` ${e.line}`;
  } else if (e.state === 'no_answer') {
    st.className = 'st-warn';
    st.textContent = e.line;
    why.textContent = ' and was stopped; Test again tries once more';
  } else {
    st.className = 'st-neutral';
    st.textContent = 'not here';
    why.textContent = ` ${e.line}`;
  }
  const out = [st, why];
  if (e.details) {
    const d = document.createElement('details');
    d.className = 'ffmpeg';
    d.dataset.id = e.id;
    d.open = open.has(e.id);
    const sum = document.createElement('summary');
    sum.textContent = 'details';
    const pre = document.createElement('pre');
    pre.textContent = e.details;
    d.append(sum, pre);
    out.push(d);
  }
  return out;
}

// A status region: rebuilt only when its words change, so a screen reader says each once.
function renderEncodersLine() {
  const line = $('#encoders-line');
  const running = encRunning() && encTest.total;
  const words = !encRunning()
    ? (encTest.took == null ? '' : `tested in ${encTest.took.toFixed(1)} s`)
    : (running ? `${encTest.phase === 'obs' ? 'trying' : 'testing'} ${Math.min(encTest.done + 1, encTest.total)} of ${encTest.total}${encTest.phase === 'obs' ? ' in OBS' : ''}` : 'starting the test');
  const key = `${words}/${encTest.phase}/${encTest.done}`;
  if (line.dataset.key === key) return;
  line.dataset.key = key;
  line.replaceChildren();
  if (running) {
    const bar = document.createElement('span');
    bar.className = 'bar';
    const fill = document.createElement('i');
    fill.style.width = `${Math.round((encTest.done / encTest.total) * 100)}%`;
    bar.append(fill);
    line.append(bar);
  }
  line.append(words);
}

function renderEncoders() {
  const body = $('#encoders tbody');
  const open = new Set($$('details[open]', body).map((d) => d.dataset.id));
  body.replaceChildren();
  const sel = $('#encoder');
  sel.replaceChildren();
  renderEncodersLine();
  $('#encoders-again').disabled = encRunning();
  if (!encoders) {
    const tr = document.createElement('tr');
    tr.innerHTML = '<td class="clip">each encoder</td><td class="sub"><span class="st-active">starting the test</span></td>';
    body.append(tr);
    sel.disabled = true;
    return;
  }
  for (const e of encoders) {
    const tr = document.createElement('tr');
    tr.innerHTML = '<td class="clip"></td><td class="sub"></td>';
    tr.children[0].textContent = e.word;
    tr.children[1].append(...encoderCell(e, open));
    body.append(tr);
    if (e.ok) {
      const o = document.createElement('option');
      o.value = e.id;
      o.textContent = e.word;
      sel.append(o);
    }
  }
  if (!encRunning()) {
    const tested = encoders.find((e) => e.id === setup().encoder);
    if (!setup().encoder || (tested && !tested.ok)) setup().encoder = firstOk();
  }
  sel.disabled = encRunning() && !setup().encoder;
  if (sel.disabled) {
    const o = document.createElement('option');
    o.value = '';
    o.textContent = 'picked when the test ends';
    sel.prepend(o);
  }
  ensureOption(sel, setup().encoder);
  sel.value = setup().encoder || '';
}

// Each row's change as the app sends it; one of an older run, or after the answer, is dropped.
function encoderEvent(p) {
  if (!p || p.run !== encTest.run || !encRunning()) return;
  const testing = (p.rows.find((e) => e.state === 'testing' || e.state === 'in_obs') || {}).id || null;
  if (p.phase !== encTest.phase) encTest.testing = null;
  encTest.phase = p.phase;
  if (testing !== encTest.testing) {
    encTest.testing = testing;
    encTest.since = Date.now();
  }
  encoders = p.rows;
  encTest.done = p.done;
  encTest.total = p.total;
  if (step === 'encoder') renderEncoders();
}

async function testEncoders() {
  const run = Date.now();
  Object.assign(encTest, { run, testing: null, since: run, done: 0, total: 0, phase: 'ffmpeg', took: null });
  encoders = null;
  clearInterval(encTest.tick);
  encTest.tick = setInterval(() => { if (step === 'encoder') renderEncoders(); }, 1000);
  if (step === 'encoder') renderEncoders();
  let rows;
  try { rows = await invoke('encoders_test', { run, asked: run }); } catch (e) { rows = [{ id: 'obs_x264', word: 'x264 on the CPU', state: 'works', ok: true, line: String(e.message || e), details: '' }]; }
  if (encTest.run !== run) return;
  clearInterval(encTest.tick);
  encoders = rows;
  encTest.took = (Date.now() - run) / 1000;
  invoke('encoders_seen', { run, pageMs: Date.now() - run }).catch(() => {});
  if (!seen.has('encoder') || !setup().encoder) setup().encoder = firstOk();
  if (step === 'encoder') renderEncoders();
}

const linux = () => !!(status && status.obs_linux);
function plugin(has, what) {
  if (has === true) return null;
  return has === false ? `needs ${what}, which this OBS does not have` : obsWords(`known once OBS answers whether it has ${what}`);
}

function renderCapture() {
  const s = setup();
  if (linux()) {
    const missing = plugin(status.obs_game_capture, 'obs-vkcapture');
    if (missing && s.game) s.game = false;
    $('#game').disabled = !!missing;
    $('#game-help').textContent = missing
      ? `a game straight from Vulkan or OpenGL ${missing}; the screen holds the game meanwhile`
      : 'obs-vkcapture\'s capture of a Vulkan or OpenGL game, any game; start it with obs-gamecapture (on Steam, obs-gamecapture %command% as its launch option)';
    $('#capture-intro').textContent = status.wayland
      ? 'The screen, through PipeWire: the desktop asks once which screen to share, and OBS keeps the answer. A game straight from it too, where OBS has obs-vkcapture.'
      : 'The screen, and a game straight from it too where OBS has obs-vkcapture.';
    $('#elevated-row').hidden = true;
  }
  setOn($('#game'), s.game);
  setOn($('#elevated'), !!s.elevated);
  const sel = $('#screen');
  sel.replaceChildren();
  for (const sc of screens) {
    const o = document.createElement('option');
    o.value = sc.id;
    o.textContent = screenWords(sc);
    sel.append(o);
  }
  sel.value = s.screen.id;
}

let devicesTried = 0;
// Windows' own device list, so the step never waits for OBS; null where there is none and OBS's stands in.
let winDevices;
async function renderAudio() {
  await tracksByDefault();
  const s = setup();
  setOn($('#desktop-on'), s.desktop);
  $('#desktop-audio').hidden = !s.desktop;
  setOn($('#mic-on'), !!s.mic);
  $('#mic').hidden = !s.mic;
  setOn($('#own-tracks'), s.own_tracks);
  setOn($('#discord-on'), !!s.discord);
  renderNoise();
  renderSources();
  renderGameAudio();
  const fill = (sel, items, value) => {
    sel.replaceChildren();
    for (const it of items) { const o = document.createElement('option'); o.value = it.value; o.textContent = it.name; sel.append(o); }
    sel.value = value;
  };
  if (winDevices === undefined) {
    try { winDevices = await invoke('audio_devices'); } catch (e) { winDevices = null; }
  }
  const connected = !!(status && status.obs && status.obs.connected);
  if (winDevices) {
    const byDefault = { name: 'Default, as Windows sets it', value: 'default' };
    fill($('#desktop-audio'), [byDefault, ...winDevices.outputs], s.desktop_audio);
    fill($('#mic'), [byDefault, ...winDevices.inputs], s.mic || 'default');
    $('#audio-line').textContent = connected ? 'as Windows lists them' : `as Windows lists them; ${obsWords('the levels come from OBS')}`;
    if (connected) levels();
    return;
  }
  try {
    const out = await invoke('obs_items', { input: 'Desktop audio', property: 'device_id' });
    const mic = await invoke('obs_items', { input: 'Microphone', property: 'device_id' });
    fill($('#desktop-audio'), out, s.desktop_audio);
    fill($('#mic'), mic.length ? mic : [{ name: 'Default', value: 'default' }], s.mic || 'default');
    $('#audio-line').textContent = 'as OBS lists them';
    levels();
  } catch (e) {
    fill($('#desktop-audio'), [{ name: 'the default output', value: 'default' }], 'default');
    fill($('#mic'), [{ name: 'the default microphone', value: 'default' }], 'default');
    $('#audio-line').textContent = obsWords('the devices come from OBS');
    if (step === 'audio' && devicesTried++ < 15) setTimeout(() => { if (step === 'audio') renderAudio(); }, 2000);
  }
}

let plan = { sources: [], words: [], apart: false };
async function renderSources() {
  plan = await audioPlan(setup());
  const s = setup();
  $('#src-game').hidden = !plan.apart;
  $('#src-discord').hidden = !plan.apart;
  const missing = linux() && !plan.apart ? plugin(status.obs_app_audio, 'the PipeWire audio capture plugin for OBS (obs-pipewire-audio-capture)') : null;
  $('#src-apart-missing').hidden = !missing;
  if (missing) $('#apart-missing-help').textContent = `${missing}; desktop audio holds them meanwhile`;
  const n = renderLevelRows($('#levels'), plan.sources, s.volumes, setLevel);
  $('#levels-panel').hidden = !n;
  wordsInto($('#audio-words'), plan.words);
}

let programs;
async function renderGameAudio() {
  const s = setup(), sel = $('#game-audio');
  setOn($('#game-on'), !!s.game_audio);
  sel.hidden = !s.game_audio;
  if (programs === undefined) {
    try { programs = await invoke('audio_programs'); } catch (e) { programs = []; }
  }
  sel.replaceChildren();
  const add = (value, name) => { const o = document.createElement('option'); o.value = value; o.textContent = name; sel.append(o); };
  if (s.game && !linux()) add('front', 'The game in front');
  for (const p of programs) add(p.value, p.name);
  if (s.game_audio && ![...sel.options].some((o) => o.value === s.game_audio)) add(s.game_audio, s.game_audio === 'front' ? 'The game in front' : s.game_audio.split(':').pop().replace(/#3A/g, ':').replace(/#22/g, '#'));
  sel.value = s.game_audio || (s.game && !linux() ? 'front' : (programs[0] || {}).value || '');
  if (linux()) {
    $('#game-audio-help').textContent = programs.length
      ? 'only a program\'s sound, picked from those playing sound now'
      : 'only a program\'s sound: start the game, then come back here to pick it';
    return;
  }
  $('#game-audio-help').textContent = s.game
    ? 'only the game\'s sound: the game in front, or a program you pick'
    : 'only a program\'s sound, the one you pick (the game in front is there with its capture on)';
}

let ownMic;
async function renderNoise() {
  const s = setup();
  const mic = !!s.mic;
  if (ownMic === undefined) {
    try { ownMic = await invoke('obs_own_mic'); } catch (e) { ownMic = null; }
  }
  $('#noise-row').hidden = !mic;
  $('#audio-advanced').hidden = !mic;
  noiseOptions($('#noise'), s, status);
  $('#noise-help').textContent = noiseHelp(s, status);
  setOn($('#gate'), !!s.gate);
  $('#own-mic-row').hidden = !mic || !ownMic || s.noise === 'own';
  if (ownMic) $('#own-mic-help').textContent = ownMicWords(ownMic);
}

function liveFilters() {
  if (status && status.obs && status.obs.connected) invoke('obs_mic_filters', { setup: setup() }).catch(() => {});
}

function setLevel(name, db) {
  const s = setup();
  s.volumes = { ...(s.volumes || {}), [name]: db };
  if (status && status.obs && status.obs.connected) invoke('obs_volume', { input: name, db }).catch(() => {});
}

let tracksTouched = false;
async function tracksByDefault() {
  const p = await audioPlan(setup());
  if (tracksTouched || setup().own_tracks || p.sources.length < 2) return;
  setup().own_tracks = true;
  setOn($('#own-tracks'), true);
}

async function sourcesChanged() {
  await tracksByDefault();
  renderNoise();
  await renderSources();
  liveFilters();
}

function levels() {
  invoke('obs_meters', { on: true }).catch(() => {});
  if (setup().desktop) invoke('obs_input_set', { input: 'Desktop audio', settings: { device_id: setup().desktop_audio } }).catch(() => {});
  if (setup().mic) invoke('obs_input_set', { input: 'Microphone', settings: { device_id: setup().mic } }).catch(() => {});
  liveFilters();
}

function obsWords(what) {
  const box = status && status.obs_box;
  if (!box) return `${what} once it runs`;
  if (box.state === 'running' || box.state === 'starting') return `${what} once it answers; it has not yet`;
  return `${what} once it answers: ${box.line}`;
}

let unanswered = null;
function renderObsNotice() {
  renderObsBar(status);
  renderAsk(status);
  const n = $('#obs-notice');
  const box = status && status.obs_box;
  const connected = !!(status && status.obs && status.obs.connected);
  if (!settings || !settings.obs.bundled || !box || connected) { unanswered = null; n.hidden = true; return; }
  if (unanswered == null) unanswered = Date.now();
  const wrong = ['silent', 'failed', 'restarting', 'waiting', 'gave_up'].includes(box.state);
  const secs = Math.round((Date.now() - unanswered) / 1000);
  if (!wrong && secs < 5) { n.hidden = true; return; }
  $('#obs-notice-line').textContent = wrong ? `OBS: ${box.line}` : `OBS started and has not answered yet (${secs} s)`;
  n.hidden = false;
}

async function showLog() {
  const d = $('#log-dialog');
  try { $('#log-lines').textContent = (await invoke('log_tail', { lines: 200 })).join('\n') || 'nothing yet'; }
  catch (e) { $('#log-lines').textContent = String(e.message || e); }
  if (!d.open) d.showModal();
  const pre = $('#log-lines');
  pre.scrollTop = pre.scrollHeight;
}

function failed(el, words) {
  el.textContent = words;
  el.hidden = false;
  $('#log-link').hidden = false;
}

async function later() {
  try {
    const now = await invoke('settings_get');
    now.obs.setup_later = true;
    settings = await invoke('settings_save', { settings: now, force: false });
  } catch (e) { toast(String(e.message || e), 'err'); return; }
  invoke('obs_meters', { on: false }).catch(() => {});
  invoke('view_show', { view: 'settings', section: 'obs' }).catch(() => {});
}

// Save and share makes a server's link, so not while This PC only is chosen.
function renderHotkeys() {
  $('#hotkey-save-share').hidden = settings.mode === 'standalone';
  $('#hotkey-save-help').textContent = settings.mode === 'standalone' ? 'OBS saves the replay, and it goes into the library on this PC' : 'OBS saves the replay, and it goes up as any clip does';
  for (const row of $$('section[data-step="hotkeys"] .srow[data-kind="hotkey"]')) fillHotkey(row);
}
function fillHotkey(row) {
  const v = get(settings, row.dataset.key) || '';
  row.dataset.value = v;
  keycaps($('button.hotkey', row), v);
}

function renderWhere() {
  const local = settings.mode === 'standalone';
  pressed($('#where-seg'), 'where', local ? 'local' : 'server');
  $('#server-row').hidden = local;
  $('#pair-row').hidden = local || !!(status && status.paired);
  $('#where-help').textContent = local ? 'reel keeps the library in the app, on this PC; a server can be added later'
    : status && status.paired ? `paired with ${status.server_name || status.server_url} as ${status.user}` : 'pair with the server: one code from its Settings, Devices';
  $('#clips-dir-help').textContent = local ? 'where OBS writes the clips; reel takes each one into the library on this PC'
    : 'where OBS writes the clips; with a server, reel removes its copy once the server has it';
  $('#server-url').value = settings.server_url || '';
  $('#clips-dir').value = settings.clips_dir || '';
}

function audioSummary(s, p) {
  const noise = { off: 'no noise suppression', speex: 'Light noise suppression', rnnoise: 'Strong noise suppression', nvidia: 'NVIDIA noise removal', own: 'your own OBS\'s filters' }[s.noise];
  const words = [...p.words];
  if (s.mic) words.push(`the microphone with ${noise}${s.gate ? ' and the noise gate' : ''}`);
  return words.join('; ');
}

async function renderDone() {
  const s = setup(), v = await video();
  const enc = (encoders || []).find((e) => e.id === s.encoder);
  const sc = screens.find((x) => x.id === s.screen.id);
  const rows = [
    ['Clip length', seconds(s.clip_seconds)],
    ['Quality', `${QUALITY[s.quality] || s.quality}${v ? `, ${v.output[0]}x${v.output[1]} at ${v.fps} fps` : ''}`],
    ['Encoder', enc ? enc.word : s.encoder],
    ['Capture', `${s.game ? 'the game in front, the screen behind it' : 'the screen'}; ${screenWords(sc)}`],
    ['Audio', audioSummary(s, await audioPlan(s))],
    ['Hotkeys', settings.mode === 'standalone' ? `Save a clip ${settings.hotkeys.save || 'none'}` : `Save and share ${settings.hotkeys.save_share || 'none'}, Save ${settings.hotkeys.save || 'none'}`],
    ['Clips go to', settings.mode === 'standalone' ? 'this PC only: the library in the app, on this PC' : (status && status.paired ? (status.server_name || status.server_url) : `${settings.server_url || 'no server yet'}, not paired yet`)],
    ['Clips folder', settings.clips_dir],
  ];
  if (s.elevated) rows.push(['Games run as administrator', 'OBS from a copy in Program Files that only an administrator can change, started with the highest rights by a task at sign-in; Windows asks once, at Finish']);
  const kv = $('#summary');
  kv.replaceChildren();
  for (const [k, val] of rows) {
    const dt = document.createElement('dt'); dt.textContent = k;
    const dd = document.createElement('dd'); dd.textContent = val;
    kv.append(dt, dd);
  }
}

const RENDER = { length: renderLength, quality: renderQuality, encoder: renderEncoders, capture: renderCapture, audio: renderAudio, hotkeys: renderHotkeys, where: renderWhere, done: renderDone };

function go(id) {
  if (step === 'audio' && id !== 'audio') invoke('obs_meters', { on: false }).catch(() => {});
  if (step !== 'choose') seen.add(step);
  step = id;
  $('#wizard').dataset.step = id;
  $$('section[data-step]').forEach((s) => { s.hidden = s.dataset.step !== id; });
  $('#footer').hidden = id === 'choose';
  const at = steps().findIndex(([k]) => k === id);
  $('#back').hidden = at <= 0;
  $('#next').textContent = id === 'done' ? 'Finish' : 'Next';
  $('#step-line').textContent = at >= 0 ? `step ${at + 1} of ${steps().length}` : '';
  $('#finish-err').hidden = true;
  devicesTried = 0;
  renderSteps();
  // A test that ran before reel's OBS did could not try the encoders in it.
  if (id === 'encoder' && encoders && !encRunning() && encoders.some((e) => e.untried)) testEncoders();
  if (RENDER[id]) RENDER[id]();
  window.scrollTo(0, 0);
  if (location.hash !== `#${id}`) history.replaceState(null, '', `#${id}`);
}

async function save(force = false) {
  settings = await invoke('settings_save', { settings, force });
}

let finishedTo = null;
let finishing = false;

function renderStepping() {
  const doing = status && status.obs_step;
  $('#elevated').disabled = !!doing;
  $('#elevated-doing').hidden = !doing;
  $('#elevated-doing').textContent = doing ? doing.line : '';
  $('#next').disabled = !!doing || finishing;
}

async function finish() {
  if (finishedTo) { invoke('view_show', finishedTo).catch(() => {}); return; }
  if (finishing) return;
  finishing = true;
  renderStepping();
  try { await saveAndLeave(); } finally { finishing = false; renderStepping(); }
}

async function saveAndLeave() {
  const err = $('#finish-err');
  const wantElevated = !!setup().elevated;
  settings.obs.bundled = true;
  settings.obs.setup_later = false;
  setup().done = true;
  try {
    await save(false);
  } catch (e) {
    const why = String(e.message || e);
    if (/Save anyway\?$/.test(why)) {
      try { await save(true); } catch (e2) { failed(err, String(e2.message || e2)); setup().done = false; return; }
    } else { failed(err, why); setup().done = false; return; }
  }
  invoke('obs_meters', { on: false }).catch(() => {});
  const ready = settings.mode === 'standalone' || (status && status.paired);
  const to = { view: ready ? 'library' : 'settings', section: ready ? null : 'server' };
  if (wantElevated && !settings.obs.setup.elevated) {
    try { status = await invoke('status_get'); } catch (e) { /* the words below stand without it */ }
    $('#finish-note-line').textContent = `Saved, and OBS starts with it. ${(status && status.obs_task_problem) || 'Capture games that run as administrator stayed off'}; it can be tried again under Settings, OBS.`;
    $('#finish-note').hidden = false;
    $('#log-link').hidden = false;
    $('#next').textContent = ready ? 'Open the library' : 'Pair with a server';
    $('#back').hidden = true;
    $('#later').hidden = true;
    finishedTo = to;
    return;
  }
  toast('Set up: OBS starts with it, hidden.', 'ok');
  invoke('view_show', to).catch(() => {});
}

function bind() {
  $('#choose-bundled').onclick = async () => {
    settings.obs.bundled = true;
    if (!setup().done && !setup().mic) setup().mic = 'default';
    // Only the default: a quality chosen before stays.
    if (!setup().done && suggested && setup().quality === 'smooth') setup().quality = suggested.quality;
    try { await save(); } catch (e) { toast(String(e.message || e), 'err'); return; }
    go('length');
  };
  $('#choose-own').onclick = async () => {
    settings.obs.bundled = false;
    setup().done = true;
    try { await save(); } catch (e) { toast(String(e.message || e), 'err'); return; }
    invoke('view_show', { view: 'settings', section: 'obs' }).catch(() => {});
  };
  $('#steps').addEventListener('click', (e) => {
    const a = e.target.closest('a[data-step]');
    if (!a) return;
    e.preventDefault();
    go(a.dataset.step);
  });
  $('#back').onclick = () => { const all = steps(), i = all.findIndex(([k]) => k === step); if (i > 0) go(all[i - 1][0]); };
  $('#next').onclick = () => { const all = steps(), i = all.findIndex(([k]) => k === step); if (step === 'done') finish(); else go(all[i + 1][0]); };
  $('#length-seg').addEventListener('click', (e) => {
    const b = e.target.closest('button[data-seconds]');
    if (b) { setup().clip_seconds = +b.dataset.seconds; renderLength(); }
  });
  $('#length-n').addEventListener('change', () => {
    const n = Math.round(+$('#length-n').value);
    if (n >= 5 && n <= 1800) setup().clip_seconds = n;
    renderLength();
  });
  $('#quality').onchange = () => { setup().quality = $('#quality').value; renderQuality(); };
  $('#rate').onchange = () => { setup().rate = $('#rate').value; renderQuality(); };
  $('#kbit').onchange = () => { setup().kbit = Math.max(0, Math.round(+$('#kbit').value)); renderQuality(); };
  $('#level').onchange = () => { setup().level = Math.min(51, Math.max(1, Math.round(+$('#level').value))); renderQuality(); };
  $('#format').onchange = () => { setup().format = $('#format').value; renderQuality(); };
  $('#encoders-again').onclick = testEncoders;
  $('#encoder').onchange = () => { setup().encoder = $('#encoder').value; };
  $('#game').addEventListener('change', () => { setup().game = isOn($('#game')); });
  $('#elevated').addEventListener('change', () => { setup().elevated = isOn($('#elevated')); });
  $('#screen').onchange = () => { const sc = screens.find((x) => x.id === $('#screen').value); if (sc) setup().screen = sc; };
  $('#desktop-audio').onchange = () => {
    setup().desktop_audio = $('#desktop-audio').value;
    invoke('obs_input_set', { input: 'Desktop audio', settings: { device_id: setup().desktop_audio } }).catch(() => {});
  };
  $('#desktop-on').addEventListener('change', () => {
    setup().desktop = isOn($('#desktop-on'));
    $('#desktop-audio').hidden = !setup().desktop;
    sourcesChanged();
  });
  $('#mic').onchange = () => {
    const v = $('#mic').value || 'default';
    setup().mic = v;
    invoke('obs_input_set', { input: 'Microphone', settings: { device_id: v } }).catch(() => {});
  };
  $('#mic-on').addEventListener('change', () => {
    const on = isOn($('#mic-on'));
    setup().mic = on ? ($('#mic').value || 'default') : null;
    $('#mic').hidden = !on;
    if (on) invoke('obs_input_set', { input: 'Microphone', settings: { device_id: setup().mic } }).catch(() => {});
    sourcesChanged();
  });
  $('#game-on').addEventListener('change', async () => {
    const on = isOn($('#game-on'));
    setup().game_audio = on ? ($('#game-audio').value || (setup().game ? 'front' : ((programs || [])[0] || {}).value || '')) : '';
    if (on && !setup().game_audio) { setOn($('#game-on'), false); toast('No program with a window is open, and the game capture is off: open the game first.', 'err'); return; }
    await renderGameAudio();
    sourcesChanged();
  });
  $('#game-audio').onchange = () => { setup().game_audio = $('#game-audio').value; sourcesChanged(); };
  $('#discord-on').addEventListener('change', () => { setup().discord = isOn($('#discord-on')); sourcesChanged(); });
  $('#own-tracks').addEventListener('change', () => { setup().own_tracks = isOn($('#own-tracks')); tracksTouched = true; renderSources(); });
  for (const row of $$('section[data-step="audio"] .srow[data-vol]')) {
    for (const i of $$('input', row)) i.addEventListener(i.type === 'range' ? 'input' : 'change', () => setLevel(row, i.value));
  }
  $('#noise').onchange = () => { setup().noise = $('#noise').value; renderNoise(); liveFilters(); };
  $('#gate').addEventListener('change', () => { setup().gate = isOn($('#gate')); liveFilters(); });
  $('#own-mic-use').onclick = () => {
    if (!ownMic) return;
    const s = setup();
    s.own_filters = ownMic.filters;
    s.own_filters_from = `"${ownMic.name}" in your own OBS`;
    s.noise = 'own';
    renderNoise();
    liveFilters();
  };
  for (const row of $$('section[data-step="hotkeys"] .srow[data-kind="hotkey"]')) {
    recorder(row, { save: async (r) => { put(settings, r.dataset.key, r.dataset.value || ''); fillHotkey(r); }, fill: fillHotkey });
  }
  recordersOffOnLeave();
  $('#where-seg').addEventListener('click', (e) => {
    const b = e.target.closest('button[data-where]');
    if (b) { settings.mode = b.dataset.where === 'local' ? 'standalone' : 'connected'; renderWhere(); }
  });
  $('#server-url').onchange = () => { settings.server_url = $('#server-url').value.trim(); };
  $('#clips-dir').onchange = () => { settings.clips_dir = $('#clips-dir').value.trim(); };
  $('#clips-browse').onclick = async () => {
    try {
      const picked = await invoke('pick_folder', { current: settings.clips_dir });
      if (picked) { settings.clips_dir = picked; renderWhere(); }
    } catch (e) { toast(String(e.message || e), 'err'); }
  };
  $('#pair-btn').onclick = async () => {
    const err = $('#pair-err');
    err.hidden = true;
    try {
      status = await invoke('pair', { serverUrl: $('#server-url').value.trim(), code: $('#pair-code').value.trim(), name: settings.device_name });
      const saved = await invoke('settings_get');
      settings.server_url = saved.server_url;
      settings.device_name = saved.device_name;
      $('#pair-code').value = '';
      const all = steps(), i = all.findIndex(([k]) => k === step);
      go(all[Math.min(i + 1, all.length - 1)][0]);
    } catch (e) { failed(err, String(e.message || e)); }
  };
  bindAsk();
  bindObsBar(async (r, before) => {
    status = r.status;
    if (before) takeInto(settings, before, r.settings);
    if (r.taken && r.taken.length) toast(takenWords(r.taken), 'ok');
    renderObsNotice();
    if (RENDER[step]) RENDER[step]();
  }, () => invoke('settings_get').catch(() => null));
  $('#choose-later').onclick = later;
  $('#later').onclick = later;
  $('#log-link').onclick = showLog;
  $('#obs-notice-log').onclick = showLog;
  $('#log-close').onclick = () => $('#log-dialog').close();
  $('#log-folder').onclick = () => invoke('open_log_folder').catch((e) => toast(String(e.message || e), 'err'));
}

async function main() {
  bind();
  try {
    settings = await invoke('settings_get');
    status = await invoke('status_get');
    screens = await invoke('screens_get');
    suggested = await invoke('obs_suggested_quality').catch(() => null);
  } catch (e) {
    toast(`no shell: ${String(e.message || e)}`, 'err');
    return;
  }
  renderStepping();
  const s = setup();
  if (!s.screen || !s.screen.id) s.screen = screens.find((x) => x.main) || screens[0] || s.screen;
  if (!settings.hotkeys.save) settings.hotkeys.save = 'Alt+F10';
  await listen('encoder-test', (ev) => encoderEvent(ev.payload));
  testEncoders();
  await listen('obs-meters', (ev) => {
    for (const [name, db] of ev.payload || []) meterIn($('#levels'), name, db);
  });
  let wasConnected = !!(status && status.obs && status.obs.connected);
  let wasPlugins = `${status.obs_app_audio}/${status.obs_game_capture}`;
  await listen('status', (ev) => {
    status = ev.payload;
    const connected = !!(status.obs && status.obs.connected);
    renderObsNotice();
    renderStepping();
    if (step === 'where') renderWhere();
    if (step === 'audio' && connected !== wasConnected) renderAudio();
    wasConnected = connected;
    const plugins = `${status.obs_app_audio}/${status.obs_game_capture}`;
    if (plugins !== wasPlugins) {
      wasPlugins = plugins;
      if (step === 'capture') renderCapture();
      if (step === 'audio') renderSources();
    }
  });
  setInterval(renderObsNotice, 1000);
  if (linux()) {
    $('#choose-intro').textContent = `reel found ${status.obs_which} and can run it a second time with settings of its own (its own profile, scenes, WebSocket and replay buffer), set up in seven short steps, each with a default. Your own OBS and its settings stay as they are, and both can run at once.`;
    $('#setup-of').textContent = 'setup of reel\'s own OBS';
    $('#choose-own-help').textContent = 'for an OBS set up already: reel connects to it over its WebSocket and uses its replay buffer. Discord turns on its Streamer Mode while it sees your OBS running; its Streamer Mode settings, Automatically Enable/Disable, turn that off';
    if (status.discord_note) $('#choose-intro').textContent += ` ${status.discord_note}`;
    $('#choose-bundled-help').textContent = 'recommended: reel starts this second OBS with the app, in its own tray icon, and writes its settings from these steps; anything can be changed in OBS itself later';
  }
  const want = location.hash.replace('#', '');
  go(settings.obs.bundled && steps().some(([k]) => k === want) ? want : (settings.obs.bundled ? steps()[0][0] : 'choose'));
}

main();
