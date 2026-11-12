// reel's header, nav and the app's bar for the app's own pages; links open in this webview, so Back comes back here.
import { setOn, switches } from './assets/js/switch.js';
import { mouseWalk } from './assets/js/walk.js';
import { appBar } from './assets/js/appbar.js';

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
  ['hotkeys', 'Hotkeys'], ['session', 'Session recording'], ['sounds', 'Sounds'], ['startup', 'Startup'], ['log', 'Log'],
  ['library', 'Library folder'], ['encoder', 'Encoder'],
];
export const STANDALONE_ONLY = ['library', 'encoder'];
// The nav by what a person does: reel's sections ('r') and the app's pages ('a'), each always, in
// standalone alone ('s') or beside a server alone ('p'); crates/server/src/routes/settings.rs
// groups reel's pages the same way.
const NAV = [
  ['Recording', ['a obs', 'a clips', 'a upload', 'a hotkeys', 'a session', 'a sounds']],
  ['Library', ['r storage', 'as library', 'r ingest', 'as encoder', 'rs sharing', 'r imports']],
  ['Server', ['a server', 'rp sharing', 'r devices', 'r access', 'r notifications']],
  ['System', ['r general', 'r appearance', 'a startup', 'a status', 'a log', 'r advanced']],
];

let serverState = 'ok';
let serverLine = '';
// Standalone: the library on this PC, no server.
let here = false;
// The bar, once it is mounted, and the status it last drew.
let bar = () => {};
let last = null;
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
  serverLine = (shape.health && shape.health.line) || '';
  if (last) bar(last, { state: serverState, line: serverLine });
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

const showSettings = (section) => invoke('view_show', { view: 'settings', section }).catch(err => toast(String(err.message || err), 'err'));
const drawn = (p) => p.then((st) => { if (st && typeof st === 'object' && st.state) renderParts(st); }).catch(e => toast(String(e.message || e), 'err'));

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
  bar = appBar({
    fix: (act) => {
      if (act === 'status') goServer('/status');
      else if (act === 'retry') invoke('offline_retry').catch(e => toast(String(e.message || e), 'err'));
      else if (act === 'obs_again') drawn(invoke('obs_again'));
      else if (act === 'resume_clips') drawn(invoke('recording_pause_set', { paused: false }));
      else if (act === 'resume_uploads') drawn(invoke('pause_set', { paused: false }));
      else if (act.startsWith('settings:')) showSettings(act.slice(9));
    },
    save: (which) => runSave(which === 'save' ? 'save_clip' : 'save_and_share'),
    share: async () => {
      try { const r = await invoke('share_last'); toast(r.line, 'ok'); } catch (e) { toast(String(e.message || e), 'err'); }
    },
    pauseClips: (on) => drawn(invoke('recording_pause_set', { paused: on })),
    pauseUploads: (on) => drawn(invoke('pause_set', { paused: on })),
    record: (on) => invoke('session_set', { on }),
    fail: (why) => toast(why, 'err'),
  });
  $('#ver-updates')?.addEventListener('click', (e) => {
    e.preventDefault();
    showSettings('startup');
  });
  const pops = [[$('#user-btn'), $('#user-menu')]];
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
  last = status;
  modeParts(status);
  bar(status, { state: serverState, line: serverLine });
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
  const dt = $('#bar-place')?.previousElementSibling;
  if (dt) dt.textContent = p.place || 'Server';
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

// The header as web/templates/base.html writes it, without the browser's status chip.
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
  <button class="avatar" id="user-btn" type="button" title="" aria-label="Menu" aria-haspopup="menu" aria-expanded="false"></button>
  <div class="menu" id="user-menu" role="menu" hidden>
    <div class="menu-head"><b></b> </div>
    <div class="menu-nav">
      <a href="#" data-page="/" role="menuitem">Library</a>
      <a href="#" data-page="/collections" role="menuitem">Collections</a>
      <a href="#" data-page="/shares" data-panel="shares" role="menuitem">Shares</a>
      <a href="#" data-page="/jobs" role="menuitem">Jobs</a>
    </div>
    <a href="#" data-page="/upload" role="menuitem">Upload a file <kbd>u</kbd></a>
    <a href="#" data-page="/settings" role="menuitem">Settings <kbd>,</kbd></a>
    <a href="#" data-page="/status" role="menuitem">Status <kbd>g t</kbd></a>
  </div>
</header>`;
}

// The bar as web/templates/base.html writes it; the mode's parts hide by data-panel.
export function footer() {
  return `<footer class="appbar" id="appbar" aria-label="This PC" hidden>
  <div class="appbar-left">
    <button class="appbar-words" id="appbar-words" type="button" aria-haspopup="dialog" aria-expanded="false" aria-controls="appbar-pop"><i class="dot" id="appbar-dot"></i><b id="appbar-word"></b><span class="appbar-detail" id="appbar-detail"></span><kbd class="hint" id="appbar-hint"></kbd><svg class="icon chev" viewBox="0 0 16 16" aria-hidden="true"><path d="M4 9.8 8 5.8l4 4"/></svg></button>
    <button class="btn small appbar-fix" id="appbar-fix" type="button" hidden></button>
  </div>
  <div class="appbar-right">
    <button class="btn small app-record" id="app-record" type="button" aria-label="Record a session" disabled><svg class="icon rec-ic" viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="4.5"/></svg><svg class="icon stop-ic" viewBox="0 0 16 16" aria-hidden="true"><rect x="4.5" y="4.5" width="7" height="7" rx="1"/></svg><span class="rec-time"></span><span class="rec-word">Record</span><kbd class="hint" data-key="record"></kbd></button>
    <button class="btn small quiet appbar-more" id="appbar-more" type="button" aria-label="More for this PC" aria-haspopup="dialog" aria-expanded="false" aria-controls="appbar-pop"><svg class="icon" viewBox="0 0 16 16" aria-hidden="true"><circle cx="3.5" cy="8" r="1.2"/><circle cx="8" cy="8" r="1.2"/><circle cx="12.5" cy="8" r="1.2"/></svg></button>
  </div>
  <div class="popover appbar-pop" id="appbar-pop" role="dialog" aria-label="This PC" hidden>
    <p class="appbar-say" id="appbar-say"></p>
    <div class="appbar-acts">
      <button class="item" id="app-save" type="button" disabled><span>Save clip</span><kbd data-key="save"></kbd></button>
      <button class="item" id="app-save-share" data-panel="save_share" type="button" disabled><span>Save and share</span><kbd data-key="save_share"></kbd></button>
      <button class="item" id="app-share" data-panel="share_last" type="button" disabled><span>Share the last clip</span><kbd data-key="share_last"></kbd></button>
      <p class="menu-note" id="app-save-why" hidden></p>
    </div>
    <div class="appbar-switches"><label class="toggle">Pause clips <button type="button" class="switch" role="switch" aria-checked="false" id="app-recording"><i></i></button></label><label class="toggle" data-panel="pause">Pause uploads <button type="button" class="switch" role="switch" aria-checked="false" id="app-pause"><i></i></button></label></div>
    <dl class="parts">
      <dt>OBS</dt><dd id="part-obs"><i class="dot"></i><span><b></b> <span class="muted"></span></span></dd>
      <dt>Server</dt><dd id="bar-place"><i class="dot ok"></i><span><b></b> <span class="muted"></span></span></dd>
      <dt>Versions</dt><dd id="part-versions"><i class="dot"></i><span><b></b> <span class="muted"></span></span></dd>
    </dl>
    <div class="pop-actions"><a class="btn small quiet" href="#" data-page="/status">Status page</a><a class="btn small quiet" href="#" id="ver-updates">Updates</a></div>
  </div>
</footer>`;
}

export async function mount(page) {
  document.documentElement.dataset.app = 'desktop';
  document.body.insertAdjacentHTML('afterbegin', header(page));
  ($('#toasts') || document.body.lastElementChild).insertAdjacentHTML('beforebegin', footer());
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
