// reel's header and nav for the app's own pages; links open in this webview, so Back comes back here.
import { isOn, setOn, switches } from './assets/js/switch.js';
import { mouseWalk } from './assets/js/walk.js';

switches();
export const tauri = window.__TAURI__ || null;
export const invoke = tauri ? tauri.core.invoke : async (cmd) => { throw new Error(`no shell for ${cmd}`); };
export const listen = tauri ? tauri.event.listen : async () => () => {};
export const $ = (sel, root = document) => root.querySelector(sel);
export const $$ = (sel, root = document) => Array.from(root.querySelectorAll(sel));
let base = '';

export function toast(text, kind = 'ok') {
  const box = $('#toasts');
  const el = document.createElement('div');
  el.className = `toast ${kind}`;
  el.textContent = text;
  box.append(el);
  setTimeout(() => el.remove(), 3500);
}

export function goServer(path) {
  invoke('open_page', { path }).catch(e => toast(String(e.message || e), 'err'));
}

function serverHref(path) { return base ? base + path : path; }

export const APP_SECTIONS = [
  ['status', 'Status'], ['server', 'Server'], ['obs', 'OBS'], ['clips', 'Clips folder and cache'], ['upload', 'Upload'],
  ['hotkeys', 'Hotkeys'], ['sounds', 'Sounds'], ['startup', 'Startup'], ['log', 'Log'],
  ['library', 'Library folder'], ['encoder', 'Encoder'],
];
export const STANDALONE_ONLY = ['library', 'encoder'];
// The nav by what a person does: reel's sections ('r') and the app's pages ('a'), each always, in
// standalone alone ('s') or beside a server alone ('p'); crates/server/src/routes/settings.rs
// groups reel's pages the same way.
const NAV = [
  ['Recording', ['a obs', 'a clips', 'a upload', 'a hotkeys', 'a sounds']],
  ['Library', ['r storage', 'as library', 'r ingest', 'as encoder', 'rs sharing', 'r imports']],
  ['Server', ['a server', 'rp sharing', 'r devices', 'r access', 'r notifications']],
  ['System', ['r general', 'r appearance', 'a startup', 'a status', 'a log', 'r advanced']],
];

const WORST = ['err', 'warn', 'active', 'ok', 'neutral'];
const worst = (states) => WORST.find(s => states.includes(s)) || 'ok';
const APP_DOT = { error: 'err', unpaired: 'err', paused: 'warn', uploading: 'active', working: 'active', preparing: 'active', idle: 'ok' };
const HEALTH_WORD = { ok: 'ok', warn: 'warning', err: 'error' };
let serverState = 'ok';
let serverLine = '';
// Standalone: the library on this PC, no server.
let here = false;
const STANDALONE_LABELS = { clips: 'Clips folder', upload: 'New clips' };

function setPart(id, state, word, line) {
  const dd = $(`#${id}`), dt = dd?.previousElementSibling;
  if (!dd) return;
  dd.hidden = false; if (dt) dt.hidden = false;
  $('.dot', dd).className = `dot ${state}`;
  $('b', dd).textContent = word || '';
  $('.muted', dd).textContent = line || '';
}

// Fills reel's header from the server's shape: the user, the server's health, the jobs count.
export function applyShape(shape, baseUrl) {
  base = (baseUrl || '').replace(/\/$/, '');
  followServer();
  $$('header.top [data-page]').forEach(a => { a.href = serverHref(a.dataset.page); });
  const who = shape.who || {};
  const btn = $('#user-btn');
  btn.hidden = !who.name;
  btn.title = who.name || '';
  btn.textContent = (who.name || '?').slice(0, 1);
  const head = $('#user-menu .menu-head');
  head.textContent = '';
  const b = document.createElement('b');
  b.textContent = who.name || '';
  head.append(b, ` ${who.line || ''}`);
  serverState = (shape.health && shape.health.state) || 'ok';
  const line = (shape.health && shape.health.line) || '';
  serverLine = line;
  $('#chip-dot').className = `dot ${serverState}`;
  $('#chip').setAttribute('aria-label', `Status: ${HEALTH_WORD[serverState] || serverState}`);
  setPart('part-server', serverState, HEALTH_WORD[serverState] || serverState, line);
  const jobs = shape.jobs || {};
  const el = $('#jobs-count');
  const live = (jobs.running || 0) + (jobs.queued || 0);
  if (jobs.failed) { el.textContent = jobs.failed; el.className = 'count failed'; el.title = `${jobs.failed} failed`; }
  else if (live) { el.textContent = live; el.className = 'count live'; el.title = `${jobs.running || 0} running, ${jobs.queued || 0} queued`; }
  else { el.textContent = ''; el.className = 'count'; el.title = ''; }
}

// The server's stylesheet replaces the app's copy once loaded; the copy's fonts stay, as the
// server sends its fonts to its own pages alone.
function followServer() {
  const own = document.querySelector('link[rel="stylesheet"][href="assets/reel.css"]');
  let link = document.querySelector('link[data-server-css]');
  if (!own) return;
  const href = base ? `${base}/assets/reel.css` : '';
  if (link && link.getAttribute('href') === href) return;
  if (link) link.remove();
  document.querySelector('style[data-own-fonts]')?.remove();
  own.disabled = false;
  if (!href) return;
  link = document.createElement('link');
  link.rel = 'stylesheet';
  link.dataset.serverCss = '';
  const fonts = document.createElement('style');
  fonts.dataset.ownFonts = '';
  fonts.textContent = [...own.sheet.cssRules].filter((r) => r instanceof CSSFontFaceRule)
    .map((r) => r.cssText.replace(/url\("?([^")]+)"?\)/g, (m, u) => `url("${new URL(u, own.href)}")`)).join('\n');
  link.addEventListener('load', () => { own.disabled = true; });
  link.addEventListener('error', () => { link.remove(); fonts.remove(); });
  link.href = href;
  own.after(link, fonts);
}

export function sectionsNav(shape, current, standalone) {
  const nav = $('nav.sections');
  if (!nav) return;
  nav.textContent = '';
  const reels = (s) => {
    const a = document.createElement('a');
    a.href = serverHref(`/settings/${s.id}`);
    a.dataset.page = `/settings/${s.id}`;
    a.dataset.section = s.id;
    a.append(s.label);
    if (s.changed > 0) {
      const n = document.createElement('span');
      n.className = 'n';
      n.textContent = s.changed;
      a.append(' ', n);
    }
    return a;
  };
  const own = (id) => {
    const a = document.createElement('a');
    a.href = `#${id}`;
    a.dataset.appSection = id;
    a.textContent = (standalone && STANDALONE_LABELS[id]) || APP_SECTIONS.find(([k]) => k === id)[1];
    if (id === current) a.setAttribute('aria-current', 'page');
    return a;
  };
  const head = (words) => {
    const h = document.createElement('span');
    h.className = 'sections-head';
    h.textContent = words;
    return h;
  };
  const sections = shape.sections || [];
  const placed = [];
  for (const [words, pages] of NAV) {
    const entries = [];
    for (const page of pages) {
      const [kind, id] = page.split(' ');
      if ((kind.includes('s') && !standalone) || (kind.includes('p') && standalone)) continue;
      if (kind[0] === 'a') entries.push(own(id));
      else {
        const s = sections.find((x) => x.id === id);
        if (s) { entries.push(reels(s)); placed.push(id); }
      }
    }
    if (entries.length) nav.append(head(words), ...entries);
  }
  nav.append(...sections.filter((s) => !placed.includes(s.id)).map(reels));
}

export function markSection(current) {
  $$('nav.sections a[data-app-section]').forEach(a => {
    if (a.dataset.appSection === current) a.setAttribute('aria-current', 'page'); else a.removeAttribute('aria-current');
  });
}

export function bindHeader() {
  mouseWalk();
  document.addEventListener('click', (e) => {
    // not the body's own data-page, which every click bubbles up to
    const a = e.target.closest('a[data-page], button[data-page]');
    if (!a || a.classList.contains('disabled')) return;
    e.preventDefault();
    goServer(a.dataset.page);
  });
  $('#search-form')?.addEventListener('submit', (e) => {
    e.preventDefault();
    const q = $('#search').value.trim();
    goServer(q ? `/?q=${encodeURIComponent(q)}` : '/');
  });
  $('#app-save')?.addEventListener('click', () => runSave('save_clip'));
  $('#app-save-share')?.addEventListener('click', () => runSave('save_and_share'));
  $('#menu-save')?.addEventListener('click', () => runSave('save_clip'));
  $('#menu-save-share')?.addEventListener('click', () => runSave('save_and_share'));
  $('#app-share')?.addEventListener('click', async () => {
    try { const r = await invoke('share_last'); toast(r.line, 'ok'); } catch (e) { toast(String(e.message || e), 'err'); }
  });
  $('#app-pause')?.addEventListener('change', async () => {
    try { await invoke('pause_set', { paused: isOn($('#app-pause')) }); } catch (e) { toast(String(e.message || e), 'err'); }
  });
  $('#ver-updates')?.addEventListener('click', (e) => {
    e.preventDefault();
    invoke('view_show', { view: 'settings', section: 'startup' }).catch(err => toast(String(err.message || err), 'err'));
  });
  const pops = [[$('#chip'), $('#chip-pop')], [$('#user-btn'), $('#user-menu')]];
  const close = () => pops.forEach(([b, p]) => { p.hidden = true; b.setAttribute('aria-expanded', 'false'); });
  for (const [btn, pop] of pops) {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const open = pop.hidden;
      close();
      if (open) { pop.hidden = false; btn.setAttribute('aria-expanded', 'true'); }
    });
    pop.addEventListener('click', (e) => e.stopPropagation());
  }
  document.addEventListener('click', close);
  const toggle = $('#search-toggle'), form = $('#search-form');
  toggle.addEventListener('click', (e) => {
    e.stopPropagation();
    const open = !form.classList.contains('open');
    form.classList.toggle('open', open);
    toggle.setAttribute('aria-expanded', String(open));
    if (open) $('#search').focus();
  });
  let chord = '';
  document.addEventListener('keydown', (e) => {
    // Windows' webview walks the history on Alt+Left and Alt+Right itself, the Linux one does not.
    if (e.altKey && !e.ctrlKey && !e.metaKey) {
      if (e.key === 'ArrowLeft') { e.preventDefault(); history.back(); }
      if (e.key === 'ArrowRight') { e.preventDefault(); history.forward(); }
      return;
    }
    const inField = /^(INPUT|TEXTAREA|SELECT)$/.test(document.activeElement?.tagName);
    if (e.key === 'Escape') { close(); if (inField) document.activeElement.blur(); return; }
    if (inField || e.ctrlKey || e.metaKey) return;
    if (e.key === '/') { e.preventDefault(); $('#search')?.focus(); $('#search')?.select(); return; }
    if (chord === 'g') { chord = ''; if (e.key === 's' && !here) goServer('/shares'); if (e.key === 'l') goServer('/'); if (e.key === 'j') goServer('/jobs'); if (e.key === 't') goServer('/status'); return; }
    if (e.key === ',') { goServer('/settings'); return; }
    if (e.key === 'g') { chord = 'g'; setTimeout(() => chord = '', 900); return; }
    if (e.key === 'u') goServer('/upload');
  });
}

export function renderParts(status) {
  modeParts(status);
  const app = APP_DOT[status.state] || 'ok';
  setPart('part-app', app, status.state_word || status.state, status.state_line || '');
  setPart('part-obs', status.obs.connected ? 'ok' : 'neutral', status.obs.connected ? 'connected' : 'off', status.obs.line || '');
  $('#chip-dot').className = `dot ${worst([serverState, app])}`;
  const word = $('#chip-word');
  word.textContent = status.state_word || status.state;
  word.className = `chip-word ${app === 'ok' ? '' : app}`;
  setOn($('#app-pause'), !!status.paused_by_user);
  $('#app-share').disabled = !status.paired;
  saveButtons(status, $('#app-save'), $('#app-save-share'));
  saveButtons(status, $('#menu-save'), $('#menu-save-share'));
  const note = $('#menu-save-why');
  if (note) {
    const why = !status.save?.ok ? status.save?.why : (!status.save_share?.ok && !here ? `Save and share: ${status.save_share?.why}` : '');
    note.textContent = why ? `Not now: ${why}` : '';
    note.hidden = !why;
  }
  renderVersions(status.versions, status.update_line);
  const hasPages = !!status.server_url || status.mode === 'standalone';
  $$('header.top [data-page], nav.sections [data-page]').forEach(a => a.classList.toggle('disabled', !hasPages));
}

export function renderVersions(v, line) {
  if (!v) return;
  setPart('part-versions', v.standing === 'update' || v.standing === 'out' ? 'active' : v.standing === 'ahead' ? 'warn' : '', v.words, line || '');
}

function modeParts(status) {
  const p = status.panel || {};
  here = status.mode === 'standalone';
  $$('[data-panel]').forEach(el => { el.hidden = p[el.dataset.panel] === false; });
  const dt = $('#part-server')?.previousElementSibling;
  if (dt) dt.textContent = p.place || 'Server';
  if (!here) return;
  if (p.place_state) setPart('part-server', ...p.place_state);
  else setPart('part-server', serverState, 'on this PC', serverLine);
}

const SAVE_WHAT = ['OBS saves the replay and it goes up as any clip does', 'OBS saves the replay, the app uploads it and copies the link'];
export function saveButtons(status, save, saveShare) {
  [[save, status.save], [saveShare, status.save_share]].forEach(([b, a], i) => {
    if (!b || !a) return;
    b.disabled = !a.ok;
    const what = i === 0 && status.mode === 'standalone' ? 'OBS saves the replay, and it goes into the library on this PC' : SAVE_WHAT[i];
    b.title = a.ok ? what : `Not now: ${a.why}`;
  });
}

export async function runSave(which) {
  try { const r = await invoke(which); toast(r.line, 'ok'); } catch (e) { toast(String(e.message || e), 'err'); }
}

// The header as web/templates/base.html writes it.
export function header(page) {
  const cur = (p) => (p === page ? ' aria-current="page"' : '');
  return `<header class="top">
  <a class="wordmark" href="#" data-page="/"><svg class="mark" viewBox="8.6 3 16.2 26" aria-hidden="true"><path d="M8.6 3L16 10.4V29H8.6Z"/><path class="arm" d="M8.6 3H24.8V10.4H16Z"/></svg>reel</a>
  <nav>
    <a href="#" data-page="/"${cur('library')}>Library</a>
    <a href="#" data-page="/collections"${cur('collections')}>Collections</a>
    <a href="#" data-page="/shares" data-panel="shares"${cur('shares')}>Shares</a>
    <a href="#" data-page="/jobs"${cur('jobs')}>Jobs<span class="count" id="jobs-count"></span></a>
    <a href="#" data-page="/settings" class="tab-settings"${cur('settings')}>Settings</a>
  </nav>
  <form class="search" action="#" method="get" id="search-form">
    <input type="search" name="q" value="" placeholder="Search titles and descriptions" aria-label="Search" id="search">
    <kbd>/</kbd>
  </form>
  <button class="quiet search-toggle" id="search-toggle" type="button" aria-label="Search" aria-expanded="false"><svg class="icon" viewBox="0 0 16 16"><circle cx="7" cy="7" r="4.5"/><path d="M10.5 10.5 14 14"/></svg></button>
  <a class="btn primary" href="#" data-page="/upload" id="upload-link" aria-label="Upload"><svg class="icon" viewBox="0 0 16 16"><path d="M8 11V3M4.5 6.5 8 3l3.5 3.5M3 13h10"/></svg><span>Upload</span></a>
  <button class="btn app-save" id="app-save" type="button" aria-label="Save clip" disabled><svg class="icon" viewBox="0 0 16 16"><circle cx="8" cy="8" r="5.5"/><circle cx="8" cy="8" r="1.8"/></svg><span>Save clip</span></button>
  <button class="btn app-save" id="app-save-share" data-panel="save_share" type="button" aria-label="Save and share" disabled><svg class="icon" viewBox="0 0 16 16"><circle cx="5.5" cy="8" r="4"/><circle cx="5.5" cy="8" r="1.2"/><path d="M11 8h4M13 6l2 2-2 2"/></svg><span>Save and share</span></button>
  <button class="btn app-share" id="app-share" data-panel="share_last" type="button" aria-label="Share the last clip"><svg class="icon" viewBox="0 0 16 16"><path d="M12 5a2 2 0 1 0 0-4 2 2 0 0 0 0 4ZM4 10a2 2 0 1 0 0-4 2 2 0 0 0 0 4Zm8 5a2 2 0 1 0 0-4 2 2 0 0 0 0 4ZM5.7 7.1l4.6-2.3M5.7 8.9l4.6 2.3"/></svg><span>Share the last clip</span></button>
  <button class="chip" id="chip" type="button" aria-haspopup="dialog" aria-expanded="false" aria-label="Status"><i class="dot ok" id="chip-dot"></i><span class="chip-word" id="chip-word"></span></button>
  <div class="popover" id="chip-pop" role="dialog" aria-label="Status" hidden>
    <dl class="parts">
      <dt>App</dt><dd id="part-app"><i class="dot"></i><span><b></b> <span class="muted"></span></span></dd>
      <dt>OBS</dt><dd id="part-obs"><i class="dot"></i><span><b></b> <span class="muted"></span></span></dd>
      <dt>Server</dt><dd id="part-server"><i class="dot ok"></i><span><b></b> <span class="muted"></span></span></dd>
      <dt>Versions</dt><dd id="part-versions"><i class="dot"></i><span><b></b> <span class="muted"></span></span></dd>
    </dl>
    <div class="pop-actions"><label class="toggle" data-panel="pause">Pause uploads <button type="button" class="switch" role="switch" aria-checked="false" id="app-pause"><i></i></button></label><a class="btn small quiet" href="#" data-page="/status">Status page</a><a class="btn small quiet" href="#" id="ver-updates">Updates</a></div>
  </div>
  <button class="avatar" id="user-btn" type="button" title="" aria-label="Menu" aria-haspopup="menu" aria-expanded="false"></button>
  <div class="menu" id="user-menu" role="menu" hidden>
    <div class="menu-head"><b></b> </div>
    <div class="menu-save"><button class="item" id="menu-save" type="button" role="menuitem" disabled>Save clip</button><button class="item" id="menu-save-share" data-panel="save_share" type="button" role="menuitem" disabled>Save and share</button><p class="menu-note" id="menu-save-why" hidden></p></div>
    <div class="menu-nav">
      <a href="#" data-page="/" role="menuitem">Library</a>
      <a href="#" data-page="/collections" role="menuitem">Collections</a>
      <a href="#" data-page="/shares" data-panel="shares" role="menuitem">Shares</a>
      <a href="#" data-page="/jobs" role="menuitem">Jobs</a>
    </div>
    <a href="#" data-page="/settings" role="menuitem">Settings <kbd>,</kbd></a>
    <a href="#" data-page="/status" role="menuitem">Status <kbd>g t</kbd></a>
  </div>
</header>`;
}

export async function mount(page) {
  document.documentElement.dataset.app = 'desktop';
  document.body.insertAdjacentHTML('afterbegin', header(page));
  bindHeader();
  let shape = { sections: [], who: {}, health: {}, jobs: {} }, base = '';
  try {
    const ps = await invoke('page_shape_get');
    shape = ps.shape || shape;
    base = ps.base || '';
  } catch (e) { /* no shell: the page stands on its own */ }
  applyShape(shape, base);
  return shape;
}
