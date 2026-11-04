// One capture phase handler on the window takes a player's key before the browser's own controls,
// so a focused video or its timeline never toggles or seeks twice and the page does not scroll.

const TYPED = /^(text|search|email|url|tel|password|number|date|datetime-local|month|time|week)$/;
const SPACE_ROLES = new Set(['button', 'switch', 'checkbox', 'radio', 'menuitem', 'tab', 'option']);
const SPACE_INPUTS = new Set(['checkbox', 'radio', 'button', 'submit', 'reset', 'color', 'file']);
const ARROW_ROLES = new Set(['slider', 'spinbutton', 'radiogroup', 'tablist', 'menu', 'listbox']);
const ARROW_INPUTS = new Set(['range', 'radio']);

const clicked = new WeakSet();
function followClicks() {
  let pressed = null;
  addEventListener('pointerdown', (e) => {
    pressed = e.target;
    const a = document.activeElement;
    if (a && a !== document.body && pressed instanceof Node && a.contains(pressed)) clicked.add(a);
  }, true);
  addEventListener('keydown', () => { pressed = null; }, true);
  addEventListener('focusin', (e) => {
    const el = e.target;
    if (pressed instanceof Node && (el.contains(pressed) || pressed.contains(el))) clicked.add(el);
    else clicked.delete(el);
    pressed = null;
  }, true);
}

function typing(el) {
  if (!el) return false;
  if (el.isContentEditable || el.tagName === 'TEXTAREA') return true;
  return el.tagName === 'INPUT' && TYPED.test(el.type || 'text');
}

function keeps(el, key) {
  if (!el || el === document.body || el.tagName === 'VIDEO') return false;
  const role = el.getAttribute('role'), input = el.tagName === 'INPUT' ? el.type : null;
  if (key.startsWith('Arrow')) return el.tagName === 'SELECT' || ARROW_ROLES.has(role) || ARROW_INPUTS.has(input);
  if (key === ' ') return !clicked.has(el) && (/^(BUTTON|SUMMARY|SELECT)$/.test(el.tagName) || SPACE_ROLES.has(role) || SPACE_INPUTS.has(input));
  return false;
}

export function playerKeys(video, { box = video.parentElement, fps = 60, stepped = () => {} } = {}) {
  followClicks();
  const frame = 1 / fps;
  const end = () => (Number.isFinite(video.duration) ? video.duration : Infinity);
  let want = null, sent = null;
  const go = () => { sent = want; video.currentTime = want; };
  // one seek at a time: a held key moves the target, and the next seek starts when one ends
  video.addEventListener('seeked', () => { if (want !== null && want !== sent) go(); else want = sent = null; });
  video.addEventListener('loadstart', () => { want = sent = null; });
  const seek = (d) => {
    want = Math.max(0, Math.min(end(), (want ?? video.currentTime) + d));
    if (sent === null || !video.seeking) go();
  };

  let hint = null, hideAt = null, run = { sign: 0, sum: 0, at: 0 };
  const say = (words) => {
    if (!box) return;
    if (!hint) { hint = document.createElement('span'); hint.className = 'key-hint'; hint.setAttribute('aria-hidden', 'true'); box.append(hint); }
    hint.textContent = words;
    hint.classList.add('shown');
    clearTimeout(hideAt);
    hideAt = setTimeout(() => hint.classList.remove('shown'), 800);
  };
  const jump = (d, at) => {
    if (video.paused) stepped();
    seek(d);
    const sign = Math.sign(d);
    run = run.sign === sign && at - run.at < 1000 ? { sign, sum: run.sum + d, at } : { sign, sum: d, at };
    say(`${d > 0 ? '+' : '-'}${Math.round(Math.abs(run.sum) * 10) / 10} s`);
  };
  const step = (d) => { stepped(); video.pause(); seek(d); };
  const volume = (d) => {
    video.muted = false;
    video.volume = Math.max(0, Math.min(1, Math.round((video.volume + d) * 20) / 20));
    say(`volume ${Math.round(video.volume * 100)}%`);
  };
  const toggle = () => { if (video.paused) video.play().catch(() => {}); else video.pause(); };
  const fullscreen = () => {
    const on = document.fullscreenElement || document.webkitFullscreenElement;
    if (on) (document.exitFullscreen || document.webkitExitFullscreen).call(document)?.catch?.(() => {});
    else (box.requestFullscreen || box.webkitRequestFullscreen || (() => {})).call(box)?.catch?.(() => {});
  };

  // Chromium's own controls keep every key once a click the page never sees focuses one: when
  // they take the focus without a Tab, or act, it goes back to the video, whose keys come here
  let tabAt = -1e9;
  const home = () => { if (document.activeElement === video) video.focus({ preventScroll: true }); };
  video.addEventListener('focusin', (e) => { if (e.timeStamp - tabAt > 500) home(); });
  for (const t of ['play', 'pause', 'seeking', 'volumechange', 'ratechange']) video.addEventListener(t, home);
  for (const t of ['fullscreenchange', 'webkitfullscreenchange']) document.addEventListener(t, home);

  let chordAt = -1e9, spaceUp = false;
  addEventListener('keydown', (e) => {
    if (e.key === 'Tab') tabAt = e.timeStamp;
    if (e.defaultPrevented || e.ctrlKey || e.metaKey || e.altKey || e.isComposing) return;
    const el = document.activeElement;
    if (typing(el) || document.querySelector('dialog[open]')) return;
    // g and the key after it are the page's chords (g l, g j)
    if (e.key === 'g' && !e.shiftKey) { chordAt = e.timeStamp; return; }
    if (e.timeStamp - chordAt < 900) { chordAt = -1e9; return; }
    const key = e.key.length === 1 && !e.shiftKey ? e.key.toLowerCase() : e.key;
    if (keeps(el, key)) return;
    const by = e.shiftKey ? 1 : 5;
    const once = !e.repeat;
    switch (key) {
      case ' ': case 'k': if (once) toggle(); if (key === ' ') spaceUp = true; break;
      case 'ArrowLeft': jump(-by, e.timeStamp); break;
      case 'ArrowRight': jump(by, e.timeStamp); break;
      case 'j': jump(-10, e.timeStamp); break;
      case 'l': jump(10, e.timeStamp); break;
      case ',': step(-frame); break;
      case '.': step(frame); break;
      case 'ArrowUp': volume(0.1); break;
      case 'ArrowDown': volume(-0.1); break;
      case 'm': if (once) { video.muted = !video.muted; say(video.muted ? 'muted' : `volume ${Math.round(video.volume * 100)}%`); } break;
      case 'f': if (once) fullscreen(); break;
      default: return;
    }
    e.preventDefault();
    e.stopImmediatePropagation();
  }, true);
  // a button a click focused would take the Space it gave up when the key comes up
  addEventListener('keyup', (e) => {
    if (e.key !== ' ' || !spaceUp) return;
    spaceUp = false;
    e.preventDefault();
    e.stopImmediatePropagation();
  }, true);
}
