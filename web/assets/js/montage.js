import { $, $$, api, toast, fmt, parseTime } from './lib.js';
import { isOn } from './switch.js';

const two = (n) => String(n).padStart(2, '0');
export const words = (seconds) => {
  const s = Math.round(seconds), h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60);
  return h ? `${h}:${two(m)}:${two(s % 60)}` : `${m}:${two(s % 60)}`;
};

const SHORTEST = 0.5;

export function montage() {
  const root = $('#montage'), list = $('#montage-list');
  if (!root || !list) return;
  const sum = $('#montage-sum'), video = $('#m-video');
  const rows = () => $$('.montage-part', list);
  const used = () => rows().filter((p) => $('input[type=checkbox]', p).checked);
  const full = (p) => +p.dataset.seconds || 0;
  const start = (p) => +p.dataset.start || 0;
  const end = (p) => { const e = +p.dataset.end; return Number.isFinite(e) && e > 0 ? Math.min(e, full(p)) : full(p); };
  const length = (p) => Math.max(0, end(p) - start(p));
  const vol = (p) => { const v = +p.dataset.volume; return Number.isFinite(v) ? v : 1; };
  const join = $('#m-join'), joinLen = $('#m-join-len'), joinBox = $('#m-join-len-box');
  const overlap = () => (join.value === 'cut' ? 0 : +joinLen.value);
  const total = () => { const u = used(); return Math.max(0, u.reduce((t, p) => t + length(p), 0) - Math.max(0, u.length - 1) * overlap()); };

  // a value the lists do not have (one asked for by hand) is added to them, so it shows as it is
  const choose = (select, value, words) => {
    if (![...select.options].some((o) => o.value === value)) select.add(new Option(words, value));
    select.value = value;
  };
  join.value = root.dataset.join || 'cut';
  const joinWant = String(+root.dataset.joinSeconds || 0.5);
  choose(joinLen, joinWant, `${joinWant} s`);
  $('#m-shape').value = root.dataset.shape || 'wide';
  $('#m-fill').checked = root.dataset.fill === 'true';
  $('#m-even').checked = root.dataset.even !== 'false';

  let sel = null; // the clip whose part is open
  let seq = false; // playing the montage, part after part
  const load = (p, at, play) => new Promise((done) => {
    const go = () => { video.currentTime = Math.min(Math.max(0, at), Math.max(0, full(p) - 0.05)); if (play) video.play().catch(() => {}); done(); };
    applyVolume(p);
    if (video.dataset.src === p.dataset.src && video.readyState >= 1) { go(); return; }
    video.dataset.src = p.dataset.src;
    video.poster = p.dataset.poster;
    video.src = p.dataset.src;
    video.addEventListener('loadedmetadata', go, { once: true });
  });
  const applyVolume = (p) => { const v = vol(p); video.muted = v === 0; video.volume = Math.min(1, Math.max(0, v)); };
  const playing = () => (seq ? used().find((p) => p.dataset.src === video.dataset.src && p === seqPart) : sel);
  let seqPart = null;
  const before = (p) => { let t = 0; for (const q of used()) { if (q === p) return t; t += length(q); } return t; };
  const stopSeq = () => { seq = false; seqPart = null; $('#m-play').textContent = 'Play the montage'; };
  async function playFrom(p, at) {
    seq = true; seqPart = p; $('#m-play').textContent = 'Stop';
    select(p, false);
    await load(p, at, true);
  }
  $('#m-play').onclick = () => {
    if (seq) { video.pause(); stopSeq(); return; }
    const u = used(); if (!u.length) return;
    playFrom(u[0], start(u[0]));
  };
  const step = (by) => {
    const u = used(); if (!u.length) return;
    const at = u.indexOf(seq ? seqPart : sel);
    const p = u[Math.min(u.length - 1, Math.max(0, at + by))];
    if (seq) playFrom(p, start(p)); else { select(p, true); }
  };
  $('#m-prev').onclick = () => step(-1);
  $('#m-next').onclick = () => step(1);
  video.addEventListener('timeupdate', () => {
    const p = seq ? seqPart : sel;
    if (!p) return;
    if (video.currentTime >= end(p) - 0.02) {
      if (seq) {
        const u = used(), next = u[u.indexOf(p) + 1];
        if (next) { playFrom(next, start(next)); return; }
        video.pause(); stopSeq();
      } else if (!video.paused) { video.pause(); video.currentTime = end(p); }
    }
    showTime();
  });
  video.addEventListener('play', () => {
    const p = seq ? seqPart : sel;
    if (p && !seq && (video.currentTime < start(p) - 0.01 || video.currentTime >= end(p) - 0.03)) video.currentTime = start(p);
  });
  video.addEventListener('pause', () => { if (seq && video.currentTime < end(seqPart) - 0.05) stopSeq(); });

  function showTime() {
    const p = seq ? seqPart : sel;
    const all = used().reduce((t, q) => t + length(q), 0);
    const at = p && used().includes(p) ? before(p) + Math.min(length(p), Math.max(0, video.currentTime - start(p))) : 0;
    $('#m-now').textContent = `${words(at)} of ${words(total())}`;
    $('#m-head').style.left = `${all ? (at / all) * 100 : 0}%`;
    $('#m-head').hidden = !p || !used().includes(p);
    if (p && sel === p) $('#m-phead').style.left = `${(video.currentTime / (full(p) || 1)) * 100}%`;
  }

  const hs = $('#m-hs'), he = $('#m-he'), strip = $('#m-strip'), tStart = $('#m-t-start'), tEnd = $('#m-t-end'), volSel = $('#m-vol');
  // the keyframes of the part's version, asked for once each, and its ticks under the strip
  const snap = $('#m-snap'), kfHost = $('#m-keyframes'), asked = new Map();
  let kf = [], frame = 1 / 30;
  const nearest = (t) => kf.reduce((a, b) => (Math.abs(b - t) < Math.abs(a - t) ? b : a));
  const snapIf = (t) => {
    if (!isOn(snap) || !kf.length || !sel) return t;
    const k = nearest(t), radius = (10 / strip.clientWidth) * full(sel);
    return Math.abs(k - t) <= radius ? k : t;
  };
  async function keyframesOf(p) {
    const rid = p.dataset.rid;
    if (!asked.has(rid)) asked.set(rid, api('GET', `/api/renditions/${rid}/keyframes`).catch(() => { asked.delete(rid); return { keyframes: [] }; }));
    const d = await asked.get(rid);
    if (sel !== p) return;
    kf = d.keyframes || [];
    frame = 1 / (d.fps || 30);
    kfHost.replaceChildren(...kf.map((t) => { const i = document.createElement('i'); i.style.left = `${(t / (full(p) || 1)) * 100}%`; return i; }));
    renderPart();
  }
  function select(p, seek) {
    if (sel !== p) { kf = []; kfHost.replaceChildren(); keyframesOf(p); }
    sel = p;
    rows().forEach((q) => q.classList.toggle('is-open', q === p));
    $$('.m-block', $('#m-blocks')).forEach((b) => b.classList.toggle('is-open', b.dataset.id === p.dataset.id));
    $('#m-trim-title').textContent = p.dataset.title;
    const img = $('#m-strip-img');
    img.hidden = true;
    img.onload = () => { img.hidden = false; };
    img.onerror = () => { img.hidden = true; };
    img.src = p.dataset.src.replace(/\.mp4$/, '.strip.jpg');
    choose(volSel, String(vol(p)), `${Math.round(vol(p) * 100)}%`);
    renderPart();
    if (seek) { if (seq) { video.pause(); stopSeq(); } load(p, start(p), false); }
  }
  function renderPart() {
    const p = sel; if (!p) return;
    const d = full(p) || 1, l = (start(p) / d) * 100, r = (end(p) / d) * 100;
    $('#m-shade-l').style.width = `${l}%`; $('#m-shade-r').style.left = `${r}%`;
    $('#m-range').style.left = `${l}%`; $('#m-range').style.width = `${r - l}%`;
    hs.style.left = `calc(${l}% - 3px)`; he.style.left = `calc(${r}% - 3px)`;
    hs.setAttribute('aria-valuetext', fmt(start(p))); he.setAttribute('aria-valuetext', fmt(end(p)));
    $$('i', kfHost).forEach((i, n) => i.classList.toggle('near', Math.abs(kf[n] - start(p)) <= frame + 0.001));
    if (document.activeElement !== tStart) tStart.value = fmt(start(p));
    if (document.activeElement !== tEnd) tEnd.value = fmt(end(p));
    const trimmed = start(p) > 0.0005 || end(p) < full(p) - 0.01;
    $('#m-trim-len').textContent = trimmed ? `${words(length(p))} of ${words(full(p))}` : `all ${words(full(p))}`;
    show();
  }
  const setStart = (t) => { const p = sel; if (!p) return; p.dataset.start = String(Math.min(Math.max(0, t), end(p) - SHORTEST).toFixed(3)); renderPart(); };
  const setEnd = (t) => { const p = sel; if (!p) return; p.dataset.end = String(Math.max(Math.min(full(p), t), start(p) + SHORTEST).toFixed(3)); renderPart(); };
  const xToTime = (x) => { const r = strip.getBoundingClientRect(); return Math.min(full(sel), Math.max(0, ((x - r.left) / r.width) * full(sel))); };
  let dragging = null;
  for (const [h, which] of [[hs, 'start'], [he, 'end']]) {
    h.addEventListener('pointerdown', (e) => { dragging = which; h.setPointerCapture(e.pointerId); e.preventDefault(); h.focus({ preventScroll: true }); });
    h.addEventListener('pointermove', (e) => {
      if (dragging !== which || !sel) return;
      const t = xToTime(e.clientX);
      if (which === 'start') setStart(snapIf(t)); else setEnd(t);
      if (video.dataset.src === sel.dataset.src) video.currentTime = which === 'start' ? start(sel) : end(sel);
    });
    h.addEventListener('pointerup', () => { dragging = null; });
    h.addEventListener('keydown', (e) => {
      if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
      e.preventDefault();
      const dt = (e.key === 'ArrowLeft' ? -1 : 1) * (e.shiftKey ? 1 : 1 / 30);
      if (which === 'start') setStart(start(sel) + dt); else setEnd(end(sel) + dt);
    });
  }
  strip.addEventListener('pointerdown', (e) => {
    if (!sel || e.target.classList.contains('handle')) return;
    if (seq) { video.pause(); stopSeq(); }
    load(sel, xToTime(e.clientX), false);
  });
  tStart.addEventListener('change', () => { const t = parseTime(tStart.value); if (!isNaN(t)) setStart(snapIf(t)); else renderPart(); });
  tEnd.addEventListener('change', () => { const t = parseTime(tEnd.value); if (!isNaN(t)) setEnd(t); else renderPart(); });
  const here = () => (sel && video.dataset.src === sel.dataset.src ? video.currentTime : null);
  $('#m-set-start').onclick = () => { const t = here(); if (t !== null) setStart(snapIf(t)); };
  snap.addEventListener('change', () => { if (isOn(snap) && sel) setStart(snapIf(start(sel))); });
  $('#m-set-end').onclick = () => { const t = here(); if (t !== null) setEnd(t); };
  $('#m-whole').onclick = () => { if (!sel) return; sel.dataset.start = '0'; sel.dataset.end = String(full(sel)); renderPart(); };
  volSel.addEventListener('change', () => { if (!sel) return; sel.dataset.volume = volSel.value; if (video.dataset.src === sel.dataset.src) applyVolume(sel); show(); });
  document.addEventListener('keydown', (e) => {
    if (/^(INPUT|TEXTAREA|SELECT)$/.test(document.activeElement?.tagName) || e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.key === 'i') { const t = here(); if (t !== null) setStart(snapIf(t)); }
    else if (e.key === 'o') { const t = here(); if (t !== null) setEnd(t); }
  });

  function show() {
    const u = used(), n = u.length;
    const joinWords = join.value === 'cut' ? '' : `, ${join.value === 'fade' ? 'crossfades' : 'fades through black'} of ${joinLen.value} s`;
    sum.textContent = `${n} ${n === 1 ? 'clip' : 'clips'}, ${words(total())} together${joinWords}`;
    joinBox.hidden = join.value === 'cut';
    rows().forEach((p) => {
      p.classList.toggle('left-out', !u.includes(p));
      const trimmed = start(p) > 0.0005 || end(p) < full(p) - 0.01;
      const v = vol(p);
      const parts = [trimmed ? `${fmt(start(p))} to ${fmt(end(p))}, ${words(length(p))} of ${words(full(p))}` : words(full(p))];
      if (v !== 1) parts.push(v === 0 ? 'silent' : `${Math.round(v * 100)}% volume`);
      $('.part-line', p).textContent = parts.join(', ');
    });
    const all = u.reduce((t, p) => t + length(p), 0) || 1;
    const blocks = $('#m-blocks');
    blocks.replaceChildren(...u.map((p, i) => {
      const b = document.createElement('button');
      b.type = 'button';
      b.className = 'm-block' + (p === sel ? ' is-open' : '');
      b.dataset.id = p.dataset.id;
      b.style.flexGrow = String(length(p) / all);
      b.style.setProperty('--poster', `url("${p.dataset.poster}")`);
      b.title = `${i + 1}. ${p.dataset.title}, ${words(length(p))}`;
      b.setAttribute('aria-label', b.title);
      const t = document.createElement('span');
      t.textContent = `${i + 1}. ${p.dataset.title}`;
      b.append(t);
      b.onclick = (e) => {
        const r = b.getBoundingClientRect();
        const at = start(p) + ((e.clientX - r.left) / r.width) * length(p);
        if (seq) playFrom(p, at); else { select(p, false); if (seq) stopSeq(); load(p, at, false); }
      };
      return b;
    }));
    const go = $('#montage-render');
    if (!go.dataset.busy) go.disabled = n < 2;
    showTime();
  }
  join.addEventListener('change', show);
  joinLen.addEventListener('change', show);

  const move = (part, by) => {
    if (by < 0) { if (part.previousElementSibling) list.insertBefore(part, part.previousElementSibling); }
    else { const next = part.nextElementSibling; if (next) list.insertBefore(part, next.nextElementSibling); }
    show();
  };
  list.addEventListener('click', (e) => {
    const b = e.target.closest('.up, .down');
    if (b) { move(b.closest('.montage-part'), b.classList.contains('up') ? -1 : 1); b.focus(); return; }
    const words = e.target.closest('.part-words');
    if (words) select(words.closest('.montage-part'), true);
  });
  list.addEventListener('change', show);
  let dragged = null;
  list.addEventListener('dragstart', (e) => { dragged = e.target.closest('.montage-part'); dragged?.classList.add('dragging'); e.dataTransfer.effectAllowed = 'move'; });
  list.addEventListener('dragend', () => { dragged?.classList.remove('dragging'); dragged = null; show(); });
  list.addEventListener('dragover', (e) => {
    const over = e.target.closest('.montage-part');
    if (!dragged || !over || over === dragged) return;
    e.preventDefault();
    const r = over.getBoundingClientRect();
    list.insertBefore(dragged, e.clientY < r.top + r.height / 2 ? over : over.nextElementSibling);
  });

  const first = used()[0] || rows()[0];
  if (first) select(first, true);
  show();

  const go = $('#montage-render');
  go.addEventListener('click', async () => {
    const u = used();
    if (u.length < 2) { toast('A montage needs two clips or more.', 'err'); return; }
    const parts = u.map((p) => {
      const part = { clip: p.dataset.id };
      if (start(p) > 0.0005) part.start = start(p);
      if (end(p) < full(p) - 0.01) part.end = end(p);
      if (vol(p) !== 1) part.volume = vol(p);
      return part;
    });
    const body = {
      parts,
      title: $('#montage-title').value,
      join: join.value,
      join_seconds: overlap() || undefined,
      shape: $('#m-shape').value,
      fill: $('#m-fill').checked,
      even_loudness: $('#m-even').checked,
    };
    const replace = $('#m-replace');
    if (replace?.checked) body.replaces = replace.dataset.id;
    go.disabled = true; go.dataset.busy = '1';
    video.pause();
    try {
      const d = await api('POST', '/api/montage', body);
      toast(`${d.title}: rendering. Its page shows the progress.`, 'ok');
      location.href = `/clips/${d.clip_id}`;
    } catch (err) { toast(err.message, 'err'); go.disabled = false; delete go.dataset.busy; }
  });
}
