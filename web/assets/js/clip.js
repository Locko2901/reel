import { $, $$, api, arm, copyText, copyWhenReady, focusOnOpen, headOf, jobEnded, jobNow, settings, toast, waitFor } from './lib.js';
import { gain as dB, level, precise as fmt, parseTime, seconds, size, split } from './format.js';
import { isOn, setOn } from './switch.js';
import { markNativePlay, rememberPlayback } from './playback.js';
import { playerKeys } from './player-keys.js';
import { backLink, goBack } from './back.js';
import { animMade } from './anim-made.js';

function bindText(id) {
  const title = $('#title'), desc = $('#description');
  if (title.readOnly) return;
  title.addEventListener('change', async () => {
    const t = title.value.trim();
    if (!t) { toast('A clip needs a title.', 'err'); return; }
    try { await api('PATCH', `/api/clips/${id}`, { title: t }); document.title = `${t} · reel`; toast('Title saved.', 'ok'); } catch (err) { toast(err.message, 'err'); }
  });
  desc.addEventListener('change', async () => { try { await api('PATCH', `/api/clips/${id}`, { description: desc.value }); toast('Description saved.', 'ok'); } catch (err) { toast(err.message, 'err'); } });
}

function followProcessing(id) {
  const show = (kind, progress) => { const t = $('#processing-text'); if (t) t.textContent = `${kind} ${Math.round(progress * 100)}%`; };
  waitFor(
    (ev, done) => {
      if (ev.type === 'job' && ev.clip_id === id && ev.state === 'running') show(ev.kind, ev.progress);
      if (ev.type === 'clip' && ev.id === id && ev.state !== 'processing') done();
    },
    async (done) => {
      const r = await fetch(`/api/clips/${id}`, { cache: 'no-store', headers: { accept: 'application/json' } });
      if (r.status === 404) { done(); return; }
      const d = await r.json();
      if (d.clip.state !== 'processing') { done(); return; }
      const j = d.jobs.find((j) => j.state === 'running');
      if (j) show(j.kind, j.progress);
    },
  ).then(() => setTimeout(() => location.reload(), 500));
}

function bindDelete(id) {
  if (!$('#delete')) return;
  $('#delete').onclick = async (e) => {
    if (!arm(e.currentTarget, 'Really delete?', 'Delete clip')) return;
    try {
      const d = await api('DELETE', `/api/clips/${id}`);
      let undone = false;
      toast(`In the trash for ${d.trash_days} days.`, '', { label: 'Undo', run: async () => { undone = true; await api('POST', `/api/clips/${id}/restore`); toast('Back in the library.', 'ok'); } }, '/trash');
      setTimeout(() => { if (!undone) goBack(); }, 8500);
    } catch (err) { toast(err.message, 'err'); }
  };
}

function bindDiscard(id) {
  if (!$('#discard')) return;
  $('#discard').onclick = async (e) => {
    if (!arm(e.currentTarget, 'File and clip go', 'Discard')) return;
    try {
      await api('POST', `/api/clips/${id}/discard`);
      toast('Discarded, file and clip.', 'ok');
      setTimeout(goBack, 600);
    } catch (err) { toast(err.message, 'err'); }
  };
}

function openLinkDialog(renditionId, e) {
  const dlg = $('#link-dialog'), f = $('#link-form');
  setOn($('#allow-download'), settings.share_download_default !== false);
  f.expires_in.value = settings.share_expiry_default || 'never';
  f.label.value = '';
  f.rendition_id.value = renditionId || '';
  dlg.showModal();
  focusOnOpen(dlg, e, f.label);
}

function addLinkRow(s, pinned) {
  $('#no-shares')?.remove();
  const row = document.createElement('div');
  row.className = 'linkrow'; row.dataset.sid = s.id; row.dataset.active = '';
  row.innerHTML = `<span class="value"><span class="trunc"><span class="a"></span><span class="b"></span></span></span><div class="row"><button class="small copy" type="button">Copy</button><button class="danger small revoke" type="button">Revoke</button></div><div class="meta sub"><span></span><span class="serves"></span><span>${s.activity}</span><span>download ${s.download ? 'on' : 'off'}</span><span>${s.expiry}</span></div>`;
  const [head, tail] = split(s.url);
  $('.trunc .a', row).textContent = head; $('.trunc .b', row).textContent = tail; $('.trunc', row).title = s.url;
  $('.copy', row).dataset.copy = s.url; $('.revoke', row).dataset.sid = s.id;
  $('.meta span', row).textContent = s.label ? `${s.label}, ${s.created}` : `created ${s.created}`;
  $('.serves', row).textContent = s.serves;
  if (pinned) $('.serves', row).classList.add('st-active');
  if (s.preview) { const p = document.createElement('span'); p.textContent = s.preview; $('.meta span:nth-child(3)', row).after(p); }
  $('#shares').prepend(row);
  return row;
}

function followLinkWait(id, rid, show) {
  let stopped = false, timer = null;
  const words = (w) => {
    const pct = w.progress > 0 ? `, ${Math.round(w.progress * 100)}%` : '';
    if (w.step === 'preview') return `Making the link: preview being made${pct}`;
    if (w.step === 'upload') return `Making the link: preview going up${pct}`;
    return 'Making the link';
  };
  const tick = async () => {
    try {
      const w = await api('GET', `/api/clips/${id}/link-wait?r=${encodeURIComponent(rid || '')}`);
      if (!stopped) show(words(w));
    } catch { /* the next tick asks again */ }
    if (!stopped) timer = setTimeout(tick, 1000);
  };
  timer = setTimeout(tick, 600);
  return () => { stopped = true; clearTimeout(timer); };
}

// The newest link that still plays: the Share panel lists the expired ones too.
const liveLink = () => $('#shares .linkrow[data-active] .copy');

// Phones and tablets only, never a PC, whatever its browser offers and however narrow its window.
const handheld = () => matchMedia('(pointer: coarse) and (hover: none)').matches;

function bindShares(id) {
  const dlg = $('#link-dialog'), open = $('#new-link');
  if (!open) return;
  open.onclick = (e) => openLinkDialog('', e);
  $$('.share-version').forEach(b => b.onclick = (e) => openLinkDialog(b.dataset.rid, e));
  $('#link-cancel').onclick = () => dlg.close();
  $('#link-create').onclick = async () => {
    const f = $('#link-form'), go = $('#link-create');
    if (go.disabled) return;
    const made = api('POST', `/api/clips/${id}/shares`, { label: f.label.value, expires_in: f.expires_in.value, allow_download: isOn($('#allow-download')), rendition_id: f.rendition_id.value });
    const copied = copyWhenReady(made.then(s => s.url));
    go.disabled = true; go.textContent = 'Making the link';
    const stop = followLinkWait(id, f.rendition_id.value, (t) => { go.textContent = t; });
    try {
      const s = await made;
      dlg.close();
      addLinkRow(s, !!f.rendition_id.value);
      const note = s.preview_note ? ` ${s.preview_note}` : '';
      if (await copied) toast(`Link made and copied. Go paste it.${note}`, 'ok'); else toast(`Link made.${note}`, 'ok');
    } catch (err) { toast(err.message, 'err'); }
    stop();
    go.disabled = false; go.textContent = 'Create and copy';
  };
}

function bindVersions(id, video) {
  $$('.make-current').forEach(b => b.onclick = async () => {
    try { const d = await api('POST', `/api/clips/${id}/current`, { rendition_id: b.dataset.rid }); toast(document.body.hasAttribute('data-standalone') ? `${d.current} is the current version now.` : `${d.current} is what the links show now.`, 'ok'); setTimeout(() => location.reload(), 600); } catch (err) { toast(err.message, 'err'); }
  });
  $$('.play-version').forEach(b => b.onclick = () => {
    const row = b.closest('.v');
    $$('#versions .v').forEach(v => v.classList.remove('playing'));
    row.classList.add('playing');
    playVersion(video, row.dataset.rid, row.dataset.url, row.dataset.poster, row.dataset.label, '');
    window.scrollTo({ top: 0, behavior: 'smooth' });
  });
  $$('.delete-version').forEach(b => b.onclick = async () => {
    if (!arm(b, 'Sure?', 'Delete')) return;
    try {
      await api('DELETE', `/api/renditions/${b.dataset.rid}`);
      b.closest('.v').remove();
      toast('Cut deleted. The original is untouched.', 'ok');
    } catch (err) { toast(err.message, 'err'); }
  });
}

function playVersion(video, rid, url, poster, label, note) {
  video.closest('.player')?.classList.add('started');
  video.pause();
  video.dataset.rid = rid;
  video.src = url; video.poster = poster;
  video.load(); video.play().catch(() => {});
  const text = $('#playing-label');
  if (text) text.innerHTML = `playing <b>${label}</b>${note}`;
}

const POSTER_MB = 25;
function bindPoster(id, video) {
  const again = $('#poster-frame'), file = $('#poster-file'), dlg = $('#poster-dialog'), preview = $('#poster-preview');
  const show = (url) => { video.poster = url; const cover = $('.player .cover'); if (cover) cover.src = url; };
  const frame = async (time, words) => {
    try {
      const d = await api('POST', `/api/clips/${id}/poster`, { time });
      show(d.poster);
      if (again) again.hidden = true;
      toast(words, 'ok');
    } catch (err) { toast(err.message, 'err'); }
  };
  $('#poster-btn').onclick = () => frame(video.currentTime, 'Poster set to this frame.');
  if (again) again.onclick = () => frame(+$('#poster-acts').dataset.time || 0, 'The poster is a frame of the video again.');
  if (!file || !dlg) return;
  let picked = null, url = '';
  $('#poster-upload').onclick = () => file.click();
  file.onchange = (e) => {
    picked = file.files[0];
    file.value = '';
    if (!picked) return;
    if (picked.size > POSTER_MB * 1024 * 1024) { toast(`The image is over ${POSTER_MB} MB.`, 'err'); return; }
    if (url) URL.revokeObjectURL(url);
    url = URL.createObjectURL(picked);
    preview.src = url;
    dlg.showModal();
    focusOnOpen(dlg, e, $('#poster-save'));
  };
  preview.onerror = () => { if (dlg.open) { dlg.close(); toast('That is not a JPEG, PNG or WebP image.', 'err'); } };
  $('#poster-cancel').onclick = () => dlg.close();
  $('#poster-save').onclick = async () => {
    const save = $('#poster-save');
    save.disabled = true;
    try {
      const r = await fetch(`/api/clips/${id}/poster/image`, { method: 'POST', body: picked, headers: { 'content-type': 'application/octet-stream' } });
      const d = await r.json().catch(() => ({}));
      if (!r.ok) throw new Error(d.error || `the upload failed (${r.status})`);
      show(d.poster);
      if (again) again.hidden = false;
      dlg.close();
      toast('The image is the poster now.', 'ok');
    } catch (err) { toast(err.message, 'err'); }
    save.disabled = false;
  };
}

function audioEditor(id, video, root, toMaster, onChange) {
  let info = {}; try { info = JSON.parse($('#level-json').textContent) || {}; } catch {}
  const mute = $('#mute'), range = $('#gain'), num = $('#gain-num'), reset = $('#gain-reset'), norm = $('#normalize'), vol = $('#vol'), line = $('#level-line');
  const rows = $$('#tracks [data-track]').map((row, i) => ({ i, row, name: $('.track-name', row).textContent, mute: $('button[role="switch"]', row), range: $('input[type="range"]', row), num: $('input[type="number"]', row), vol: $('.vol', row), gain: 0 }));
  const multi = rows.length > 1;
  let gainDb = 0, graph = null, failed = '', applying = false;
  const silent = () => info.measured && info.peak == null;
  const playing = () => rows.filter(r => !isOn(r.mute));
  const asItWas = () => rows.every(r => isOn(r.mute) === (r.i !== 0) && r.gain === 0);
  const changed = () => info.audio && !isOn(mute) && (gainDb !== 0 || isOn(norm) || (multi && !asItWas()));
  const room = () => Math.round((info.ceiling - info.peak) * 10) / 10;
  const levelling = () => Math.round((info.target - info.loudness) * 10) / 10;
  const onMaster = () => (video.dataset.rid || root.dataset.current) === root.dataset.master;
  const mixing = () => graph?.tracks && onMaster();
  function follow() {
    const els = () => graph.tracks.map(t => t.el);
    const sync = (force) => els().forEach((el) => {
      if (force || Math.abs(el.currentTime - video.currentTime) > 0.15) el.currentTime = video.currentTime;
      el.playbackRate = video.playbackRate;
    });
    const start = () => { if (!mixing()) return; sync(true); els().forEach(el => el.play().catch(() => {})); };
    video.addEventListener('play', start);
    video.addEventListener('pause', () => els().forEach(el => el.pause()));
    video.addEventListener('seeked', () => { if (mixing()) sync(true); });
    video.addEventListener('ratechange', () => sync(false));
    video.addEventListener('timeupdate', () => { if (mixing() && !video.paused) sync(false); });
    if (!video.paused) start();
  }
  function ensureGraph() {
    if (graph || !window.AudioContext) return;
    try {
      const ctx = new AudioContext(), master = ctx.createGain(), own = ctx.createGain();
      master.connect(ctx.destination);
      ctx.createMediaElementSource(video).connect(own).connect(master);
      video.addEventListener('play', () => ctx.resume());
      video.addEventListener('loadstart', applyGain);
      graph = { ctx, master, own, tracks: null };
      if (multi && info.master) {
        graph.tracks = rows.map((r) => {
          const el = new Audio(`/media/renditions/${info.master}/track/${r.i}.m4a`);
          el.preload = 'auto';
          const g = ctx.createGain();
          ctx.createMediaElementSource(el).connect(g).connect(master);
          return { el, gain: g };
        });
        follow();
      }
    } catch { graph = null; }
  }
  function applyGain() {
    if (!graph) return;
    const set = (node, v) => node.gain.setTargetAtTime(v, graph.ctx.currentTime, 0.015);
    const on = onMaster();
    if (graph.tracks) {
      graph.tracks.forEach((t, i) => set(t.gain, on && !isOn(rows[i].mute) ? Math.pow(10, rows[i].gain / 20) : 0));
      if (!on) graph.tracks.forEach(t => t.el.pause());
    }
    set(graph.own, graph.tracks && on ? 0 : 1);
    let value = 1;
    if (on) {
      const db = isOn(norm) ? (info.loudness != null ? levelling() : 0) : gainDb;
      value = isOn(mute) ? 0 : Math.pow(10, db / 20);
    }
    set(graph.master, value);
  }
  function describe() {
    if (!info.audio) return 'The original has no audio.';
    if (failed) return `The audio could not be measured: ${failed}`;
    if (!info.measured) return "Measuring the original's audio.";
    if (silent()) return "The original's audio is silent.";
    let text = `${multi ? 'Its loudest track peaks' : 'The original peaks'} at ${level(info.peak, 'dBTP')} and plays at ${level(info.loudness, 'LUFS')}: room for ${dB(room())} before it clips.`;
    if (isOn(mute)) return `${text} Muted, the cut has no audio.`;
    if (multi && !playing().length) return `${text} No track plays, the cut has no audio.`;
    if (isOn(norm)) {
      text += ` Normalising moves it ${dB(levelling())}`;
      return levelling() > room() ? `${text}, with the peaks held at ${level(info.ceiling, 'dBTP')}.` : `${text}.`;
    }
    if (gainDb > room()) text += ` At ${dB(gainDb)} the limiter holds the peaks at ${level(info.ceiling, 'dBTP')}.`;
    return text;
  }
  function update() {
    const off = !info.audio || isOn(mute) || isOn(norm);
    range.disabled = off; num.disabled = off; reset.disabled = off || gainDb === 0;
    vol.classList.toggle('off', off);
    norm.disabled = !info.audio || isOn(mute) || silent();
    for (const r of rows) {
      const quiet = isOn(mute) || isOn(r.mute);
      r.mute.disabled = isOn(mute);
      r.range.disabled = quiet; r.num.disabled = quiet;
      r.vol.classList.toggle('off', quiet);
    }
    line.textContent = describe();
    if (changed() && !applying) toMaster();
    applyGain();
    onChange();
  }
  const half = (v) => Math.max(-20, Math.min(20, Math.round((Number.isFinite(v) ? v : 0) * 2) / 2));
  function setGain(v, from) {
    gainDb = half(v);
    if (from !== range) range.value = gainDb;
    if (from !== num) num.value = gainDb;
    update();
  }
  for (const r of rows) {
    const setTrack = (v, from) => { r.gain = half(v); if (from !== r.range) r.range.value = r.gain; if (from !== r.num) r.num.value = r.gain; update(); };
    r.range.addEventListener('input', () => { ensureGraph(); setTrack(+r.range.value, r.range); });
    r.num.addEventListener('change', () => { ensureGraph(); setTrack(parseFloat(r.num.value), null); });
    r.mute.addEventListener('change', () => { ensureGraph(); update(); });
  }
  range.addEventListener('input', () => { ensureGraph(); setGain(+range.value, range); });
  num.addEventListener('change', () => { ensureGraph(); setGain(parseFloat(num.value), null); });
  reset.onclick = () => { setGain(0, null); range.focus(); };
  norm.addEventListener('change', () => { ensureGraph(); update(); });
  mute.addEventListener('change', () => { ensureGraph(); update(); });
  if (info.audio && !info.measured && info.master) {
    api('GET', `/api/renditions/${info.master}/level`)
      .then((d) => { info = { ...info, measured: true, loudness: d.loudness, peak: d.peak }; update(); })
      .catch((err) => { failed = err.message; update(); });
  }
  update();
  return {
    changed,
    // The sound of a cut's recipe, or as the original is with none.
    apply: (r = {}) => {
      applying = true;
      setOn(mute, !!r.mute);
      setOn(norm, !!r.normalize);
      for (const row of rows) {
        const t = r.tracks?.[row.i];
        setOn(row.mute, t ? !t.on : row.i !== 0);
        row.gain = half(t?.gain_db || 0); row.range.value = row.gain; row.num.value = row.gain;
      }
      setGain(r.gain_db || 0, null);
      applying = false;
    },
    body: () => ({
      mute: isOn(mute),
      gain_db: isOn(norm) ? 0 : gainDb,
      normalize: isOn(norm),
      ...(multi ? { tracks: rows.map(r => ({ on: !isOn(r.mute), gain_db: r.gain })) } : {}),
    }),
    words: () => {
      const parts = [];
      if (multi && !asItWas()) parts.push(`mixing ${playing().map(r => (r.gain ? `${r.name} ${dB(r.gain)}` : r.name)).join(' and ')}`);
      if (isOn(norm)) parts.push(`levelled to ${info.target} LUFS`);
      else if (gainDb) parts.push(`at ${dB(gainDb)}`);
      return parts.join(', ');
    },
    kbit: info.kbit,
  };
}

function trimEditor(id, video, duration, fps, titleField) {
  const strip = $('#filmstrip'), hs = $('#h-start'), he = $('#h-end'), range = $('#range'), shadeL = $('#shade-l'), shadeR = $('#shade-r'), head = $('#head');
  const tStart = $('#t-start'), tEnd = $('#t-end'), tLen = $('#t-len'), plan = $('#plan'), snap = $('#snap'), mute = $('#mute');
  let audio = null;
  const target = $('#cut-target'), cutTitle = $('#cut-title');
  let kf = []; try { kf = JSON.parse($('#kf-json').textContent) || []; } catch {}
  const kfHost = $('#keyframes');
  kf.forEach(t => { const i = document.createElement('i'); i.style.left = `${t / duration * 100}%`; kfHost.append(i); });
  // With a cut current the editor opens on it, its range on the original and its sound.
  let from = null; try { from = JSON.parse($('#from-json')?.textContent || 'null'); } catch {}
  const st = from ? { start: Math.max(0, from.start), end: Math.min(duration, from.end) } : { start: 0, end: duration };
  const frame = 1 / fps;
  const previewRow = $('#preview-row'), previewBox = $('#preview-cut'), loopBox = $('#preview-loop'), readout = $('#preview-time');
  const root = $('#clip');
  // the picture: a window of it, tall or square, and fades in and out
  const reframe = $('#reframe'), reframeX = $('#reframe-x'), xBox = $('#reframe-x-box'), fadeIn = $('#fade-in'), fadeOut = $('#fade-out'), win = $('#reframe-window');
  const picture = () => {
    const p = {};
    if (reframe?.value) p.reframe = { shape: reframe.value, x: +reframeX.value };
    if (+fadeIn?.value > 0) p.fade_in = +fadeIn.value;
    if (+fadeOut?.value > 0) p.fade_out = +fadeOut.value;
    return p;
  };
  const pictured = () => Object.keys(picture()).length > 0;
  const pictureWords = () => {
    const p = picture(), w = [];
    if (p.reframe) w.push(p.reframe.shape === 'tall' ? 'framed tall (9:16)' : 'framed square');
    if (p.fade_in && p.fade_out) w.push('fading in and out'); else if (p.fade_in) w.push('fading in'); else if (p.fade_out) w.push('fading out');
    return w.join(' and ');
  };
  // the window is drawn over the original only: it is what the render crops
  const onOriginal = () => (video.dataset.rid || root.dataset.current) === root.dataset.master;
  function showWindow() {
    if (!win) return;
    const shape = reframe?.value;
    xBox.hidden = !shape;
    if (!shape || !video.videoWidth || !onOriginal()) { win.hidden = true; return; }
    const box = video.getBoundingClientRect(), host = win.parentElement.getBoundingClientRect();
    const scale = Math.min(box.width / video.videoWidth, box.height / video.videoHeight);
    const vw = video.videoWidth * scale, vh = video.videoHeight * scale;
    const left0 = box.left - host.left + (box.width - vw) / 2, top0 = box.top - host.top + (box.height - vh) / 2;
    const a = shape === 'tall' ? 9 / 16 : 1;
    const ww = Math.min(vw, vh * a), wh = Math.min(vh, vw / a), x = +reframeX.value;
    win.style.left = `${left0 + (vw - ww) * x}px`; win.style.top = `${top0 + (vh - wh) * x}px`;
    win.style.width = `${ww}px`; win.style.height = `${wh}px`;
    win.hidden = false;
  }
  const trimmed = () => st.start > 0.0005 || st.end < duration - 0.05;
  // opened on a cut, the player keeps that cut until the range is first moved
  let pristine = !!from;
  const previewing = () => trimmed() && isOn(previewBox) && !pristine;
  function masterPlaying() {
    const rid = video.dataset.rid || root.dataset.current;
    if (rid === root.dataset.master) return;
    const row = $$('#versions .v').find(v => v.dataset.rid === root.dataset.master);
    if (!row) return;
    playVersion(video, row.dataset.rid, row.dataset.url, row.dataset.poster, row.dataset.label, ', the cut is previewed on it');
    video.pause();
  }
  function showReadout() {
    if (!previewing()) { readout.textContent = ''; return; }
    readout.textContent = `${fmt(Math.max(0, video.currentTime - st.start))} of ${fmt(st.end - st.start)}`;
  }
  function syncPreview() {
    previewRow.hidden = !trimmed();
    if (previewing()) {
      masterPlaying();
      if (video.currentTime < st.start - 0.01 || video.currentTime > st.end + 0.01) video.currentTime = st.start;
    }
    showReadout();
  }
  video.addEventListener('play', () => {
    if (previewing() && (video.currentTime < st.start - 0.01 || video.currentTime >= st.end - 0.02)) video.currentTime = st.start;
  });
  video.addEventListener('timeupdate', () => {
    if (!previewing()) return;
    if (video.currentTime >= st.end) {
      if (isOn(loopBox) || video.loop) video.currentTime = st.start;
      else { video.pause(); video.currentTime = st.end; }
    }
    showReadout();
  });
  video.addEventListener('seeking', () => {
    if (!previewing()) return;
    if (video.currentTime < st.start - 0.01) video.currentTime = st.start;
    else if (video.currentTime > st.end + 0.01) video.currentTime = st.end;
  });
  previewBox.addEventListener('change', () => { pristine = false; syncPreview(); });
  const nearestKf = (t) => kf.length ? kf.reduce((a, b) => Math.abs(b - t) < Math.abs(a - t) ? b : a) : null;
  const onKeyframe = (t) => t <= 0.0005 || (nearestKf(t) !== null && Math.abs(nearestKf(t) - t) <= frame + 0.001);
  function render() {
    const l = st.start / duration * 100, r = st.end / duration * 100;
    shadeL.style.width = `${l}%`; shadeR.style.left = `${r}%`;
    range.style.left = `${l}%`; range.style.width = `${r - l}%`;
    hs.style.left = `calc(${l}% - 3px)`; he.style.left = `calc(${r}% - 3px)`;
    hs.setAttribute('aria-valuetext', fmt(st.start)); he.setAttribute('aria-valuetext', fmt(st.end));
    tStart.value = fmt(st.start); tEnd.value = fmt(st.end); tLen.textContent = seconds(st.end - st.start);
    $$('i', kfHost).forEach((i, n) => i.classList.toggle('near', Math.abs(kf[n] - st.start) <= frame + 0.001));
    syncPreview();
    const trimmed = st.start > 0.0005 || st.end < duration - 0.05;
    const sound = audio && audio.changed() ? ` The audio is encoded again at ${audio.kbit} kbit/s, ${audio.words()}.` : '';
    const btn = $('#render');
    const asFrom = from && fromBody !== null && Math.abs(st.start - from.start) < 0.001 && Math.abs(st.end - Math.min(duration, from.end)) < 0.001 && JSON.stringify(audio.body()) === fromBody && JSON.stringify(picture()) === fromPicture;
    if (btn && !btn.dataset.rendering) btn.disabled = !!asFrom;
    if (asFrom) { plan.innerHTML = `this is ${from.label} as it is: move a handle or change the sound or picture to make a new version from it`; return; }
    if (!trimmed && !isOn(mute) && !sound && !pictured()) { plan.innerHTML = 'nothing selected yet: drag the handles, or press <kbd>i</kbd> and <kbd>o</kbd> while the video plays'; return; }
    if (pictured()) { plan.innerHTML = `<span class="st-warn">re-encode</span>, ${pictureWords()}: the whole cut is encoded again, which takes a while.${sound}`; return; }
    if (!trimmed && isOn(mute)) { plan.innerHTML = '<span class="st-ok">stream copy</span>, the audio track is dropped and the video is untouched.'; return; }
    if (!trimmed) { plan.innerHTML = `<span class="st-ok">video copied</span>, the whole clip as a new version with the video untouched.${sound}`; return; }
    if (onKeyframe(st.start) && sound) plan.innerHTML = `<span class="st-ok">video copied</span>, the start sits on a keyframe, so the video is cut without encoding it again.${sound}`;
    else if (onKeyframe(st.start)) plan.innerHTML = '<span class="st-ok">stream copy</span>, the start sits on a keyframe, so this cut takes about a second and loses nothing.';
    else { const k = nearestKf(st.start); plan.innerHTML = `<span class="st-warn">re-encode</span>, the start is between keyframes${k !== null ? ` (nearest at ${fmt(k)})` : ''}; the whole cut is encoded again, which takes a while but cuts exactly here.${sound}`; }
  }
  function setStart(t) { pristine = false; st.start = Math.min(Math.max(0, t), st.end - 0.2); render(); }
  function setEnd(t) { pristine = false; st.end = Math.max(Math.min(duration, t), st.start + 0.2); render(); }
  function snapIf(t) { if (!isOn(snap) || !kf.length) return t; const k = nearestKf(t); const radius = 10 / strip.clientWidth * duration; return Math.abs(k - t) <= radius ? k : t; }
  const xToTime = (clientX) => { const r = strip.getBoundingClientRect(); return Math.min(duration, Math.max(0, (clientX - r.left) / r.width * duration)); };
  let dragging = null;
  for (const [h, which] of [[hs, 'start'], [he, 'end']]) {
    h.addEventListener('pointerdown', (e) => { dragging = which; h.setPointerCapture(e.pointerId); e.preventDefault(); h.focus({ preventScroll: true }); });
    h.addEventListener('pointermove', (e) => { if (dragging !== which) return; const t = xToTime(e.clientX); if (which === 'start') setStart(snapIf(t)); else setEnd(t); video.currentTime = which === 'start' ? st.start : st.end; });
    h.addEventListener('pointerup', () => dragging = null);
    h.addEventListener('keydown', (e) => { const d = e.shiftKey ? 1 : frame; if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') { e.preventDefault(); const dt = e.key === 'ArrowLeft' ? -d : d; if (which === 'start') setStart(st.start + dt); else setEnd(st.end + dt); } });
  }
  strip.addEventListener('pointerdown', (e) => { if (e.target.classList.contains('handle')) return; video.currentTime = xToTime(e.clientX); });
  video.addEventListener('timeupdate', () => { head.style.left = `${video.currentTime / duration * 100}%`; });
  tStart.addEventListener('change', () => { const t = parseTime(tStart.value); if (!isNaN(t)) setStart(snapIf(t)); else render(); });
  tEnd.addEventListener('change', () => { const t = parseTime(tEnd.value); if (!isNaN(t)) setEnd(t); else render(); });
  snap.addEventListener('change', () => { if (isOn(snap)) setStart(snapIf(st.start)); });
  let fromBody = null, fromPicture = '{}';
  audio = audioEditor(id, video, root, masterPlaying, () => render());
  if (from) {
    audio.apply(from); fromBody = JSON.stringify(audio.body());
    if (reframe && from.reframe) { reframe.value = from.reframe.shape; reframeX.value = String(from.reframe.x); }
    if (fadeIn) fadeIn.value = String(from.fade_in || 0);
    if (fadeOut) fadeOut.value = String(from.fade_out || 0);
    fromPicture = JSON.stringify(picture());
  }
  // a frame is chosen on the original, so the player shows it (a version may be framed already)
  const toOriginal = () => { if (reframe?.value && !onOriginal()) masterPlaying(); };
  [reframe, fadeIn, fadeOut].forEach((el) => el?.addEventListener('change', () => { pristine = false; toOriginal(); showWindow(); render(); }));
  reframeX?.addEventListener('input', () => { pristine = false; toOriginal(); showWindow(); render(); });
  video.addEventListener('loadedmetadata', showWindow);
  addEventListener('resize', showWindow);
  toOriginal();
  showWindow();
  $('#whole-original')?.addEventListener('click', (e) => {
    from = null; pristine = false; st.start = 0; st.end = duration; audio.apply();
    if (reframe) { reframe.value = ''; reframeX.value = '0.5'; fadeIn.value = '0'; fadeOut.value = '0'; showWindow(); }
    $('#edit-desc').textContent = 'non-destructive, the original stays; cuts come from the original';
    e.currentTarget.hidden = true; render();
  });
  target.addEventListener('change', () => { cutTitle.hidden = target.value !== 'clip'; if (!cutTitle.hidden) { cutTitle.value = cutTitle.value || `${titleField.value} cut`; cutTitle.focus(); } });
  document.addEventListener('keydown', (e) => {
    if (/^(INPUT|TEXTAREA|SELECT)$/.test(document.activeElement?.tagName) || e.ctrlKey || e.metaKey) return;
    if (e.key === 'i') { setStart(snapIf(video.currentTime)); }
    else if (e.key === 'o') { setEnd(video.currentTime); }
    else if (e.key === 'e') { e.preventDefault(); titleField.focus(); titleField.select(); }
    else if (e.key === 'c') { const live = liveLink(); if (live) live.click(); else $('#new-link')?.click(); }
    else if (e.key === 'd') { location.href = `/media/clips/${id}/download`; }
  });
  render();
  const submit = async () => {
    setOn(previewBox, false); syncPreview();
    const body = { ...audio.body(), ...picture(), as_new_clip: target.value === 'clip', title: cutTitle.value };
    if (st.start > 0.0005) body.start = st.start;
    if (st.end < duration - 0.05) body.end = st.end;
    try {
      const d = await api('POST', `/api/clips/${id}/cuts`, body);
      const btn = $('#render'); btn.disabled = true; btn.dataset.rendering = ''; btn.textContent = `Rendering ${d.label}`;
      const step = (j, done, fail) => {
        if (j.state === 'running') btn.textContent = `Rendering ${d.label}, ${Math.round(j.progress * 100)}%`;
        if (j.state === 'done') done();
        const why = jobEnded(j, 'no reason recorded');
        if (why) fail(why);
      };
      waitFor(
        (ev, done, fail) => { if (ev.type === 'job' && ev.id === d.job_id) step(ev, done, fail); },
        async (done, fail) => { const j = await jobNow(d.job_id); if (j) step(j, done, fail); else done(); },
      ).then(() => {
        if (d.clip_id) { toast(`Saved as its own clip: ${d.clip_title}.`, 'ok'); setTimeout(() => location.href = `/clips/${d.clip_id}`, 900); }
        else { toast(document.body.hasAttribute('data-standalone') ? 'Cut rendered. It is the current version now.' : 'Cut rendered. The links show it now.', 'ok'); setTimeout(() => location.reload(), 700); }
      }, (why) => { toast(why.cancelled ? 'The cut was cancelled in Jobs.' : `Cut failed: ${why.message}`, 'err'); btn.disabled = false; delete btn.dataset.rendering; btn.textContent = 'Render cut'; });
    } catch (err) { toast(err.message, 'err'); }
  };
  $('#render').onclick = submit;
  $$('#quick button').forEach((b) => {
    const n = +b.dataset.seconds;
    if (duration < n + 0.5) { b.disabled = true; b.title = `The clip is ${seconds(duration)} long.`; return; }
    b.onclick = () => {
      const from = duration - n;
      const near = kf.filter(k => k > 0 && k <= from + 0.001 && k >= from - 2.5);
      setEnd(duration);
      setStart(isOn(snap) && near.length ? Math.max(...near) : from);
      target.value = 'version'; cutTitle.hidden = true;
      submit();
    };
  });
}

function sendLink(id, titleField) {
  const btn = $('#send-link'), hint = $('#send-link-hint');
  if (!btn || !handheld() || !navigator.canShare || !navigator.canShare({ url: location.href })) return;
  btn.hidden = false; hint.hidden = false;
  const idle = btn.innerHTML, icon = $('svg', btn).outerHTML;
  const say = (t) => { btn.innerHTML = icon; btn.append(t); };
  btn.onclick = async () => {
    if (btn.disabled) return;
    let stop = null;
    const done = () => { stop?.(); stop = null; btn.disabled = false; btn.innerHTML = idle; };
    try {
      let url = liveLink()?.dataset.copy;
      if (!url) {
        btn.disabled = true; say('Making the link');
        stop = followLinkWait(id, '', say);
        const s = await api('POST', `/api/clips/${id}/shares`, { label: '', expires_in: settings.share_expiry_default || 'never', allow_download: settings.share_download_default !== false, rendition_id: '' });
        done();
        addLinkRow(s, false);
        url = s.url;
      }
      await navigator.share({ url, title: titleField.value });
    } catch (err) {
      done();
      if (err.name === 'AbortError') return;
      // a wait outlasts the tap's permission to open the share sheet; the next tap has it
      if (err.name === 'NotAllowedError' && liveLink()) { toast('The link is made. Tap Send to an app again to send it.', 'ok'); return; }
      const url = liveLink()?.dataset.copy;
      if (url && await copyText(url)) toast('The share sheet did not open; the link is copied instead.', 'err');
      else toast(err.message, 'err');
    }
  };
}

async function fetchFile(url, onPct) {
  const res = await fetch(url);
  if (!res.ok || !res.body) throw new Error(`${res.status}: the file did not come`);
  const total = +res.headers.get('content-length') || 0, reader = res.body.getReader(), parts = [];
  let got = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    parts.push(value); got += value.length;
    if (total) onPct(Math.round(got / total * 100));
  }
  return new Blob(parts, { type: 'video/mp4' });
}

function animPanel() {
  const box = $('#anim');
  if (!box) return;
  let info = null;
  try { info = JSON.parse($('#anim-json').textContent); } catch { /* no version yet */ }
  const fmt = $('#anim-format'), width = $('#anim-width'), fps = $('#anim-fps'), btn = $('#anim-btn'), hint = $('#anim-hint');
  const url = () => `${btn.dataset.base}.${fmt.value}?w=${width.value}&fps=${fps.value}`;
  const word = () => $('span', btn);
  const sync = () => {
    btn.href = url();
    if (!info) { box.hidden = true; return; }
    if (info.long) {
      [fmt, width, fps].forEach((c) => { c.disabled = true; });
      btn.classList.add('disabled');
      btn.setAttribute('aria-disabled', 'true');
      hint.textContent = `An animated GIF or WebP, for places that show no video, is ${info.max} s at most, and this clip is ${info.length}: trim it in the editor first (Keep the last 15 s, or the handles), then make one of that version.`;
      return;
    }
    const [low, high] = info.sizes[`${fmt.value}-${width.value}-${fps.value}`] || ['', ''];
    const webp = fmt.querySelector('option[value="webp"]') ? '' : ` WebP is not offered: ${document.body.hasAttribute('data-standalone') ? 'the ffmpeg here' : 'this server\'s ffmpeg'} has no WebP encoder.`;
    hint.textContent = `For places that show no video, without sound. About ${low} to ${high}, more the busier the picture; made once and kept.${webp}`;
  };
  fmt.addEventListener('change', () => { fps.value = fmt.value === 'webp' ? '24' : '12'; sync(); });
  width.addEventListener('change', sync);
  fps.addEventListener('change', sync);
  sync();
  btn.addEventListener('click', async (e) => {
    e.preventDefault();
    if (btn.classList.contains('busy') || btn.classList.contains('disabled')) return;
    const target = url();
    const idle = () => { btn.classList.remove('busy'); word().textContent = 'Make and download'; };
    try {
      if ((await headOf(target)).status === 200) { location.href = target; return; }
      const r = await fetch(target, { cache: 'no-store' });
      if (r.status === 200) { r.body?.cancel(); location.href = target; return; }
      const d = await r.json();
      if (d.error) throw new Error(d.error);
      btn.classList.add('busy');
      word().textContent = 'Making it';
      // asked for as well as heard of: a missed event never leaves the button making it for good
      const step = (j, fail) => {
        if (j.state === 'running') word().textContent = `Making it, ${Math.round(j.progress * 100)}%`;
        const why = jobEnded(j, 'it could not be made');
        if (why) fail(why);
      };
      await waitFor(
        (ev, done, fail) => { if (ev.type !== 'job' || ev.rendition_id !== d.rendition_id) return; step(ev, fail); if (ev.state === 'done') done(); },
        async (done, fail) => {
          const j = await jobNow(d.job_id);
          if (j) step(j, fail);
          if (!jobEnded(j) && (await headOf(target)).status === 200) done();
        },
      );
      idle();
      location.href = target;
    } catch (err) { idle(); toast(err.message, 'err'); }
  });
}

function sendPanel(root, id, titleField) {
  const dlBtn = $('#download-btn'), quality = $('#quality'), hint = $('#download-hint');
  const fileBtn = $('#send-file'), fileNote = $('#send-file-note'), fileHint = $('#send-file-hint');
  if (!dlBtn || !quality) return;
  const mb = (bytes) => bytes ? size(+bytes) : '';
  const original = () => quality.value === 'original';
  const copyUrl = () => `${dlBtn.dataset.copy}?limit=${quality.value}`;
  const chosen = () => quality.options[quality.selectedIndex];
  async function status() {
    if (original()) return { ready: true, url: dlBtn.dataset.download, size: null };
    const head = await fetch(copyUrl(), { method: 'HEAD' });
    if (head.status === 200) return { ready: true, url: copyUrl(), size: head.headers.get('content-length') };
    if (head.status === 202) return { ready: false, url: copyUrl() };
    const why = await fetch(copyUrl()).then(r => r.json()).then(d => d.error).catch(() => null);
    throw new Error(why || `${head.status}: the copy could not be made`);
  }
  // the events say how far the copy is, and its own address answers 200 once it is made,
  // whichever the page hears first
  async function follow(onProgress) {
    const url = copyUrl(), o = chosen(), limit = quality.value;
    const first = await fetch(url, { cache: 'no-store' });
    if (first.status === 200) { first.body?.cancel(); return { ready: true, url, size: first.headers.get('content-length') }; }
    const info = await first.json();
    if (info.error) throw new Error(info.error);
    const step = (j, fail) => {
      if (j.state === 'running') onProgress(Math.round(j.progress * 100));
      const why = jobEnded(j, 'the copy failed');
      if (why) fail(why);
    };
    const there = async (done) => { const h = await headOf(url); if (h.status === 200) done({ ready: true, url, size: h.headers.get('content-length') }); };
    const s = await waitFor(
      (ev, done, fail) => { if (ev.type !== 'job' || ev.rendition_id !== info.rendition_id) return; step(ev, fail); if (ev.state === 'done') there(done).catch(() => {}); },
      async (done, fail) => { const j = await jobNow(info.job_id); if (j) step(j, fail); if (!jobEnded(j)) await there(done); },
    );
    o.dataset.ready = '1'; o.textContent = `Under ${limit} MB, ready, ${mb(s.size)}`;
    return s;
  }
  const label = (text) => { dlBtn.lastChild.textContent = text; };
  const resetButton = () => { dlBtn.classList.remove('busy'); label('Download'); };
  dlBtn.addEventListener('click', async (e) => {
    if (original()) return;
    e.preventDefault();
    if (dlBtn.classList.contains('busy')) return;
    try {
      let s = await status();
      if (!s.ready) {
        dlBtn.classList.add('busy'); label('Making the copy');
        s = await follow((pct) => label(`Making the copy, ${pct}%`));
      }
      resetButton();
      location.href = s.url;
    } catch (err) { resetButton(); toast(err.message, 'err'); }
  });
  const fileWords = () => {
    if (fileBtn.hidden) return;
    if (original()) fileNote.textContent = `the original, ${root.dataset.currentSize}`;
    else fileNote.textContent = chosen().dataset.ready ? `the copy under ${quality.value} MB, ready` : `a copy under ${quality.value} MB is made first`;
  };
  quality.addEventListener('change', fileWords);
  const probe = new File([new Blob(['x'])], 'x.mp4', { type: 'video/mp4' });
  if (!handheld() || !navigator.canShare || !navigator.canShare({ files: [probe] })) return;
  fileBtn.hidden = false; fileHint.hidden = false;
  fileHint.textContent = 'The file goes to the app you pick at the quality chosen above. A copy that does not exist yet is made first, with the progress on the button; tap again once it is ready.';
  fileWords();
  let fetched = null;
  fileBtn.onclick = async () => {
    if (fileBtn.disabled) return;
    try {
      const s = await status();
      if (!s.ready) {
        fileBtn.disabled = true; fileNote.textContent = '';
        fileHint.textContent = `A copy under ${quality.value} MB is made first. Tap Send the file again when it is ready.`;
        const done = await follow((pct) => { fileNote.textContent = `making the copy, ${pct}%`; });
        fileBtn.disabled = false;
        fileNote.textContent = `ready, ${mb(done.size)}`;
        toast('The copy is ready. Tap Send the file to send it.', 'ok');
        return;
      }
      let file = fetched?.url === s.url ? fetched.file : null;
      if (!file) {
        fileBtn.disabled = true; fileNote.textContent = 'fetching';
        const blob = await fetchFile(s.url, (pct) => { fileNote.textContent = `fetching, ${pct}%`; });
        file = new File([blob], root.dataset.downloadName, { type: 'video/mp4' });
        fetched = { url: s.url, file };
        fileBtn.disabled = false;
      }
      fileWords();
      await navigator.share({ files: [file], title: titleField.value });
      fetched = null;
    } catch (err) {
      fileBtn.disabled = false; fileWords();
      if (err.name === 'NotAllowedError' && fetched) { fileNote.textContent = `ready, ${mb(fetched.file.size)}`; toast('The file is here. Tap Send the file again to send it.', 'ok'); return; }
      if (err.name !== 'AbortError') toast(err.message, 'err');
    }
  };
}

export function clip() {
  const root = $('#clip');
  if (!root) return;
  const id = root.dataset.id, duration = +root.dataset.duration || 0, fps = +root.dataset.fps || 60, ready = root.dataset.ready === 'true';
  const video = $('#video'), titleField = $('#title');
  if (video) {
    markNativePlay();
    rememberPlayback(video);
    const started = () => video.closest('.player')?.classList.add('started');
    video.addEventListener('play', started, { once: true });
    video.addEventListener('seeking', started, { once: true });
    const overlay = $('#play-overlay');
    if (overlay) overlay.onclick = () => video.play().catch(() => {});
    playerKeys(video, { box: video.closest('.player'), fps });
  }
  backLink();
  bindText(id);
  if (!ready) followProcessing(id);
  bindDelete(id);
  bindDiscard(id);
  bindShares(id);
  if (!ready || !video) return;
  bindVersions(id, video);
  if ($('#edit')) {
    bindPoster(id, video);
    trimEditor(id, video, duration, fps, titleField);
  }
  sendLink(id, titleField);
  sendPanel(root, id, titleField);
  animPanel();
  animMade(id);
}
