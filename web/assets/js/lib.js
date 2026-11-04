export const $ = (s, r = document) => r.querySelector(s);
export const $$ = (s, r = document) => [...r.querySelectorAll(s)];
export const settings = (() => { try { return JSON.parse(document.body.dataset.settings || '{}'); } catch { return {}; } })();

// `link`: where the words lead, as a delete's "In the trash for 7 days" leads to the trash.
export function toast(text, kind = '', action = null, link = '') {
  const host = $('#toasts');
  if (!host) return;
  const el = document.createElement('div');
  el.className = `toast ${kind}`;
  el.setAttribute('role', 'status');
  const words = document.createElement(link ? 'a' : 'span');
  words.className = 'toast-words';
  words.textContent = text;
  if (link) words.href = link;
  const acts = document.createElement('span');
  acts.className = 'toast-acts';
  if (action) {
    const b = document.createElement('button');
    b.className = 'small';
    b.textContent = action.label;
    b.onclick = () => { action.run(); el.remove(); };
    acts.append(b);
  }
  const x = document.createElement('button');
  x.className = 'quiet small';
  x.textContent = 'Dismiss';
  x.onclick = () => el.remove();
  acts.append(x);
  el.append(words, ' ', acts);
  host.replaceChildren(el);
  setTimeout(() => el.remove(), action ? 9000 : 4000);
}

// From the keyboard the focus goes to `first`, from a pointer to the dialog itself so no ring
// shows; on closing, the ring WebKit would draw on the focus given back is taken off.
export function focusOnOpen(dlg, event, first) {
  if (event && event.detail === 0 && first) { first.focus(); return; }
  if (!dlg.hasAttribute('tabindex')) dlg.tabIndex = -1;
  dlg.focus({ preventScroll: true });
  if (dlg.tagName === 'DIALOG') dlg.addEventListener('close', () => { const a = document.activeElement; if (a && a !== document.body && a.matches(':focus-visible')) a.blur(); }, { once: true });
}

export async function copyText(text) {
  try { await navigator.clipboard.writeText(text); return true; }
  catch {
    const ta = document.createElement('textarea');
    ta.value = text; document.body.append(ta); ta.select();
    const ok = document.execCommand('copy'); ta.remove(); return ok;
  }
}

// The clipboard is asked inside the click, so the browser lets the write happen when the text
// comes; where it cannot hold a promise, the text is copied once it is there.
export async function copyWhenReady(promise) {
  if (window.ClipboardItem && navigator.clipboard?.write) {
    try {
      await navigator.clipboard.write([new ClipboardItem({ 'text/plain': promise.then(t => new Blob([t], { type: 'text/plain' })) })]);
      return true;
    } catch { /* below */ }
  }
  try { return await copyText(await promise); } catch { return false; }
}

export async function api(method, url, body) {
  const res = await fetch(url, { method, headers: body ? { 'content-type': 'application/json' } : {}, body: body ? JSON.stringify(body) : undefined });
  let data = null;
  try { data = await res.json(); } catch {}
  if (!res.ok) throw new Error((data && data.error) || `${res.status} ${res.statusText}`);
  return data;
}

export { precise as fmt, parseTime } from './format.js';

let sse = null, retry = null, wait = 0, missed = false, signedOut = false, before = null;
const listeners = new Set();
const FIRST_WAIT = 3000, LONGEST_WAIT = 60000;

async function probe() {
  try {
    const res = await fetch('/api/jobs/counts', { redirect: 'manual', cache: 'no-store', headers: { accept: 'application/json' } });
    if (res.type === 'opaqueredirect' || res.status === 401) return 'out';
    if (res.status >= 500) return 'down';
    if (!(res.headers.get('content-type') || '').includes('json')) return 'out';
    return res.ok ? 'in' : 'down';
  } catch { return 'down'; }
}

function session(out) {
  if (out === signedOut) return;
  signedOut = out;
  document.documentElement.dataset.session = out ? 'out' : 'in';
  const chip = $('#chip'), dot = $('#chip-dot'), word = $('#chip-word'), part = $('#part-server');
  if (!chip || !dot || !word) return;
  if (out) {
    before = { dot: dot.className, word: word.textContent, wordClass: word.className, label: chip.getAttribute('aria-label'), part: part?.innerHTML };
    dot.className = 'dot neutral';
    word.textContent = 'signed out';
    word.className = 'chip-word';
    chip.setAttribute('aria-label', 'Status: signed out, reload to sign in');
    if (part) { $('.dot', part).className = 'dot neutral'; $('b', part).textContent = 'signed out'; $('.muted', part).textContent = 'reload to sign in'; }
  } else if (before) {
    dot.className = before.dot;
    word.textContent = before.word;
    word.className = before.wordClass;
    chip.setAttribute('aria-label', before.label || '');
    if (part && before.part != null) part.innerHTML = before.part;
    before = null;
  }
}

function connect() {
  clearTimeout(retry);
  retry = null;
  const es = new EventSource('/api/events');
  sse = es;
  es.onopen = () => {
    wait = 0;
    if (missed) { missed = false; listeners.forEach(f => f({ type: 'resumed' })); }
  };
  es.onmessage = (e) => {
    let ev; try { ev = JSON.parse(e.data); } catch { return; }
    // the server fell behind and skipped events: as a stream that came back
    if (ev.type === 'missed') ev = { type: 'resumed' };
    listeners.forEach(f => f(ev));
  };
  es.onerror = () => {
    es.close();
    if (sse !== es) return;
    sse = null;
    missed = true;
    check();
  };
}

async function check() {
  session(await probe() === 'out');
  wait = Math.min(wait ? wait * 2 : FIRST_WAIT, LONGEST_WAIT);
  clearTimeout(retry);
  retry = setTimeout(() => {
    retry = null;
    if (sse || !listeners.size) return;
    if (signedOut) check(); else connect();
  }, wait);
}

async function wake() {
  if (sse || !listeners.size || (!retry && !signedOut)) return;
  if (await probe() !== 'in' || sse) return;
  session(false);
  wait = 0;
  connect();
}
window.addEventListener('focus', wake);
document.addEventListener('visibilitychange', () => { if (document.visibilityState === 'visible') wake(); });

export function onEvent(fn) {
  listeners.add(fn);
  if (!sse && !retry && !signedOut) connect();
  return () => listeners.delete(fn);
}

// A stream that dropped or skipped an event would leave the page waiting for good, so `ask`
// also looks every 3 s, and at once when the stream comes back.
export function waitFor(event, ask, every = 3000) {
  return new Promise((resolve, reject) => {
    let over = false, asking = false, timer = null, off = null;
    const end = (settle) => (v) => { if (over) return; over = true; clearTimeout(timer); off?.(); settle(v); };
    const done = end(resolve), fail = end(reject);
    const tick = async () => {
      clearTimeout(timer);
      if (over || asking) return;
      asking = true;
      try { await ask(done, fail); } catch { /* asked again on the next tick */ }
      asking = false;
      if (!over) timer = setTimeout(tick, every);
    };
    off = onEvent((ev) => {
      if (over) return;
      if (ev.type === 'resumed') tick(); else event(ev, done, fail);
    });
    timer = setTimeout(tick, every);
  });
}

export async function jobNow(id) {
  if (!id) return null;
  const r = await fetch(`/api/jobs/${id}`, { cache: 'no-store', headers: { accept: 'application/json' } });
  if (r.status === 404) return null;
  if (!r.ok) throw new Error(`${r.status}`);
  return r.json();
}
export const jobEnded = (j, failed) => j?.state === 'failed' ? new Error(j.error || failed) : j?.state === 'cancelled' ? Object.assign(new Error('cancelled in Jobs'), { cancelled: true }) : null;
export const headOf = (url) => fetch(url, { method: 'HEAD', cache: 'no-store' });

export function arm(button, label, fallback) {
  if (button.dataset.armed === '1') return true;
  button.dataset.armed = '1';
  button.textContent = label;
  button.classList.add('danger');
  setTimeout(() => { button.dataset.armed = ''; button.textContent = fallback; button.classList.remove('danger'); }, 3000);
  return false;
}
