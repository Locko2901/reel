import { $, $$, api, arm, copyText, focusOnOpen, toast } from './lib.js';

export function access() {
  const users = $('#users');
  if (!users) return;
  const dlg = $('#user-dialog'), form = $('#user-form'), title = $('#user-dialog-title');
  let mode = 'new', target = null;
  const open = (kind, row) => {
    mode = kind; target = row;
    title.textContent = kind === 'new' ? 'New user' : kind === 'rename' ? `Rename ${row.dataset.name}` : `A new password for ${row.dataset.name}`;
    $('#user-name-field').hidden = kind === 'password';
    $('#user-password-field').hidden = kind === 'rename';
    $('#user-role-field').hidden = kind !== 'new';
    form.name.value = kind === 'rename' ? row.dataset.name : ''; form.password.value = ''; form.role.value = 'member';
    dlg.showModal();
    (kind === 'password' ? form.password : form.name).focus();
  };
  $('#add-user').onclick = () => open('new', null);
  $('#user-cancel').onclick = () => dlg.close();
  $('#user-save').onclick = async () => {
    try {
      if (mode === 'new') {
        const d = await api('POST', '/api/users', { name: form.name.value, password: form.password.value, role: form.role.value });
        toast(`${d.name} can sign in now.`, 'ok');
      } else if (mode === 'rename') {
        await api('PATCH', `/api/users/${target.dataset.user}`, { name: form.name.value });
        toast(`${target.dataset.name} is ${form.name.value.trim()} now.`, 'ok');
      } else {
        await api('PATCH', `/api/users/${target.dataset.user}`, { password: form.password.value });
        toast(`${target.dataset.name} has a new password; their other sessions are out.`, 'ok');
      }
      dlg.close(); setTimeout(() => location.reload(), 600);
    } catch (err) { toast(err.message, 'err'); }
  };
  const linkDlg = $('#link-dialog');
  const showLink = async (row, e) => {
    const d = await api('POST', `/api/users/${row.dataset.user}/link`);
    const name = row.dataset.name;
    $('#link-dialog-title').textContent = `Link ${name} to the identity provider`;
    $('#link-line').textContent = `Send this link to ${name}. Whoever opens it and signs in at the identity provider is ${name} here from then on, with everything that is ${name}'s. It works once; a new link replaces it.`;
    $('#link-url').textContent = d.url;
    $('#link-until').textContent = `Works until ${d.until}.`;
    linkDlg.showModal();
    focusOnOpen(linkDlg, e, $('#link-copy'));
  };
  $('#link-copy').onclick = async () => {
    if (await copyText($('#link-url').textContent)) toast('Link copied.', 'ok');
    else toast('Copying did not work; select the link and copy it.', 'err');
  };
  $('#link-done').onclick = () => linkDlg.close();
  users.addEventListener('click', async (e) => {
    const b = e.target.closest('button[data-user-act]');
    if (!b) return;
    const row = b.closest('tr');
    const act = b.dataset.userAct;
    if (act === 'password' || act === 'rename') { open(act, row); return; }
    if (act === 'disable' && !arm(b, 'Sure?', 'Disable')) return;
    if (act === 'unlink' && !arm(b, 'Sure?', 'Unlink')) return;
    try {
      if (act === 'link') { await showLink(row, e); return; }
      if (act === 'unlink') {
        await api('DELETE', `/api/users/${row.dataset.user}/link`);
        toast(`${row.dataset.name} is unlinked and signed out; a new link lets them in again.`, 'ok');
      } else {
        await api('PATCH', `/api/users/${row.dataset.user}`, { disabled: act === 'disable' });
        toast(act === 'disable' ? `${row.dataset.name} is out until enabled again.` : `${row.dataset.name} is back.`, 'ok');
      }
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
