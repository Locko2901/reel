import { $, $$, api, toast } from './lib.js';

export function setup() {
  if (document.body.dataset.page !== 'setup') return;
  const finish = async () => { try { await api('POST', '/api/setup/done'); location.href = '/'; } catch (err) { toast(err.message, 'err'); } };
  $('#setup-done')?.addEventListener('click', finish);
  $('#setup-skip')?.addEventListener('click', finish);
  $('#folders-again')?.addEventListener('click', () => location.reload());
  $$('button[data-mkdir]').forEach(b => b.onclick = async () => {
    try { await api('POST', '/api/setup/mkdir', { key: b.dataset.mkdir }); toast('Made.', 'ok'); setTimeout(() => location.reload(), 500); }
    catch (err) { toast(err.message, 'err'); }
  });
  $('#url-check')?.addEventListener('click', async (e) => {
    const b = e.currentTarget; b.disabled = true;
    const out = $('#url-result');
    try { const d = await api('POST', '/api/setup/url-check'); out.hidden = false; out.textContent = d.line; out.className = d.ok ? 'small st-ok' : 'small st-warn'; }
    catch (err) { toast(err.message, 'err'); }
    b.disabled = false;
  });
  $('#doctor-btn')?.addEventListener('click', async (e) => {
    const b = e.currentTarget; b.disabled = true; b.textContent = 'Probing';
    try { await api('POST', '/api/status/doctor'); location.reload(); } catch (err) { toast(err.message, 'err'); b.disabled = false; b.textContent = 'Run again'; }
  });
}
