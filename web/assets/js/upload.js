import { $, jobNow, waitFor } from './lib.js';
import { Paused, sendInPieces } from './pieces.js';

function bar(row, sent, total, word) {
  const p = total ? Math.floor(sent / total * 100) : 0;
  $('.bar i', row).style.width = `${p}%`;
  $('.mono', row).textContent = word ? `${word} ${p}%` : `${p}%`;
}

// One file in pieces, its row in `list`; `then(clip)` once the library has it.
export function send(file, list, then) {
  const row = document.createElement('div');
  row.className = 'notice active';
  row.innerHTML = '<span class="grow ellipsis"></span><span class="bar" style="width:160px"><i style="width:0"></i></span><span class="mono small">0%</span>';
  $('span', row).textContent = file.name;
  list.prepend(row);
  const go = async () => {
    row.className = 'notice active';
    try {
      const d = await sendInPieces(file, { progress: (sent, total, word) => bar(row, sent, total, word) });
      const c = d.clip;
      row.className = 'notice ok';
      row.innerHTML = '<span class="grow"></span><span class="mono small" data-job>queued</span>';
      const line = $('span', row), a = document.createElement('a');
      a.href = `/clips/${c.id}`;
      a.textContent = c.title;
      if (d.status === 'exists' || d.status === 'done') {
        line.append('already in the library: ', a);
        $('[data-job]', row).textContent = '';
      } else {
        line.append('uploaded, now encoding: ', a);
        const say = (j, done) => {
          const m = $('[data-job]', row);
          if (m) m.textContent = j.state === 'done' ? 'ready' : j.state === 'failed' ? `failed: ${j.error}` : j.state === 'cancelled' ? 'cancelled' : j.state === 'running' ? `${Math.round(j.progress * 100)}%` : m.textContent;
          if (['done', 'failed', 'cancelled'].includes(j.state)) done();
        };
        waitFor(
          (ev, done) => { if (ev.type === 'job' && ev.id === c.job_id) say(ev, done); },
          async (done) => { const j = await jobNow(c.job_id); if (j) say(j, done); else done(); },
        );
      }
      then?.(c);
    } catch (e) {
      row.className = 'notice err';
      row.innerHTML = '<span class="grow"></span>';
      $('span', row).textContent = `${file.name}: ${e.message}`;
      if (e instanceof Paused) {
        const again = document.createElement('button');
        again.className = 'btn small';
        again.type = 'button';
        again.textContent = 'Retry';
        again.onclick = () => { again.remove(); go(); };
        row.append(again);
      }
    }
  };
  go();
}

function dropOn(target, list, shown) {
  ['dragenter', 'dragover'].forEach((t) => target.addEventListener(t, (e) => {
    if (![...(e.dataTransfer?.types || [])].includes('Files')) return;
    e.preventDefault();
    target.classList.add('over');
  }));
  ['dragleave', 'drop'].forEach((t) => target.addEventListener(t, (e) => {
    if (t === 'dragleave' && target.contains(e.relatedTarget)) return;
    target.classList.remove('over');
  }));
  target.addEventListener('drop', (e) => {
    const files = [...(e.dataTransfer?.files || [])];
    if (!files.length) return;
    e.preventDefault();
    shown?.();
    files.forEach((f) => send(f, list));
  });
}

export function upload() {
  const list = $('#uploads');
  if (!list) return;
  const drop = $('#drop'), input = $('#file');
  if (drop && input) {
    input.onchange = () => { [...input.files].forEach((f) => send(f, list)); input.value = ''; };
    dropOn(drop, list);
  }
  if ($('.lib-head')) dropOn(document.body, list, () => { list.hidden = false; });
}
