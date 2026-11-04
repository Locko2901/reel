// Shown when the server cannot be reached, so it must never need the server.
import { $, invoke, listen, mount, renderParts, toast } from './shell.js';

let status = null;

function render() {
  if (!status) return;
  renderParts(status);
  const off = status.offline;
  if (off) {
    $('#offline-title').textContent = off.title || 'Cannot reach the server';
    $('#offline-words').textContent = off.words || '';
    $('#offline-code').textContent = off.code || '';
    $('#offline-retry-line').textContent = off.retry_line || '';
  }
  const here = status.mode === 'standalone';
  $('#offline-standalone').hidden = here;
  if (here) $('#offline-ways').textContent = 'The library stays on this PC as it was. The app\'s settings are always there, in the tray\'s Open settings and with Ctrl+, in this window.';
}

async function main() {
  await mount('library');
  $('#offline-retry').onclick = () => invoke('offline_retry').catch(e => toast(String(e.message || e), 'err'));
  $('#offline-settings').onclick = () => invoke('view_show', { view: 'settings', section: status && status.mode === 'standalone' ? 'status' : 'server' }).catch(e => toast(String(e.message || e), 'err'));
  $('#offline-standalone').onclick = async () => {
    try {
      const s = await invoke('settings_get');
      s.mode = 'standalone';
      await invoke('settings_save', { settings: s, force: true });
      toast('Standalone: reel starts on this PC', 'ok');
    } catch (e) { toast(String(e.message || e), 'err'); }
  };
  try { status = await invoke('status_get'); render(); } catch (e) { $('#offline-words').textContent = 'no shell'; }
  // reached from the history once the server is back, the page leaves for the server's pages
  if (status && !status.offline) invoke('offline_retry').catch(() => {});
  await listen('status', (ev) => { status = ev.payload; render(); });
}

main();
