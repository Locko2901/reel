// What a clip hears: the chosen sources, a level row with its meter for each, and where each goes.
import { $$, invoke } from './shell.js';

export async function audioPlan(setup) {
  const none = { sources: [], words: [], apart: false };
  try { return (await invoke('obs_audio', { setup })) || none; } catch (e) { return none; }
}

function levelRow(name, onLevel) {
  const row = document.createElement('div');
  row.className = 'srow';
  row.dataset.vol = name;
  row.innerHTML = '<div class="slabel"><b></b><span class="shelp">its volume in OBS; the meter shows it after the volume</span><span class="meter" aria-hidden="true"><i></i></span></div>'
    + '<div class="sctl"><span class="vol"><input type="range" min="-30" max="10" step="1" value="0"><span class="ctl-group"><input type="number" class="in-num" min="-30" max="10" step="1" value="0"><span class="unit">dB</span></span></span></div>';
  row.querySelector('b').textContent = `${name} level`;
  const [range, num] = row.querySelectorAll('input');
  range.setAttribute('aria-label', `${name} level, in dB`);
  num.setAttribute('aria-label', `${name} level, in dB, as a number`);
  range.addEventListener('input', () => { num.value = range.value; onLevel(name, +range.value, false); });
  for (const i of [range, num]) i.addEventListener('change', () => onLevel(name, Math.max(-30, Math.min(10, Math.round(+i.value || 0))), true));
  return row;
}

// The level rows of `box` made the chosen sources' and nothing else, a row kept as it is
// while it is still chosen; `onLevel(name, db, done)`.
export function renderLevelRows(box, sources, volumes, onLevel) {
  const had = new Map($$('.srow[data-vol]', box).map((r) => [r.dataset.vol, r]));
  const rows = sources.map((s) => had.get(s.name) || levelRow(s.name, onLevel));
  box.replaceChildren(...rows);
  for (const row of rows) {
    if (row.contains(document.activeElement)) continue;
    for (const i of $$('input', row)) i.value = (volumes || {})[row.dataset.vol] || 0;
  }
  return rows.length;
}

export function meterIn(box, name, db) {
  const row = $$('.srow[data-vol]', box).find((r) => r.dataset.vol === name);
  const bar = row && row.querySelector('.meter i');
  if (bar) bar.style.width = `${Math.max(0, Math.min(100, (db + 60) / 60 * 100))}%`;
}

export function wordsInto(el, words) {
  el.replaceChildren(...words.map((w) => { const p = document.createElement('span'); p.textContent = `${w}.`; return p; }));
  el.hidden = !words.length;
}
