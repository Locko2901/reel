import { $, api, arm, focusOnOpen, toast } from './lib.js';

export function devices() {
  const btn = $('#pair-device');
  if (!btn) return;
  const dlg = $('#pair-dialog');
  let timer = null;
  const countdown = (until) => {
    clearInterval(timer);
    const tick = () => {
      const left = Math.max(0, Math.round((until - Date.now()) / 1000));
      $('#pair-expiry').textContent = left > 0 ? `expires in ${Math.floor(left / 60)}:${String(left % 60).padStart(2, '0')}` : 'expired; make a new one';
      if (left <= 0) clearInterval(timer);
    };
    tick();
    timer = setInterval(tick, 1000);
  };
  btn.onclick = async (e) => {
    btn.disabled = true;
    try {
      const d = await api('POST', '/api/devices/pair-code');
      $('#pair-code').textContent = d.code;
      if (d.server_url) $('#pair-url').textContent = d.server_url;
      countdown(Date.now() + d.expires_in * 1000);
      dlg.showModal();
      focusOnOpen(dlg, e, $('#pair-copy'));
    } catch (err) { toast(err.message, 'err'); }
    btn.disabled = false;
  };
  $('#pair-copy').onclick = async () => {
    try { await navigator.clipboard.writeText($('#pair-code').textContent); toast('Code copied.', 'ok'); }
    catch { toast('Copy it by hand.', 'err'); }
  };
  $('#pair-done').onclick = () => { dlg.close(); clearInterval(timer); setTimeout(() => location.reload(), 300); };
  $('#devices')?.addEventListener('click', async (e) => {
    const b = e.target.closest('button[data-device-act]');
    if (!b) return;
    const row = b.closest('tr');
    if (!arm(b, 'Sure?', 'Revoke')) return;
    try {
      await api('DELETE', `/api/devices/${row.dataset.device}`);
      toast(`${row.dataset.name} is out; it has to pair again.`, 'ok');
      setTimeout(() => location.reload(), 600);
    } catch (err) { toast(err.message, 'err'); }
  });
}
