import { $, $$, api, copyWhenReady, focusOnOpen, settings, toast } from './lib.js';
import { setStar } from './star.js';

const handheld = () => matchMedia('(pointer: coarse) and (hover: none)').matches;
const narrow = () => matchMedia('(max-width: 600px)').matches;
const GAP = 12;

function newLink(id) {
  return api('POST', `/api/clips/${id}/shares`, {
    label: '',
    expires_in: settings.share_expiry_default || 'never',
    allow_download: settings.share_download_default !== false,
    rendition_id: '',
  });
}

async function shareLink(card, title) {
  const id = card.dataset.id;
  const made = { link: null };
  const url = card.dataset.link ? Promise.resolve(card.dataset.link) : api('GET', `/api/clips/${id}`).then(async (d) => {
    if (d.link) return d.link;
    made.link = await newLink(id);
    card.dataset.shared = '1';
    return made.link.url;
  }).then((u) => { card.dataset.link = u; return u; });
  if (handheld() && navigator.share) {
    try { await navigator.share({ url: await url, title }); }
    catch (err) {
      // a wait outlasts the tap's permission to open the share sheet; the next tap has it
      if (err.name === 'NotAllowedError') toast('The link is ready. Tap Send the link again to send it.', 'ok');
      else if (err.name !== 'AbortError') toast(err.message, 'err');
    }
    return;
  }
  const copied = copyWhenReady(url);
  try {
    await url;
    const note = made.link?.preview_note ? ` ${made.link.preview_note}` : '';
    const what = made.link ? 'Link made' : 'Link';
    toast(await copied ? `${what} and copied. Go paste it.${note}` : `${what}: ${await url}`, 'ok');
  } catch (err) { toast(err.message, 'err'); }
}

function renameDialog(card) {
  const dlg = $('#rename-dialog'), field = $('#rename-form input[name="title"]');
  const before = $('.card-title', card).textContent.trim();
  field.value = before;
  const save = async () => {
    const t = field.value.trim();
    if (!t) { field.focus(); return; }
    if (t === before) { dlg.close(); return; }
    try {
      await api('PATCH', `/api/clips/${card.dataset.id}`, { title: t });
      $('.card-title', card).textContent = t;
      card.dataset.title = t.toLowerCase();
      $('.card-more', card).setAttribute('aria-label', `More for ${t}`);
      dlg.close();
      toast('Title saved.', 'ok');
    } catch (err) { toast(err.message, 'err'); }
  };
  $('#rename-save').onclick = save;
  $('#rename-cancel').onclick = () => dlg.close();
  field.onkeydown = (e) => { if (e.key === 'Enter') { e.preventDefault(); save(); } };
  dlg.showModal();
  field.focus(); field.select();
}

function setsDialog(card) {
  const dlg = $('#sets-dialog'), id = card.dataset.id;
  const ids = () => new Set((card.dataset.sets || '').split(' ').filter(Boolean));
  const keep = (set) => { card.dataset.sets = [...set].join(' '); };
  $('#sets-clip').textContent = $('.card-title', card).textContent.trim();
  const tick = async (box) => {
    try {
      await api(box.checked ? 'PUT' : 'DELETE', `/api/collections/${box.dataset.set}/clips/${id}`);
      const set = ids();
      if (box.checked) set.add(box.dataset.set); else set.delete(box.dataset.set);
      keep(set);
      const name = box.closest('label').querySelector('span').textContent;
      toast(box.checked ? `In ${name} now.` : `Out of ${name}.`, 'ok');
    } catch (err) { box.checked = !box.checked; toast(err.message, 'err'); }
  };
  const now = ids();
  $$('input[data-set]', dlg).forEach((box) => { box.checked = now.has(box.dataset.set); box.onchange = () => tick(box); });
  const field = $('#sets-new');
  const make = async () => {
    const name = field.value.trim();
    if (!name) { field.focus(); return; }
    try {
      const made = await api('POST', '/api/collections', { name });
      await api('PUT', `/api/collections/${made.id}/clips/${id}`);
      let list = $('#sets-list');
      if (!list) {
        list = document.createElement('fieldset');
        list.className = 'setbox pick-list'; list.id = 'sets-list'; list.setAttribute('aria-label', 'Collections');
        $('#sets-clip').after(list);
      }
      const label = document.createElement('label');
      label.className = 'check pick';
      label.innerHTML = '<input type="checkbox" checked> <span></span>';
      const box = $('input', label);
      box.dataset.set = made.id; box.onchange = () => tick(box);
      $('span', label).textContent = made.name;
      list.append(label);
      const set = ids(); set.add(made.id); keep(set);
      field.value = '';
      toast(`In ${made.name} now.`, 'ok');
    } catch (err) { toast(err.message, 'err'); }
  };
  $('#sets-make').onclick = make;
  field.onkeydown = (e) => { if (e.key === 'Enter') { e.preventDefault(); make(); } };
  $('#sets-done').onclick = () => dlg.close();
  dlg.showModal();
  ($('input[data-set]', dlg) || field).focus();
}

async function trash(card) {
  const id = card.dataset.id;
  try {
    const d = await api('DELETE', `/api/clips/${id}`);
    card.hidden = true;
    toast(`In the trash for ${d.trash_days} days.`, '', { label: 'Undo', run: async () => {
      try { await api('POST', `/api/clips/${id}/restore`); card.hidden = false; toast('Back in the library.', 'ok'); }
      catch (err) { toast(err.message, 'err'); }
    } }, '/trash');
  } catch (err) { toast(err.message, 'err'); }
}

export function clipMenu() {
  const menu = $('#card-menu');
  if (!menu) return;
  let card = null, button = null;
  const item = (act) => $(`[data-act="${act}"]`, menu);
  const items = () => $$('[role="menuitem"]', menu).filter((i) => !i.hidden);
  const del = item('delete');
  const unarm = () => { del.dataset.armed = ''; $('span', del).textContent = 'Delete'; $('.note', del).textContent = ''; };

  const place = (room) => {
    if (!button || menu.hidden) return;
    if (narrow()) {
      menu.classList.add('sheet');
      menu.style.top = ''; menu.style.left = '';
      document.body.style.paddingBottom = `${menu.offsetHeight + GAP}px`;
      const space = innerHeight - menu.offsetHeight;
      const r = card.getBoundingClientRect();
      if (room && r.bottom > space - GAP) scrollBy({ top: Math.min(r.top - GAP, r.bottom - space + GAP) });
      return;
    }
    menu.classList.remove('sheet');
    const w = menu.offsetWidth, h = menu.offsetHeight;
    let b = button.getBoundingClientRect();
    const short = b.bottom + 4 + h - (innerHeight - GAP);
    const below = document.documentElement.scrollHeight - innerHeight - scrollY;
    if (room && short > 0 && below >= short) { scrollBy({ top: short }); b = button.getBoundingClientRect(); }
    const c = card.getBoundingClientRect();
    let left = Math.max(GAP, Math.min(b.right - w, innerWidth - w - GAP)), top = b.bottom + 4;
    if (top + h > innerHeight - GAP) {
      top = Math.max(GAP, Math.min(b.bottom - h, innerHeight - h - GAP));
      if (c.right + 8 + w <= innerWidth - GAP) left = c.right + 8;
      else if (c.left - 8 - w >= GAP) left = c.left - 8 - w;
      else top = Math.max(GAP, b.top - h - 4);
    }
    menu.style.left = `${Math.round(left)}px`;
    menu.style.top = `${Math.round(top)}px`;
  };

  const close = (refocus) => {
    if (menu.hidden) return;
    menu.hidden = true;
    document.body.style.paddingBottom = '';
    button?.setAttribute('aria-expanded', 'false');
    unarm();
    if (refocus) button?.focus();
    card = null;
  };

  const open = (btn, e) => {
    card = btn.closest('.card'); button = btn;
    const id = card.dataset.id, edit = card.hasAttribute('data-edit');
    const ready = (card.dataset.state || 'ready') === 'ready';
    $('#card-menu-title').textContent = $('.card-title', card).textContent.trim();
    item('open').href = `/clips/${id}`;
    item('download').href = `/media/clips/${id}/download`;
    item('download').hidden = !ready;
    const link = item('link');
    if (link) {
      link.hidden = !ready || (!edit && card.dataset.shared === '0');
      $('span', link).textContent = handheld() && navigator.share ? 'Send the link' : 'Copy link';
      $('.note', link).textContent = '';
    }
    item('star').textContent = card.hasAttribute('data-star') ? 'Unstar' : 'Star';
    item('rename').hidden = !edit;
    item('sets').hidden = !$('#sets-dialog');
    del.hidden = !edit;
    unarm();
    menu.hidden = false;
    btn.setAttribute('aria-expanded', 'true');
    place(true);
    focusOnOpen(menu, e, items()[0]);
  };

  document.addEventListener('click', (e) => {
    const btn = e.target.closest('.card-more');
    if (btn) {
      e.preventDefault(); e.stopPropagation();
      const again = !menu.hidden && button === btn;
      close(false);
      if (!again) open(btn, e);
      return;
    }
    if (!menu.hidden && !menu.contains(e.target)) close(false);
  });
  document.addEventListener('keydown', (e) => {
    const btn = e.target.closest?.('.card-more');
    if (btn && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
      e.preventDefault(); e.stopPropagation();
      open(btn, { detail: 0 });
      if (e.key === 'ArrowUp') items().at(-1)?.focus();
    }
  }, true);
  menu.addEventListener('keydown', (e) => {
    const list = items(), at = list.indexOf(document.activeElement);
    const go = (i) => { e.preventDefault(); list[(i + list.length) % list.length]?.focus(); };
    if (e.key === 'Tab') { close(false); return; }
    e.stopPropagation();
    if (e.key === 'Escape') { e.preventDefault(); close(true); }
    else if (e.key === 'ArrowDown') go(at + 1);
    else if (e.key === 'ArrowUp') go(at < 0 ? -1 : at - 1);
    else if (e.key === 'Home') go(0);
    else if (e.key === 'End') go(-1);
    else if ((e.key === ' ' || e.key === 'Enter') && document.activeElement?.tagName === 'A') { e.preventDefault(); document.activeElement.click(); }
  });
  addEventListener('resize', () => place(false));
  addEventListener('scroll', () => { if (!narrow()) place(false); }, { passive: true });

  menu.addEventListener('click', async (e) => {
    const it = e.target.closest('[data-act]');
    if (!it || !card) return;
    const act = it.dataset.act, c = card, title = $('#card-menu-title').textContent;
    if (act === 'open' || act === 'download') { close(false); return; }
    e.preventDefault();
    if (act === 'delete') {
      if (del.dataset.armed !== '1') {
        del.dataset.armed = '1';
        $('span', del).textContent = 'Really delete?';
        $('.note', del).textContent = 'to the trash';
        return;
      }
      close(true);
      await trash(c);
      return;
    }
    close(act === 'link' || act === 'star');
    if (act === 'star') {
      const on = !c.hasAttribute('data-star');
      if (await setStar(c.dataset.id, on)) toast(on ? `Starred: ${title}.` : `Unstarred: ${title}.`, 'ok');
    } else if (act === 'link') await shareLink(c, title);
    else if (act === 'rename') renameDialog(c);
    else if (act === 'sets') setsDialog(c);
  });
  for (const id of ['#rename-dialog', '#sets-dialog']) $(id)?.addEventListener('close', () => { if (button?.isConnected) button.focus(); });
}
