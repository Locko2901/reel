import { $, api, toast } from './lib.js';

export function status() {
  if (document.body.dataset.page !== 'status') return;
  const run = async (btn, path, words) => {
    btn.disabled = true;
    const was = btn.textContent;
    btn.textContent = words;
    try { await api('POST', path); location.reload(); }
    catch (err) { toast(err.message, 'err'); btn.disabled = false; btn.textContent = was; }
  };
  $('#doctor-btn')?.addEventListener('click', (e) => run(e.currentTarget, '/api/status/doctor', 'Probing'));
  $('#update-btn')?.addEventListener('click', (e) => run(e.currentTarget, '/api/status/update-check', 'Checking'));
}
