import { $, $$, api, copyWhenReady, focusOnOpen, toast } from './lib.js';
import { showStar } from './star.js';

const typing = (el) => /^(INPUT|TEXTAREA|SELECT)$/.test(el?.tagName) || el?.isContentEditable;

export function skippedWords(skipped) {
  if (!skipped?.length) return '';
  const by = new Map();
  for (const s of skipped) by.set(s.why, (by.get(s.why) || 0) + 1);
  return `Skipped ${skipped.length}: ${[...by].map(([why, n]) => (by.size > 1 ? `${n} ${why}` : why)).join('; ')}.`;
}

export function selectSeveral() {
  const grid = $('#grid'), btn = $('#select-btn'), bar = $('#select-bar');
  if (!grid || !btn || !bar) return;
  const count = $('#select-count'), note = $('#select-note');
  const acts = $$('[data-bulk]', bar), del = $('[data-bulk="delete"]', bar), montage = $('[data-bulk="montage"]', bar), star = $('[data-bulk="star"]', bar);
  let on = false, anchor = null, pressed = null, longFired = false, order = 0;
  const cards = () => $$('.card', grid).filter((c) => !c.hidden);
  const picked = () => cards().filter((c) => c.hasAttribute('data-picked'));
  const inPickOrder = () => picked().sort((a, b) => a.dataset.pickedAt - b.dataset.pickedAt);
  const unarm = () => { del.dataset.armed = ''; del.textContent = 'Delete'; };
  const say = (words) => { note.textContent = words; note.hidden = !words; };

  const set = (card, yes) => {
    if (yes && !card.hasAttribute('data-picked')) card.dataset.pickedAt = String(++order);
    card.toggleAttribute('data-picked', yes);
    $('.card-link', card)?.setAttribute('aria-checked', String(yes));
  };
  const sync = () => {
    const n = picked().length;
    count.textContent = n ? `${n} picked` : 'Pick clips';
    acts.forEach((b) => { b.disabled = n === 0; });
    montage.disabled = n < 2;
    star.textContent = n && picked().every((c) => c.hasAttribute('data-star')) ? 'Unstar' : 'Star';
    unarm();
  };
  const enter = () => {
    if (on) return;
    on = true;
    document.body.classList.add('selecting');
    btn.setAttribute('aria-pressed', 'true');
    bar.hidden = false;
    for (const c of $$('.card', grid)) { const l = $('.card-link', c); l.setAttribute('role', 'checkbox'); l.setAttribute('aria-checked', 'false'); }
    say('');
    sync();
  };
  const leave = () => {
    if (!on) return;
    on = false;
    document.body.classList.remove('selecting');
    btn.setAttribute('aria-pressed', 'false');
    bar.hidden = true;
    for (const c of $$('.card', grid)) { c.removeAttribute('data-picked'); const l = $('.card-link', c); l.removeAttribute('role'); l.removeAttribute('aria-checked'); }
    anchor = null;
  };
  const toggle = (card, range) => {
    const yes = !card.hasAttribute('data-picked');
    if (range && anchor && anchor !== card && cards().includes(anchor)) {
      const list = cards(), a = list.indexOf(anchor), b = list.indexOf(card);
      const span = list.slice(Math.min(a, b), Math.max(a, b) + 1);
      (a < b ? span : span.reverse()).forEach((c) => set(c, yes));
    } else set(card, yes);
    anchor = card;
    sync();
  };
  btn.addEventListener('click', () => (on ? leave() : enter()));
  $('#select-done').addEventListener('click', () => { leave(); btn.focus(); });

  grid.addEventListener('click', (e) => {
    const card = e.target.closest('.card');
    if (longFired) { e.preventDefault(); e.stopPropagation(); longFired = false; return; }
    if (!on || !card || e.target.closest('.card-more, .card-star')) return;
    e.preventDefault();
    e.stopPropagation();
    toggle(card, e.shiftKey);
  }, true);
  grid.addEventListener('keydown', (e) => {
    if (!on || (e.key !== ' ' && e.key !== 'Enter')) return;
    const card = e.target.closest('.card-link') && e.target.closest('.card');
    if (!card) return;
    e.preventDefault();
    e.stopPropagation();
    toggle(card, e.shiftKey);
  }, true);

  grid.addEventListener('touchstart', (e) => {
    const card = e.target.closest('.card');
    if (!card || e.touches.length > 1 || e.target.closest('.card-more, .card-star')) return;
    const t = e.touches[0];
    pressed = { card, x: t.clientX, y: t.clientY, timer: setTimeout(() => {
      longFired = true;
      enter();
      set(card, true);
      anchor = card;
      sync();
      navigator.vibrate?.(15);
    }, 500) };
  }, { passive: true });
  const cancel = () => { if (pressed) clearTimeout(pressed.timer); pressed = null; };
  grid.addEventListener('touchmove', (e) => {
    const t = e.touches[0];
    if (pressed && Math.hypot(t.clientX - pressed.x, t.clientY - pressed.y) > 10) cancel();
  }, { passive: true });
  grid.addEventListener('touchend', () => { cancel(); setTimeout(() => { longFired = false; }, 400); });
  grid.addEventListener('touchcancel', cancel);
  grid.addEventListener('contextmenu', (e) => { if (longFired || on) e.preventDefault(); });

  const menuOpen = () => { const m = $('#card-menu'); return !!m && !m.hidden; };
  document.addEventListener('keydown', (e) => {
    if (typing(document.activeElement) || document.querySelector('dialog[open]') || menuOpen()) return;
    if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === 'a') {
      e.preventDefault();
      enter();
      cards().forEach((c) => set(c, true));
      sync();
    } else if (e.key === 'Escape' && on) {
      e.preventDefault();
      leave();
      btn.focus();
    }
  });

  const ids = (list) => list.map((c) => c.dataset.id);
  const after = (d, list) => {
    const done = new Set(d.done || []);
    list.filter((c) => done.has(c.dataset.id)).forEach((c) => set(c, false));
    say(skippedWords(d.skipped));
    sync();
  };

  del.addEventListener('click', async () => {
    const list = picked();
    if (!list.length) return;
    if (del.dataset.armed !== '1') {
      del.dataset.armed = '1';
      del.textContent = list.length === 1 ? 'Really delete it?' : `Really delete ${list.length}?`;
      setTimeout(unarm, 4000);
      return;
    }
    unarm();
    del.disabled = true;
    count.textContent = `Deleting ${list.length}...`;
    try {
      const d = await api('POST', '/api/bulk/delete', { ids: ids(list) });
      const gone = new Set(d.done);
      for (const c of list) if (gone.has(c.dataset.id)) { c.hidden = true; c.setAttribute('data-gone', ''); set(c, false); }
      after(d, list);
      if (d.done.length) {
        const n = d.done.length;
        toast(`${n === 1 ? 'In' : `${n} clips in`} the trash for ${d.trash_days} days.`, '', { label: 'Undo', run: async () => {
          try { await api('POST', '/api/bulk/restore', { ids: d.done }); location.reload(); }
          catch (err) { toast(err.message, 'err'); }
        } }, '/trash');
      }
      if (!picked().length && !d.skipped.length) leave();
    } catch (err) { toast(err.message, 'err'); sync(); }
  });

  star.addEventListener('click', async () => {
    const list = picked();
    if (!list.length) return;
    const on = star.textContent === 'Star';
    try {
      const d = await api('POST', '/api/bulk/star', { ids: ids(list), on });
      d.done.forEach((id) => showStar(id, on));
      after(d, list);
      const n = d.done.length;
      if (n) toast(`${n === 1 ? 'The clip' : `${n} clips`} ${on ? 'starred' : 'unstarred'}.`, 'ok');
    } catch (err) { toast(err.message, 'err'); }
  });

  montage.addEventListener('click', () => {
    const list = inPickOrder();
    const most = 100;
    if (list.length > most) { toast(`A montage takes at most ${most} clips; ${list.length} are picked.`, 'err'); return; }
    if (list.length > 1) location.href = `/montage?clips=${ids(list).join(',')}`;
  });

  $('[data-bulk="links"]', bar)?.addEventListener('click', async () => {
    const list = picked();
    if (!list.length) return;
    const asked = api('POST', '/api/bulk/links', { ids: ids(list) });
    const copied = copyWhenReady(asked.then((d) => d.links.map((l) => l.url).join('\n')));
    try {
      const d = await asked;
      for (const l of d.links) { const c = $(`.card[data-id="${l.id}"]`, grid); if (c) { c.dataset.shared = '1'; c.dataset.link = l.url; } }
      after({ done: d.links.map((l) => l.id), skipped: d.skipped }, list);
      const n = d.links.length, made = d.made ? `, ${d.made} of them made just now` : '';
      if (n) toast(await copied ? `${n === 1 ? 'Its link' : `${n} links, one a line,`} copied${made}. Go paste ${n === 1 ? 'it' : 'them'}.` : d.links.map((l) => l.url).join(' '), 'ok');
    } catch (err) { toast(err.message, 'err'); }
  });

  // a plain download of the pick's token, so the browser's bar or the app's Downloads take it
  const dl = $('#download-dialog');
  $('[data-bulk="download"]', bar).addEventListener('click', async (e) => {
    const list = picked();
    if (!list.length) return;
    let d;
    try { d = await api('POST', '/api/bulk/download', { ids: ids(list) }); } catch (err) { toast(err.message, 'err'); return; }
    after({ done: [], skipped: d.skipped }, list);
    if (!d.token) { toast('None of the picked clips can be downloaded.', 'err'); return; }
    const n = d.clips;
    $('#download-what').textContent = `${n === 1 ? '1 clip' : `${n} clips`} as one zip: a folder each, named by its title, with its files and clip.json (its details, versions and live links).`;
    const radios = $$('input[name="download-which"]', dl), note = $('#download-note'), go = $('#download-go');
    for (const r of radios) {
      const c = d.choices[r.value];
      $(`[data-size="${r.value}"]`, dl).textContent = `${c.files} file${c.files === 1 ? '' : 's'}, ${c.size}`;
      r.closest('label').classList.toggle('over', c.over);
    }
    const which = () => radios.find((r) => r.checked)?.value || 'current';
    const show = () => {
      const c = d.choices[which()];
      const left = d.skipped.length ? ` ${skippedWords(d.skipped)}` : '';
      note.textContent = c.over ? `${c.size} is over the ${d.most} one download may be: pick fewer clips, or the current version alone.${left}` : left.trim();
      note.className = c.over ? 'small st-err' : 'small muted';
      note.hidden = !note.textContent;
      go.disabled = c.over || !c.files;
      go.textContent = c.over ? 'Download' : `Download ${c.size}`;
    };
    radios.forEach((r) => { r.onchange = show; });
    radios[0].checked = true;
    show();
    go.onclick = () => {
      const c = d.choices[which()];
      if (c.over) return;
      dl.close();
      location.href = `/api/bulk/download/${d.token}?which=${which()}`;
      toast(`Downloading ${n === 1 ? '1 clip' : `${n} clips`}, ${c.size}, as one zip.`, 'ok');
    };
    $('#download-cancel').onclick = () => dl.close();
    dl.showModal();
    focusOnOpen(dl, e, go);
  });

  const dlg = $('#collect-dialog');
  const into = async (body, name) => {
    const list = picked();
    try {
      const d = await api('POST', '/api/bulk/collect', { ids: ids(list), ...body });
      dlg.close();
      const done = new Set(list.map((c) => c.dataset.id));
      (d.skipped || []).forEach((s) => done.delete(s.id));
      for (const c of list) {
        if (!done.has(c.dataset.id)) continue;
        const sets = new Set((c.dataset.sets || '').split(' ').filter(Boolean));
        sets.add(d.collection.id);
        c.dataset.sets = [...sets].join(' ');
      }
      after({ done: [...done], skipped: d.skipped }, list);
      toast(`${d.added === 1 ? 'The clip' : `${d.added} clips`} in ${name || d.collection.name} now.`, 'ok', null, `/collections/${d.collection.id}`);
    } catch (err) { toast(err.message, 'err'); }
  };
  $('[data-bulk="collect"]', bar).addEventListener('click', async (e) => {
    const list = picked();
    if (!list.length) return;
    $('#collect-what').textContent = list.length === 1 ? '1 clip picked' : `${list.length} clips picked`;
    const box = $('#collect-list');
    box.replaceChildren();
    try {
      const d = await api('GET', '/api/collections');
      const here = $('#set')?.dataset.id;
      for (const c of d.collections.filter((c) => c.open && c.id !== here)) {
        const b = document.createElement('button');
        b.type = 'button'; b.className = 'item';
        b.textContent = c.name;
        b.onclick = () => into({ collection: c.id }, c.name);
        box.append(b);
      }
    } catch (err) { toast(err.message, 'err'); return; }
    box.hidden = !box.children.length;
    const field = $('#collect-new');
    field.value = '';
    const make = () => { const name = field.value.trim(); if (name) into({ name }, name); else field.focus(); };
    $('#collect-make').onclick = make;
    field.onkeydown = (ev) => { if (ev.key === 'Enter') { ev.preventDefault(); make(); } };
    $('#collect-cancel').onclick = () => dlg.close();
    dlg.showModal();
    focusOnOpen(dlg, e, box.firstElementChild || field);
  });
}
