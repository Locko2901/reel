// The app's own Settings section: every row saves as it changes.
import { $, $$, APP_SECTIONS, STANDALONE_ONLY, applyShape, invoke, listen, markSection, mount, renderParts, runSave, saveButtons, sectionsNav, toast } from './shell.js';
import { isOn, setOn } from './assets/js/switch.js';
import { keycaps, recorder, recordersOffOnLeave } from './hotkeys.js';
import { noiseHelp, noiseOptions, ownMicWords } from './noise.js';
import { ensureOption, syncCustom, takenWords } from './inobs.js';
import { bindObsBar, renderObsBar } from './obsbar.js';
import { audioPlan, meterIn, renderLevelRows, wordsInto } from './sources.js';
import { bindLibrary, refreshLibrary, renderLibrary } from './library.js';

const CTL = 'input, select, button[role="switch"]';

let settings = null;
const HW_WORD = { none: 'x264 on the CPU', nvenc: 'NVIDIA NVENC', amf: 'AMD AMF', qsv: 'Intel Quick Sync', vaapi: 'VAAPI' };
let status = null;
let shapeKey = '';

function get(obj, path) { return path.split('.').reduce((o, k) => (o == null ? undefined : o[k]), obj); }
function set(obj, path, value) {
  const keys = path.split('.');
  let o = obj;
  for (const k of keys.slice(0, -1)) { if (o[k] == null) o[k] = {}; o = o[k]; }
  o[keys[keys.length - 1]] = value;
}

function fill(row) {
  const key = row.dataset.key, kind = row.dataset.kind;
  const v = get(settings, key);
  if (kind === 'hotkey') { row.dataset.value = v || ''; keycaps($('button.hotkey', row), v || ''); return; }
  const ctl = $(CTL, row);
  if (!ctl) return;
  if (kind === 'bool') setOn(ctl, !!v);
  else if (kind === 'volume') { ctl.value = Math.round((v ?? 1) * 100); $('.vol', row).textContent = ctl.value; }
  else if (kind === 'list') ctl.value = (v || []).join(', ');
  else if (kind === 'file') ctl.value = v || '';
  else ctl.value = v == null ? '' : v;
}

function read(row) {
  const key = row.dataset.key, kind = row.dataset.kind;
  if (kind === 'hotkey') return row.dataset.value || '';
  const ctl = $(CTL, row);
  if (kind === 'bool') return isOn(ctl);
  if (kind === 'volume') return Math.max(0, Math.min(100, +ctl.value)) / 100;
  if (kind === 'int') return Math.round(+ctl.value);
  if (kind === 'list') return ctl.value.split(',').map(s => s.trim().toLowerCase().replace(/^\./, '')).filter(Boolean);
  if (kind === 'file') return ctl.value.trim() === '' ? null : ctl.value.trim();
  return ctl.value;
}

// The elevated switch shows its click until every answer is in, whatever order answers and events come in.
let elevatedAsks = 0;
function syncElevated() {
  if (elevatedAsks > 0 || !status || !settings || status.obs_elevated === undefined) return;
  settings.obs.setup.elevated = status.obs_elevated;
  setOn($(CTL, $('#elevated-app-row')), status.obs_elevated);
}

async function save(row, force = false) {
  const key = row.dataset.key;
  const base = settings;
  const next = JSON.parse(JSON.stringify(settings));
  set(next, key, read(row));
  const err = $('.err', row);
  const held = key === 'obs.setup.elevated' && $(CTL, row);
  if (held) { elevatedAsks++; held.disabled = true; }
  let answer;
  try {
    answer = await invoke('settings_save', { settings: next, base, force });
  } catch (e) {
    row.classList.add('refused');
    err.textContent = String(e.message || e);
    err.hidden = false;
  }
  if (answer) {
    settings = answer;
    row.classList.remove('refused');
    err.hidden = true;
  }
  fill(row);
  if (held) {
    elevatedAsks--;
    syncElevated();
    held.disabled = !!(status && status.obs_step) || elevatedAsks > 0;
    if (!answer || get(answer, key) !== get(next, key)) { if (status) renderBundled(); return; }
  }
  if (!answer) return;
  const ctl = $('.sctl', row);
  let ok = $('.ok', ctl);
  if (!ok) { ok = document.createElement('span'); ok.className = 'ok'; ctl.append(ok); }
  ok.textContent = 'saved';
  setTimeout(() => ok.remove(), 1800);
  if (key.startsWith('obs.setup.') && status) renderBundled();
}

let capAsk = () => {}, capAt = 0;
function capLine() {
  const field = $('.srow[data-key="standalone.cap_gb"] input'), line = $('#cap-line');
  if (!field || !line) return;
  let timer = null;
  const ask = async () => {
    capAt = Date.now();
    try {
      const d = await invoke('standalone_cap', { gb: Math.max(0, Math.round(+field.value || 0)) });
      line.textContent = d.line || '';
      line.classList.toggle('st-warn', /the cap moves/.test(d.line || ''));
    } catch { line.textContent = ''; }
  };
  field.addEventListener('input', () => { clearTimeout(timer); timer = setTimeout(ask, 300); });
  field.addEventListener('change', ask);
  capAsk = ask;
}

function bindRows() {
  capLine();
  for (const row of $$('.srow[data-key]')) {
    const ctl = $(CTL, row);
    if (!ctl) continue;
    ctl.addEventListener('change', () => save(row));
    if (row.dataset.kind === 'volume') ctl.addEventListener('input', () => { $('.vol', row).textContent = ctl.value; });
  }
  for (const b of $$('button[data-browse]')) {
    b.addEventListener('click', async () => {
      const row = b.closest('.srow');
      try {
        const picked = await invoke('pick_wav');
        if (picked) { $('input', row).value = picked; await save(row); }
      } catch (e) { toast(String(e.message || e), 'err'); }
    });
  }
  for (const b of $$('button[data-preview]')) {
    b.addEventListener('click', async () => {
      b.disabled = true;
      try { const r = await invoke('cue_preview', { cue: b.dataset.preview }); toast(r.line, 'ok'); }
      catch (e) { toast(String(e.message || e), 'err'); }
      b.disabled = false;
    });
  }
  $('#browse-dir')?.addEventListener('click', async () => {
    const row = $('.srow[data-key="clips_dir"]');
    try {
      const picked = await invoke('pick_folder', { current: settings.clips_dir });
      if (picked) { $('input', row).value = picked; await save(row); }
    } catch (e) { toast(String(e.message || e), 'err'); }
  });
}

function bindHotkeys() {
  for (const row of $$('.srow[data-kind="hotkey"]')) recorder(row, { save, fill });
  recordersOffOnLeave();
}

let current = 'status';
function sections() {
  history.scrollRestoration = 'manual';
  const root = $('#settings'), body = $('#sections-body');
  const known = (id) => APP_SECTIONS.some(([k]) => k === id);
  const open = (id, push, chosen = true) => {
    if (!known(id)) id = 'status';
    current = id;
    if (id === 'library') capAsk();
    root.dataset.section = id;
    $$('.group', body).forEach(g => g.hidden = g.dataset.appSection !== id);
    $$('.section-line, .panel, .srow', body).forEach(el => el.hidden = el.hasAttribute('data-off'));
    $('#no-match').hidden = true;
    markSection(id);
    if (push) history.pushState(null, '', `#${id}`);
    window.scrollTo(0, 0);
    if (id === 'log') loadLog();
    invoke('obs_meters', { on: id === 'obs' }).catch(() => {});
    if (id === 'obs' && settings && settings.obs.bundled) invoke('obs_read').then((r) => doneInObs(r, false, true)).catch(() => {});
    invoke('page_section', { section: id, chosen }).catch(() => {});
  };
  document.addEventListener('click', (e) => {
    const a = e.target.closest('nav.sections a[data-app-section]');
    if (!a) return;
    e.preventDefault();
    open(a.dataset.appSection, true);
  });
  window.addEventListener('popstate', () => open(location.hash.replace('#', '') || 'status', false));
  open(location.hash.replace('#', '') || 'status', false, false);
  window.openSection = (id) => open(id, true);
  const filter = $('#settings-filter');
  let searched = false;
  filter?.addEventListener('input', () => {
    const q = filter.value.trim().toLowerCase();
    if (q && !searched) { searched = true; invoke('page_section', { section: current, chosen: true }).catch(() => {}); }
    if (!q) { open(current, false); return; }
    let hits = 0;
    $$('.group', body).forEach(g => g.hidden = false);
    $$('.section-line', body).forEach(l => l.hidden = true);
    $$('.panel', body).forEach(p => {
      let any = false;
      $$('.srow', p).forEach(r => {
        const hit = `${$('.shelp', r)?.textContent || ''} ${$('b', r)?.textContent || ''} ${r.dataset.key || ''}`.toLowerCase().includes(q);
        r.hidden = !hit || r.hasAttribute('data-off'); any = any || !r.hidden;
      });
      p.hidden = !any || p.hasAttribute('data-off'); hits += p.hidden ? 0 : 1;
    });
    $('#no-match').hidden = hits > 0;
  });
}

// data-off keeps it hidden when a section opens or the filter runs.
function off(el, yes) { el.toggleAttribute('data-off', yes); el.hidden = yes; }

function renderMode() {
  const here = status.mode === 'standalone', os = status.linux ? 'linux' : 'windows';
  for (const el of $$('[data-standalone], [data-os]')) {
    const w = el.dataset.standalone;
    const mode = w === 'hide' ? here : w === 'paired' ? here && !status.paired : w === 'only' ? !here : false;
    off(el, mode || (!!el.dataset.os && el.dataset.os !== os));
  }
  for (const el of $$('[data-standalone-text]')) {
    if (el.dataset.connectedText == null) el.dataset.connectedText = el.textContent;
    el.textContent = here ? el.dataset.standaloneText : el.dataset.connectedText;
  }
  for (const el of $$('[data-linux-text]')) {
    if (el.dataset.windowsText == null) el.dataset.windowsText = el.textContent;
    el.textContent = status.linux ? el.dataset.linuxText : el.dataset.windowsText;
  }
  // beside a server, each page's line says it is this PC's (app.css); reel's say theirs are the server's
  $('#settings')?.toggleAttribute('data-paired', !here);
}

function esc(s) { return String(s ?? ''); }

function kv(dl, pairs) {
  dl.innerHTML = '';
  for (const [k, v, cls] of pairs) {
    const dt = document.createElement('dt'); dt.textContent = k;
    const dd = document.createElement('dd'); dd.textContent = v; if (cls) dd.className = cls;
    dl.append(dt, dd);
  }
}

function renderObs() {
  const st = status && status.obs_state;
  const dl = $('#obs-kv');
  $('#obs-kv-empty').hidden = !!st;
  $('#obs-state-line').textContent = st ? `OBS ${st.obs_version}, obs-websocket ${st.ws_version}` : 'read over the WebSocket once OBS is connected';
  $$('#obs-change-panel .srow').forEach(row => {
    const ctl = $(CTL, row);
    ctl.disabled = !st;
    if (row.dataset.obs === 'video_kbit') {
      ctl.disabled = !st || st.advanced || !!st.quality;
      const why = $('#obs-bitrate-why');
      why.hidden = !ctl.disabled || !st;
      why.textContent = !st ? '' : st.advanced ? 'the bitrate sits in the encoder settings in OBS, under Settings, Output, Recording' : st.quality ? `the recording quality is "${st.quality}" in OBS; a bitrate applies to "same as stream" only` : '';
    }
  });
  $('#obs-browse-dir').disabled = !st;
  if (!st) { dl.innerHTML = ''; return; }
  const kbit = st.video_kbit != null ? `${st.video_kbit} kbit/s` : st.quality ? `quality ${st.quality}` : 'in the encoder settings (advanced output mode)';
  kv(dl, [
    ['Output mode', st.advanced ? 'advanced' : 'simple'],
    ['Recording folder', st.record_dir],
    ['Format', st.format_name || st.format],
    ['Encoder', st.encoder_name || st.encoder],
    ['Video', kbit],
    ['Audio', st.audio_kbit != null ? `${st.audio_kbit} kbit/s, track${st.audio_tracks.length > 1 ? 's' : ''} ${st.audio_tracks.join(', ')}` : `track${st.audio_tracks.length > 1 ? 's' : ''} ${st.audio_tracks.join(', ')}`],
    ['Replay buffer', `${st.replay_enabled ? 'enabled' : 'disabled'}, ${st.replay_seconds} s, ${st.replay_active ? 'running' : 'stopped'}`, st.replay_active ? 'st-ok' : 'st-warn'],
    ['Recording', st.recording ? 'recording now' : 'not recording', st.recording ? 'st-active' : ''],
    ['Video size', `${st.base[0]} \u00d7 ${st.base[1]} canvas, ${st.output[0]} \u00d7 ${st.output[1]} output, ${Math.round(st.fps * 100) / 100} fps`],
  ]);
  $('#obs-bitrate-help').textContent = 'simple output mode with the recording quality "same as stream"';
  for (const row of $$('#obs-change-panel .srow')) {
    const ctl = $(CTL, row);
    if (document.activeElement === ctl) continue;
    const v = st[row.dataset.obs];
    ctl.value = v == null ? '' : v;
  }
}

async function applyObs(row, change) {
  const err = $('.err', row);
  try {
    const r = await invoke('obs_apply', { change });
    row.classList.remove('refused');
    err.hidden = true;
    toast(r.lines.join('; ') || 'nothing to change', 'ok');
  } catch (e) {
    row.classList.add('refused');
    const text = String(e.message || e);
    err.textContent = text;
    err.hidden = false;
    if (text.endsWith('Save anyway?')) {
      const b = document.createElement('button'); b.className = 'small again'; b.textContent = 'Save anyway';
      b.onclick = () => save(row, true);
      err.append(' ', b);
    }
  }
}

function bindObs() {
  for (const row of $$('#obs-change-panel .srow')) {
    const ctl = $(CTL, row);
    const key = row.dataset.obs;
    const fire = () => {
      const raw = ctl.value.trim();
      if (raw === '') return;
      const value = ctl.type === 'number' ? Math.round(+raw) : raw;
      applyObs(row, { [key]: value });
    };
    ctl.addEventListener('change', fire);
    if (ctl.tagName === 'INPUT' && ctl.type !== 'number') ctl.addEventListener('keydown', (e) => { if (e.key === 'Enter') fire(); });
  }
  $('#obs-browse-dir').onclick = async () => {
    try {
      const picked = await invoke('pick_folder');
      if (picked) { $('.srow[data-obs="record_dir"] input').value = picked; applyObs($('.srow[data-obs="record_dir"]'), { record_dir: picked }); }
    } catch (e) { toast(String(e.message || e), 'err'); }
  };
  $('#obs-refresh').onclick = async () => {
    try { await invoke('obs_state_get', { refresh: true }); } catch (e) { toast(String(e.message || e), 'err'); }
  };
}

function renderLocal() {
  const standalone = status.mode === 'standalone';
  for (const id of ['#local-panel', '#library-panel', '#cap-panel', '#encoder-panel']) off($(id), !standalone);
  renderLibrary(status);
  $('#mode-line').textContent = standalone ? (status.local ? 'reel runs on this PC' : status.local_line || 'standalone') : 'connected to a server';
  if (!standalone) { if (STANDALONE_ONLY.includes(current)) window.openSection?.('status'); return; }
  $('#local-line').textContent = status.local_line || '';
  const l = status.local;
  $('#encoder-line').textContent = l ? `in use: ${HW_WORD[l.backend] || l.backend}` : '';
  const dl = $('#local-kv');
  if (!l) { dl.innerHTML = ''; $('#local-ffmpeg').textContent = ''; return; }
  kv(dl, [
    ['Library', l.library_dir],
    ['reel', l.url],
    ['Encoder', HW_WORD[l.backend] || l.backend],
    ['Doctor', (l.notes || []).join('; ') || 'nothing to note'],
    ['ffmpeg', `${l.ffmpeg_version || 'not found'}`],
  ]);
  const f = l.ffmpeg || {};
  $('#local-ffmpeg').textContent = f.name ? `${f.name}, ${f.license}; source ${f.source}; build ${f.build_page}; the licence text is ${f.license_file} next to the app` : '';
}

// Only an encoder whose last test here recorded in OBS can be picked; x264 always can, and the
// one in use stays as it is.
function renderEncoderChoices(s) {
  const row = $('.srow[data-key="obs.setup.encoder"]');
  const help = $('.shelp', row);
  if (help.dataset.base == null) help.dataset.base = help.textContent;
  const known = new Map((status.obs_encoders || []).map((v) => [v.id, v]));
  const not = [];
  for (const o of $('select', row).options) {
    if (o.dataset.word == null) o.dataset.word = o.textContent;
    const v = known.get(o.value);
    const ok = o.value === 'obs_x264' || o.value === s.encoder || !!(v && v.ok);
    o.disabled = !ok;
    o.textContent = ok ? o.dataset.word : `${o.dataset.word} (${v ? 'not here' : 'not tested'})`;
    if (!ok) not.push(`${o.dataset.word}: ${v ? v.why : 'not tested on this PC yet'}`);
  }
  help.textContent = not.length ? `${help.dataset.base}. Not offered: ${not.join('; ')}` : help.dataset.base;
}

let ownMicApp = null;
function renderChoicesApp() {
  if (!settings || !status) return;
  const s = settings.obs.setup;
  syncCustom($('.srow[data-key="obs.setup.quality"] select'), s);
  ensureOption($('.srow[data-key="obs.setup.encoder"] select'), s.encoder);
  renderEncoderChoices(s);
  ensureOption($('.srow[data-key="obs.setup.format"] select'), s.format);
  for (const [k, v] of [['obs.setup.quality', s.quality], ['obs.setup.encoder', s.encoder || 'obs_x264'], ['obs.setup.format', s.format]]) {
    const sel = $(`.srow[data-key="${k}"] select`);
    if (sel !== document.activeElement && sel.value !== v) sel.value = v;
  }
}

let appDevices, appPrograms;
async function renderLevelsApp() {
  if (!settings || !status) return;
  const s = settings.obs.setup;
  const p = await audioPlan(s);
  renderLevelRows($('#levels-app'), p.sources, s.volumes, levelApp);
  wordsInto($('#audio-words-app'), p.words);
  off($('#route-app-row'), false);
  off($('#game-app-row'), !p.apart);
  off($('#discord-app-row'), !p.apart);
  setOn($('#mic-app-on'), !!s.mic);
  setOn($('#game-app-on'), !!s.game_audio);
  if (appDevices === undefined) appDevices = await invoke('audio_devices').catch(() => null);
  const pick = (sel, items, value) => {
    if (sel === document.activeElement) return;
    sel.replaceChildren(...items.map((it) => { const o = document.createElement('option'); o.value = it.value; o.textContent = it.name; return o; }));
    if (value && ![...sel.options].some((o) => o.value === value)) sel.append(new Option(value, value));
    sel.value = value || (items[0] || {}).value || '';
  };
  const mic = $('#mic-app');
  let inputs = (appDevices && appDevices.inputs) || [];
  if (!appDevices && status.obs && status.obs.connected) inputs = await invoke('obs_items', { input: 'Microphone', property: 'device_id' }).catch(() => []);
  const byDefault = { value: 'default', name: appDevices ? 'Default, as Windows sets it' : 'Default' };
  pick(mic, [byDefault, ...inputs.filter((d) => d.value !== 'default')], s.mic || 'default');
  mic.hidden = !s.mic;
  const game = $('#game-app');
  if (s.game_audio && appPrograms === undefined && p.apart) appPrograms = await invoke('audio_programs').catch(() => []);
  const games = [...(s.game && !status.obs_linux ? [{ value: 'front', name: 'The game in front' }] : []), ...(appPrograms || [])];
  pick(game, games, s.game_audio);
  game.hidden = !s.game_audio;
}

function levelApp(name, db, done) {
  invoke('obs_volume', { input: name, db }).catch(() => {});
  if (done) saveLevel(name, db);
}

async function saveSetup(change, err) {
  const next = JSON.parse(JSON.stringify(settings));
  change(next.obs.setup);
  try { settings = await invoke('settings_save', { settings: next, base: settings, force: false }); err.hidden = true; }
  catch (e) { err.textContent = String(e.message || e); err.hidden = false; }
  $$('.srow[data-key]').forEach(fill);
  renderBundled();
}

async function saveLevel(name, db) {
  const next = JSON.parse(JSON.stringify(settings));
  next.obs.setup.volumes = { ...(next.obs.setup.volumes || {}), [name]: db };
  try { settings = await invoke('settings_save', { settings: next, base: settings, force: false }); }
  catch (e) { toast(String(e.message || e), 'err'); }
}

let memoryMbApp;
async function renderLengthApp() {
  if (!settings || !status || !status.bundled) return;
  const s = settings.obs.setup;
  let v = null;
  try { v = await invoke('obs_video', { setup: s }); } catch (e) { return; }
  if (!v) return;
  if (memoryMbApp === undefined) memoryMbApp = await invoke('system_memory').catch(() => null);
  const gb = (mb) => (mb >= 1000 ? `${(mb / 1024).toLocaleString('en', { maximumFractionDigits: 1 })} GB` : `${Math.round(mb)} MB`);
  const words = [`what the replay buffer keeps before a save, up to 30 minutes; about ${gb(v.buffer_mb)} of memory now`];
  if (memoryMbApp && v.buffer_mb > memoryMbApp / 4) words.push(`more than a quarter of this PC's ${gb(memoryMbApp)}`);
  const limit = status.server_upload_max_bytes;
  if (limit && settings.mode !== 'standalone' && (v.buffer_mb / 1.25) * 1048576 > limit) words.push(`a clip is about ${gb(v.buffer_mb / 1.25)}, more than the ${gb(limit / 1048576)} your server takes`);
  const help = $('#length-app-help');
  help.textContent = words.join('; ');
  help.classList.toggle('st-warn', words.length > 1);
}

function renderNoiseApp() {
  if (!settings || !status) return;
  const s = settings.obs.setup, mic = !!s.mic;
  off($('#noise-app-row'), !mic);
  off($('#gate-app-row'), !mic);
  off($('#own-mic-app-row'), !mic || !ownMicApp || s.noise === 'own');
  noiseOptions($('#noise-app-row select'), s, status);
  $('#noise-app-help').textContent = noiseHelp(s, status);
  if (ownMicApp) $('#own-mic-app-help').textContent = ownMicWords(ownMicApp);
}

function renderCopyApp() {
  const c = status.obs_copy, s = settings && settings.obs.setup;
  const elevated = !!(s && s.elevated && status.obs_box);
  $('#setup-app-desc').textContent = elevated
    ? 'each change is written into the copy of OBS in Program Files and OBS starts again with it; Windows asks once for each change'
    : 'each change is written into OBS\'s settings and OBS starts again with it, a few seconds';
  off($('#copy-app-row'), !c);
  if (!c) return;
  const btn = $('#copy-app-put');
  let help;
  if (c.left) {
    help = `a copy of OBS with the rights is still in ${c.folder}, and its task would start it at sign-in, though Capture games that run as administrator is off. Remove it; Windows asks once`;
    btn.textContent = 'Remove it';
  } else if (!c.there) {
    help = status.obs_box && status.obs_box.without_rights
      ? `OBS for games that run as administrator runs from a copy in ${c.folder} that only an administrator can change, and the copy is not there: OBS runs without the highest rights, so games that run as administrator are not captured. Windows asks once`
      : `OBS for games that run as administrator now runs from a copy in ${c.folder} that only an administrator can change; until it is in place, OBS runs as before. Windows asks once`;
    btn.textContent = 'Put it in place';
  } else if (c.named_before) {
    help = `the copy in ${c.folder} still runs OBS as obs64.exe, the name Discord turns its Streamer Mode on for; reel's OBS now runs as reel-capture.exe. Windows asks once to update it`;
    btn.textContent = 'Update the copy';
  } else if (c.stale) {
    help = `this version of reel brought OBS ${c.wanted}; the copy in ${c.folder} is still OBS ${c.version || 'of an earlier version'}. Windows asks once to update it`;
    btn.textContent = 'Update the copy';
  } else if (!c.task) {
    help = `the task that starts OBS with the rights is missing from the copy in ${c.folder}${status.obs_box && status.obs_box.without_rights ? ', so OBS runs without them and games that run as administrator are not captured' : ''}. Update the copy puts it back; Windows asks once`;
    btn.textContent = 'Update the copy';
  } else if (c.tray) {
    help = `OBS with the rights still shows its own tray icon: reel starts it hidden, without one, and opens and hides it with Open OBS. Update the copy sets the copy in ${c.folder} up for that; Windows asks once`;
    btn.textContent = 'Update the copy';
  } else {
    help = `OBS ${c.version} in ${c.folder}, which only an administrator can change; reel writes its settings there with Windows' Yes, and everyday changes (the levels, the filters) over the WebSocket`;
  }
  btn.hidden = !c.left && c.there && c.task && !c.stale && !c.tray;
  btn.dataset.remove = c.left ? '1' : '';
  $('#copy-app-help').textContent = help;
}

function renderBundled() {
  const box = status.obs_box, inUse = !!box;
  off($('#bundled-panel'), !status.bundled);
  $('#bundled-line').textContent = box ? box.line : (status.bundled ? 'not in use; the app connects to your own OBS' : '');
  renderObsBar(status);
  // Wayland lets no app show or hide another's window: on Linux OBS keeps its own tray icon.
  const lin = !!status.obs_linux;
  if (lin) {
    const enc = $('[data-key="obs.setup.encoder"] select');
    if (!enc.querySelector('option[value="ffmpeg_vaapi_tex"]')) {
      const was = enc.value;
      enc.querySelectorAll('option[value="h264_texture_amf"], option[value="obs_qsv11_v2"]').forEach((o) => o.remove());
      enc.querySelector('option[value="obs_x264"]').before(new Option('VAAPI H.264 (AMD, Intel)', 'ffmpeg_vaapi_tex'));
      enc.value = settings ? settings.obs.setup.encoder || was : was;
    }
    $('#bundled-title').textContent = 'reel\'s own OBS';
    $('#bundled-use-label').textContent = 'Run a second OBS of reel\'s';
    $('#bundled-use-help').textContent = `${status.obs_which.replace(/^t/, 'T')}, run a second time with settings of its own: started with the app, in its own tray icon, set up by reel; your own OBS and its settings stay as they are. Off, the app connects to your own OBS as below`;
    $('#game-app-help').textContent = status.obs_game_capture === false
      ? 'needs obs-vkcapture, which this OBS does not have; the screen holds the game meanwhile'
      : 'obs-vkcapture\'s capture of a Vulkan or OpenGL game started with obs-gamecapture, over the screen';
    $('#elevated-app-row').hidden = true;
  }
  $('#bundled-again').hidden = !inUse || box.state !== 'gave_up';
  const show = $('#bundled-show');
  show.hidden = !inUse || !box.can_show || lin;
  show.disabled = !box || !['running', 'silent'].includes(box.state);
  $('#bundled-hide').hidden = !inUse || !box.can_show || !box.shown || lin;
  $('#bundled-setup').hidden = !status.bundled || !settings || settings.obs.setup.done;
  renderChoicesApp();
  renderNoiseApp();
  renderLevelsApp();
  renderLengthApp();
  renderCopyApp();
  const doing = status.obs_step;
  for (const b of [$('#elevated-app-row .switch'), $('#copy-app-put'), $('#bundled-setup')]) b.disabled = !!doing;
  if (elevatedAsks > 0) $('#elevated-app-row .switch').disabled = true;
  syncElevated();
  const why = doing ? doing.line : status.obs_task_problem;
  $('#elevated-why').hidden = !why;
  $('#elevated-why').textContent = why || '';
  off($('#obs-ws-panel'), inUse);
  off($('#bundled-setup-panel'), !inUse);
  off($('#obs-change-panel'), inUse);
  $('#obs-section-line').textContent = inUse
    ? (!box.can_show
      ? 'OBS runs as administrator: open it from its tray icon to change anything there; reel keeps working as long as its WebSocket and the replay buffer stay on.'
      : lin
        ? 'OBS\'s window opens from its own tray icon: change anything there; reel keeps working as long as its WebSocket and the replay buffer stay on.'
        : 'Change anything in OBS itself; reel keeps working as long as its WebSocket and the replay buffer stay on.')
    : 'OBS tells the app the moment a replay is saved (Tools, WebSocket Server Settings, enable the server). The folder watch below stays on as a safety net.';
}

async function doneInObs(r, always, quiet) {
  status = r.status;
  takeSettings(r.settings);
  if (!quiet && (always || (r.taken && r.taken.length))) toast(takenWords(r.taken), 'ok');
}

function takeSettings(next) {
  settings = next;
  const typing = document.activeElement && document.activeElement.matches('input, select, textarea');
  $$('.srow[data-key]')
    .filter((row) => !(typing && row.contains(document.activeElement)))
    .filter((row) => !(elevatedAsks > 0 && row.id === 'elevated-app-row'))
    .forEach(fill);
  renderStatus();
}

function renderStatus() {
  if (!status) return;
  renderParts(status);
  renderMode();
  const here = status.mode === 'standalone';
  renderObs();
  renderLocal();
  $('#clips-dir-help').textContent = status.watcher.from_obs
    ? `OBS says ${status.watcher.dir}; this folder is watched when OBS is not connected or Follow OBS is off`
    : 'watched while OBS is not connected or Follow OBS is off; a file counts once its size holds still and it can be opened';
  const lib = status.local;
  kv($('#conn'), [
    ...(here ? [['Library', lib ? `on this PC, reel runs; in ${lib.library_dir}` : (status.local_line || 'reel on this PC is not running'), lib ? 'st-ok' : /did not start/.test(status.local_line || '') ? 'st-err' : 'st-active']] : []),
    ...(here && status.paired ? [['Server', `${status.server_name || status.server_url} as ${status.user}, for Upload to the server`, 'st-ok']] : []),
    ...(here ? [] : [['Server', status.paired ? `${status.server_name || status.server_url} as ${status.user}, device ${status.device_name}` : (status.server_url ? 'not paired yet' : 'no server set'), status.paired ? 'st-ok' : 'st-warn']]),
    ['OBS', status.obs.line, status.obs.connected ? 'st-ok' : 'st-neutral'],
    ...(status.obs_checks || []).filter((c) => c.name !== "OBS's WebSocket").map((c) => [c.name, c.line, c.ok ? 'st-ok' : 'st-warn']),
    ['Folder', status.watcher.line, status.watcher.ok ? 'st-ok' : 'st-err'],
    ['Hotkey', status.hotkey_line, status.hotkey_ok ? 'st-ok' : 'st-err'],
    ...(status.tray_line ? [['Tray', status.tray_line, 'st-warn']] : []),
  ]);
  off($('#obs-checks-panel'), !(status.obs_checks || []).length);
  kv($('#obs-checks'), (status.obs_checks || []).map((c) => [c.name, c.line, c.ok ? 'st-ok' : 'st-warn']));
  if (status.hotkeys_how) $('#hotkeys-section-line').textContent = status.hotkeys_how;
  const q = $('#queue tbody');
  q.innerHTML = '';
  $('#queue-empty').hidden = status.queue.length > 0;
  $('#queue-line').textContent = status.queue.length ? `${status.queue.length} waiting` : '';
  for (const item of status.queue) {
    const tr = document.createElement('tr');
    const pct = item.total ? Math.round(item.sent / item.total * 100) : 0;
    const stateCell = item.state === 'uploading'
      ? `<span class="bar"><i style="width:${pct}%"></i></span><span class="st-active">${pct}%</span>`
      : item.state === 'failed' ? `<span class="st-err">failed</span> <span class="faint small">${esc(item.error)}</span>`
      : item.state === 'paused' ? `<span class="st-warn">paused</span>`
      : `<span class="st-neutral">${esc(item.state)}</span>`;
    tr.innerHTML = `<td class="clip"></td><td class="sub">${stateCell}</td><td class="right acts"></td>`;
    tr.children[0].textContent = item.name;
    const acts = tr.children[2];
    if (item.state === 'found' || item.state === 'failed') {
      const b = document.createElement('button'); b.className = 'small'; b.textContent = item.state === 'failed' ? 'Retry' : here ? 'Add now' : 'Upload now';
      b.onclick = () => invoke('upload_now', { path: item.path }).catch(e => toast(String(e.message || e), 'err'));
      acts.append(b);
    }
    const f = document.createElement('button'); f.className = 'small quiet'; f.textContent = 'Open folder';
    f.onclick = () => invoke('open_folder', { path: item.path }).catch(e => toast(String(e.message || e), 'err'));
    acts.append(f);
    q.append(tr);
  }
  const r = $('#recent tbody');
  r.innerHTML = '';
  $('#recent-empty').hidden = status.recent.length > 0;
  for (const c of status.recent) {
    const tr = document.createElement('tr');
    const word = c.phase ? `<span class="st-active">${esc(c.phase)}</span>`
      : c.state === 'confirmed' ? '<span class="st-ok">on the server</span>'
      : c.state === 'deleted' ? '<span class="st-neutral">on the server, local copy removed</span>'
      : c.state === 'failed' ? `<span class="st-err">failed</span>`
      : `<span class="st-neutral">${esc(c.state)}</span>`;
    tr.innerHTML = `<td class="clip"></td><td class="sub"></td><td class="sub">${word}</td><td class="right acts"></td>`;
    tr.children[0].textContent = c.title || c.name;
    tr.children[1].textContent = c.when;
    const acts = tr.children[3];
    if (c.local_clip_id && !c.clip_id && status.paired) {
      const u = document.createElement('button'); u.className = 'small'; u.textContent = 'Upload to the server';
      u.onclick = () => invoke('upload_local', { clipId: c.local_clip_id }).catch(e => toast(String(e.message || e), 'err'));
      acts.append(u);
    }
    if (c.local_clip_id && (here || !c.clip_id)) {
      const o = document.createElement('button'); o.className = 'small quiet'; o.textContent = 'Open';
      o.onclick = () => invoke('open_clip', { clipId: c.local_clip_id }).catch(e => toast(String(e.message || e), 'err'));
      acts.append(o);
    }
    if (c.clip_id && !here) {
      const b = document.createElement('button'); b.className = 'small'; b.textContent = 'Copy link';
      b.onclick = async () => { try { const d = await invoke('copy_link', { clipId: c.clip_id }); toast(d.line, 'ok'); } catch (e) { toast(String(e.message || e), 'err'); } };
      acts.append(b);
      const o = document.createElement('button'); o.className = 'small quiet'; o.textContent = 'Open';
      o.onclick = () => invoke('open_clip', { clipId: c.clip_id }).catch(e => toast(String(e.message || e), 'err'));
      acts.append(o);
    }
    if (c.path && c.state !== 'deleted') {
      const f = document.createElement('button'); f.className = 'small quiet'; f.textContent = 'Open folder';
      f.onclick = () => invoke('open_folder', { path: c.path }).catch(e => toast(String(e.message || e), 'err'));
      acts.append(f);
    }
    r.append(tr);
  }
  off($('#pair-row'), status.paired);
  off($('#unpair-row'), !status.paired);
  $('#pair-line').textContent = status.paired ? 'paired' : 'not paired';
  $('#paired-as').textContent = status.paired ? `${status.device_name}, as ${status.user} on ${status.server_name || status.server_url}` : '';
  $('#obs-line').textContent = status.obs.line;
  $('#watch-line').textContent = status.watcher.line;
  $('#hotkey-line').textContent = status.hotkey_line;
  kv($('#about'), [['Version', [status.version, status.versions?.kind].filter(Boolean).join(', ')], ...(status.bundled ? [['OBS', `OBS Studio ${status.obs_version}, included`]] : []), ['Settings', status.config_dir], ['Data', status.data_dir]]);
  $('#about-obs').hidden = !status.bundled;
  $('#uninstall').hidden = !status.uninstaller;
  $('#about-obs-version').textContent = status.obs_version;
  $('#about-obs-source').href = status.obs_source;
  renderBundled();
  renderSave();
  renderDiscord();
  $('#normalize-state span').textContent = here ? 'in the library on this PC' : status.normalize_new == null ? (status.paired ? 'not said by this server' : 'not paired')
    : status.normalize_new ? `on, to ${status.loudness_target} LUFS` : 'off';
  $('#normalize-open').hidden = here || !status.server_admin;
  const v = status.versions;
  if (v) kv($('#versions-kv'), [
    ['This app', `${v.app}, ${v.kind}${v.obs ? ` (OBS ${v.obs})` : ''}`],
    ...(here ? [] : [['Server', v.server || 'not known yet']]),
    ['Newest release', v.newest || 'not known yet'],
    ['Updates', [status.updated_from ? `updated from ${status.updated_from}` : '', status.update_line].filter(Boolean).join('; ')],
  ]);
  $('#versions-help').textContent = !v ? '' : here ? `this app ${v.app}, ${v.kind}${v.obs ? ` (OBS ${v.obs})` : ''}; the newest release ${v.newest || 'not known yet'}`
    : `this app ${v.app}, ${v.kind}${v.obs ? ` (OBS ${v.obs})` : ''}; ${v.server ? `its server ${v.server}` : 'no server known yet'}${v.newest ? `; the newest release ${v.newest}` : ''}`;
  const asked = status.update_checked_at ? new Date(status.update_checked_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : '';
  $('#update-help').textContent = [status.updated_from ? `updated from ${status.updated_from}` : '', status.update_line, asked && !here ? `the server asked at ${asked}` : ''].filter(Boolean).join('; ');
  $('#update-before').textContent = status.update_before || '';
  $('#update-before').hidden = !status.update_before;
  $('#update-notes-body').hidden = !(status.update_version && status.update_notes);
  $('#update-notes-version').textContent = status.update_version || '';
  $('#update-notes').textContent = plainNotes(status.update_notes || '');
  $('#update-install').hidden = !status.update_version;
  $('#update-install').textContent = status.update_version ? `Install reel ${status.update_version}` : 'Install';
  $('#update-releases').hidden = !here || !!status.update_version;
}

// release-please's Markdown as plain text, a link reduced to its words.
function plainNotes(text) {
  return text.split('\n').map((l) => l.replace(/^#+\s*/, '').replace(/^\s*[*-]\s+/, '\u2022 ')
    .replace(/\*\*([^*]+)\*\*/g, '$1').replace(/\[([^\]]+)\]\([^)]*\)/g, '$1').replace(/\s*\(\s*\)/g, '')).join('\n').replace(/\n{3,}/g, '\n\n');
}

function renderDiscord() {
  $('#discord-note').textContent = status.discord_note || '';
  off($('#discord-notice'), !status.discord_note);
}

function renderSave() {
  saveButtons(status, $('#obs-save'), $('#obs-save-share'));
  const st = status.obs_state, a = status.save || { ok: false, why: '' }, b = status.save_share || a;
  $('#save-state').textContent = !a.ok ? `Not now: ${a.why}.`
    : `OBS's replay buffer runs and keeps the last ${st && st.replay_seconds ? `${st.replay_seconds} s` : 'seconds'}.${b.ok || status.mode === 'standalone' ? '' : ` Save and share not now: ${b.why}.`}`;
}

async function loadLog() {
  try {
    const lines = await invoke('log_tail', { lines: 300 });
    const box = $('#log-lines');
    box.textContent = lines.length ? lines.join('\n') : 'nothing yet';
    box.scrollTop = box.scrollHeight;
  } catch (e) { $('#log-lines').textContent = String(e.message || e); }
}

function bindButtons() {
  $('#obs-save').onclick = () => runSave('save_clip');
  $('#obs-save-share').onclick = () => runSave('save_and_share');
  $('#pair-btn').onclick = async () => {
    const row = $('.srow[data-key="server_url"]');
    const err = $('#pair-err');
    const url = $('input', row).value.trim();
    const code = $('#pair-code').value.trim();
    const name = $('.srow[data-key="device_name"] input').value.trim();
    $('#pair-btn').disabled = true;
    try {
      status = await invoke('pair', { serverUrl: url, code, name });
      err.hidden = true;
      $('#pair-code').value = '';
      settings = await invoke('settings_get');
      $$('.srow[data-key]').forEach(fill);
      renderStatus();
      if ((status.obs_checks || []).some((c) => !c.ok)) {
        window.openSection('obs');
        toast(`Paired with ${status.server_name || url}. Next, OBS: what reel needs from it is at the top.`, 'ok');
      } else {
        toast(`Paired with ${status.server_name || url}.`, 'ok');
      }
    } catch (e) { err.textContent = String(e.message || e); err.hidden = false; }
    $('#pair-btn').disabled = false;
  };
  $('#unpair-btn').onclick = async () => {
    try { status = await invoke('unpair'); renderStatus(); toast('Unpaired. The server keeps the device until you revoke it there.', 'ok'); }
    catch (e) { toast(String(e.message || e), 'err'); }
  };
  $('#setup-again').onclick = () => invoke('view_show', { view: 'setup', section: null }).catch((e) => toast(String(e.message || e), 'err'));
  $('#bundled-setup').onclick = $('#setup-again').onclick;
  $('#mic-app-on').addEventListener('change', () => {
    const on = isOn($('#mic-app-on'));
    saveSetup((s) => { s.mic = on ? ($('#mic-app').value || 'default') : null; }, $('#mic-app-err'));
  });
  $('#mic-app').onchange = () => saveSetup((s) => { s.mic = $('#mic-app').value || 'default'; }, $('#mic-app-err'));
  $('#game-app-on').addEventListener('change', async () => {
    const on = isOn($('#game-app-on'));
    if (on && appPrograms === undefined) appPrograms = await invoke('audio_programs').catch(() => []);
    const first = settings.obs.setup.game ? 'front' : ((appPrograms || [])[0] || {}).value;
    if (on && !first) { setOn($('#game-app-on'), false); toast('No program with a window is open, and the game capture is off: open the game first.', 'err'); return; }
    saveSetup((s) => { s.game_audio = on ? first : ''; }, $('#game-app-err'));
  });
  $('#game-app').onchange = () => saveSetup((s) => { s.game_audio = $('#game-app').value; }, $('#game-app-err'));
  $('#own-mic-app-use').onclick = async () => {
    if (!ownMicApp) return;
    const next = JSON.parse(JSON.stringify(settings));
    Object.assign(next.obs.setup, { own_filters: ownMicApp.filters, own_filters_from: `"${ownMicApp.name}" in your own OBS`, noise: 'own' });
    try {
      settings = await invoke('settings_save', { settings: next, base: settings, force: false });
      $$('.srow[data-key]').forEach(fill);
      renderNoiseApp();
    } catch (e) { toast(String(e.message || e), 'err'); }
  };
  $('#bundled-show').onclick = async () => {
    try { status = await invoke('obs_show', { shown: true }); renderStatus(); }
    catch (e) { toast(String(e.message || e), 'err'); }
  };
  $('#bundled-again').onclick = async () => {
    try { status = await invoke('obs_again'); renderStatus(); }
    catch (e) { toast(String(e.message || e), 'err'); }
  };
  bindObsBar((r) => doneInObs(r, false));
  $('#copy-app-put').onclick = async () => {
    const b = $('#copy-app-put'), err = $('#copy-app-err');
    b.disabled = true;
    const remove = !!b.dataset.remove;
    try {
      status = await invoke(remove ? 'obs_copy_remove' : 'obs_copy_again');
      err.hidden = true;
      renderStatus();
      toast(remove ? 'The copy of OBS left in Program Files is removed.' : 'The copy of OBS in Program Files is in place.', 'ok');
    }
    catch (e) { err.textContent = String(e.message || e); err.hidden = false; }
    b.disabled = !!(status && status.obs_step);
  };
  $('#uninstall').onclick = () => invoke('uninstall').catch(e => toast(String(e.message || e), 'err'));
  $('#about-obs-source').onclick = (e) => {
    e.preventDefault();
    const open = window.__TAURI__ && window.__TAURI__.opener && window.__TAURI__.opener.openUrl;
    if (open && status) open(status.obs_source).catch(() => {});
  };
  $('#obs-test').onclick = async () => {
    const line = $('#obs-test-line'), box = $('#obs-test-result'), main = $('#obs-test-main'), detail = $('#obs-test-detail');
    line.className = ''; line.textContent = 'connecting'; box.hidden = true;
    const show = (ok, text, more) => {
      line.className = ok ? 'st-ok' : 'st-err'; line.textContent = ok ? 'ok' : 'failed';
      main.className = ok ? 'st-ok' : 'st-err'; main.textContent = text;
      detail.textContent = more || ''; detail.hidden = !more;
      box.hidden = false;
    };
    try {
      const r = await invoke('obs_test', {
        host: $('.srow[data-key="obs.host"] input').value.trim(),
        port: +$('.srow[data-key="obs.port"] input').value,
        password: $('.srow[data-key="obs.password"] input').value,
      });
      show(r.ok, r.line, r.detail);
    } catch (e) { show(false, String(e.message || e), ''); }
  };
  $('#normalize-open').onclick = () => invoke('open_page', { path: '/settings/ingest' }).catch(e => toast(String(e.message || e), 'err'));
  $('#normalize-open-here').onclick = () => invoke('open_page', { path: '/settings/ingest' }).catch(e => toast(String(e.message || e), 'err'));
  $('#versions-updates').onclick = () => window.openSection?.('startup');
  $('#update-check').onclick = async () => {
    try { status = await invoke('update_check'); renderStatus(); toast(status.update_line, 'ok'); } catch (e) { toast(String(e.message || e), 'err'); }
  };
  $('#update-install').onclick = () => invoke('update_install').catch(e => toast(String(e.message || e), 'err'));
  $('#update-releases').onclick = () => invoke('open_releases').catch(e => toast(String(e.message || e), 'err'));
  $('#log-refresh').onclick = loadLog;
  $('#log-folder').onclick = () => invoke('open_log_folder').catch(e => toast(String(e.message || e), 'err'));
}

async function reshape() {
  const key = status ? `${status.mode}|${status.server_url}|${status.paired}|${!!status.offline}` : '';
  if (key === shapeKey) return;
  shapeKey = key;
  try {
    const ps = await invoke('page_shape_get');
    applyShape(ps.shape, ps.base);
    sectionsNav(ps.shape, current, status?.mode === 'standalone');
    if (status) renderParts(status);
  } catch (e) { /* the shell is gone; the page stays as it is */ }
}

async function main() {
  const shaped = mount('settings');
  sections();
  bindButtons();
  bindRows();
  bindHotkeys();
  bindObs();
  bindLibrary();
  try {
    settings = await invoke('settings_get');
    ownMicApp = await invoke('obs_own_mic').catch(() => null);
    $$('.srow[data-key]').forEach(fill);
    capAsk();
    off($('#elevated-start-row'), !settings.startup.elevated);
    status = await invoke('status_get');
    shapeKey = `${status.mode}|${status.server_url}|${status.paired}|${!!status.offline}`;
    renderStatus();
    if (status.mode === 'standalone') refreshLibrary();
  } catch (e) {
    toast(`no shell: ${String(e.message || e)}`, 'err');
  }
  sectionsNav(await shaped, current, status?.mode === 'standalone');
  if (status) renderParts(status);
  await listen('status', (ev) => { status = ev.payload; renderStatus(); reshape(); if (Date.now() - capAt > 5000) capAsk(); });
  await listen('open-section', (ev) => { if (window.openSection) window.openSection(ev.payload); });
  await listen('settings', (ev) => takeSettings(ev.payload));
  await listen('obs-meters', (ev) => {
    for (const [name, db] of ev.payload || []) meterIn($('#levels-app'), name, db);
  });
  setInterval(() => { if (current === 'log') loadLog(); }, 5000);
}

main();
