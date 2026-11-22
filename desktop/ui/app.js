// The app's own Settings section: every row saves as it changes.
import { $, $$, APP_SECTIONS, STANDALONE_ONLY, applyShape, goServer, invoke, listen, mount, renderParts, runSave, saveButtons, serverHref, toast } from './shell.js';
import { NAV, unsortedCount } from './nav.js';
import { isOn, setOn } from './assets/js/switch.js';
import { bindTips, enhance, tipHit, tipHitsOff } from './assets/js/tip.js';
import { keycaps, recorder, recordersOffOnLeave } from './hotkeys.js';
import { noiseHelp, noiseOptions, ownMicWords } from './noise.js';
import { ensureOption, takenWords } from './inobs.js';
import { bindObsBar, renderObsBar } from './obsbar.js';
import { bindAsk, renderAsk } from './ask.js';
import { audioPlan, meterIn, renderLevelRows, wordsInto } from './sources.js';
import { bindLibrary, refreshLibrary, renderLibrary } from './library.js';
import { bindGames, opened as gamesOpened, refresh as gamesRefresh } from './games.js';
import { renderPresence } from './presence.js';

const CTL = 'input, select, button[role="switch"]';

let settings = null;
const HW_WORD = { none: 'x264 on the CPU', nvenc: 'NVIDIA NVENC', amf: 'AMD AMF', qsv: 'Intel Quick Sync', vaapi: 'VAAPI' };
let status = null;
let shapeKey = '';
let shape = null;
// The clip length's stops; the box beside them takes any length.
const STOPS = [15, 30, 60, 120, 300, 600];
const QUALITY_WORD = { light: 'Light', smooth: 'Smooth', sharp: 'Sharp', native: 'Native', custom: 'as set in OBS' };

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
  if (kind === 'cards') { pick(row, v); return; }
  if (kind === 'length') { if (!row.dataset.own) showLength(v); return; }
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
  if (kind === 'cards') return $('[role="radio"].on', row)?.dataset.value ?? get(settings, key);
  if (kind === 'length') return Math.round(+$('#length-n').value);
  const ctl = $(CTL, row);
  if (kind === 'bool') return isOn(ctl);
  if (kind === 'volume') return Math.max(0, Math.min(100, +ctl.value)) / 100;
  if (kind === 'int') return Math.round(+ctl.value);
  if (kind === 'list') return ctl.value.split(',').map(s => s.trim().toLowerCase().replace(/^\./, '')).filter(Boolean);
  if (kind === 'file') return ctl.value.trim() === '' ? null : ctl.value.trim();
  return ctl.value;
}

function pick(row, value) {
  for (const b of $$('[role="radio"]', row)) {
    const on = b.dataset.value === String(value);
    b.classList.toggle('on', on);
    b.setAttribute('aria-checked', String(on));
    b.tabIndex = on || (!$('[role="radio"][data-value="' + value + '"]', row) && b === $('[role="radio"]', row)) ? 0 : -1;
  }
}

const stopAt = (secs) => Math.max(0, STOPS.reduce((at, x, i) => (secs >= x ? i : at), 0));
// Not while it is being changed: the next save says the length.
function showLength(secs) {
  const n = $('#length-n'), range = $('#length-stops');
  if (document.activeElement === n || document.activeElement === range) return;
  n.value = secs ?? '';
  range.value = stopAt(+secs || 0);
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
  $$(`.srow[data-key="${key}"]`).forEach(fill);
  if (held) {
    elevatedAsks--;
    syncElevated();
    held.disabled = !!(status && status.obs_step) || elevatedAsks > 0;
    if (!answer || get(answer, key) !== get(next, key)) { if (status) renderBundled(); return; }
  }
  if (!answer) return;
  const ctl = $('.sctl', row) || row;
  let ok = $('.ok', ctl);
  if (!ok) { ok = document.createElement('span'); ok.className = 'ok'; ctl.append(ok); }
  ok.textContent = 'saved';
  setTimeout(() => ok.remove(), 1800);
  depends();
  if (key === 'games.steam' || key === 'upload.wait_for_games' || key.startsWith('session.') || key === 'obs.replay_buffer') gamesRefresh(true);
  if (key === 'obs.replay_buffer') await bufferForGames();
  if (key.startsWith('session.')) renderRecordNow();
  if (key.startsWith('obs.setup.') && status) renderBundled();
  if (key.startsWith('session.') || key.startsWith('hotkeys.') || key.startsWith('obs.')) renderModes();
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
  for (const row of $$('.srow[data-kind="cards"]')) {
    for (const b of $$('[role="radio"]', row)) {
      b.addEventListener('click', () => {
        if (b.classList.contains('on')) return;
        pick(row, b.dataset.value);
        save(row);
      });
      b.addEventListener('keydown', (e) => {
        const all = $$('[role="radio"]', row).filter((x) => !x.hidden);
        const step = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[e.key];
        if (!step) return;
        e.preventDefault();
        const next = all[(all.indexOf(b) + step + all.length) % all.length];
        next.focus();
        next.click();
      });
    }
  }
  const range = $('#length-stops'), n = $('#length-n');
  range.addEventListener('input', () => { n.value = STOPS[+range.value]; renderCosts(); });
  range.addEventListener('change', saveLength);
  n.addEventListener('change', () => { range.value = stopAt(+n.value); saveLength(); });
  for (const row of $$('.srow[data-key]')) {
    if (row.dataset.kind === 'cards' || row.dataset.kind === 'length') continue;
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

// Reel's own OBS keeps the setup's length; an OBS of one's own is told its replay length.
async function saveLength() {
  const row = $('#length-row');
  const secs = Math.round(+$('#length-n').value);
  if (!secs) return;
  if (row.dataset.own) { await applyObs(row, { replay_seconds: secs }); return; }
  await save(row);
  renderCosts();
}

function bindHotkeys() {
  for (const row of $$('.srow[data-kind="hotkey"]')) recorder(row, { save, fill });
  recordersOffOnLeave();
}

let current = 'first';
const PAGE = Object.fromEntries(APP_SECTIONS);

// Where each entry of the history was left, by an id in its state, in the session's storage that
// outlives this page while the window shows a server page: Back and Forward come back there.
const PLACES = 'reel-places';
const places = () => { try { return JSON.parse(sessionStorage.getItem(PLACES)) || {}; } catch { return {}; } };
function entry() {
  let k = history.state && history.state.place;
  if (!k) {
    k = `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
    history.replaceState({ ...(history.state || {}), place: k }, '');
  }
  return k;
}
function keep(k, y) {
  const all = places();
  delete all[k];
  all[k] = y;
  const keys = Object.keys(all);
  for (const x of keys.slice(0, Math.max(0, keys.length - 50))) delete all[x];
  try { sessionStorage.setItem(PLACES, JSON.stringify(all)); } catch { /* the place is not kept */ }
}
let pending = null;
function watchPlace() {
  const flush = () => { if (pending) keep(pending.k, pending.y); pending = null; };
  addEventListener('scroll', () => {
    const k = entry();
    if (pending && pending.k !== k) flush();
    if (!pending) requestAnimationFrame(flush);
    pending = { k, y: Math.round(scrollY) };
  }, { passive: true });
  addEventListener('pagehide', flush);
}
// The page may still be filling in (Games and apps' list, the log): try until it is long enough,
// for two seconds at most, or until the wheel, a key or a touch takes over.
let wantY = null, wantUntil = 0;
function placeAt(y) {
  wantY = y || 0;
  wantUntil = Date.now() + 2000;
  const step = () => {
    if (wantY === null) return;
    window.scrollTo(0, wantY);
    if (Math.abs(scrollY - wantY) < 2 || Date.now() > wantUntil) { wantY = null; return; }
    requestAnimationFrame(step);
  };
  step();
}
for (const ev of ['wheel', 'keydown', 'pointerdown', 'touchstart']) addEventListener(ev, () => { wantY = null; }, { passive: true, capture: true });
const placeOf = () => places()[entry()] || 0;
const cameBack = () => ['back_forward', 'reload'].includes(performance.getEntriesByType('navigation')[0]?.type) && !!(history.state && history.state.place);
// A page of the nav table: its kind (reel's or the app's) and when it shows.
const pageOf = (page) => {
  const [kind, id] = page.split(' ');
  return { kind, id, app: kind[0] === 'a', standalone: kind.includes('s'), paired: kind.includes('p') };
};
const shows = (p, here) => !(p.standalone && !here) && !(p.paired && here);

function sections() {
  history.scrollRestoration = 'manual';
  const root = $('#settings'), body = $('#sections-body');
  for (const g of $$('.group', body)) {
    const h = document.createElement('h2');
    h.className = 'hit-page';
    h.hidden = true;
    h.textContent = PAGE[g.dataset.appSection];
    g.prepend(h);
  }
  enhance(body);
  // on Linux, Essentials' Save a clip keeps the note's warning in sight and its how in the tip
  const saveTip = $('#g-first .srow[data-key="hotkeys.save"] .tip');
  if (saveTip) {
    const how = document.createElement('span');
    how.className = 'tip-extra';
    how.id = 'hotkeys-how-tip';
    how.hidden = true;
    saveTip.insertBefore(how, $('.tip-more', saveTip));
  }
  bindTips();
  const known = (id) => id in PAGE;
  const open = (want, push, chosen = true, y = 0) => {
    // a program's page under Games and apps is games/<its id>; Status at its log is status/log
    let [id, sub] = String(want || 'first').split('/');
    if (!known(id)) { id = 'first'; sub = undefined; }
    const here = status && status.mode === 'standalone';
    if (status && !here && STANDALONE_ONLY.includes(id)) { id = 'first'; sub = undefined; }
    if (here && id === 'upload') id = 'clips';
    current = id;
    if (id === 'library') capAsk();
    root.dataset.section = id;
    $$('.group', body).forEach(g => g.hidden = g.dataset.appSection !== id);
    $$('.section-line, .page-top, .panel, .srow, .q-h, #summary', body).forEach(el => el.hidden = el.hasAttribute('data-off'));
    $$('.hit-page', body).forEach(h => h.hidden = true);
    tipHitsOff(body);
    $('#no-match').hidden = true;
    markNav();
    if (push) history.pushState(null, '', `#${sub ? `${id}/${sub}` : id}`);
    placeAt(y);
    gamesOpened(id === 'games', id === 'games' ? sub : undefined);
    if (id === 'session') renderRecordNow();
    if (id === 'first') { renderBufferNote(); renderCosts(); }
    // Status at its log: once the lines are in, so the page is long enough to get there
    if (id === 'status') loadLog(true).then(() => { if (sub === 'log' && !y && current === 'status') $('#log-panel')?.scrollIntoView(); });
    invoke('obs_meters', { on: id === 'capture' }).catch(() => {});
    if (id === 'capture' && settings && settings.obs.bundled) invoke('obs_read').then((r) => doneInObs(r, false, true)).catch(() => {});
    invoke('page_section', { section: id, chosen }).catch(() => {});
  };
  document.addEventListener('click', (e) => {
    const a = e.target.closest('a[data-app-section]');
    if (!a) return;
    e.preventDefault();
    open(a.dataset.appSection, true);
  });
  window.addEventListener('popstate', () => open(location.hash.replace('#', '') || 'first', false, true, placeOf()));
  open(location.hash.replace('#', '') || 'first', false, false, cameBack() ? placeOf() : 0);
  watchPlace();
  // out of the back/forward cache, whose scroll the webview does not put back under 'manual'
  addEventListener('pageshow', (e) => { if (e.persisted) placeAt(placeOf()); });
  window.openSection = (id) => open(id, true);
  $('#app-select').addEventListener('change', (e) => {
    const [kind, id] = e.target.value.split(':');
    if (kind === 'app') open(id, true);
    else { e.target.value = `app:${current}`; goServer(`/settings/${id}`); }
  });
  const filter = $('#settings-filter');
  let searched = false;
  filter?.addEventListener('input', () => {
    const q = filter.value.trim().toLowerCase();
    if (q && !searched) { searched = true; invoke('page_section', { section: current, chosen: true }).catch(() => {}); }
    if (!q) { open(current, false); return; }
    let hits = 0;
    $$('.group', body).forEach(g => g.hidden = false);
    $$('.section-line, .page-top, .q-h, #summary', body).forEach(l => l.hidden = true);
    $$('.panel', body).forEach(p => {
      let any = false;
      $$('.srow', p).forEach(r => {
        const hit = `${$('.shelp', r)?.textContent || ''} ${$('b', r)?.textContent || ''} ${r.dataset.key || ''}`.toLowerCase().includes(q);
        r.hidden = !hit || r.hasAttribute('data-off'); any = any || !r.hidden;
        tipHit(r, hit ? q : '');
      });
      p.hidden = !any || p.hasAttribute('data-off'); hits += p.hidden ? 0 : 1;
    });
    $$('.group', body).forEach(g => {
      const any = $$('.panel', g).some(p => !p.hidden);
      $('.hit-page', g).hidden = !any;
      g.hidden = !any;
    });
    $('#no-match').hidden = hits > 0;
  });
}

// The nav, as the server's: Essentials, this PC's pages, then reel's groups, the server's pages
// or the library's on this PC with the app's that belong among them; a select under 960 px.
let navKey = '';
function renderNav() {
  const here = !!(status ? status.mode === 'standalone' : document.body.hasAttribute('data-standalone'));
  const secs = (shape && shape.sections) || [];
  const key = JSON.stringify([here, secs.map((x) => [x.id, x.label, x.changed])]);
  if (key === navKey) return;
  navKey = key;
  const links = [], choices = [], placed = new Set();
  const option = (value, label) => { const o = document.createElement('option'); o.value = value; o.textContent = label; return o; };
  for (const [words, pages] of NAV) {
    const items = [], opts = [];
    for (const p of pages.map(pageOf).filter((x) => shows(x, here))) {
      const a = document.createElement('a');
      a.href = '#';
      if (p.app) {
        a.dataset.appSection = p.id;
        a.append(PAGE[p.id]);
        if (p.id === 'games') a.append(' ', unsortedCount(status ? status.unsorted : 0));
        opts.push(option(`app:${p.id}`, PAGE[p.id]));
      } else {
        const sec = secs.find((x) => x.id === p.id);
        if (!sec) continue;
        placed.add(sec.id);
        a.dataset.page = `/settings/${sec.id}`;
        a.href = serverHref(`/settings/${sec.id}`);
        a.append(sec.label);
        if (sec.changed > 0) { const n = document.createElement('span'); n.className = 'n'; n.textContent = sec.changed; a.append(' ', n); }
        opts.push(option(`reel:${sec.id}`, sec.label));
      }
      items.push(a);
    }
    if (!items.length) continue;
    if (words) {
      const h = document.createElement('span');
      h.className = 'sections-head';
      h.textContent = words;
      links.push(h);
      const g = document.createElement('optgroup');
      g.label = words;
      g.append(...opts);
      choices.push(g);
    } else choices.push(...opts);
    links.push(...items);
  }
  // a section of a newer server's that the table does not know yet, at the end as the server has it
  for (const sec of secs.filter((x) => !placed.has(x.id))) {
    const a = document.createElement('a');
    a.dataset.page = `/settings/${sec.id}`;
    a.href = serverHref(`/settings/${sec.id}`);
    a.textContent = sec.label;
    links.push(a);
    choices.push(option(`reel:${sec.id}`, sec.label));
  }
  $('#app-nav').replaceChildren(...links);
  $('#app-select').replaceChildren(...choices);
  if (status) renderParts(status);
  markNav();
}

function markNav() {
  const nav = $('#app-nav');
  for (const a of $$('a[data-app-section]', nav)) {
    if (a.dataset.appSection !== current) { a.removeAttribute('aria-current'); continue; }
    a.setAttribute('aria-current', 'page');
    // a nav taller than the window scrolls on its own: the page shown stays in sight there
    if (a.offsetTop < nav.scrollTop || a.offsetTop + a.offsetHeight > nav.scrollTop + nav.clientHeight) nav.scrollTop = a.offsetTop - nav.clientHeight / 2;
  }
  $('#app-select').value = `app:${current}`;
}

// Essentials' sentence: what saves a clip, how much, when, at what, and where it goes.
function renderSummary() {
  const el = $('#summary');
  const text = (status && status.pages && status.pages.summary) || '';
  el.replaceChildren(...text.split('**').map((t, i) => {
    if (i % 2 === 0) return t;
    const b = document.createElement('b');
    b.textContent = t;
    return b;
  }));
  off(el, !text);
}

// The two cards' words carry the key and the length as they are now.
const lenWords = (secs) => (secs >= 120 && secs % 60 === 0 ? `${secs / 60} min` : `${secs} s`);
function renderModes() {
  if (!settings) return;
  const own = !(status && status.obs_box);
  const secs = own ? status && status.obs_state && status.obs_state.replay_seconds : settings.obs.setup.clip_seconds;
  const len = secs ? `the last ${lenWords(secs)}` : 'the last seconds';
  const key = settings.hotkeys.save.trim();
  const saves = key ? `${key} saves ${len}` : `Save clip saves ${len}`;
  const grace = settings.session.grace_seconds;
  $('#mode-always').textContent = `${saves} at any time, also on the desktop.`;
  $('#mode-games').textContent = `The buffer runs while a game runs, and stops ${grace ? `${lenWords(grace)} after the last one closes` : 'once the last one closes'}. ${saves}.`;
}

// A row whose switch is off hides the rows that depend on it, as reel's own settings pages do.
function depends() {
  for (const r of $$('[data-depends]')) {
    const on = !!(settings && get(settings, r.dataset.depends));
    r.toggleAttribute('data-dep-off', !on);
  }
}

// Only while a game runs needs reel to start OBS's replay buffer: with an OBS of one's own that
// is Start the replay buffer, turned on with it and said.
async function bufferForGames() {
  renderBufferNote();
  if (!settings || settings.obs.replay_buffer !== 'games' || (status && status.obs_box) || settings.obs.start_replay) return;
  const row = $('.srow[data-key="obs.start_replay"]');
  setOn($(CTL, row), true);
  await save(row);
  if (settings.obs.start_replay) toast('Start it in OBS, under Clips, is on now: reel starts your OBS\'s replay buffer with a game and stops it after.', 'ok');
  renderBufferNote();
}

function renderBufferNote() {
  const note = $('#buffer-note');
  if (!note || !settings) return;
  const own = !(status && status.obs_box);
  const show = settings.obs.replay_buffer === 'games' && own && !settings.obs.start_replay;
  note.textContent = show ? 'Start it in OBS, under Clips, is off: reel cannot start your OBS\'s replay buffer with a game until it is on.' : '';
  note.classList.toggle('st-warn', show);
  note.hidden = !show;
}

// The games that record by themselves now, from the list and Session recording's switch.
async function renderRecordNow() {
  const box = $('#record-now');
  if (!box) return;
  let page;
  try { page = await invoke('programs_get'); } catch { return; }
  const every = page.every || {};
  const names = (page.entries || []).filter((e) => e.class !== 'ignored' && (e.own.record === 'on'
    || (e.own.record === 'as_class' && e.class === 'game' && every.record))).map((e) => e.name);
  box.textContent = names.length ? names.join(', ') : every.record ? 'every game; none in the list yet' : 'none: set Record by itself on a game\'s page under Games and apps';
}

// The waits' words follow the mode: the library's heavy work standalone, uploads connected.
function renderWaits() {
  if (!status || !settings) return;
  const here = status.mode === 'standalone', both = here && status.paired;
  const tail = 'A game is what Games and apps calls one, and a program not sorted yet while it has the whole screen; a program\'s own setting there turns it on or off for that game alone';
  const [label, help] = !here
    ? ['Uploads wait while a game has the screen', `off: each clip goes up as it is found, also while you play. On: uploads wait while a game has the screen and go on after, for a slow connection or a busy PC. ${tail}`]
    : both
      ? ['Uploads and encoding wait while a game has the screen', `off: the library's heavy work (a re-encode, the copies, the loop and the strip) and the uploads to the server go on while you play. On: they wait while a game has the screen and go on after, for a PC a game keeps busy or a slow connection. ${tail}`]
      : ['Encoding waits while a game has the screen', `off: the library's heavy work (a re-encode, the copies, the loop and the strip) goes on while you play. On: it waits while a game has the screen and goes on after, for a PC a game keeps busy. ${tail}`];
  $('#waits-label').textContent = label;
  $('#waits-help').textContent = help;
  $('#waits-row .switch').setAttribute('aria-label', label);
}

// data-off keeps it hidden when a section opens or the filter runs.
function off(el, yes) { el.toggleAttribute('data-off', yes); el.hidden = yes; }

function renderMode() {
  const here = status.mode === 'standalone', os = status.linux ? 'linux' : 'windows';
  for (const el of $$('[data-standalone], [data-os]')) {
    const w = el.dataset.standalone;
    const mode = w === 'hide' ? here : w === 'paired' ? here && !status.paired : w === 'only' ? !here : w === 'paired-only' ? !(here && status.paired) : false;
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
  for (const el of $$('[data-linux-label]')) {
    if (el.dataset.windowsLabel == null) el.dataset.windowsLabel = el.getAttribute('aria-label');
    el.setAttribute('aria-label', status.linux ? el.dataset.linuxLabel : el.dataset.windowsLabel);
  }
  // standalone, the waits are the library's (Encoding) and a paired server's sending is Connection's
  if (here) {
    $('#encoder-waits').append($('#waits-row'), $('#wan-row'));
    $('#send-rows').append($('#bandwidth-row'), $('#chunk-row'));
  } else {
    $('#upload-rows').append($('#waits-row'), $('#wan-row'), $('#bandwidth-row'), $('#chunk-row'));
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
  $$('.srow[data-obs]').forEach(row => {
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
    ['Profile', st.profile || 'not named'],
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
  for (const row of $$('.srow[data-obs]')) {
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
  for (const row of $$('.srow[data-obs]')) {
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
  if (standalone && current === 'upload') window.openSection?.('clips');
  if (!standalone) { if (STANDALONE_ONLY.includes(current)) window.openSection?.('first'); return; }
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
  $('#encoder-note').textContent = not.length ? `Not offered: ${not.join('; ')}` : '';
  $('#encoder-note').hidden = !not.length;
}

let ownMicApp = null;
function renderChoicesApp() {
  if (!settings || !status) return;
  const s = settings.obs.setup;
  ensureOption($('.srow[data-key="obs.setup.encoder"] select'), s.encoder);
  renderEncoderChoices(s);
  ensureOption($('.srow[data-key="obs.setup.format"] select'), s.format);
  pick($('#quality-row'), s.quality);
  for (const [k, v] of [['obs.setup.encoder', s.encoder || 'obs_x264'], ['obs.setup.format', s.format]]) {
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

const size = (mb) => (mb >= 1000 ? `${(mb / 1024).toLocaleString('en', { maximumFractionDigits: 1 })}\u00a0GB` : `${Math.round(mb)}\u00a0MB`);
let memoryMbApp, costsKey = '';
// What the clip length and the quality cost: the replay buffer's memory and a clip's size, from
// the setup's own figures (ObsSetup::video), as the setup says them.
async function renderCosts() {
  if (!settings || !status) return;
  const row = $('#length-row'), effect = $('#length-effect'), n = $('#length-n'), range = $('#length-stops');
  const own = !status.obs_box;
  row.dataset.own = own ? '1' : '';
  if (own) {
    const st = status.obs_state;
    n.min = 1; n.max = 3600;
    n.disabled = range.disabled = !st;
    if (st) showLength(st.replay_seconds);
    $('#length-help').textContent = st ? 'what a save keeps: your OBS\'s replay length, written into its profile' : 'what a save keeps: your OBS\'s replay length, once OBS is connected';
    effect.textContent = '';
    effect.hidden = true;
    costsKey = '';
    const kbit = st && st.video_kbit;
    $('#quality-own').textContent = `your own OBS sets the picture${st ? `: ${st.output[0]} \u00d7 ${st.output[1]}, ${Math.round(st.fps)} frames a second${kbit ? `, ${kbit} kbit/s` : ''}` : ''}; under Capture, the format and the bitrate`;
    return;
  }
  n.min = 5; n.max = 1800;
  n.disabled = range.disabled = false;
  $('#length-help').textContent = 'what a save keeps, up to 30 minutes';
  const s = settings.obs.setup;
  const secs = Math.round(+n.value) || s.clip_seconds;
  const key = JSON.stringify([s, secs, status.server_upload_max_bytes, settings.mode]);
  if (key === costsKey) return;
  costsKey = key;
  const ask = (q, t) => invoke('obs_video', { setup: { ...s, quality: q, clip_seconds: t } }).catch(() => null);
  const v = await ask(s.quality, secs);
  if (memoryMbApp === undefined) memoryMbApp = await invoke('system_memory').catch(() => null);
  if (v) {
    const words = [`${lenWords(secs)} \u00d7 ${QUALITY_WORD[s.quality] || s.quality} = about ${size(v.buffer_mb)} of memory; a clip of about ${size(v.buffer_mb / 1.25)}`];
    if (memoryMbApp && v.buffer_mb > memoryMbApp / 4) words.push(`more than a quarter of this PC's ${size(memoryMbApp)}`);
    const limit = status.server_upload_max_bytes;
    if (limit && settings.mode !== 'standalone' && (v.buffer_mb / 1.25) * 1048576 > limit) words.push(`more than the ${size(limit / 1048576)} your server takes`);
    effect.textContent = words.join('; ');
    effect.classList.toggle('st-warn', words.length > 1);
    effect.hidden = false;
  }
  const custom = s.quality === 'custom';
  const cv = custom && s.custom_video;
  $('#quality-help').textContent = custom
    ? 'the picture every clip and session recording gets, and what it costs; a card sets one of reel\'s instead of OBS\'s'
    : 'the picture every clip and session recording gets, and what it costs';
  $('#quality-note').textContent = custom ? `as set in OBS${cv ? `: ${cv.output[0]} \u00d7 ${cv.output[1]}, ${Math.round(cv.fps[0] / (cv.fps[1] || 1))} frames a second` : ''}` : '';
  $('#quality-note').hidden = !custom;
  for (const q of ['light', 'smooth', 'sharp', 'native']) {
    const m = await ask(q, 60);
    if (!m) continue;
    $(`.p-res[data-res="${q}"]`).textContent = `${m.output[1]}p${m.fps}`;
    $(`.p-size[data-size="${q}"]`).textContent = `${size(m.buffer_mb / 1.25)} a minute`;
  }
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
  $('#setup-app-desc').textContent = !status.obs_box
    ? 'written into your OBS\'s profile; refused while OBS records; a running replay buffer starts again so the change takes'
    : elevated
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
    $('#game-app-help').textContent = 'obs-vkcapture\'s capture of a Vulkan or OpenGL game started with obs-gamecapture, over the screen';
    $('#game-app-note').textContent = 'needs obs-vkcapture, which this OBS does not have; the screen holds the game meanwhile';
    $('#game-app-note').hidden = status.obs_game_capture !== false;
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
  renderCosts();
  renderCopyApp();
  const doing = status.obs_step;
  for (const b of [$('#elevated-app-row .switch'), $('#copy-app-put'), $('#bundled-setup')]) b.disabled = !!doing;
  if (elevatedAsks > 0) $('#elevated-app-row .switch').disabled = true;
  syncElevated();
  const why = doing ? doing.line : status.obs_task_problem;
  $('#elevated-why').hidden = !why;
  $('#elevated-why').textContent = why || '';
  off($('#obs-ws-panel'), inUse);
  for (const el of $$('[data-box]')) off(el, el.dataset.box === 'reel' ? !inUse : inUse);
  off($('#quality-row'), !inUse);
  off($('#quality-own-row'), inUse);
  renderModes();
  $('#obs-section-line').textContent = inUse
    ? (!box.can_show
      ? 'OBS runs as administrator: open it from its tray icon to change anything there; reel keeps working as long as its WebSocket and the replay buffer stay on.'
      : lin
        ? 'OBS\'s window opens from its own tray icon: change anything there; reel keeps working as long as its WebSocket and the replay buffer stay on.'
        : 'Change anything in OBS itself; reel keeps working as long as its WebSocket and the replay buffer stay on.')
    : 'OBS tells the app the moment a replay is saved (Tools, WebSocket Server Settings, enable the server). The folder watch stays on as a safety net.';
}

async function doneInObs(r, always, quiet) {
  status = r.status;
  takeSettings(r.settings);
  if (!quiet && (always || (r.taken && r.taken.length))) toast(takenWords(r.taken), 'ok');
}

function takeSettings(next) {
  settings = next;
  depends();
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
  renderAsk(status);
  renderMode();
  renderWaits();
  renderBufferNote();
  renderPresence(status);
  renderSummary();
  renderNav();
  const here = status.mode === 'standalone';
  renderObs();
  renderLocal();
  $('#clips-dir-help').textContent = status.watcher.from_obs
    ? 'this folder is watched when OBS is not connected or Follow OBS is off'
    : 'watched while OBS is not connected or Follow OBS is off; a file counts once its size holds still and it can be opened';
  $('#clips-dir-note').textContent = status.watcher.from_obs ? `OBS says ${status.watcher.dir}` : '';
  $('#clips-dir-note').hidden = !status.watcher.from_obs;
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
  // what reel needs from an OBS of one's own opens by itself while something is missing
  if ((status.obs_checks || []).some((c) => !c.ok)) $('#obs-more').open = true;
  const [linuxWarn, ...linuxHow] = (status.hotkeys_how || '').split(/(?<=\.) (?=There )/);
  for (const el of $$('.hotkeys-how')) {
    const short = !!el.closest('#g-first');
    el.textContent = short ? linuxWarn : status.hotkeys_how || '';
    el.hidden = !status.hotkeys_how;
  }
  const howTip = $('#hotkeys-how-tip');
  if (howTip) { howTip.textContent = linuxHow.join(' '); howTip.hidden = !linuxHow.length; }
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
  // the keys' line says only what went wrong
  for (const el of $$('.hotkey-line')) {
    el.textContent = status.hotkey_ok ? '' : status.hotkey_line;
    el.classList.toggle('st-warn', !status.hotkey_ok);
    el.hidden = status.hotkey_ok;
  }
  const how = $('#g-hotkeys .panel > .body:not(.flush)');
  if (how) how.hidden = status.hotkey_ok && !status.hotkeys_how;
  kv($('#about'), [['Version', [status.version, status.versions?.kind].filter(Boolean).join(', ')], ...(status.bundled ? [['OBS', `OBS Studio ${status.obs_version}, included`]] : []), ['Settings', status.config_dir], ['Data', status.data_dir]]);
  $('#about-obs').hidden = !status.bundled;
  $('#uninstall').hidden = !status.uninstaller;
  $('#about-obs-version').textContent = status.obs_version;
  $('#about-obs-source').href = status.obs_source;
  renderBundled();
  renderSave();
  renderDiscord();
  setOn($('#look'), !!status.update_check);
  $('#look').disabled = status.update_check == null;
  const v = status.versions;
  if (v) kv($('#versions-kv'), [
    ['This app', `${v.app}, ${v.kind}${v.obs ? ` (OBS ${v.obs})` : ''}`],
    ...(here ? [] : [['Server', v.server || 'not known yet']]),
    ['Newest release', v.newest || 'not known yet'],
    ['Updates', [status.updated_from ? `updated from ${status.updated_from}` : '', status.update_line].filter(Boolean).join('; ')],
  ]);
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

// The log opens at its end and follows new lines there; scrolled up to read, it stays put.
async function loadLog(opened = false) {
  try {
    const lines = await invoke('log_tail', { lines: 300 });
    const box = $('#log-lines');
    const end = opened || box.scrollHeight - box.scrollTop - box.clientHeight < 24;
    box.textContent = lines.length ? lines.join('\n') : 'nothing yet';
    if (end) box.scrollTop = box.scrollHeight;
  } catch (e) { $('#log-lines').textContent = String(e.message || e); }
}

function bindButtons() {
  for (const b of $$('button[data-go-section]')) b.addEventListener('click', () => window.openSection?.(b.dataset.goSection));
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
        window.openSection('capture');
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
  $('#own-setup').onclick = $('#setup-again').onclick;
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
  bindAsk();
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
  $('#look').addEventListener('change', async () => {
    const on = isOn($('#look')), err = $('#look-err');
    try { await invoke('update_check_set', { on }); err.hidden = true; status = await invoke('status_get'); renderStatus(); }
    catch (e) { setOn($('#look'), !on); err.textContent = String(e.message || e); err.hidden = false; }
  });
  $('#update-check').onclick = async () => {
    try { status = await invoke('update_check'); renderStatus(); toast(status.update_line, 'ok'); } catch (e) { toast(String(e.message || e), 'err'); }
  };
  $('#update-install').onclick = () => invoke('update_install').catch(e => toast(String(e.message || e), 'err'));
  $('#update-releases').onclick = () => invoke('open_releases').catch(e => toast(String(e.message || e), 'err'));
  $('#log-refresh').onclick = () => loadLog(true);
  $('#log-folder').onclick = () => invoke('open_log_folder').catch(e => toast(String(e.message || e), 'err'));
}

async function reshape() {
  const key = status ? `${status.mode}|${status.server_url}|${status.paired}|${!!status.offline}` : '';
  if (key === shapeKey) return;
  shapeKey = key;
  try {
    const ps = await invoke('page_shape_get');
    applyShape(ps.shape, ps.base);
    shape = ps.shape;
    renderNav();
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
  bindGames();
  try {
    settings = await invoke('settings_get');
    ownMicApp = await invoke('obs_own_mic').catch(() => null);
    $$('.srow[data-key]').forEach(fill);
    depends();
    capAsk();
    off($('#elevated-start-row'), !settings.startup.elevated);
    status = await invoke('status_get');
    shapeKey = `${status.mode}|${status.server_url}|${status.paired}|${!!status.offline}`;
    renderStatus();
    if (status.mode === 'standalone') refreshLibrary();
  } catch (e) {
    toast(`no shell: ${String(e.message || e)}`, 'err');
  }
  shape = await shaped;
  renderNav();
  delete document.documentElement.dataset.wait;
  await listen('status', (ev) => { status = ev.payload; renderStatus(); reshape(); gamesRefresh(); if (Date.now() - capAt > 5000) capAsk(); });
  await listen('open-section', (ev) => { if (window.openSection) window.openSection(ev.payload); });
  await listen('settings', (ev) => takeSettings(ev.payload));
  await listen('obs-meters', (ev) => {
    for (const [name, db] of ev.payload || []) meterIn($('#levels-app'), name, db);
  });
  setInterval(() => { if (current === 'status') loadLog(); }, 5000);
}

main();
