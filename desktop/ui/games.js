// Settings, Games and apps: the list, a program's own page, Steam, adding one and renaming one.
import { $, invoke, toast } from './shell.js';

const CLASS = { game: 'Game', not_a_game: 'Not a game', not_sorted: 'Not sorted yet', ignored: 'Ignored' };
const CLASSES = ['game', 'not_a_game', 'not_sorted', 'ignored'];
const NUM = ['no', 'one', 'two', 'three', 'four', 'five', 'six', 'seven', 'eight', 'nine'];

let page = null;
let shown = null;
let open = false;
let last = '';
let asked = 0;

// A small builder: no markup from a name or a path ever reaches innerHTML.
function h(tag, attrs, ...kids) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v == null || v === false) continue;
    if (k === 'class') el.className = v;
    else if (k.startsWith('on')) el.addEventListener(k.slice(2), v);
    else el.setAttribute(k, v === true ? '' : v);
  }
  for (const k of kids.flat()) if (k != null && k !== false) el.append(k);
  return el;
}

const count = (n, one, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;
const sep = () => h('span', { class: 'sep' }, '·');
const fail = (e) => toast(String(e.message || e), 'err');
const go = (section) => window.openSection?.(section);

// The waits' words follow the mode, as their row under New clips (Upload when connected) does.
function waits() {
  if (!page.standalone) return { page: 'Upload', page_id: 'upload', row: 'Uploads wait', on: 'uploads wait', off: 'uploads go on', help: 'while it has the screen', never: 'uploads never wait for it', sub: 'Uploads wait' };
  if (page.paired) return { page: 'New clips', page_id: 'upload', row: 'Uploads and encoding wait', on: 'uploads and encoding wait', off: 'uploads and encoding go on', help: 'uploads, and the library\'s heavy work, while it has the screen', never: 'uploads and encoding never wait for it', sub: 'Uploads and encoding wait' };
  return { page: 'New clips', page_id: 'upload', row: 'Encoding waits', on: 'encoding waits', off: 'encoding goes on', help: 'the library\'s heavy work, while it has the screen', never: 'encoding never waits for it', sub: 'Encoding waits' };
}

function sameDay(a, b) { return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate(); }
function dayWord(ts) {
  const d = new Date(ts), now = new Date(), y = new Date(now);
  y.setDate(now.getDate() - 1);
  if (sameDay(d, now)) return 'today';
  if (sameDay(d, y)) return 'yesterday';
  return d.toLocaleDateString('en', d.getFullYear() === now.getFullYear() ? { month: 'short', day: 'numeric' } : { month: 'short', day: 'numeric', year: 'numeric' });
}
function whenWord(ts) {
  const w = dayWord(ts);
  return w === 'today' ? `today, ${new Date(ts).toLocaleTimeString('en', { hour: '2-digit', minute: '2-digit', hourCycle: 'h23' })}` : w;
}

function clipsWords(e) {
  const parts = [];
  if (e.clips) parts.push(count(e.clips, 'clip'));
  if (e.recordings) parts.push(count(e.recordings, 'session recording'));
  return parts.join(', ');
}

// The line under a program's name: its class, its own settings, where it came from, its clips, when seen.
function line(e) {
  const w = waits();
  const bits = [e.class === 'not_sorted' ? h('span', { class: 'st-warn' }, 'not sorted yet') : CLASS[e.class]];
  if (!page.linux && e.own.wait !== 'as_class') bits.push(h('span', { class: 'own' }, e.own.wait === 'on' ? w.on : w.off));
  if (!page.linux && e.naming === 'window') bits.push(h('span', { class: 'own' }, 'named by its window'));
  if (e.fresh) bits.push(`new from Steam ${dayWord(e.came_at)}`);
  if (e.application) bits.push('from Steam, an application');
  if (e.came === 'steam_shortcut') bits.push('added to Steam by hand');
  if (e.uninstalled) bits.push('not installed in Steam');
  const clips = clipsWords(e);
  if (clips) bits.push(clips);
  bits.push(e.seen_at ? `seen ${whenWord(e.seen_at)}` : e.steam_app ? 'not played yet' : 'not seen yet');
  const out = [];
  bits.forEach((b, i) => { if (i) out.push(sep()); out.push(b); });
  return out;
}

const CHEV = () => {
  const s = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  s.setAttribute('class', 'icon chev');
  s.setAttribute('viewBox', '0 0 16 16');
  s.setAttribute('aria-hidden', 'true');
  const p = document.createElementNS('http://www.w3.org/2000/svg', 'path');
  p.setAttribute('d', 'M6 3.5 10.5 8 6 12.5');
  s.append(p);
  return s;
};

function badge(c) {
  return h('span', { class: c === 'not_sorted' ? 'badge warn' : 'badge' }, CLASS[c]);
}

function link(words, section) {
  return h('a', { href: `#${section}`, 'data-go': section }, words);
}

const GUIDE = 'https://locko2901.github.io/reel/games-and-apps/';
function guide() {
  return h('a', { href: GUIDE, onclick: (e) => {
    e.preventDefault();
    window.__TAURI__?.opener?.openUrl(GUIDE).catch(fail);
  } }, 'How the list works');
}

function renderTop() {
  const w = waits(), line = $('#games-line');
  line.replaceChildren(
    'Which programs are games, the name each gives its clips, and what one program does apart from its class.',
    ...(page.linux
      ? [' On Linux reel cannot tell which program is in front: the list fills from Steam and by hand, and clips there are named as before.']
      : [' What every game gets is set where it belongs: the waits under ', link(w.page, w.page_id), '.']),
    ' ', guide(), '.',
  );
  $('#games-order').hidden = page.linux;
}

function renderSteam() {
  const s = page.steam, desc = $('#steam-desc'), note = $('#steam-note');
  const libs = s.libraries === 1 ? 'one Steam library' : `${NUM[s.libraries] || s.libraries} Steam libraries`;
  desc.textContent = s.found ? `${libs} on this PC, read here: nothing leaves the PC` : s.read ? 'not found on this PC' : 'looking for Steam on this PC';
  let words;
  if (!s.found) words = s.error ? `Steam's files did not read: ${s.error}` : 'Steam is not installed on this PC, or reel did not find it: nothing to add. Once Steam is installed, its games come in by themselves';
  else if (!s.on) words = 'off: Steam adds nothing, and the games it added stay in the list';
  else words = `${s.in_list} in the list from Steam${s.last ? `; the last, ${s.last.name}, came in ${dayWord(s.last.at)}` : ''}`;
  note.textContent = words;
  const row = $('#not-again-row'), box = $('#not-again');
  const gone = page.not_again || [];
  row.toggleAttribute('data-off', !gone.length);
  row.hidden = !gone.length;
  box.replaceChildren(...gone.map((n) => h('span', { class: 'not-again' },
    h('span', { class: 'small' }, n.name),
    h('button', { class: 'small', type: 'button', onclick: () => addBack(n) }, 'Add back'))));
}

async function addBack(n) {
  try {
    await invoke('program_add_back', { app: n.app });
    toast(`${n.name} comes back from Steam at its next read, in a few seconds.`, 'ok');
    refresh(true);
  } catch (e) { fail(e); }
}

function sorted(list) {
  const rank = (e) => (e.class === 'not_sorted' ? 0 : e.class === 'ignored' ? 2 : 1);
  return [...list].sort((a, b) => rank(a) - rank(b) || a.name.localeCompare(b.name, 'en', { sensitivity: 'base' }));
}

function renderList() {
  const list = page.entries, rows = $('#games-rows');
  const unsorted = list.filter((e) => e.class === 'not_sorted').length;
  $('#games-count').textContent = list.length ? `${count(list.length, 'program')}${unsorted ? `, ${unsorted} not sorted yet` : ''}` : 'empty';
  $('#games-find').hidden = list.length < 2;
  if (!list.length) { rows.replaceChildren(empty()); return; }
  const q = $('#games-find').value.trim().toLowerCase();
  const hit = (e) => !q || e.name.toLowerCase().includes(q) || String(e.path).toLowerCase().includes(q);
  const out = [];
  let sub = '';
  for (const e of sorted(list).filter(hit)) {
    const s = e.class === 'not_sorted' ? 'Not sorted yet: open one to sort it' : 'Sorted, by name';
    if (s !== sub) { out.push(h('span', { class: 'games-sub' }, s)); sub = s; }
    out.push(h('a', { class: `game-link${e.class === 'not_sorted' ? ' unsorted' : ''}`, href: `#games/${e.id}`, 'data-id': e.id },
      h('span', null, h('b', null, e.name), e.steam_app && e.came === 'steam' ? h('span', { class: 'from' }, 'Steam') : null, h('span', { class: 'line' }, line(e))),
      h('span', { class: 'cls' }, badge(e.class)),
      CHEV()));
  }
  if (!out.length) out.push(h('p', { class: 'muted games-none' }, 'No program in the list matches.'));
  rows.replaceChildren(...out);
}

function empty() {
  const w = waits();
  const steps = page.linux
    ? [h('li', null, h('span', null, '1'), 'Steam installs it, when Steam is on this PC,'), h('li', null, h('span', null, '2'), 'or you add it: one that runs now, or its file.')]
    : [h('li', null, h('span', null, '1'), 'a clip or a session recording is saved while it is in front,'),
      h('li', null, h('span', null, '2'), 'it takes the whole screen, as a game in full screen does,'),
      h('li', null, h('span', null, '3'), 'or you add it: one that runs now, or its file.')];
  return h('div', { class: 'games-empty' },
    h('strong', null, 'No program in the list yet.'),
    h('span', null, 'reel lists a program the first time'),
    h('ol', null, steps),
    h('button', { type: 'button', class: 'primary', onclick: () => addDialog() }, 'Add a program'),
    h('span', { class: 'faint small' }, page.linux
      ? 'With Steam installed, its games come in by themselves (Steam, above). Browsers, Explorer, Discord, OBS and reel itself never come in: reel knows they are no game.'
      : `With Steam installed, its games come in by themselves (Steam, above). Until a program is sorted reel acts as before: in full screen it counts as a game for the waits, when they are on under ${w.page}. Browsers, Explorer, Discord, OBS and reel itself never come in: reel knows they are no game.`));
}

// ---------- a program's page ----------

function cardWords(c) {
  const w = waits();
  if (page.linux) {
    return {
      game: 'counts as a game',
      not_a_game: 'a full screen tool, a player, a VM',
      not_sorted: 'new in the list, until sorted',
      ignored: 'as if it were not there, and it is not listed again',
    }[c];
  }
  return {
    game: `counts as a game: clips saved in front of it take its name. ${w.sub} while it has the screen only when that is on under ${w.page}`,
    not_a_game: `a full screen tool, a player, a VM: ${w.never}; clips saved in front of it take its name`,
    not_sorted: `new in the list, and acting as before until sorted: in full screen it counts as a game for the waits, when they are on under ${w.page}`,
    ignored: 'as if it were not there: no clip takes its name, nothing waits for it, and it is not listed again',
  }[c];
}

function short(name) {
  const cut = name.split(/[:(\u2013-]/)[0].trim();
  return cut.length >= 3 && cut.length < name.length ? cut : name;
}

// Where the waits' value comes from: every game, then its class, then its own setting.
function trace(e) {
  const w = waits(), every = page.waits_on;
  const own = e.own.wait;
  const byClass = e.class === 'game' ? every : e.class === 'not_sorted' ? (every ? 'in full screen' : false) : false;
  const showClass = e.class !== 'game' && (e.class === 'ignored' || String(byClass) !== String(every));
  const steps = [h('span', { class: own !== 'as_class' || showClass ? 'lost' : 'won' }, 'Every game, set under ', link(w.page, w.page_id), `: ${every ? 'on' : 'off'}`)];
  if (showClass) steps.push(h('span', { class: own !== 'as_class' && e.class !== 'ignored' ? 'lost' : 'won' }, `${CLASS[e.class]}: ${byClass === true ? 'on' : byClass || 'off'}`));
  if (own !== 'as_class' && e.class !== 'ignored') steps.push(h('span', { class: 'won' }, `${short(e.name)}: ${own}`));
  return h('span', { class: 'trace' }, steps);
}

function row(label, help, ctl, extra) {
  return h('div', { class: 'srow' },
    h('div', { class: 'slabel' }, h('b', null, label), help ? h('span', { class: 'shelp' }, help) : null, extra || null),
    h('div', { class: 'sctl' }, ctl));
}

function panel(title, desc, actions, ...body) {
  return h('div', { class: 'panel' },
    h('header', null, h('h2', null, title), desc ? h('span', { class: 'desc' }, desc) : null, actions ? h('div', { class: 'actions' }, actions) : null),
    ...body);
}

function nameHelp(e) {
  const base = 'clips and session recordings saved in front of it take this name';
  if (e.name_from === 'steam') return `${base}; its name in Steam`;
  if (e.name_from === 'guess') return `${base}; guessed from its file: ${e.guess}`;
  return e.guess && e.guess !== e.name ? `${base}; its file would give it ${e.guess}` : base;
}

function cameWords(e) {
  const day = dayWord(e.came_at);
  if (e.came === 'steam') return `came in ${day}, from Steam`;
  return `came in ${day}: ${e.came_words}`;
}

function removeWords(e) {
  if (e.came === 'steam') return page.linux ? 'Steam does not add it again. The class Ignored keeps it out for good' : 'Steam does not add it again; it comes back as Not sorted yet only when you play it. The class Ignored keeps it out for good';
  if (page.linux) return 'you can add it again with Add a program; the class Ignored keeps it in the list as no game';
  return 'it comes back as Not sorted yet when it is seen again; the class Ignored keeps it out';
}

function renderProgram() {
  if (!page) return;
  const box = $('#games-program');
  const e = page.entries.find((x) => x.id === shown);
  if (!e) { show(null); return; }
  if (box.contains(document.activeElement) && document.activeElement.matches('input')) return;
  const w = waits();
  const owns = [];
  if (!page.linux && e.own.wait !== 'as_class') owns.push(e.own.wait === 'on' ? w.on : w.off);
  if (!page.linux && e.naming === 'window') owns.push('named by its window');
  const name = h('input', { type: 'text', class: 'in-text', 'aria-label': 'Name', value: e.name, maxlength: '80' });
  name.addEventListener('keydown', (ev) => { if (ev.key === 'Enter') name.blur(); if (ev.key === 'Escape') { name.value = e.name; name.blur(); } });
  name.addEventListener('change', () => rename(e, name));
  const path = h('span', { class: 'mono small path' }, String(e.path));
  const clips = clipsWords(e);
  const naming = h('select', { 'aria-label': 'Clips are named' },
    h('option', { value: 'name' }, `after ${short(e.name)}`), h('option', { value: 'window' }, 'after its window\'s title'));
  naming.value = e.naming;
  naming.addEventListener('change', () => set(e, { naming: naming.value }));
  const programRows = [
    row('Name', nameHelp(e), name),
    e.folder
      ? row('Folder', `in a Steam library, app ${e.steam_app}: every program in it counts as this game, but one listed on its own${e.uninstalled ? '; not installed in Steam now, it stays with its name' : ''}`, path)
      : row('File', e.came === 'steam_shortcut' ? 'added to Steam by hand, outside its store' : '', path),
    row('In the library', 'clips and session recordings of this PC named after it', [h('span', { class: 'muted small' }, clips || 'none yet'),
      clips ? h('button', { class: 'small', type: 'button', onclick: () => invoke('open_page', { path: `/?q=${encodeURIComponent(e.name)}` }).catch(fail) }, 'Show them') : null]),
  ];
  if (!page.linux) programRows.push(row('Clips are named', 'after this name, or after its window\'s title, for an emulator or a launcher that shows the game it runs', naming));
  const cards = h('div', { class: 'classes', role: 'radiogroup', 'aria-label': 'Class' }, CLASSES.map((c) =>
    h('button', { type: 'button', role: 'radio', class: `opt${c === e.class ? ' on' : ''}`, 'aria-checked': String(c === e.class), onclick: () => { if (c !== e.class) set(e, { class: c }); } },
      h('b', null, h('i'), CLASS[c]), h('span', null, cardWords(c)))));
  const parts = [
    h('p', { class: 'crumb' }, h('a', { href: '#games', 'data-go': 'games' }, 'Games and apps'), h('span', { class: 'sep' }, '/'), h('span', null, e.name)),
    h('div', { class: 'program-title' }, h('h2', null, e.name), h('span', { class: 'muted small' }, [CLASS[e.class], ...owns].join(', '))),
    panel('The program', cameWords(e), h('button', { class: 'small', type: 'button', onclick: () => invoke('open_folder', { path: String(e.path) }).catch(fail) }, 'Show in folder'),
      h('div', { class: 'body flush' }, programRows)),
    panel('Class', page.linux ? 'what it is' : 'what it is; its own settings below change what the class gives it', null, cards),
  ];
  if (!page.linux) {
    const tri = h('div', { class: 'seg tri', role: 'group', 'aria-label': w.row }, [['as_class', 'As its class'], ['on', 'On'], ['off', 'Off']].map(([v, words]) =>
      h('button', { type: 'button', 'aria-pressed': String(e.own.wait === v), disabled: e.class === 'ignored', onclick: () => { if (v !== e.own.wait) set(e, { wait: v }); } }, words)));
    const label = h('b', null, w.row, e.own.wait !== 'as_class' ? h('span', { class: 'badge own' }, 'own setting') : null);
    parts.push(panel('This program', 'each as its class, or on or off for this program alone', null, h('div', { class: 'body flush' },
      h('div', { class: 'srow' }, h('div', { class: 'slabel' }, label, h('span', { class: 'shelp' }, w.help), trace(e)), h('div', { class: 'sctl' }, tri)))));
  }
  parts.push(h('div', { class: 'row remove-row' },
    h('button', { class: 'small quiet', type: 'button', onclick: () => remove(e) }, 'Remove from the list'),
    h('span', { class: 'muted small' }, removeWords(e))));
  box.replaceChildren(...parts);
}

async function set(e, change) {
  try {
    const v = await invoke('program_set', { id: e.id, change });
    Object.assign(e, v);
    renderProgram();
    renderList();
    refresh(true);
  } catch (err) { fail(err); }
}

async function remove(e) {
  try {
    await invoke('program_remove', { id: e.id });
    toast(e.steam_app && e.came === 'steam' ? `${e.name} is out of the list; Steam does not add it again.` : `${e.name} is out of the list.`, 'ok');
    show(null, true);
    refresh(true);
  } catch (err) { fail(err); }
}

// ---------- the rename's question ----------

function dialog(id) {
  let d = $(`#${id}`);
  if (!d) { d = h('dialog', { class: 'games-dialog', id }); document.body.append(d); }
  return d;
}

async function rename(e, input) {
  const name = input.value.trim();
  if (name === e.name) return;
  let plan;
  try { plan = await invoke('program_rename_plan', { id: e.id, name }); } catch (err) { input.value = e.name; fail(err); return; }
  const n = plan.clips + plan.recordings;
  if (!n) { await doRename(e, plan.name, false); return; }
  const d = dialog('rename-dialog');
  const what = [plan.clips ? count(plan.clips, 'clip') : '', plan.recordings ? count(plan.recordings, 'session recording') : ''].filter(Boolean).join(' and ');
  const list = h('div', { class: 'ask-list' });
  for (const [was, now] of plan.examples) list.append(h('span', { class: 'was' }, was), h('span', { class: 'now' }, now));
  if (n > plan.examples.length) list.append(h('span', { class: 'faint more' }, plan.examples.length ? `and ${n - plan.examples.length} more` : `${what}, their titles changed by hand`));
  const server = plan.on_server
    ? h('p', { class: 'muted small' }, plan.on_server === 1
      ? 'On the server, the one this PC sent keeps its title: the app cannot change a title there. Change it on its page on the server.'
      : `On the server, the ${plan.on_server} this PC sent keep their titles: the app cannot change a title there. Change one on its page on the server.`)
    : null;
  const only = h('button', { type: 'button' }, 'New clips only');
  const all = h('button', { type: 'button', class: 'primary' }, n === 1 ? 'Rename it too' : `Rename these ${n} too`);
  d.replaceChildren(
    h('header', null, `${plan.name}: older clips too?`),
    h('div', { class: 'body' },
      h('p', null, `${what} saved in front of it carry the old name, ${plan.old}${plan.examples.length ? ':' : '.'}`),
      plan.examples.length ? list : null,
      h('p', { class: 'muted small' }, `A title you changed by hand stays as it is. New clips take ${plan.name} either way.`),
      server),
    h('footer', null, only, all));
  const answer = await new Promise((resolve) => {
    only.onclick = () => resolve(false);
    all.onclick = () => resolve(true);
    d.onclose = () => resolve(null);
    // the dialog itself takes the focus: the Enter that ended the name must not answer it
    d.tabIndex = -1;
    d.showModal();
    d.focus();
  });
  d.onclose = null;
  if (d.open) d.close();
  if (answer === null) { input.value = e.name; return; }
  await doRename(e, plan.name, answer, n);
}

async function doRename(e, name, older, n = 0) {
  try {
    const v = await invoke('program_rename', { id: e.id, name, older });
    Object.assign(e, v);
    toast(older ? `Renamed, and ${count(n, 'older one')} with it.` : `Renamed: clips take ${name} from now on.`, 'ok');
    document.activeElement?.blur();
    renderProgram();
    refresh(true);
  } catch (err) { fail(err); renderProgram(); }
}

// ---------- Add a program ----------

async function addDialog() {
  const d = dialog('add-dialog');
  const linux = page.linux;
  let kind = 'running', running = null, picked = null, chosen = null;
  const name = h('input', { type: 'text', class: 'in-text', 'aria-label': 'Name', maxlength: '80' });
  const hint = h('span', { class: 'hint' });
  const cls = h('select', { 'aria-label': 'Class' }, CLASSES.map((c) => h('option', { value: c }, CLASS[c])));
  cls.value = 'game';
  const addBtn = h('button', { type: 'button', class: 'primary', disabled: true }, 'Add');
  const seg = h('div', { class: 'seg', role: 'group', 'aria-label': 'Add' },
    h('button', { type: 'button', 'aria-pressed': 'true', 'data-kind': 'running' }, 'Running now'),
    h('button', { type: 'button', 'aria-pressed': 'false', 'data-kind': 'file' }, 'A file'));
  const body = h('div', { class: 'add-body' });
  const fields = [
    h('label', { class: 'dfield' }, h('span', null, 'Name'), name, hint),
    h('label', { class: 'dfield' }, h('span', null, 'Class'), cls),
  ];
  const choose = (c) => {
    chosen = c;
    name.value = c ? c.name : '';
    hint.textContent = !c ? '' : c.steam ? 'its name in Steam: it sits in a Steam library' : `guessed from the file: ${c.guess}; clips take the name you give`;
    addBtn.disabled = !c;
  };
  const drawRunning = () => {
    const words = linux ? 'The programs running now outside the system\'s own folders. One in the list already opens there.' : 'The programs with a window, now. One in the list already opens there.';
    if (!running) { body.replaceChildren(h('p', { class: 'muted small' }, 'Looking at what runs...')); return; }
    if (!running.length) { body.replaceChildren(h('p', { class: 'muted small' }, linux ? 'No program runs now outside the system\'s own folders. Start the game, or pick its file.' : 'No program with a window runs now. Start the game, or pick its file.')); return; }
    body.replaceChildren(h('p', { class: 'muted small' }, words), h('div', { class: 'picklist', role: 'radiogroup', 'aria-label': 'Running now' }, running.map((p) =>
      h('button', { type: 'button', role: 'radio', class: chosen === p ? 'on' : '', 'aria-checked': String(chosen === p), onclick: () => {
        if (p.listed) { d.close(); show(p.listed, true); return; }
        choose(p); drawRunning();
      } },
      h('i'), h('span', { class: 'pick-name' }, h('b', null, p.name), h('span', { class: 'file' }, String(p.path))),
      h('span', { class: 't' }, p.listed ? h('span', { class: 'in' }, 'in the list') : p.title ? `window: ${p.title}` : '')))));
  };
  const drawFile = () => {
    const pick = h('button', { class: 'small', type: 'button', onclick: async () => {
      try {
        const got = await invoke('program_pick');
        if (!got) return;
        if (got.listed) { d.close(); show(got.listed, true); toast('That program is in the list already.', 'ok'); return; }
        picked = got; choose(got); drawFile();
      } catch (e) { fail(e); }
    } }, picked ? 'Pick another file' : 'Pick a file');
    body.replaceChildren(
      picked ? h('div', { class: 'picked' }, h('span', { class: 'muted small' }, 'The file you picked'), h('span', { class: 'mono' }, String(picked.path))) : null,
      h('div', { class: 'row' }, pick, h('span', { class: 'faint small' }, linux ? 'a program' : 'a program (.exe) or a shortcut to one')));
  };
  seg.addEventListener('click', (ev) => {
    const b = ev.target.closest('button[data-kind]');
    if (!b || b.dataset.kind === kind) return;
    kind = b.dataset.kind;
    for (const x of seg.children) x.setAttribute('aria-pressed', String(x === b));
    choose(kind === 'file' ? picked : null);
    if (kind === 'file') drawFile(); else drawRunning();
  });
  addBtn.onclick = async () => {
    if (!chosen) return;
    addBtn.disabled = true;
    try {
      const v = await invoke('program_add', { path: String(chosen.path), name: name.value, class: cls.value });
      d.close();
      toast(`${v.name} is in the list as ${CLASS[v.class] === 'Game' ? 'a Game' : CLASS[v.class]}.`, 'ok');
      refresh(true);
    } catch (e) { fail(e); addBtn.disabled = false; }
  };
  name.addEventListener('input', () => { addBtn.disabled = !chosen || !name.value.trim(); });
  d.replaceChildren(h('header', null, 'Add a program'), h('div', { class: 'body' }, seg, body, ...fields),
    h('footer', null, h('button', { type: 'button', onclick: () => d.close() }, 'Cancel'), addBtn));
  drawRunning();
  d.showModal();
  try { running = await invoke('programs_running'); } catch (e) { running = []; fail(e); }
  if (kind === 'running') drawRunning();
}

// ---------- the section ----------

function draw() {
  if (!page) return;
  renderTop();
  renderSteam();
  renderList();
  if (shown) renderProgram();
}

// Shows the list, or a program's page; with `push` the window's Back comes back.
export function show(id, push = false) {
  shown = id || null;
  const list = $('#games-list'), prog = $('#games-program');
  if (!list) return;
  list.hidden = !!shown;
  prog.hidden = !shown;
  if (push) history.pushState(null, '', shown ? `#games/${shown}` : '#games');
  if (shown) { renderProgram(); window.scrollTo(0, 0); }
}

export async function refresh(force = false) {
  if (!open && !force) return;
  const mine = ++asked;
  let got;
  try { got = await invoke('programs_get'); } catch (e) { return; }
  if (mine !== asked) return;
  const key = JSON.stringify(got);
  if (key === last && !force) return;
  last = key;
  page = got;
  draw();
}

// The section opened or closed; `sub` is a program's id from the address.
export function opened(on, sub) {
  open = on;
  if (!on) return;
  show(sub || null);
  refresh(true);
}

export function bindGames() {
  $('#games-add')?.addEventListener('click', () => addDialog());
  $('#games-find')?.addEventListener('input', () => renderList());
  document.addEventListener('click', (e) => {
    const a = e.target.closest('#g-games a[data-go], #g-games a.game-link');
    if (!a) return;
    e.preventDefault();
    if (a.dataset.id) show(a.dataset.id, true);
    else if (a.dataset.go === 'games') show(null, true);
    else go(a.dataset.go);
  });
}
