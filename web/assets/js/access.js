import { $, $$, api, arm, toast } from './lib.js';

export function access() {
  const users = $('#users');
  if (!users) return;
  const dlg = $('#user-dialog'), form = $('#user-form'), title = $('#user-dialog-title');
  let mode = 'new', target = null;
  const open = (kind, row) => {
    mode = kind; target = row;
    title.textContent = kind === 'new' ? 'New user' : `A new password for ${row.dataset.name}`;
    $('#user-name-field').hidden = kind !== 'new';
    $('#user-role-field').hidden = kind !== 'new';
    form.name.value = ''; form.password.value = ''; form.role.value = 'member';
    dlg.showModal();
    (kind === 'new' ? form.name : form.password).focus();
  };
  $('#add-user').onclick = () => open('new', null);
  $('#user-cancel').onclick = () => dlg.close();
  $('#user-save').onclick = async () => {
    try {
      if (mode === 'new') {
        const d = await api('POST', '/api/users', { name: form.name.value, password: form.password.value, role: form.role.value });
        toast(`${d.name} can sign in now.`, 'ok');
      } else {
        await api('PATCH', `/api/users/${target.dataset.user}`, { password: form.password.value });
        toast(`${target.dataset.name} has a new password; their other sessions are out.`, 'ok');
      }
      dlg.close(); setTimeout(() => location.reload(), 600);
    } catch (err) { toast(err.message, 'err'); }
  };
  users.addEventListener('click', async (e) => {
    const b = e.target.closest('button[data-user-act]');
    if (!b) return;
    const row = b.closest('tr');
    const act = b.dataset.userAct;
    if (act === 'password') { open('password', row); return; }
    if (act === 'disable' && !arm(b, 'Sure?', 'Disable')) return;
    try {
      await api('PATCH', `/api/users/${row.dataset.user}`, { disabled: act === 'disable' });
      toast(act === 'disable' ? `${row.dataset.name} is out until enabled again.` : `${row.dataset.name} is back.`, 'ok');
      setTimeout(() => location.reload(), 600);
    } catch (err) { toast(err.message, 'err'); }
  });
  users.addEventListener('change', async (e) => {
    const sel = e.target.closest('select.role');
    if (!sel) return;
    const row = sel.closest('tr');
    try { await api('PATCH', `/api/users/${row.dataset.user}`, { role: sel.value }); toast(`${row.dataset.name} is ${sel.value === 'admin' ? 'an admin' : 'a member'} now.`, 'ok'); }
    catch (err) { toast(err.message, 'err'); sel.value = sel.value === 'admin' ? 'member' : 'admin'; }
  });
  $('#signout-all')?.addEventListener('click', async (e) => {
    if (!arm(e.currentTarget, 'Every device, this one too?', 'Sign out everywhere')) return;
    try { await api('POST', '/api/sessions/signout-all'); location.href = '/signin'; }
    catch (err) { toast(err.message, 'err'); }
  });
}
