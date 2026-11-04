import { $, $$, api, arm, toast } from './lib.js';

async function makeSet(name) {
  return api('POST', '/api/collections', { name });
}

function clipPanel() {
  const panel = $('#sets-panel');
  if (!panel) return;
  const clip = panel.dataset.clip;
  const tick = async (box) => {
    try {
      await api(box.checked ? 'PUT' : 'DELETE', `/api/collections/${box.dataset.set}/clips/${clip}`);
      const name = box.closest('label').querySelector('span').textContent;
      toast(box.checked ? `In ${name} now.` : `Out of ${name}.`, 'ok');
    } catch (err) { box.checked = !box.checked; toast(err.message, 'err'); }
  };
  $$('input[data-set]', panel).forEach(box => box.addEventListener('change', () => tick(box)));
  const field = $('#new-set', panel), add = $('#make-set', panel);
  const make = async () => {
    const name = field.value.trim();
    if (!name) { field.focus(); return; }
    try {
      const set = await makeSet(name);
      await api('PUT', `/api/collections/${set.id}/clips/${clip}`);
      let box = $('.setbox', panel);
      if (!box) { box = document.createElement('span'); box.className = 'setbox'; box.setAttribute('role', 'group'); box.setAttribute('aria-label', 'Collections'); panel.querySelector('.body').prepend(box); }
      const label = document.createElement('label'); label.className = 'check';
      label.innerHTML = '<input type="checkbox" checked> <span></span>';
      const input = $('input', label); input.dataset.set = set.id; input.addEventListener('change', () => tick(input));
      $('span', label).textContent = set.name;
      box.append(label);
      field.value = '';
      toast(`In ${set.name} now.`, 'ok');
    } catch (err) { toast(err.message, 'err'); }
  };
  add.addEventListener('click', make);
  field.addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); make(); } });
}

function listPage() {
  const open = $('#new-set-open'), dlg = $('#new-set-dialog'), add = $('#make-set'), field = $('#new-set');
  if (!open || !dlg) return;
  open.addEventListener('click', () => { dlg.showModal(); field.focus(); });
  $('#new-set-cancel').addEventListener('click', () => dlg.close());
  const make = async () => {
    const name = field.value.trim();
    if (!name) { field.focus(); return; }
    try { const set = await makeSet(name); location.href = `/collections/${set.id}`; } catch (err) { toast(err.message, 'err'); }
  };
  add.addEventListener('click', make);
  field.addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); make(); } });
}

function setPage() {
  const head = $('#set');
  if (!head) return;
  const id = head.dataset.id;
  const name = $('#set-name');
  name?.addEventListener('change', async () => {
    try { const d = await api('PATCH', `/api/collections/${id}`, { name: name.value }); document.title = `${d.name} · reel`; toast('Name saved.', 'ok'); } catch (err) { toast(err.message, 'err'); }
  });
  $('#set-delete')?.addEventListener('click', async (e) => {
    if (!arm(e.currentTarget, 'The clips stay', 'Delete the collection')) return;
    try { await api('DELETE', `/api/collections/${id}`); location.href = '/collections'; } catch (err) { toast(err.message, 'err'); }
  });
  $('#set-link-make')?.addEventListener('click', async () => {
    try { await api('POST', `/api/collections/${id}/link`, {}); toast('Link made. The one before stopped working.', 'ok'); setTimeout(() => location.reload(), 500); } catch (err) { toast(err.message, 'err'); }
  });
  $('#set-revoke')?.addEventListener('click', async (e) => {
    if (!arm(e.currentTarget, 'Sure?', 'Revoke')) return;
    try { await api('DELETE', `/api/collections/${id}/link`); toast('Link revoked. It stopped working just now.'); setTimeout(() => location.reload(), 500); } catch (err) { toast(err.message, 'err'); }
  });
}

export function collections() {
  clipPanel();
  listPage();
  setPage();
}
