// The bar while OBS's window is up; after Hide OBS what changed is read back, and `done` gets
// {taken, settings, status} and what `prepare` gave.
import { $, invoke, toast } from './shell.js';

export function renderObsBar(status) {
  const bar = $('#obs-bar');
  if (!bar) return;
  const box = status && status.obs_box;
  bar.hidden = !(box && box.shown && box.can_show);
}

export function bindObsBar(done, prepare) {
  for (const b of [$('#obs-bar-hide'), $('#bundled-hide')]) {
    if (!b) continue;
    b.onclick = async () => {
      b.disabled = true;
      try {
        const before = prepare ? await prepare() : null;
        await done(await invoke('obs_done'), before);
      } catch (e) { toast(String(e.message || e), 'err'); }
      b.disabled = false;
    };
  }
}
