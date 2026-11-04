import { $, $$, api, focusOnOpen, onEvent, toast } from './lib.js';
import { hoverPlayback } from './playback.js';

// Starts again on its own end rather than through `loop`, which WebKitGTK 2.50 ends a
// second or two early.
function hoverLoop(card) {
  const poster = $('.poster', card);
  card.addEventListener('mouseenter', () => {
    if (!card.dataset.hover || poster.classList.contains('processing') || document.documentElement.dataset.hover === 'off') return;
    let v = $('video', poster);
    if (!v) {
      v = document.createElement('video');
      v.playsInline = true; v.preload = 'none';
      v.addEventListener('playing', () => v.classList.add('playing'));
      v.addEventListener('ended', () => { if (card.matches(':hover')) v.play().catch(() => {}); });
      poster.append(v);
    }
    if (!v.src || v.error) v.src = card.dataset.hover;
    hoverPlayback(v);
    v.play().catch(() => { v.muted = true; v.play().catch(() => {}); });
  });
  card.addEventListener('mouseleave', () => { const v = $('video', poster); if (v) { v.pause(); v.classList.remove('playing'); v.currentTime = 0; } });
}

function gridKeys(grid) {
  document.addEventListener('keydown', (e) => {
    if (/^(INPUT|TEXTAREA|SELECT)$/.test(document.activeElement?.tagName)) return;
    if (document.activeElement?.closest('.card-menu, dialog')) return;
    const cards = $$('.card', grid).filter(c => !c.hidden);
    let idx = cards.findIndex(c => c.contains(document.activeElement));
    const cols = Math.max(1, Math.round(grid.clientWidth / (cards[0]?.clientWidth + 18 || 300)));
    let next = idx;
    if (e.key === 'j' || e.key === 'ArrowRight') next = Math.min(cards.length - 1, idx + 1);
    else if (e.key === 'k' || e.key === 'ArrowLeft') next = Math.max(0, idx - 1);
    else if (e.key === 'ArrowDown') next = Math.min(cards.length - 1, idx + cols);
    else if (e.key === 'ArrowUp') next = Math.max(0, idx - cols);
    else return;
    e.preventDefault(); $('.card-link', cards[next])?.focus();
  });
}

function sortsAfter(sort, a, b) {
  const when = (c) => c.dataset.when || '';
  const dur = (c) => +c.dataset.dur || 0;
  const title = (c) => c.dataset.title || '';
  const shared = (c) => +c.dataset.shared || 0;
  switch (sort) {
    case 'oldest': return when(b) > when(a);
    case 'longest': return dur(b) < dur(a) || (dur(b) === dur(a) && when(b) < when(a));
    case 'title': return title(b) > title(a) || (title(b) === title(a) && when(b) < when(a));
    case 'shared': return shared(b) < shared(a) || (shared(b) === shared(a) && when(b) < when(a));
    default: return when(b) < when(a);
  }
}

function place(grid, sort, card, day) {
  const dated = sort === 'newest' || sort === 'oldest';
  const cards = $$('.card', grid);
  let before = cards.find(c => c !== card && sortsAfter(sort, card, c)) || null;
  if (dated && day && day.key) {
    let group = $(`.day[data-key="${day.key}"]`, grid);
    if (!group) {
      group = document.createElement('div');
      group.className = 'day'; group.dataset.key = day.key;
      const span = document.createElement('span'); span.textContent = day.label; group.append(span);
      const groups = $$('.day', grid);
      const after = groups.find(g => sort === 'newest' ? g.dataset.key < day.key : g.dataset.key > day.key) || null;
      grid.insertBefore(group, after);
      before = group.nextElementSibling;
    } else if (!before || !inGroup(grid, before, group)) {
      before = nextGroup(group);
    }
  }
  grid.insertBefore(card, before);
}

function inGroup(grid, card, group) {
  let el = card.previousElementSibling;
  while (el) { if (el.classList.contains('day')) return el === group; el = el.previousElementSibling; }
  return false;
}
function nextGroup(group) {
  let el = group.nextElementSibling;
  while (el && !el.classList.contains('day')) el = el.nextElementSibling;
  return el;
}
function tidyDays(grid) {
  for (const d of $$('.day', grid)) {
    const n = d.nextElementSibling;
    if (!n || n.classList.contains('day')) d.remove();
  }
}

function liveLibrary(grid, refilter) {
  const sort = new URLSearchParams(location.search).get('sort') || 'newest';
  const pending = new Map();
  const refresh = async (id) => {
    if (pending.has(id)) return;
    pending.set(id, true);
    try {
      const d = await api('GET', `/api/library/card/${id}?sort=${encodeURIComponent(sort)}`);
      let card = $(`.card[data-id="${id}"]`, grid);
      const fresh = document.createElement('template');
      fresh.innerHTML = d.html.trim();
      const next = fresh.content.firstElementChild;
      if (card) card.replaceWith(next); else place(grid, sort, next, d.day);
      hoverLoop(next);
      tidyDays(grid);
      summary(d.summary);
      refilter();
    } catch (err) {
      if (/^404/.test(err.message)) remove(id);
    } finally { pending.delete(id); }
  };
  // many clips leaving at once ask for the head's line once
  let summaryTimer = null;
  const remove = (id) => {
    const card = $(`.card[data-id="${id}"]`, grid);
    if (!card) return;
    card.remove();
    tidyDays(grid);
    clearTimeout(summaryTimer);
    summaryTimer = setTimeout(() => api('GET', `/api/library/summary?sort=${encodeURIComponent(sort)}`).then(d => summary(d.summary)).catch(() => {}), 300);
    refilter();
  };
  const summary = (line) => {
    if (!line) return;
    grid.dataset.summary = line;
    if ($('#summary') && !$('#search')?.value.trim()) $('#summary').textContent = line;
  };
  let activityTimer = null;
  const activity = () => {
    clearTimeout(activityTimer);
    activityTimer = setTimeout(async () => {
      try {
        const doc = new DOMParser().parseFromString(await (await fetch(location.pathname + location.search)).text(), 'text/html');
        const fresh = doc.querySelector('#activity'), now = $('#activity');
        if (fresh && now) now.replaceWith(fresh);
        else if (fresh) $('#disk-detail')?.after(fresh);
        else now?.remove();
      } catch { /* the next event tries again */ }
    }, 800);
  };
  onEvent((ev) => {
    if (ev.type === 'clip') {
      if (ev.state === 'trashed' || ev.state === 'gone') remove(ev.id);
      else refresh(ev.id);
      return;
    }
    if (ev.type !== 'job') return;
    if (ev.state !== 'running' || ev.progress === 0) activity();
    if (!ev.clip_id) return;
    if (ev.state === 'running') {
      const card = $(`.card[data-id="${ev.clip_id}"]`, grid);
      if (!card) { refresh(ev.clip_id); return; }
      if (ev.kind !== 'ingest') return;
      const pct = Math.round(ev.progress * 100);
      const flag = $('.flag', card), bar = $('.progress i', card);
      if (flag) flag.textContent = `encoding ${pct}%`;
      if (bar) bar.style.width = `${pct}%`;
      return;
    }
    if (['done', 'failed', 'discarded', 'cancelled'].includes(ev.state)) refresh(ev.clip_id);
  });
}

function diskRefresh() {
  const btn = $('#disk-refresh'), line = $('#disk-line'), detail = $('#disk-detail');
  const sum = $('#summary'), pop = $('#disk-pop'), popRefresh = $('#disk-pop-refresh');
  if (!btn || !line) return;
  line.onclick = () => { detail.hidden = !detail.hidden; line.setAttribute('aria-expanded', String(!detail.hidden)); };
  const refresh = async (b) => {
    b.disabled = true;
    try {
      const d = await api('GET', '/api/disk?refresh=1');
      line.textContent = d.line; line.title = d.detail; detail.textContent = d.detail;
      $('#disk-pop-line').textContent = d.line; $('#disk-pop-detail').textContent = d.detail;
    } catch (err) { toast(err.message, 'err'); }
    b.disabled = false;
  };
  btn.onclick = () => refresh(btn);
  if (!sum || !pop) return;
  popRefresh.onclick = () => refresh(popRefresh);
  const show = (open) => {
    if (open) $('#disk-pop-sum').textContent = sum.textContent;
    pop.hidden = !open;
    sum.setAttribute('aria-expanded', String(open));
  };
  sum.addEventListener('click', (e) => { e.stopPropagation(); show(pop.hidden); });
  pop.addEventListener('click', (e) => e.stopPropagation());
  document.addEventListener('click', () => show(false));
  document.addEventListener('keydown', (e) => { if (e.key === 'Escape' && !pop.hidden) { show(false); sum.focus(); } });
}

function filterControl(picks, apply) {
  const btn = $('#filter-btn'), dlg = $('#filter'), label = $('#filter-label'), line = $('#picked');
  if (!btn || !dlg) return () => {};
  const name = (i) => i.closest('label').querySelector('span').textContent;
  const phone = () => matchMedia('(max-width: 600px), (max-height: 500px)').matches;
  const open = (e) => {
    if (dlg.open) return;
    if (phone()) dlg.showModal(); else dlg.show();
    btn.setAttribute('aria-expanded', 'true');
    focusOnOpen(dlg, e, picks.find((i) => i.checked) || picks[0]);
  };
  btn.addEventListener('click', (e) => { e.stopPropagation(); if (dlg.open) dlg.close(); else open(e); });
  $('#filter-done').addEventListener('click', () => dlg.close());
  dlg.addEventListener('close', () => { btn.setAttribute('aria-expanded', 'false'); if (dlg.contains(document.activeElement) || document.activeElement === document.body) btn.focus(); });
  dlg.addEventListener('click', (e) => { if (e.target === dlg && dlg.matches(':modal')) dlg.close(); e.stopPropagation(); });
  document.addEventListener('click', () => { if (dlg.open && !dlg.matches(':modal')) dlg.close(); });
  dlg.addEventListener('keydown', (e) => { if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); dlg.close(); btn.focus(); } });
  return () => {
    const on = picks.filter((i) => i.checked);
    const games = on.filter((i) => i.dataset.game !== undefined).map(name);
    const sets = on.filter((i) => i.dataset.set !== undefined).map(name);
    const part = (list, many) => (list.length === 1 ? list[0] : list.length ? `${list.length} ${many}` : '');
    const words = [part(games, 'games'), part(sets, 'collections')].filter(Boolean).join(', ');
    label.textContent = words || 'Filter';
    btn.classList.toggle('on', on.length > 0);
    btn.setAttribute('aria-label', words ? `Filter: ${on.map(name).join(', ')}` : 'Filter');
    line.replaceChildren(...on.map((i) => {
      const chip = document.createElement('button');
      chip.type = 'button';
      chip.className = 'pick-chip';
      chip.setAttribute('aria-label', `Remove ${name(i)}`);
      chip.innerHTML = '<span></span><svg class="icon" viewBox="0 0 16 16" aria-hidden="true"><path d="M4.5 4.5l7 7M11.5 4.5l-7 7"/></svg>';
      chip.firstChild.textContent = name(i);
      chip.addEventListener('click', () => {
        const at = [...line.children].indexOf(chip);
        i.checked = false;
        apply();
        (line.children[Math.min(at, line.children.length - 1)] || btn).focus();
      });
      return chip;
    }));
    line.hidden = on.length === 0;
  };
}

// Picks of one kind are any of, the kinds and the search all of.
function liveSearch(grid) {
  const box = $('#search');
  const picks = $$('#filter input[type="checkbox"]');
  const star = $('#starred-btn');
  if (!box && !picks.length && !star) return () => {};
  const onlyStarred = () => star?.getAttribute('aria-pressed') === 'true';
  const days = () => $$('.day', grid), sub = $('#summary'), none = $('#no-match'), sortQ = $('#sort-q');
  const lower = (s) => s.replace(/[A-Z]/g, (c) => c.toLowerCase());
  const served = lower(grid.dataset.q || '');
  const whole = sub ? sub.textContent : '';
  const chosen = (kind) => new Set(picks.filter(i => i.checked && i.dataset[kind] !== undefined).map(i => i.dataset[kind]));
  const url = (q) => {
    const u = new URL(location.href);
    if (q) u.searchParams.set('q', q); else u.searchParams.delete('q');
    for (const kind of ['game', 'set']) { u.searchParams.delete(kind); for (const k of chosen(kind)) u.searchParams.append(kind, k); }
    if (onlyStarred()) u.searchParams.set('starred', '1'); else u.searchParams.delete('starred');
    return u.pathname + u.search;
  };
  const from = new URLSearchParams(location.search);
  star?.setAttribute('aria-pressed', String(from.get('starred') === '1'));
  for (const i of picks) {
    const kind = i.dataset.game !== undefined ? 'game' : 'set';
    i.checked = from.getAll(kind).includes(i.dataset[kind]);
  }
  let widen = null;
  const apply = () => {
    const raw = box ? box.value.trim() : '', q = lower(raw);
    const games = chosen('game'), sets = chosen('set');
    const starredOnly = onlyStarred();
    const picked = games.size + sets.size > 0 || starredOnly;
    if (sortQ) sortQ.value = raw;
    if (served && !q.includes(served)) { clearTimeout(widen); widen = setTimeout(() => location.replace(url(raw)), 350); return; }
    let shown = 0;
    for (const c of $$('.card', grid)) {
      const hit = (!q || c.dataset.text.includes(q))
        && (!starredOnly || c.hasAttribute('data-star'))
        && (!games.size || games.has(c.dataset.game || ''))
        && (!sets.size || (c.dataset.sets || '').split(' ').some(id => sets.has(id)));
      c.hidden = !hit || c.hasAttribute('data-gone');
      if (!c.hidden) shown += 1;
    }
    for (const d of days()) {
      let el = d.nextElementSibling, any = false;
      while (el && !el.classList.contains('day')) { if (!el.hidden) { any = true; break; } el = el.nextElementSibling; }
      d.hidden = !any;
    }
    const others = [raw && `"${raw}"`, ...picks.filter(i => i.checked).map(i => i.closest('label').querySelector('span').textContent)].filter(Boolean).join(', ');
    const words = [starredOnly && 'starred', others].filter(Boolean).join(', ');
    if (sub) sub.textContent = q || picked ? `${shown} ${shown === 1 ? 'clip' : 'clips'}: ${words}` : (grid.dataset.summary || whole);
    if (none) {
      $('strong', none).textContent = starredOnly && !others ? 'Nothing starred yet: the star on a clip, its page or its menu stars it.'
        : starredOnly ? `No starred clip matches ${others}.`
        : q && !picked ? `No clip matches "${raw}".` : `No clip matches ${words}.`;
      none.hidden = shown > 0;
    }
    grid.hidden = shown === 0;
    history.replaceState(null, '', url(raw));
    showPicks();
  };
  const showPicks = filterControl(picks, () => apply());
  picks.forEach(i => i.addEventListener('change', apply));
  star?.addEventListener('click', () => { star.setAttribute('aria-pressed', String(!onlyStarred())); apply(); });
  document.addEventListener('reel:star', () => { if (onlyStarred()) apply(); });
  if (box) {
    box.addEventListener('input', apply);
    box.form?.addEventListener('submit', (e) => { e.preventDefault(); apply(); box.blur(); });
    box.addEventListener('keydown', (e) => { if (e.key === 'Escape' && box.value) { e.stopPropagation(); box.value = ''; apply(); } });
    if (box.value) { box.focus(); const n = box.value.length; box.setSelectionRange(n, n); }
  }
  $('#clear-search')?.addEventListener('click', () => { if (box) box.value = ''; picks.forEach(i => { i.checked = false; }); star?.setAttribute('aria-pressed', 'false'); apply(); box?.focus(); });
  if (picks.some(i => i.checked) || onlyStarred()) apply(); else showPicks();
  return () => { if ((box && box.value.trim()) || picks.some(i => i.checked) || onlyStarred()) apply(); };
}

export function library() {
  if (document.body.dataset.page !== 'library' || $('#clip')) return;
  const grid = $('#grid');
  if (grid) {
    $$('.card', grid).forEach(hoverLoop);
    gridKeys(grid);
    const refilter = liveSearch(grid);
    liveLibrary(grid, refilter);
  } else if ($('#library-empty')) {
    // only the library's own empty page: the montage and Stats pages say "library" too
    onEvent((ev) => { if (ev.type === 'clip' && ev.state !== 'trashed') location.reload(); });
  }
  diskRefresh();
}
