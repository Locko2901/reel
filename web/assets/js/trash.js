import { $, $$, api, focusOnOpen, toast } from './lib.js';
import { skippedWords } from './select.js';

function confirmFor(line, e) {
  const dlg = $('#trash-confirm');
  $('#trash-confirm-line').textContent = line;
  dlg.showModal();
  focusOnOpen(dlg, e, $('#trash-confirm-cancel'));
  return new Promise((resolve) => {
    const done = (ok) => { dlg.close(); resolve(ok); };
    $('#trash-confirm-go').onclick = () => done(true);
    $('#trash-confirm-cancel').onclick = () => done(false);
    dlg.onclose = () => resolve(false);
  });
}

export function trash() {
  if (document.body.dataset.page !== 'trash') return;
  const rows = () => $$('tr[data-id]');
  const picked = () => rows().filter((r) => $('.trash-pick', r)?.checked);
  const all = $('#trash-all'), bulk = $('#trash-bulk'), count = $('#trash-picked');
  const sync = () => {
    const n = picked().length, can = rows().filter((r) => $('.trash-pick', r)).length;
    if (bulk) bulk.hidden = n === 0;
    if (count) count.textContent = n ? `${n} picked` : 'Pick';
    if (all) { all.checked = n > 0 && n === can; all.indeterminate = n > 0 && n < can; }
  };
  // the rows acted on leave in one pass; the others stay picked, the toast says why
  const settle = (list, d, words) => {
    const done = new Set(d.done);
    for (const row of list) if (done.has(row.dataset.id)) row.remove();
    sync();
    const n = d.done.length, skipped = skippedWords(d.skipped);
    if (n || skipped) toast([n ? words(n) : '', skipped].filter(Boolean).join(' '), skipped ? 'err' : 'ok');
    if (!rows().length) setTimeout(() => location.reload(), 900);
  };
  const restore = async (list) => {
    try {
      const d = await api('POST', '/api/bulk/restore', { ids: list.map((r) => r.dataset.id) });
      const links = !document.body.hasAttribute('data-standalone');
      settle(list, d, (n) => (n === 1 ? `Back in the library${links ? ', with its links' : ''}.` : `${n} clips back in the library${links ? ', with their links' : ''}.`));
    } catch (err) { toast(err.message, 'err'); }
  };
  const purge = async (list, e) => {
    const line = list.length === 1 ? `Delete ${list[0].dataset.title} for good?` : `Delete ${list.length} clips for good?`;
    if (!await confirmFor(line, e)) return;
    try {
      const d = await api('POST', '/api/bulk/purge', { ids: list.map((r) => r.dataset.id) });
      settle(list, d, (n) => (n === 1 ? 'Deleted for good.' : `${n} clips deleted for good.`));
    } catch (err) { toast(err.message, 'err'); }
  };
  document.addEventListener('click', (e) => {
    const row = e.target.closest('tr[data-id]');
    if (e.target.closest('.trash-restore')) restore([row]);
    else if (e.target.closest('.trash-delete')) purge([row], e);
  });
  let anchor = null;
  $$('.trash-pick').forEach((box) => box.addEventListener('click', (e) => {
    const boxes = $$('.trash-pick');
    if (e.shiftKey && anchor && anchor !== box && boxes.includes(anchor)) {
      const a = boxes.indexOf(anchor), b = boxes.indexOf(box);
      boxes.slice(Math.min(a, b), Math.max(a, b) + 1).forEach((x) => { x.checked = box.checked; });
    }
    anchor = box;
    sync();
  }));
  document.addEventListener('keydown', (e) => {
    const el = document.activeElement;
    if (/^(INPUT|TEXTAREA|SELECT)$/.test(el?.tagName) && el.type !== 'checkbox') return;
    if (document.querySelector('dialog[open]')) return;
    if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === 'a' && $$('.trash-pick').length) {
      e.preventDefault();
      $$('.trash-pick').forEach((b) => { b.checked = true; });
      sync();
    } else if (e.key === 'Escape' && picked().length) {
      $$('.trash-pick').forEach((b) => { b.checked = false; });
      sync();
    }
  });
  all?.addEventListener('change', () => { $$('.trash-pick').forEach((b) => { b.checked = all.checked; }); sync(); });
  $('#trash-restore-picked')?.addEventListener('click', () => restore(picked()));
  $('#trash-delete-picked')?.addEventListener('click', (e) => purge(picked(), e));
  $('#trash-empty')?.addEventListener('click', async (e) => {
    const n = rows().length;
    if (!await confirmFor(`Empty the trash: delete ${n === 1 ? 'its clip' : `all ${n} clips`} for good?`, e)) return;
    try {
      const d = await api('POST', '/api/storage/purge-trash');
      toast(d.line || 'The trash is empty.', 'ok');
      setTimeout(() => location.reload(), 600);
    } catch (err) { toast(err.message, 'err'); }
  });
  sync();
}
