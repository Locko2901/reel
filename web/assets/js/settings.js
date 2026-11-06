import { $, $$, api, arm, toast } from './lib.js';
import { applyPrefs, prefs } from './global.js';
import { isOn, setOn } from './switch.js';

const CTL = 'input, select, button[role="switch"]';

const optionsOf = (row) => $$('.sopt', row).map(o => ({ value: o.dataset.opt, sw: $('button[role="switch"]', o) }));

function rawOf(row) {
  if (row.dataset.kind === 'set') return optionsOf(row).filter(o => isOn(o.sw)).map(o => o.value).join(', ');
  const ctl = $(CTL, row);
  if (!ctl) return null;
  return row.dataset.kind === 'bool' ? (isOn(ctl) ? 'true' : 'false') : ctl.value;
}

function show(row, d) {
  row.classList.remove('refused');
  $('.err', row).hidden = true;
  $('.changed', row).hidden = !d.changed;
  $('.changed', row).textContent = row.dataset.kind === 'set' ? 'changed from the defaults' : `changed, was ${d.default_text}`;
  $('.reset', row).hidden = !d.changed;
  const ctl = $(CTL, row);
  if (row.dataset.kind === 'set') { const on = d.value.split(',').map(v => v.trim()); optionsOf(row).forEach(o => setOn(o.sw, on.includes(o.value))); }
  else if (row.dataset.kind === 'bool') { setOn(ctl, d.value === 'true'); row.dataset.on = d.value; dependents(); }
  else if (row.dataset.kind === 'secret') { ctl.value = ''; ctl.placeholder = d.value === 'set' ? 'set, type to replace' : 'not set'; }
  else ctl.value = d.value;
}

// Parents come before their rows in the page, so one pass settles a chain.
function dependents() {
  $$('[data-depends]').forEach(r => {
    const p = $(`[data-key="${r.dataset.depends}"]`);
    const on = p && !('depOff' in p.dataset) && p.dataset.on === 'true';
    if (on) delete r.dataset.depOff; else r.dataset.depOff = '';
  });
}

async function save(row, value, undoing) {
  const key = row.dataset.key;
  try {
    const d = await api('PATCH', '/api/settings', { [key]: value });
    show(row, d);
    const words = d.words ?? d.value;
    const back = d.old_table === null ? null : d.old_table;
    const changedAtAll = d.old !== d.value;
    if (undoing) toast(`${d.label} is back to ${words}.`, 'ok');
    else if (changedAtAll && row.dataset.kind !== 'secret') toast(`${d.label} saved: ${words}.`, 'ok', { label: 'Undo', run: () => save(row, back, true) });
    else toast(`${d.label} saved.`, 'ok');
    for (const k of d.cleared || []) {
      const r = $(`.srow[data-key="${k}"]`);
      if (r) show(r, { value: 'not set', changed: false, default_text: 'the empty value' });
    }
    if (d.note) toast(d.note);
    counts();
  } catch (err) {
    row.classList.add('refused');
    const e = $('.err', row);
    e.textContent = err.message;
    e.hidden = false;
  }
}

function counts() {
  $$('.sections a[data-section]').forEach(a => {
    const n = $$(`.panel[data-section="${a.dataset.section}"] .srow .changed:not([hidden])`).length;
    let tag = $('.n', a);
    if (!n) { tag?.remove(); return; }
    if (!tag) { tag = document.createElement('span'); tag.className = 'n'; a.append(' ', tag); }
    tag.textContent = n;
  });
}

function actions() {
  document.addEventListener('click', async (e) => {
    const b = e.target.closest('button[data-action]');
    if (!b) return;
    if (b.dataset.confirm && !arm(b, b.dataset.confirm, b.dataset.label)) return;
    const row = b.closest('.srow'), note = $('.note', row);
    const tell = (text, cls) => { note.textContent = text; note.className = `note ${cls}`; note.hidden = false; };
    const field = $('input[data-field]', row);
    b.disabled = true;
    try {
      let extra = {};
      try { extra = b.dataset.extra ? JSON.parse(b.dataset.extra) : {}; } catch {}
      const body = field ? { [field.dataset.field]: field.value, ...extra } : (b.dataset.extra ? extra : undefined);
      const d = await api('POST', b.dataset.action, body);
      tell(d.line, 'ok');
      toast(d.line, 'ok');
      if (d.reload) setTimeout(() => location.reload(), 800);
    } catch (err) { tell(err.message, 'err'); }
    b.disabled = false;
  });
}

function browserPrefs() {
  const controls = $$('[data-pref]');
  if (!controls.length) return;
  const p = prefs();
  for (const c of controls) {
    const k = c.dataset.pref;
    const sw = c.getAttribute('role') === 'switch';
    if (sw) setOn(c, p[k] !== false);
    else c.value = p[k] || 'medium';
    c.addEventListener('change', () => {
      const now = prefs();
      now[k] = sw ? isOn(c) : c.value;
      try { localStorage.setItem('reel-prefs', JSON.stringify(now)); } catch {}
      applyPrefs();
      toast('Kept in this browser.', 'ok');
    });
  }
}

async function logs() {
  const box = $('#log-lines');
  if (!box) return;
  const load = async () => { try { const d = await api('GET', '/api/logs?lines=200'); box.textContent = d.lines.length ? d.lines.join('\n') : 'nothing yet'; box.scrollTop = box.scrollHeight; } catch (err) { box.textContent = err.message; } };
  $('#logs-refresh')?.addEventListener('click', load);
  await load();
}

async function settingsHistory() {
  const table = $('#history');
  if (!table) return;
  const body = $('tbody', table);
  try {
    const d = await api('GET', '/api/settings/history');
    const rows = d.history || [];
    $('#history-empty').hidden = rows.length > 0;
    for (const h of rows) {
      const tr = document.createElement('tr');
      const show = (v, words) => v === null || v === undefined ? 'the value under the table' : (v === '' ? 'empty' : (words || v));
      tr.innerHTML = `<td class="clip"></td><td class="sub"></td><td class="sub"></td><td class="sub"></td><td class="sub"></td><td class="right acts"><button class="quiet small">Revert</button></td>`;
      tr.children[0].textContent = h.label || h.key;
      tr.children[1].textContent = show(h.old, h.old_words);
      tr.children[2].textContent = show(h.new, h.new_words);
      tr.children[3].textContent = h.user || '';
      tr.children[4].textContent = h.when;
      $('button', tr).onclick = async (e) => {
        if (!arm(e.currentTarget, 'Sure?', 'Revert')) return;
        try { await api('PATCH', '/api/settings', { [h.key]: h.old === undefined ? null : h.old }); toast(`${h.label || h.key} is back to ${show(h.old, h.old_words)}.`, 'ok'); setTimeout(() => location.reload(), 600); }
        catch (err) { toast(err.message, 'err'); }
      };
      body.append(tr);
    }
  } catch (err) { $('#history-empty').textContent = err.message; $('#history-empty').hidden = false; }
}

function capLine() {
  const row = $('.srow[data-key="storage.cap_gb"]');
  const field = row && $('input', row);
  if (!field) return;
  let note = $('.snote', row), timer = null;
  if (!note) { note = document.createElement('span'); note.className = 'snote'; $('.shelp', row).after(note); }
  note.setAttribute('aria-live', 'polite');
  const ask = async () => {
    const gb = Math.max(0, Math.round(+field.value || 0));
    try {
      const d = await api('GET', `/api/storage/cap?gb=${gb}`);
      note.textContent = d.line;
      note.className = `snote${d.moves > 0 ? ' st-warn' : ''}`;
    } catch { /* the line stays as it was */ }
  };
  field.addEventListener('input', () => { clearTimeout(timer); timer = setTimeout(ask, 300); });
  field.addEventListener('change', ask);
}

export function settings() {
  capLine();
  dependents();
  actions();
  browserPrefs();
  logs();
  settingsHistory();
  $$('.srow[data-key]:not(.locked), .sgroup[data-key]:not(.locked)').forEach(row => {
    const ctl = $(CTL, row);
    if (!ctl) return;
    if (row.dataset.kind === 'set') optionsOf(row).forEach(o => o.sw.addEventListener('change', () => save(row, rawOf(row), false)));
    else ctl.addEventListener('change', () => save(row, rawOf(row), false));
    $('.reset', row)?.addEventListener('click', () => save(row, null, false));
  });
  const root = $('#settings');
  if (!root) return;
  const body = $('#sections-body');
  const select = $('#sections-select');
  function open(id, push) {
    if (push && 'index' in root.dataset) scrollTo(0, 0);
    delete root.dataset.index;
    root.dataset.section = id;
    $$('[data-section]', body).forEach(el => el.hidden = el.dataset.section !== id);
    $$('.srow, .sgroup', body).forEach(r => r.hidden = false);
    $('#no-match').hidden = true;
    $$('.sections a[data-section]', root).forEach(a => { if (a.dataset.section === id) a.setAttribute('aria-current', 'page'); else a.removeAttribute('aria-current'); });
    if (select) select.value = id;
    if (push) history.pushState(null, '', `/settings/${id}`);
  }
  $$('.sections a[data-section]', root).forEach(a => a.addEventListener('click', (e) => { e.preventDefault(); open(a.dataset.section, true); }));
  select?.addEventListener('change', () => open(select.value, true));
  const atIndex = () => location.pathname.replace(/\/$/, '') === '/settings';
  window.addEventListener('popstate', () => { open(atIndex() ? 'general' : location.pathname.split('/').pop(), false); if (atIndex()) root.dataset.index = ''; });
  const filter = $('#settings-filter');
  filter?.addEventListener('input', () => {
    const q = filter.value.trim().toLowerCase();
    if (!q) { open(root.dataset.section, false); if (atIndex()) root.dataset.index = ''; return; }
    delete root.dataset.index;
    let hits = 0;
    $$('.section-line', body).forEach(l => l.hidden = true);
    $$('.panel', body).forEach(p => {
      let any = false;
      $$('.srow', p).filter(r => !r.closest('.sgroup')).forEach(r => {
        const hit = !('depOff' in r.dataset) && `${r.dataset.label || ''} ${r.dataset.key || ''} ${$('.shelp', r)?.textContent || ''} ${$('b', r)?.textContent || ''}`.toLowerCase().includes(q);
        r.hidden = !hit; any = any || hit;
      });
      $$('.sgroup', p).forEach(g => {
        const hit = !('depOff' in g.dataset) && `${g.dataset.label} ${g.dataset.key} ${g.textContent}`.toLowerCase().includes(q);
        g.hidden = !hit; any = any || hit;
      });
      p.hidden = !any; hits += any ? 1 : 0;
    });
    $('#no-match').hidden = hits > 0;
  });
}
