// The bundled OBS's setup, saved at Finish; OBS runs on the defaults meanwhile so the audio step has levels.
// Without one (reel light), or with Use my own OBS, the steps for an OBS of one's own: the
// connection is saved as it is left, so the replay buffer's step shows what reel finds there.
import { $, $$, invoke, listen, toast } from './shell.js';
import { isOn, setOn } from './assets/js/switch.js';
import { keycaps, recorder, recordersOffOnLeave } from './hotkeys.js';
import { noiseHelp, noiseOptions, ownMicWords } from './noise.js';
import { ensureOption, syncCustom, takeInto, takenWords } from './inobs.js';
import { bindObsBar, renderObsBar } from './obsbar.js';
import { bindAsk, renderAsk } from './ask.js';
import { audioPlan, meterIn, renderLevelRows, wordsInto } from './sources.js';

const GUIDED = [['length', 'Clip length'], ['quality', 'Quality'], ['encoder', 'Encoder'], ['capture', 'What to capture'], ['audio', 'Audio'], ['hotkeys', 'Hotkeys'], ['games', 'Games'], ['where', 'Where clips go'], ['look', 'New versions'], ['done', 'Done']];
const OWN = [['obs', 'Your OBS'], ['needs', 'The replay buffer'], ['hotkeys', 'Hotkeys'], ['games', 'Games'], ['where', 'Where clips go'], ['look', 'New versions'], ['done', 'Done']];
let chose = null;
const own = () => chose === 'own' || !!(status && !status.bundled);
// New versions is standalone's: a paired app takes its updates from its server.
const pathSteps = (all) => (settings && settings.mode !== 'standalone' ? all.filter(([k]) => k !== 'look') : all);
const steps = () => pathSteps(own() ? OWN : GUIDED);
const COUNT = { 4: 'four', 5: 'five', 6: 'six', 7: 'seven', 8: 'eight', 9: 'nine' };
const counted = (all) => COUNT[pathSteps(all).length - 1] || String(pathSteps(all).length - 1);
const stepsCount = () => counted(own() ? OWN : GUIDED);
const QUALITY = { light: 'Light: 1080p30', smooth: 'Smooth: 1080p60', sharp: 'Sharp: 1440p60', native: 'Native: your screen', custom: 'As set in OBS' };
const FORMAT = { hybrid_mp4: 'hybrid MP4', mkv: 'MKV' };
let settings = null;
let status = null;
let screens = [];
let encoders = null;
let suggested = null;
let step = 'choose';
// Look for new versions on GitHub: on in a first setup; a setup run again shows the library's
// own Check for updates and writes it only when it is changed here.
let look = null;
// Finished before: reel light's own setup, else the setup of the package with OBS.
const again = () => (status && !status.bundled ? !settings.obs.own_setup_due : setup().done);
const lookOn = () => look ?? (again() ? !!(status && status.update_check) : true);
const lookWords = () => (lookOn() ? 'looked for on GitHub a minute after reel on this PC starts, then every six hours' : 'only when you press Check now, under Settings, Startup');
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

function stepLine() {
  const at = steps().findIndex(([k]) => k === step);
  $('#back').hidden = at <= 0;
  $('#step-line').textContent = at >= 0 ? `step ${at + 1} of ${steps().length}` : '';
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

const obsUp = () => !!(status && status.obs && status.obs.connected);

function kvInto(dl, rows) {
  dl.replaceChildren();
  for (const [k, val, cls] of rows) {
    const dt = document.createElement('dt'); dt.textContent = k;
    const dd = document.createElement('dd'); dd.textContent = val;
    if (cls) dd.className = cls;
    dl.append(dt, dd);
  }
}

function renderObs() {
  const o = settings.obs;
  $('#obs-host').value = o.host;
  $('#obs-port').value = o.port;
  $('#obs-password').value = o.password;
  testObs();
}

// The Test button's answer for the values on the step; an answer to values changed since is dropped.
let tests = 0;
async function testObs() {
  const run = ++tests;
  const o = settings.obs, line = $('#obs-test-line');
  line.className = 'st-active';
  line.textContent = `connecting to ${o.host}:${o.port}`;
  $('#obs-test').disabled = true;
  let r;
  try { r = await invoke('obs_test', { host: o.host, port: o.port, password: o.password }); }
  catch (e) { r = { ok: false, line: String(e.message || e), detail: '' }; }
  if (run !== tests) return;
  $('#obs-test').disabled = false;
  line.className = r.ok ? 'st-ok' : 'st-err';
  line.textContent = r.ok ? 'connected' : 'not connected';
  const found = r.ok ? (r.found && r.found.length ? r.found : [{ name: 'OBS', line: r.line }]) : [];
  kvInto($('#obs-found'), found.map((f) => [f.name, f.line, f.ok == null ? '' : (f.ok ? 'st-ok' : 'st-warn')]));
  $('#obs-found').hidden = !found.length;
  const words = $('#obs-test-words'), detail = $('#obs-test-detail');
  words.className = 'st-err';
  words.textContent = r.ok ? '' : r.line;
  words.hidden = r.ok;
  detail.textContent = (!r.ok && r.detail) || '';
  detail.hidden = r.ok || !r.detail;
}

// Changes one part of what is saved now, and the same in the page's copy, which Finish saves.
async function saveNow(change) {
  change(settings);
  try {
    const now = await invoke('settings_get');
    change(now);
    await invoke('settings_save', { settings: now, force: false });
    status = await invoke('status_get');
  } catch (e) { toast(String(e.message || e), 'err'); }
}

let savedTo = null;
function saveConnection() {
  const o = settings.obs, to = JSON.stringify([o.host, o.port, o.password]);
  if (o.enabled && to === savedTo) return;
  savedTo = to;
  saveNow((s) => Object.assign(s.obs, { enabled: true, host: o.host, port: o.port, password: o.password }));
}

function renderNeeds() {
  const checks = (status && status.obs_checks) || [];
  kvInto($('#needs'), checks.length ? checks.map((c) => [c.name, c.line, c.ok ? 'st-ok' : 'st-warn']) : [['OBS', 'known once reel connects to it', 'st-neutral']]);
  setOn($('#start-replay'), settings.obs.start_replay);
  const st = obsUp() ? status.obs_state : null;
  const n = st ? st.replay_seconds : null;
  pressed($('#replay-seg'), 'seconds', n);
  for (const b of $$('#replay-ctl button, #replay-ctl input')) b.disabled = !st;
  if (document.activeElement !== $('#replay-n')) $('#replay-n').value = n ?? '';
  $('#replay-help').textContent = st
    ? `OBS's replay buffer keeps the last ${seconds(n)}; a change here is written into OBS's profile over the WebSocket, and a running buffer starts again with it`
    : 'known once OBS answers';
}

async function setReplay(n) {
  const err = $('#replay-err');
  err.hidden = true;
  try {
    const r = await invoke('obs_apply', { change: { replay_seconds: n } });
    toast(r.lines.join('; ') || 'nothing to change', 'ok');
  } catch (e) { err.textContent = String(e.message || e); err.hidden = false; }
  try { status = await invoke('status_get'); } catch { /* the next status event brings it */ }
  renderNeeds();
}

// OBS's recording folder while the clips folder follows it and OBS answers; null otherwise.
function obsFolder() {
  const st = own() && settings.clips_follow_obs && obsUp() && status.obs_state;
  return st && st.record_dir ? st.record_dir : null;
}

async function setFolder(dir) {
  if (!dir) return;
  if (!obsFolder()) { settings.clips_dir = dir; renderWhere(); return; }
  try {
    const r = await invoke('obs_apply', { change: { record_dir: dir } });
    toast(r.lines.join('; ') || 'nothing to change', 'ok');
    status = await invoke('status_get');
  } catch (e) { toast(String(e.message || e), 'err'); }
  renderWhere();
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
  $('#hotkeys-own').hidden = !own();
  for (const row of $$('section[data-step="hotkeys"] .srow[data-kind="hotkey"]')) fillHotkey(row);
}
function fillHotkey(row) {
  const v = get(settings, row.dataset.key) || '';
  row.dataset.value = v;
  keycaps($('button.hotkey', row), v);
}

// Steam's games as the app last read them, asked once.
let steam = null;
const NUM = ['no', 'one', 'two', 'three', 'four', 'five', 'six', 'seven', 'eight', 'nine'];
const plural = (n, one, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;
const front = () => !(status && status.linux);

async function renderGames() {
  setOn($('#steam-on'), !!settings.games.steam);
  $('#games-after').textContent = front()
    ? 'The rest of the list fills as you play. Settings, Games and apps sorts it: which programs are games, and the name each gives its clips.'
    : 'On Linux reel cannot tell which program is in front, so the rest of the list fills by hand, under Settings, Games and apps.';
  if (!steam) {
    try { steam = await invoke('steam_get'); } catch { steam = { found: false, read: true, games: [], applications: [], shortcuts: [], libraries: 0 }; }
  }
  const s = steam, found = s.found;
  const libs = s.libraries === 1 ? 'one Steam library' : `${NUM[s.libraries] || s.libraries} Steam libraries`;
  $('#steam-step-desc').textContent = found ? `${libs} on this PC, read here: nothing leaves the PC` : 'Steam is not installed on this PC, or reel did not find it';
  $('#steam-step-found').hidden = !found;
  if (found) {
    const shown = s.games.slice(0, 6);
    $('#steam-step-names').textContent = s.games.length
      ? `${plural(s.games.length, 'game')}: ${shown.join(', ')}${s.games.length > shown.length ? `, and ${s.games.length - shown.length} more` : ''}`
      : 'No game installed in Steam yet.';
    const more = [];
    if (s.applications.length) more.push(`${plural(s.applications.length, 'application')}, ${s.applications.slice(0, 3).join(', ')}${s.applications.length > 3 ? ' and more' : ''}`);
    if (s.shortcuts.length) more.push(`${plural(s.shortcuts.length, 'program')} added to Steam by hand`);
    $('#steam-step-more').textContent = `${more.length ? `And ${more.join(', and ')}, which ${s.applications.length + s.shortcuts.length === 1 ? 'comes' : 'come'} in as Not sorted yet. ` : ''}Steam's own tools and soundtracks stay out.`;
  }
  $('#steam-step-help').textContent = found
    ? `${s.games.length ? 'these' : 'its games'}, and each new one as it is installed, as Games with their names in Steam${front() ? ', so clips are named after them from the first one' : ''}; off, Steam adds nothing. Settings, Games and apps changes it later`
    : 'nothing to add now; once Steam is installed, its games come in by themselves, each as it is installed, as Games with their names in Steam';
}

function gamesSummary() {
  const s = steam || { found: false, games: [] }, on = !!settings.games.steam;
  const rest = front() ? 'the rest fills as you play' : 'the rest fills by hand';
  if (!on) return `Steam adds nothing; ${front() ? 'the list fills as you play' : 'the list fills by hand'}`;
  if (!s.found) return `Steam not found: its games come in once it is installed; ${rest}`;
  return `${plural(s.games.length, 'Steam game')} ${s.games.length === 1 ? 'goes' : 'go'} into the list as Games, and each new one as it is installed; ${rest}`;
}

function renderLook() {
  setOn($('#look'), lookOn());
}

function renderWhere() {
  const local = settings.mode === 'standalone';
  pressed($('#where-seg'), 'where', local ? 'local' : 'server');
  $('#server-row').hidden = local;
  $('#pair-row').hidden = local || !!(status && status.paired);
  $('#where-help').textContent = local ? 'reel keeps the library in the app, on this PC; a server can be added later'
    : status && status.paired ? `paired with ${status.server_name || status.server_url} as ${status.user}` : 'pair with the server: one code from its Settings, Devices';
  const folder = obsFolder();
  const after = local ? 'reel takes each one into the library on this PC' : 'with a server, reel removes its copy once the server has it';
  $('#clips-dir-help').textContent = folder ? `OBS's recording folder, read from OBS; ${after}. A folder picked here is set in OBS, over the WebSocket`
    : own() && settings.clips_follow_obs ? `watched until OBS answers, then its recording folder; ${after}`
      : `where OBS writes the clips; ${after}`;
  $('#server-url').value = settings.server_url || '';
  $('#clips-dir').value = folder || settings.clips_dir || '';
}

function audioSummary(s, p) {
  const noise = { off: 'no noise suppression', speex: 'Light noise suppression', rnnoise: 'Strong noise suppression', nvidia: 'NVIDIA noise removal', own: 'your own OBS\'s filters' }[s.noise];
  const words = [...p.words];
  if (s.mic) words.push(`the microphone with ${noise}${s.gate ? ' and the noise gate' : ''}`);
  return words.join('; ');
}

function ownSummary() {
  const o = settings.obs, st = obsUp() ? status.obs_state : null;
  const replay = ((status && status.obs_checks) || []).find((c) => c.name === 'The replay buffer');
  return [
    ['OBS', st ? `OBS ${st.obs_version} at ${o.host}:${o.port}${st.profile ? `, its profile ${st.profile}` : ''}` : `not connected yet: reel connects once OBS answers at ${o.host}:${o.port}`],
    ['Replay buffer', replay ? replay.line : 'known once OBS answers'],
    ['Start the replay buffer', o.start_replay ? 'on: reel turns it on and starts it whenever OBS has it off' : 'off: you start it in OBS'],
  ];
}

async function renderDone() {
  $('#done-intro').textContent = own()
    ? 'That is the setup. Finish saves it: reel stays connected to your OBS, and the hotkeys work from then on.'
    : 'That is the setup. Finish starts OBS with it, hidden; the hotkeys work from then on.';
  if (own()) { summaryInto([...ownSummary(), ...commonSummary()]); return; }
  const s = setup(), v = await video();
  const enc = (encoders || []).find((e) => e.id === s.encoder);
  const sc = screens.find((x) => x.id === s.screen.id);
  const rows = [
    ['Clip length', seconds(s.clip_seconds)],
    ['Quality', `${QUALITY[s.quality] || s.quality}${v ? `, ${v.output[0]}x${v.output[1]} at ${v.fps} fps` : ''}`],
    ['Encoder', enc ? enc.word : s.encoder],
    ['Capture', `${s.game ? 'the game in front, the screen behind it' : 'the screen'}; ${screenWords(sc)}`],
    ['Audio', audioSummary(s, await audioPlan(s))],
    ...commonSummary(),
  ];
  if (s.elevated) rows.push(['Games run as administrator', 'OBS from a copy in Program Files that only an administrator can change, started with the highest rights by a task at sign-in; Windows asks once, at Finish']);
  summaryInto(rows);
}

function commonSummary() {
  const folder = obsFolder();
  const rows = [
    ['Hotkeys', `${settings.mode === 'standalone' ? `Save a clip ${settings.hotkeys.save || 'none'}` : `Save and share ${settings.hotkeys.save_share || 'none'}, Save ${settings.hotkeys.save || 'none'}`}, Record a session ${settings.hotkeys.record || 'none'}`],
    ['Games', gamesSummary()],
    ['Clips go to', settings.mode === 'standalone' ? 'this PC only: the library in the app, on this PC' : (status && status.paired ? (status.server_name || status.server_url) : `${settings.server_url || 'no server yet'}, not paired yet`)],
    ['Clips folder', folder ? `${folder}, OBS's recording folder` : settings.clips_dir],
  ];
  if (settings.mode === 'standalone') rows.push(['New versions', lookWords()]);
  return rows;
}

function summaryInto(rows) {
  const kv = $('#summary');
  kv.replaceChildren();
  for (const [k, val] of rows) {
    const dt = document.createElement('dt'); dt.textContent = k;
    const dd = document.createElement('dd'); dd.textContent = val;
    if (k === 'New versions') dd.id = 'summary-look';
    kv.append(dt, dd);
  }
}

const RENDER = { obs: renderObs, needs: renderNeeds, length: renderLength, quality: renderQuality, encoder: renderEncoders, capture: renderCapture, audio: renderAudio, hotkeys: renderHotkeys, games: renderGames, where: renderWhere, look: renderLook, done: renderDone };

function go(id) {
  if (step === 'audio' && id !== 'audio') invoke('obs_meters', { on: false }).catch(() => {});
  if (step === 'obs' && id !== 'obs') saveConnection();
  if (step !== 'choose') seen.add(step);
  step = id;
  $('#wizard').dataset.step = id;
  $$('section[data-step]').forEach((s) => { s.hidden = s.dataset.step !== id; });
  $('#footer').hidden = id === 'choose';
  $('#next').textContent = id === 'done' ? 'Finish' : 'Next';
  stepLine();
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
  const mine = own();
  const wantElevated = !mine && !!setup().elevated;
  const lookWrite = settings.mode === 'standalone' && (!again() || look !== null);
  const lookWanted = lookOn();
  const was = { bundled: settings.obs.bundled, done: setup().done, due: settings.obs.own_setup_due };
  // reel light's own setup leaves reel's to come, for a switch to reel, the package with OBS.
  if (!mine || status.bundled) {
    settings.obs.bundled = !mine;
    setup().done = true;
  }
  settings.obs.own_setup_due = false;
  settings.obs.setup_later = false;
  const undo = () => { settings.obs.bundled = was.bundled; setup().done = was.done; settings.obs.own_setup_due = was.due; };
  try {
    await save(false);
  } catch (e) {
    const why = String(e.message || e);
    if (/Save anyway\?$/.test(why)) {
      try { await save(true); } catch (e2) { failed(err, String(e2.message || e2)); undo(); return; }
    } else { failed(err, why); undo(); return; }
  }
  invoke('obs_meters', { on: false }).catch(() => {});
  if (lookWrite) await invoke('update_check_set', { on: lookWanted }).catch((e) => toast(String(e.message || e), 'err'));
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
  toast(mine ? 'Set up: reel connects to your OBS whenever it runs.' : 'Set up: OBS starts with it, hidden.', 'ok');
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
    try { await save(); } catch (e) { toast(String(e.message || e), 'err'); return; }
    chose = 'own';
    $('#setup-of').textContent = 'setup for your own OBS';
    go('obs');
  };
  for (const [id, key] of [['#obs-host', 'host'], ['#obs-port', 'port'], ['#obs-password', 'password']]) {
    $(id).addEventListener('change', () => {
      const v = key === 'port' ? Math.round(+$(id).value) : $(id).value.trim();
      if (key === 'port' && !(v >= 1 && v <= 65535)) { $(id).value = settings.obs.port; return; }
      settings.obs[key] = key === 'password' ? $(id).value : v;
      testObs();
    });
  }
  $('#obs-test').onclick = testObs;
  $('#start-replay').addEventListener('change', async () => {
    const on = isOn($('#start-replay'));
    await saveNow((s) => { s.obs.start_replay = on; });
    if (step === 'needs') renderNeeds();
  });
  const seg = $('#length-seg').cloneNode(true);
  seg.id = 'replay-seg';
  $('#replay-ctl').prepend(seg);
  seg.addEventListener('click', (e) => {
    const b = e.target.closest('button[data-seconds]');
    if (b) setReplay(+b.dataset.seconds);
  });
  $('#replay-n').addEventListener('change', () => {
    const n = Math.round(+$('#replay-n').value);
    if (n >= 1 && n <= 3600) setReplay(n); else renderNeeds();
  });
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
  $('#look').addEventListener('change', () => {
    look = isOn($('#look'));
    const line = $('#summary-look');
    if (line) line.textContent = lookWords();
  });
  $('#elevated').addEventListener('change', () => { setup().elevated = isOn($('#elevated')); });
  $('#steam-on').addEventListener('change', () => { settings.games.steam = isOn($('#steam-on')); });
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
    if (b) { settings.mode = b.dataset.where === 'local' ? 'standalone' : 'connected'; renderWhere(); renderSteps(); stepLine(); }
  });
  $('#server-url').onchange = () => { settings.server_url = $('#server-url').value.trim(); };
  $('#clips-dir').onchange = () => setFolder($('#clips-dir').value.trim());
  $('#clips-browse').onclick = async () => {
    try {
      const picked = await invoke('pick_folder', { current: obsFolder() || settings.clips_dir });
      if (picked) setFolder(picked);
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
    settings.games ||= { steam: true };
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
  savedTo = JSON.stringify([settings.obs.host, settings.obs.port, settings.obs.password]);
  await listen('encoder-test', (ev) => encoderEvent(ev.payload));
  if (status.bundled) testEncoders();
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
    if (step === 'needs') renderNeeds();
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
  $$('.steps-count').forEach((n) => { n.textContent = stepsCount(); });
  $$('.own-count').forEach((n) => { n.textContent = counted(OWN); });
  if (linux()) {
    $('#choose-intro').textContent = `reel found ${status.obs_which} and can run it a second time with settings of its own (its own profile, scenes, WebSocket and replay buffer), set up in ${stepsCount()} short steps, each with a default. Your own OBS and its settings stay as they are, and both can run at once.`;
    $('#setup-of').textContent = 'setup of reel\'s own OBS';
    $('#choose-own-help').textContent = `for an OBS set up already: reel connects to it over its WebSocket and uses its replay buffer, set up in ${counted(OWN)} short steps. Discord turns on its Streamer Mode while it sees your OBS running; its Streamer Mode settings, Automatically Enable/Disable, turn that off`;
    if (status.discord_note) $('#choose-intro').textContent += ` ${status.discord_note}`;
    $('#choose-bundled-help').textContent = 'recommended: reel starts this second OBS with the app, in its own tray icon, and writes its settings from these steps; anything can be changed in OBS itself later';
  }
  const want = location.hash.replace('#', '');
  if (!status.bundled) {
    $('#setup-of').textContent = 'setup for your own OBS';
    go(steps().some(([k]) => k === want) ? want : steps()[0][0]);
    return;
  }
  go(settings.obs.bundled && steps().some(([k]) => k === want) ? want : (settings.obs.bundled ? steps()[0][0] : 'choose'));
}

main();
