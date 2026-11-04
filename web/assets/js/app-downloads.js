// The app's webview shows no download bar, so the app tells the page as a download starts,
// goes and ends, and the page says so in a toast. In a browser nothing here runs.
import { $ } from './lib.js';
import { size } from './format.js';

export function appDownloads() {
  const t = window.__TAURI__;
  const call = t && t.core && typeof t.core.invoke === 'function' ? t.core.invoke : null;
  if (!call || !window.__REEL_DESKTOP__) return;
  const timers = new Map();
  // the app calls this on each change of a download; what the toast says goes to its log
  window.__reelDownload = (d) => {
    const host = $('#toasts');
    if (!host || !d) return '';
    const pct = d.total && d.got ? `, ${Math.min(99, Math.floor(d.got * 100 / d.total))}%` : '';
    const words = d.state === 'done' ? `Saved to ${d.folder}: ${d.name}`
      : d.state === 'failed' ? `${d.name} did not download: ${d.reason}`
        : `Downloading ${d.name}${d.total ? `, ${size(d.total)}` : ''}${pct}`;
    let el = host.querySelector(`.toast[data-download="${Number(d.id)}"]`);
    const fresh = !el;
    if (fresh) {
      el = document.createElement('div');
      el.dataset.download = String(Number(d.id));
      el.setAttribute('role', 'status');
      const w = document.createElement('span');
      w.className = 'toast-words';
      const a = document.createElement('span');
      a.className = 'toast-acts';
      el.append(w, ' ', a);
      host.replaceChildren(el);
    }
    el.className = `toast${d.state === 'done' ? ' ok' : d.state === 'failed' ? ' err' : ''}`;
    $('.toast-words', el).textContent = words;
    if (fresh || d.state !== 'running') {
      const button = (label, cls, run) => {
        const b = document.createElement('button');
        b.type = 'button';
        b.className = cls;
        b.textContent = label;
        b.onclick = run;
        return b;
      };
      const open = (reveal) => () => call('app_download_open', { id: d.id, reveal }).catch(() => {});
      const acts = [];
      if (d.state === 'done') {
        acts.push(button('Show in folder', 'small', open(true)));
        if (d.openable) acts.push(button('Open', 'small', open(false)));
      }
      acts.push(button('Dismiss', 'quiet small', () => el.remove()));
      $('.toast-acts', el).replaceChildren(...acts);
    }
    clearTimeout(timers.get(d.id));
    if (d.state !== 'running') timers.set(d.id, setTimeout(() => el.remove(), 9000));
    return words;
  };
}
