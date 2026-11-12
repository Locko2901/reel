// The desktop app's bar on reel's pages, shown only where the app grants this origin its commands.
import { setPart } from './header.js';
import { appDownloads } from './app-downloads.js';
import { mouseWalk } from './walk.js';
import { appBar } from './appbar.js';
import { toast } from './lib.js';
const $ = (sel, root = document) => root.querySelector(sel);
const $$ = (sel, root = document) => Array.from(root.querySelectorAll(sel));

function invoke() {
  const t = window.__TAURI__;
  return t && t.core && typeof t.core.invoke === 'function' ? t.core.invoke : null;
}

function shape() {
  const dot = $('#chip-dot');
  const menuHead = $('#user-menu .menu-head');
  const name = $('#user-btn')?.title.trim() || '';
  const jobs = $('#jobs-count');
  const n = jobs ? parseInt(jobs.textContent, 10) || 0 : 0;
  const server = $('#part-server');
  return {
    sections: $$('.sections a[data-section]').map(a => ({
      id: a.dataset.section,
      label: (a.childNodes[0]?.textContent || '').trim(),
      changed: parseInt($('.n', a)?.textContent, 10) || 0,
    })),
    who: { name, line: (menuHead?.textContent || '').replace(name, '').trim(), admin: !!$('.top .tab-settings') },
    health: { state: ['ok', 'warn', 'err'].find(c => dot?.classList.contains(c)) || 'ok', line: server ? $('.muted', server).textContent : '' },
    jobs: { queued: 0, running: jobs?.classList.contains('live') ? n : 0, failed: jobs?.classList.contains('failed') ? n : 0 },
  };
}

export function renderVersions(v, line) {
  if (!v) return;
  setPart('part-versions', v.standing === 'update' || v.standing === 'out' ? 'active' : v.standing === 'ahead' ? 'warn' : '', v.words, line || '');
  const row = $('#status-app');
  if (row) row.textContent = `${v.app}, ${v.kind}${v.obs ? ` (OBS ${v.obs})` : ''}; its updater: ${line || 'not asked yet'}`;
}

// The update's question, which the app sends only to the library's own page: Install now, Later.
function renderAsk(a) {
  const bar = $('#update-ask');
  if (!bar) return;
  bar.hidden = !a;
  if (!a) return;
  $('#update-ask-title').textContent = `${a.title}.`;
  $('#update-ask-words').textContent = a.words;
}

export function appShell() {
  const call = invoke();
  if (!call || !window.__REEL_DESKTOP__) return;
  document.documentElement.dataset.app = 'desktop';
  appDownloads();
  $$('.app-part, .sections .app-link').forEach(el => el.hidden = false);
  if (!document.body.hasAttribute('data-standalone')) $$('#sections-body .section-line').forEach(l => l.append(' Kept on the server: the same for every PC and browser.'));
  // The page's own read of the server's health, or of the library on this PC.
  const chipDot = $('#chip-dot');
  const server = () => document.documentElement.dataset.session === 'out'
    ? { state: 'neutral', word: 'signed out', line: 'reload to sign in' }
    : { state: ['ok', 'warn', 'err'].find(c => chipDot?.classList.contains(c)) || 'ok', line: $('#part-server .muted')?.textContent || '' };
  let render = () => {};
  const run = (cmd, args) => call(cmd, args).then(render).catch((e) => toast(String(e && e.message ? e.message : e), 'err'));
  const bar = appBar({
    fix: (act) => {
      if (act === 'status') location.href = '/status';
      else if (act === 'retry') location.reload();
      else if (act === 'obs_again') run('app_obs_again');
      else if (act === 'resume_clips') run('app_recording_pause_set', { paused: false });
      else if (act === 'resume_uploads') run('app_pause_set', { paused: false });
      else if (act.startsWith('settings:')) call('app_open_settings', { section: act.slice(9), shape: shape() }).catch(() => {});
    },
    save: (which) => call(which === 'save' ? 'app_save_clip' : 'app_save_and_share').then(() => poll()).catch(() => {}),
    share: () => call('app_share_last').catch(() => {}),
    pauseClips: (on) => run('app_recording_pause_set', { paused: on }),
    pauseUploads: (on) => run('app_pause_set', { paused: on }),
    record: (on) => call('app_session_set', { on }),
    fail: (why) => toast(why, 'err'),
  });
  render = (st) => {
    if (!st) return;
    bar(st, server());
    renderVersions(st.versions, st.update_line);
    renderAsk(st.update_ask);
  };
  const poll = async () => {
    try { render(await call('app_status')); } catch (e) { /* the app went away; the page stays a page */ }
  };
  let timer = 0;
  const start = () => { poll().then(() => { delete document.documentElement.dataset.wait; }); clearInterval(timer); timer = setInterval(poll, 2000); };
  start();
  document.addEventListener('visibilitychange', () => { if (!document.hidden) poll(); });
  window.addEventListener('pagehide', () => clearInterval(timer));
  // Back brings a page out of the back/forward cache without running its scripts again.
  window.addEventListener('pageshow', (e) => { if (e.persisted) start(); });
  for (const [id, install] of [['#update-ask-install', true], ['#update-ask-later', false]]) {
    const b = $(id);
    if (!b) continue;
    b.addEventListener('click', async () => {
      b.disabled = true;
      try {
        await call('app_update_answer', { install });
        if (!install) $('#update-ask').hidden = true;
      } catch (e) { /* the app went away; the page stays a page */ }
      b.disabled = false;
    });
  }
  $('#ver-updates')?.addEventListener('click', (e) => {
    e.preventDefault();
    call('app_open_settings', { section: 'startup', shape: shape() }).catch(() => {});
  });
  $$('.sections a[data-app-section]').forEach(a => a.addEventListener('click', (e) => {
    e.preventDefault();
    call('app_open_settings', { section: a.dataset.appSection, shape: shape() }).catch(() => {});
  }));
  // Windows' webview walks the history on Alt+Left and Alt+Right itself, the Linux one does not.
  document.addEventListener('keydown', (e) => {
    if (!e.altKey || e.ctrlKey || e.metaKey) return;
    if (e.key === 'ArrowLeft') { e.preventDefault(); history.back(); }
    if (e.key === 'ArrowRight') { e.preventDefault(); history.forward(); }
  });
  mouseWalk();
}
