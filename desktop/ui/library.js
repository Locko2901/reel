// This app, Server, Library: where the library is, a move to another drive, a copy to keep.
import { $, invoke, toast } from './shell.js';

let info = null;
let plan = null;
let shown = '';
let dismissed = '';
let wasRunning = false;

function size(b) {
  if (b >= 1e12) return `${(b / 1e12).toFixed(1)} TB`;
  if (b >= 1e9) return `${(b / 1e9).toFixed(1)} GB`;
  if (b >= 1e6) return `${(b / 1e6).toFixed(1)} MB`;
  if (b >= 1e3) return `${Math.round(b / 1e3)} kB`;
  return `${b} B`;
}

function left(job) {
  const secs = (Date.now() - job.started_ms) / 1000;
  if (!job.total || job.done <= 0 || secs < 3) return '';
  const rest = (job.total - job.done) / (job.done / secs);
  if (rest < 60) return 'under a minute left';
  return `about ${Math.round(rest / 60)} min left`;
}

const PHASES = {
  move: {
    starting: () => 'stopping reel on this PC',
    renaming: () => 'renaming the folder',
    copying: (j) => `copying, ${size(j.done)} of ${size(j.total)}`,
    checking: (j) => `checking the copy, ${size(j.done)} of ${size(j.total)}`,
    switching: () => 'starting reel from there',
    removing: () => 'removing the old folder',
  },
  use: { starting: () => 'switching' },
  copy: {
    starting: () => 'starting',
    database: () => 'copying the database',
    copying: (j) => `copying the clips, ${size(j.done)} of ${size(j.total)}`,
    checking: (j) => `checking the copy, ${size(j.done)} of ${size(j.total)}`,
  },
};

const TITLE = { move: 'Moving the library to', use: 'Taking up the library in', copy: 'Copying the library to' };

function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
}

function button(text, cls, onclick) {
  const b = el('button', cls, text);
  b.type = 'button';
  b.onclick = onclick;
  return b;
}

function notice(kind, words, buttons, bar) {
  const n = el('div', `notice ${kind}`);
  n.setAttribute('role', 'status');
  const w = el('span', 'grow');
  for (const part of words) w.append(typeof part === 'string' ? document.createTextNode(part) : part);
  n.append(w);
  if (buttons.length) {
    const a = el('div', 'actions');
    a.append(...buttons);
    n.append(a);
  }
  if (bar != null) {
    const b = el('span', 'bar');
    const i = el('i');
    i.style.width = `${Math.min(100, Math.max(0, bar))}%`;
    b.append(i);
    n.append(b);
  }
  return n;
}

const path = (p) => el('span', 'mono', p);

function planWords(p) {
  const t = p.p;
  if (p.kind === 'copy') {
    return [
      `Make a copy of the library, ${size(t.bytes)}, in `, path(t.dir), `. There is ${size(t.free)} free there. `,
      'reel goes on while it copies, and the copy is checked file by file before it gets its name.',
      ...(t.removable ? [' The drive can be unplugged: wait until the copy says it is done.'] : []),
    ];
  }
  if (t.kind === 'use') {
    return [
      path(t.dir), ` holds a reel library with ${t.clips} ${t.clips === 1 ? 'clip' : 'clips'}. Use it? `,
      'reel on this PC starts from it; the library in ', path(info ? info.dir : ''), ' stays where it is, and picking it again goes back.',
    ];
  }
  if (t.same_drive) {
    return ['Move the library to ', path(t.to), ': the same drive, so the folder is renamed, at once.'];
  }
  return [
    `Move the library, ${size(t.bytes)}, to `, path(t.to), `. There is ${size(t.free)} free there. `,
    'reel on this PC stops while every file is copied and checked: the clips you save meanwhile wait in the clips folder and go in after. ',
    'The library here is removed once the copy is checked.',
    ...(t.removable ? [' The drive can be unplugged: while it is, reel on this PC waits for it, and new clips wait in the clips folder.'] : []),
  ];
}

async function start() {
  const p = plan;
  plan = null;
  try {
    if (p.kind === 'copy') await invoke('library_copy', { into: p.picked });
    else await invoke('library_move', { picked: p.picked });
  } catch (e) {
    plan = { ...p, error: String(e.message || e) };
  }
  render(lastStatus);
}

let lastStatus = null;

function jobKey(j) { return `${j.kind}|${j.to}|${j.started_ms}|${j.line}`; }

function render(status) {
  lastStatus = status;
  const box = $('#library-job');
  if (!box || !status) return;
  const lib = status.library || {};
  const job = lib.job;
  const running = !!(job && job.running);
  $('#library-move').disabled = running;
  $('#library-copy').disabled = running || !status.local;
  $('#library-copy').title = status.local ? '' : 'the copy is made through reel on this PC, which is not running';
  const line = $('#library-line');
  line.textContent = '';
  line.classList.toggle('st-err', !!lib.away);
  if (lib.away) line.append(lib.away);
  else if (info) {
    line.append(path(info.dir), info.drive
      ? `: ${size(info.bytes)}, on ${info.drive} with ${size(info.free)} free of ${size(info.total)}`
      : `: ${size(info.bytes)}; ${size(info.free)} free of ${size(info.total)} on its drive`);
    if (info.removable) line.append(', a drive that can be unplugged');
  } else line.append(path(lib.dir || ''));
  let n = null;
  if (running) {
    const words = (PHASES[job.kind] || {})[job.phase];
    const t = left(job);
    const stoppable = job.kind === 'copy' || ['starting', 'copying', 'checking'].includes(job.phase);
    n = notice('active', [
      `${TITLE[job.kind] || 'Working on'} `, path(job.to), `: ${words ? words(job) : job.phase}${t ? `, ${t}` : ''}`,
    ], stoppable ? [button('Stop', 'small quiet', () => invoke('library_stop'))] : [],
    job.total ? (job.done * 100) / job.total : 0);
  } else if (plan && plan.error) {
    n = notice('err', [plan.error.replace(/^./, (c) => c.toUpperCase()) + (/[.!?]$/.test(plan.error) ? '' : '.')],
      [button('Dismiss', 'small quiet', () => { plan = null; render(lastStatus); })]);
  } else if (plan) {
    const go = plan.kind === 'copy' ? 'Make the copy' : plan.p.kind === 'use' ? 'Use it' : 'Move';
    n = notice('active', planWords(plan), [
      button(go, 'small primary', start),
      button('Cancel', 'small quiet', () => { plan = null; render(lastStatus); }),
    ]);
  } else if (job && job.line && jobKey(job) !== dismissed) {
    const acts = [];
    if (job.ok && job.kind === 'copy') acts.push(button('Show in folder', 'small', () => invoke('open_folder', { path: job.to }).catch((e) => toast(String(e.message || e), 'err'))));
    acts.push(button('Dismiss', 'small quiet', () => { dismissed = jobKey(job); render(lastStatus); }));
    n = notice(job.ok ? 'ok' : 'err', [job.line], acts);
  }
  const key = n ? n.textContent + n.className : '';
  box.hidden = !n;
  if (key !== shown) {
    box.replaceChildren(...(n ? [n] : []));
    shown = key;
    // the confirm of a plan just made takes the focus, so Enter answers it
    if (n && plan && !plan.error) $('button.primary', n)?.focus();
  }
  if (wasRunning && !running) refreshLibrary();
  wasRunning = running;
}

async function pick(kind) {
  let picked;
  try {
    picked = await invoke('pick_folder', { current: kind === 'move' && info ? info.dir : null });
  } catch (e) {
    toast(String(e.message || e), 'err');
    return;
  }
  if (!picked) return;
  try {
    const p = kind === 'copy'
      ? await invoke('library_copy_plan', { into: picked })
      : await invoke('library_plan', { picked });
    plan = { kind, picked, p };
  } catch (e) {
    plan = { kind, picked, error: String(e.message || e) };
  }
  render(lastStatus);
}

export async function refreshLibrary() {
  try {
    info = await invoke('library_get');
  } catch {
    info = null;
  }
  render(lastStatus);
}

export function bindLibrary() {
  $('#library-move')?.addEventListener('click', () => pick('move'));
  $('#library-copy')?.addEventListener('click', () => pick('copy'));
  $('#library-open')?.addEventListener('click', () => {
    const dir = info ? info.dir : lastStatus && lastStatus.library && lastStatus.library.dir;
    if (dir) invoke('open_folder', { path: dir }).catch((e) => toast(String(e.message || e), 'err'));
  });
}

export { render as renderLibrary };
