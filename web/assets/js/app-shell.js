// The desktop app's parts in reel's header, shown only where the app grants this origin its commands.
import { isOn, setOn } from './switch.js';
import { setPart, worst } from './header.js';
import { appDownloads } from './app-downloads.js';
import { mouseWalk } from './walk.js';
import { recordControl } from './record.js';
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

export const APP_DOT = { error: 'err', unpaired: 'err', paused: 'warn', uploading: 'active', working: 'active', preparing: 'active', recording: 'active', idle: 'ok' };

export function appShell() {
  const call = invoke();
  if (!call || !window.__REEL_DESKTOP__) return;
  document.documentElement.dataset.app = 'desktop';
  appDownloads();
  $$('.browser-part').forEach(el => el.hidden = true);
  $$('.app-part:not(.app-paired), #app-share, #app-save, #app-save-share, #app-record, .sections .app-link').forEach(el => el.hidden = false);
  if (!document.body.hasAttribute('data-standalone')) $$('#sections-body .section-line').forEach(l => l.append(' Kept on the server: the same for every PC and browser.'));
  const chipDot = $('#chip-dot'), chipWord = $('#chip-word');
  const serverState = ['ok', 'warn', 'err'].find(c => chipDot?.classList.contains(c)) || 'ok';
  const pause = $('#app-pause'), recording = $('#app-recording'), share = $('#app-share');
  const record = $('#app-record') ? recordControl($('#app-record'), $('#menu-record'),
    (on) => call('app_session_set', { on }), (why) => toast(why, 'err')) : () => {};
  const render = (st) => {
    if (!st) return;
    record(st);
    const app = APP_DOT[st.state] || 'ok';
    setPart('part-app', app, st.state_word || st.state, st.state_line || '');
    setPart('part-obs', st.obs_connected ? 'ok' : 'neutral', st.obs_connected ? 'connected' : 'off', st.obs_line || '');
    if (document.documentElement.dataset.session !== 'out') {
      if (chipDot) chipDot.className = `dot ${worst([serverState, app])}`;
      if (chipWord) { chipWord.textContent = st.state_word || st.state; chipWord.className = `chip-word ${app === 'ok' ? '' : app}`; }
    }
    renderVersions(st.versions, st.update_line);
    renderAsk(st.update_ask);
    $$('.app-paired').forEach(el => el.hidden = !(st.panel ? st.panel.pause : st.paired));
    if (pause) setOn(pause, !!st.paused);
    if (recording) setOn(recording, !!st.recording_paused);
    if (share) share.disabled = !st.can_share;
    const here = document.body.hasAttribute('data-standalone');
    [['#app-save', '#menu-save', st.save, here ? 'OBS saves the replay, and it goes into the library on this PC' : 'OBS saves the replay and it goes up as any clip does'],
      ['#app-save-share', '#menu-save-share', st.save_share, 'OBS saves the replay, the app uploads it and copies the link']].forEach(([head, item, a, what]) => {
      for (const b of [$(head), $(item)]) {
        if (!b || !a) continue;
        b.disabled = !a.ok;
        b.title = a.ok ? what : `Not now: ${a.why}`;
      }
    });
    const note = $('#menu-save-why');
    if (note && st.save) {
      const why = !st.save.ok ? st.save.why : (!st.save_share.ok && $('#menu-save-share') ? `Save and share: ${st.save_share.why}` : '');
      note.textContent = why ? `Not now: ${why}` : '';
      note.hidden = !why;
    }
  };
  const poll = async () => {
    try { render(await call('app_status')); } catch (e) { /* the app went away; the page stays a page */ }
  };
  let timer = 0;
  const start = () => { poll(); clearInterval(timer); timer = setInterval(poll, 2000); };
  start();
  document.addEventListener('visibilitychange', () => { if (!document.hidden) poll(); });
  window.addEventListener('pagehide', () => clearInterval(timer));
  // Back brings a page out of the back/forward cache without running its scripts again.
  window.addEventListener('pageshow', (e) => { if (e.persisted) start(); });
  if (share) share.addEventListener('click', () => call('app_share_last').catch(() => {}));
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
  for (const [ids, cmd] of [[['#app-save', '#menu-save'], 'app_save_clip'], [['#app-save-share', '#menu-save-share'], 'app_save_and_share']]) {
    for (const id of ids) $(id)?.addEventListener('click', () => call(cmd).then(() => poll()).catch(() => {}));
  }
  if (pause) pause.addEventListener('change', async () => {
    try { render(await call('app_pause_set', { paused: isOn(pause) })); } catch (e) { /* as above */ }
  });
  if (recording) recording.addEventListener('change', async () => {
    try { render(await call('app_recording_pause_set', { paused: isOn(recording) })); } catch (e) { /* as above */ }
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
