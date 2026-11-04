import { $, api, arm, onEvent, toast } from './lib.js';

export function jobs() {
  if (document.body.dataset.page !== 'jobs') return;
  document.addEventListener('click', async (e) => {
    const b = e.target.closest('button[data-act]');
    if (!b) return;
    const { act, id } = b.dataset;
    if (act === 'cancel' && !arm(b, 'Sure?', 'Cancel')) return;
    if (act === 'discard' && !arm(b, 'File and clip go', 'Discard')) return;
    try {
      const d = await api('POST', `/api/jobs/${id}/${act}`);
      toast(act === 'retry' ? `Queued again, attempt ${d.attempts + 1}.` : act === 'cancel' ? 'Cancelled.' : act === 'discard' ? 'Discarded, file and clip.' : 'Dismissed.', 'ok');
      setTimeout(() => location.reload(), 600);
    } catch (err) { toast(err.message, 'err'); }
  });
  let timer = null;
  const later = () => { clearTimeout(timer); timer = setTimeout(() => location.reload(), 800); };
  onEvent((ev) => {
    if (ev.type === 'resumed') { later(); return; }
    if (ev.type !== 'job') return;
    if (ev.state === 'running') {
      const row = $(`[data-job="${ev.id}"]`);
      if (!row || !$('.bar i', row)) { later(); return; }
      const pct = Math.round(ev.progress * 100);
      $('.bar i', row).style.width = `${pct}%`;
      $('.pct', row).textContent = `${pct}%`;
      return;
    }
    later();
  });
}
