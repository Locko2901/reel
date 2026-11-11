// The update's question on the app's pages: Install now and Later, in the page and never raised.
import { $, invoke, toast } from './shell.js';

export function renderAsk(status) {
  const bar = $('#update-ask');
  if (!bar) return;
  const a = status && status.update_ask;
  bar.hidden = !a;
  if (!a) return;
  $('#update-ask-title').textContent = `${a.title}.`;
  $('#update-ask-words').textContent = a.words;
}

export function bindAsk() {
  for (const [id, install] of [['#update-ask-install', true], ['#update-ask-later', false]]) {
    const b = $(id);
    if (!b) continue;
    b.onclick = async () => {
      b.disabled = true;
      try {
        await invoke('update_answer', { install });
        if (!install) $('#update-ask').hidden = true;
      } catch (e) { toast(String(e.message || e), 'err'); }
      b.disabled = false;
    };
  }
}
