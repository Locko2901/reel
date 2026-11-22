// A setting's explanation behind a question mark beside its name, on the server's Settings and on
// the app's own pages. Words marked with data-docs ("<page>#<anchor>" under DOCS) move into the
// tip unchanged, so the filter still reads them and a screen reader hears them on the icon.

export const DOCS = 'https://locko2901.github.io/reel/settings/';

const ICON = '<svg class="icon" viewBox="0 0 16 16" aria-hidden="true" focusable="false"><circle cx="8" cy="8" r="6.25"/><path d="M6.3 6.4a1.75 1.75 0 1 1 2.45 1.6c-.47.21-.75.6-.75 1.1v.35"/><path d="M8 11.45v.05"/></svg>';

let n = 0;
const el = (tag, cls, text) => { const e = document.createElement(tag); if (cls) e.className = cls; if (text != null) e.textContent = text; return e; };

export function docsPath(where) {
  const [page, anchor] = String(where).split('#');
  return `${page}/${anchor ? `#${anchor}` : ''}`;
}

function make(name, words) {
  const id = `tip-${++n}`;
  if (!name.id) name.id = `${id}-name`;
  if (!words.id) words.id = `${id}-words`;
  const btn = el('button', 'tip-btn');
  btn.type = 'button';
  btn.innerHTML = `<span id="${id}-about" hidden>About</span>${ICON}`;
  btn.setAttribute('aria-labelledby', `${id}-about ${name.id}`);
  btn.setAttribute('aria-describedby', words.id);
  btn.setAttribute('aria-expanded', 'false');
  btn.setAttribute('aria-controls', id);
  const tip = el('span', 'tip');
  tip.id = id;
  const more = el('a', 'tip-more', 'More in the docs');
  more.href = DOCS + docsPath(words.dataset.docs);
  more.dataset.docs = docsPath(words.dataset.docs);
  more.target = '_blank';
  more.rel = 'noopener';
  words.before(tip);
  tip.append(words, more);
  return { btn, tip };
}

// Rows, group lines and page lines under `root` get their icon; one that has it already is left.
export function enhance(root = document) {
  for (const words of root.querySelectorAll('.slabel > .shelp[data-docs]')) {
    const name = words.parentElement.querySelector(':scope > b');
    if (!name) continue;
    const sname = el('span', 'sname');
    name.before(sname);
    sname.append(name);
    const { btn, tip } = make(name, words);
    sname.append(btn);
    sname.after(tip);
  }
  for (const words of root.querySelectorAll('.panel > header > .desc[data-docs]')) {
    const name = words.parentElement.querySelector(':scope > h2');
    if (!name) continue;
    const hname = el('span', 'hname');
    name.before(hname);
    hname.append(name);
    const { btn, tip } = make(name, words);
    hname.append(btn, tip);
  }
  for (const words of root.querySelectorAll('.section-line[data-docs]')) {
    if (words.closest('.tip')) continue;
    const top = el('div', 'page-top');
    if (words.dataset.section) top.dataset.section = words.dataset.section;
    top.hidden = words.hidden;
    const name = el('h2', 'page-name', words.dataset.name || '');
    words.before(top);
    top.append(name);
    if (words.textContent.trim() || words.dataset.standaloneText) {
      const { btn, tip } = make(name, words);
      top.append(btn, tip);
    }
    if (words.dataset.scope) top.append(el('span', 'scope', words.dataset.scope));
  }
}

let open = null;
let shut = 0;
let quiet = false;

// Between the page's top bar and whatever covers its foot: the app's bar, a phone's tabs.
function bounds() {
  let top = 8, bottom = window.innerHeight - 8;
  for (const bar of document.querySelectorAll('header.top')) {
    const r = bar.getBoundingClientRect();
    if (r.height && getComputedStyle(bar).position !== 'static') top = Math.max(top, r.bottom + 6);
  }
  for (const bar of document.querySelectorAll('.appbar, .tabbar')) {
    const r = bar.getBoundingClientRect();
    if (r.height && getComputedStyle(bar).display !== 'none') bottom = Math.min(bottom, r.top - 6);
  }
  return { top, bottom };
}

function place(btn, tip) {
  const vw = document.documentElement.clientWidth, m = vw < 600 ? 12 : 8;
  const w = Math.min(320, vw - 2 * m);
  tip.style.width = `${w}px`;
  const r = btn.getBoundingClientRect(), b = bounds();
  const h = tip.offsetHeight;
  const left = Math.min(Math.max(m, r.left - 10), vw - w - m);
  let top = r.bottom + 6;
  if (top + h > b.bottom) top = r.top - 6 - h >= b.top ? r.top - 6 - h : Math.max(b.top, b.bottom - h);
  tip.style.left = `${Math.round(left)}px`;
  tip.style.top = `${Math.round(top)}px`;
}

function show(btn, pinned) {
  clearTimeout(shut);
  const tip = document.getElementById(btn.getAttribute('aria-controls'));
  if (!tip) return;
  if (open && open.btn !== btn) hide();
  tip.classList.add('open');
  btn.setAttribute('aria-expanded', 'true');
  btn.classList.toggle('pinned', !!pinned);
  open = { btn, tip, pinned: !!pinned || !!(open && open.btn === btn && open.pinned) };
  place(btn, tip);
}

function hide() {
  clearTimeout(shut);
  if (!open) return;
  open.tip.classList.remove('open');
  open.btn.setAttribute('aria-expanded', 'false');
  open.btn.classList.remove('pinned');
  open = null;
}

// A short wait, so the pointer can cross from the icon to the tip's link.
function later() { clearTimeout(shut); shut = setTimeout(() => { if (open && !open.pinned) hide(); }, 180); }

// The app opens the docs in the browser: its own pages through its opener; a server page in its
// window asks the app, which opens nothing but the docs (an app without that command: the opener).
function openDocs(e, more) {
  const tauri = window.__TAURI__;
  if (!tauri) return;
  e.preventDefault();
  const opener = () => (tauri.opener && tauri.opener.openUrl ? tauri.opener.openUrl(more.href) : Promise.reject());
  const own = location.protocol === 'tauri:' || location.hostname === 'tauri.localhost';
  Promise.resolve()
    .then(() => (own ? opener() : tauri.core.invoke('app_open_docs', { path: more.dataset.docs })))
    .catch(() => (own ? undefined : opener()))
    .catch(() => {});
}

let bound = false;
export function bindTips() {
  if (bound) return;
  bound = true;
  document.addEventListener('pointerover', (e) => {
    if (e.pointerType !== 'mouse') return;
    const btn = e.target.closest && e.target.closest('.tip-btn');
    if (btn) { if (!open || open.btn !== btn) show(btn, false); else clearTimeout(shut); return; }
    if (open && e.target.closest && e.target.closest('.tip') === open.tip) clearTimeout(shut);
  });
  document.addEventListener('pointerout', (e) => {
    if (e.pointerType !== 'mouse' || !open || open.pinned) return;
    const from = e.target.closest && (e.target.closest('.tip-btn') || e.target.closest('.tip'));
    const to = e.relatedTarget && e.relatedTarget.closest && (e.relatedTarget.closest('.tip-btn') || e.relatedTarget.closest('.tip'));
    if (from && (from === open.btn || from === open.tip) && to !== open.btn && to !== open.tip) later();
  });
  document.addEventListener('click', (e) => {
    const btn = e.target.closest && e.target.closest('.tip-btn');
    if (btn) {
      e.preventDefault();
      if (open && open.btn === btn && open.pinned) { hide(); return; }
      show(btn, true);
      return;
    }
    const more = e.target.closest && e.target.closest('.tip-more');
    if (more) { openDocs(e, more); return; }
    if (open && !(e.target.closest && e.target.closest('.tip') === open.tip)) hide();
  });
  document.addEventListener('focusin', (e) => {
    const btn = e.target.closest && e.target.closest('.tip-btn');
    if (btn && quiet) { quiet = false; return; }
    if (btn && btn.matches(':focus-visible')) { show(btn, false); return; }
    if (open && !open.pinned && !(e.target.closest && (e.target.closest('.tip') === open.tip || e.target === open.btn))) hide();
  });
  document.addEventListener('focusout', (e) => {
    if (!open || open.pinned) return;
    const to = e.relatedTarget;
    if (to && (to === open.btn || open.tip.contains(to))) return;
    if (e.target === open.btn || open.tip.contains(e.target)) later();
  });
  document.addEventListener('keydown', (e) => {
    if (e.key !== 'Escape' || !open) return;
    const back = open.tip.contains(document.activeElement) ? open.btn : null;
    hide();
    // back on the icon without opening its tip again
    if (back) { quiet = true; back.focus(); quiet = false; }
    e.stopPropagation();
  }, true);
  const again = () => { if (open) place(open.btn, open.tip); };
  addEventListener('scroll', again, { passive: true });
  addEventListener('resize', again);
}

// While the filter is on, a row found only by its tip's words shows them under its name.
export function tipHit(row, q) {
  const words = row.querySelector('.tip .shelp');
  const seen = `${row.querySelector('.slabel b')?.textContent || ''} ${row.dataset.key || ''} ${row.dataset.label || ''}`.toLowerCase();
  const hit = !!q && !!words && !seen.includes(q) && words.textContent.toLowerCase().includes(q);
  row.classList.toggle('tip-hit', hit);
  return hit;
}

export function tipHitsOff(root = document) {
  for (const r of root.querySelectorAll('.tip-hit')) r.classList.remove('tip-hit');
  hide();
}
