import { $, $$, api, arm, onEvent, toast } from './lib.js';

// The list is asked for again every 3 s while a picture is being made, so a missed event never
// leaves a row making.
export function animMade(id) {
  const box = $('#anim-made');
  if (!box) return;
  let timer = null, asking = false, again = false;
  const making = () => !!$('[data-making]', box);
  const later = () => { clearTimeout(timer); timer = making() ? setTimeout(draw, 3000) : null; };
  async function draw() {
    if (asking) { again = true; return; }
    asking = true;
    try {
      const d = await api('GET', `/api/clips/${id}/anims`);
      box.innerHTML = d.html;
      box.hidden = !$('.made', box);
      bind();
    } catch { /* asked again below or on the next event */ }
    asking = false;
    if (again) { again = false; draw(); return; }
    later();
  }
  function bind() {
    $$('.anim-delete', box).forEach((b) => b.onclick = async () => {
      if (!arm(b, 'Sure?', 'Delete')) return;
      try {
        await api('DELETE', `/api/renditions/${b.dataset.rid}`);
        toast('Deleted. Make and download makes it again.', 'ok');
      } catch (err) { toast(err.message, 'err'); }
      draw();
    });
  }
  onEvent((ev) => {
    if (ev.type === 'resumed') { draw(); return; }
    if (ev.type !== 'job' || ev.kind !== 'anim' || ev.clip_id !== id) return;
    const state = ev.state === 'running' && $(`.made[data-rid="${ev.rendition_id}"] .made-state`, box);
    if (state) { if (ev.progress >= 0.01) state.textContent = `making, ${Math.round(ev.progress * 100)}%`; return; }
    draw();
  });
  bind();
  later();
}
