// The body of a function the app runs at the start of every page in its window, after
// desktop/ui/nav.js without its exports (shell.rs). The nav of the server's Settings pages is the
// server's, and a server older than the app names the app's pages as the app did then.
if (!/^https?:$/.test(location.protocol) || location.hostname === 'tauri.localhost') return;
const navOf = () => document.querySelector('#settings nav.sections');

const pageOf = (page) => {
  const [kind, id] = page.split(' ');
  return { kind, id, app: kind[0] === 'a', standalone: kind.includes('s'), paired: kind.includes('p') };
};

const fill = () => {
  const nav = navOf();
  if (!nav || !nav.querySelector('a[data-app-section]')) return;
  const standalone = document.body.hasAttribute('data-standalone');
  const shows = (p) => !(p.standalone && !standalone) && !(p.paired && standalone);
  const want = NAV.flatMap(([, pages]) => pages.map(pageOf)).filter((p) => p.app && shows(p)).map((p) => p.id);
  const have = [...nav.querySelectorAll('a[data-app-section]')].map((a) => a.dataset.appSection);
  if (want.join() === have.join()) return;
  nav.querySelectorAll('a[data-app-section], .sections-head.app-link').forEach((el) => el.remove());
  // as the server draws the app's pages: the page's own script shows them and opens them
  const entry = (id) => {
    const a = document.createElement('a');
    a.className = 'app-link';
    a.href = '#';
    a.dataset.appSection = id;
    a.hidden = true;
    a.textContent = APP_SECTIONS.find(([k]) => k === id)[1];
    return a;
  };
  let prev = null;
  const put = (el) => { if (prev) prev.after(el); else nav.prepend(el); prev = el; };
  for (const [words, pages] of NAV) {
    const shown = pages.map(pageOf).filter(shows);
    const head = words && [...nav.querySelectorAll('.sections-head')].find((h) => h.textContent.trim() === words);
    if (head) prev = head;
    else if (words && shown.some((p) => p.app)) {
      const h = document.createElement('span');
      h.className = 'sections-head app-link';
      h.hidden = true;
      h.textContent = words;
      put(h);
    }
    for (const p of shown) {
      const there = p.app ? null : nav.querySelector(`a[data-section="${p.id}"]`);
      if (there) prev = there;
      else if (p.app) put(entry(p.id));
    }
  }
};

// Under 960 px the nav is a select: the app's pages join the server's in it, in the nav's
// groups, and open in the app.
const choices = () => {
  const select = document.getElementById('sections-select');
  const call = window.__TAURI__?.core?.invoke;
  if (!select || !call || select.querySelector('option[value^="app:"]')) return;
  const standalone = document.body.hasAttribute('data-standalone');
  const shows = (p) => !(p.standalone && !standalone) && !(p.paired && standalone);
  const was = select.value;
  const theirs = [...select.options];
  const option = (value, label) => { const o = document.createElement('option'); o.value = value; o.textContent = label; return o; };
  const out = [];
  for (const [words, pages] of NAV) {
    const items = pages.map(pageOf).filter(shows).map((p) => (p.app
      ? option(`app:${p.id}`, APP_SECTIONS.find(([k]) => k === p.id)[1])
      : theirs.find((o) => o.value === p.id))).filter(Boolean);
    if (!items.length) continue;
    if (!words) { out.push(...items); continue; }
    const g = document.createElement('optgroup');
    g.label = words;
    g.append(...items);
    out.push(g);
  }
  const placed = new Set(out.flatMap((x) => (x.tagName === 'OPTGROUP' ? [...x.children] : [x])));
  out.push(...theirs.filter((o) => !placed.has(o)));
  select.replaceChildren(...out);
  select.value = was;
  select.addEventListener('change', (e) => {
    if (!select.value.startsWith('app:')) return;
    e.stopImmediatePropagation();
    const id = select.value.slice(4);
    select.value = document.getElementById('settings')?.dataset.section || was;
    call('app_open_settings', { section: id, shape: null }).catch(() => {});
  }, true);
};

// A released server's Settings spread over the whole window: the same frame as the app's pages.
const frame = () => {
  if (!document.getElementById('settings')) return;
  const style = document.createElement('style');
  style.textContent = `.settings { max-width: 974px; margin-left: auto; margin-right: auto; }
@media (min-width: 960px) { .settings { grid-template-columns: 190px minmax(0, 760px); gap: 24px; } }
.page-head:has(~ .settings) { max-width: 974px; margin-left: auto; margin-right: auto; }
.sections, body.has-appbar .sections { max-height: calc(100vh - 128px - var(--appbar)); }
.sections > * { flex-shrink: 0; }`;
  document.head.append(style);
};

const count = async () => {
  const a = navOf()?.querySelector('a[data-app-section="games"]');
  const call = window.__TAURI__?.core?.invoke;
  // only once the page's own script reached the app: this call must not stand in for it
  if (!a || !call || document.documentElement.dataset.app !== 'desktop') return;
  let n;
  try { n = (await call('app_status')).unsorted; } catch { return; }
  if (typeof n !== 'number') return;
  const span = unsortedCount(n);
  const was = a.querySelector('#games-n');
  if (was) was.replaceWith(span); else a.append(' ', span);
};

// In the window, Settings is the app's: it lists this PC's pages and the server's.
const toSettings = (e) => {
  const call = window.__TAURI__?.core?.invoke;
  if (!call || document.documentElement.dataset.app !== 'desktop') return;
  e.preventDefault();
  e.stopImmediatePropagation();
  call('app_open_settings', { section: null, shape: null }).catch(() => {});
};
document.addEventListener('click', (e) => {
  if (e.button === 0 && !e.ctrlKey && !e.metaKey && !e.shiftKey && e.target.closest?.('header.top a[href="/settings"]')) toSettings(e);
}, true);
document.addEventListener('keydown', (e) => {
  if (e.key !== ',' || e.ctrlKey || e.metaKey || e.altKey) return;
  if (/^(INPUT|TEXTAREA|SELECT)$/.test(document.activeElement?.tagName) || document.activeElement?.isContentEditable) return;
  toSettings(e);
}, true);

// A section opened from the nav of a released server's Settings starts at its top; Back puts the
// one before where it was, as the webview keeps it.
const sectionTop = (e) => {
  const a = e.target.closest?.('#settings .sections a[data-section]');
  if (a && e.button === 0 && !e.ctrlKey && !e.metaKey && !e.shiftKey && !e.altKey) setTimeout(() => scrollTo(0, 0));
};
document.addEventListener('click', sectionTop);
document.addEventListener('change', (e) => { if (e.target.id === 'sections-select') setTimeout(() => scrollTo(0, 0)); });

// A released server whose rows carry no question marks: they get the app's, by their keys, as a
// server with them draws them. Its own filter still finds a row by the words in a tip.
const LINKS = { '/trash': 'open-trash', '/api/export/database': 'export-database', '/api/export/config': 'export-config', '/setup': 'setup-again', '/status': 'doctor' };
const GROUPS = ['Now', 'Scan a folder', 'Logs', 'Settings history', 'Users', 'Sign-in attempts', 'The app', 'Paired devices'];
const BROWSER = { 'Hover previews on cards': 'hover-previews', 'Card size': 'card-size', 'Key hints': 'key-hints', Accent: 'accent' };
const tips = () => {
  const body = document.getElementById('sections-body');
  if (!body || document.querySelector('[data-docs]')) return;
  const slug = (words) => words.toLowerCase().split(/[^a-z0-9]+/).filter(Boolean).join('-');
  const sectionOf = (el) => el.closest('[data-section]')?.dataset.section;
  for (const row of body.querySelectorAll('.sgroup[data-key], .srow[data-key]')) {
    const where = `${sectionOf(row)}#${row.dataset.key.replace(/[._]/g, '-')}`;
    row.querySelectorAll('.slabel > .shelp').forEach((w) => { w.dataset.docs = where; });
  }
  for (const row of body.querySelectorAll('.srow.action')) {
    const first = row.querySelector('.sctl a, .sctl button');
    const id = first && (first.id || LINKS[first.getAttribute('href')]);
    const words = row.querySelector('.slabel > .shelp');
    // the disk's and the external storage's lines say what is there now: they stay in sight
    if (!id || !words || id === 'disk-refresh' || id === 'external-check') continue;
    if (id === 'open-trash') {
      const [held, ...how] = words.textContent.split('; ');
      const note = document.createElement('span');
      note.className = 'snote';
      note.textContent = held;
      words.after(note);
      words.textContent = how.join('; ');
    }
    words.dataset.docs = `${sectionOf(row)}#${id}`;
  }
  for (const desc of body.querySelectorAll('.panel > header > .desc')) {
    const name = desc.parentElement.querySelector('h2')?.textContent || '';
    if (GROUPS.includes(name)) desc.dataset.docs = `${sectionOf(desc)}#${slug(name)}`;
  }
  for (const row of body.querySelectorAll('.panel[data-section="appearance"] .srow:not([data-key])')) {
    const at = BROWSER[row.querySelector('b')?.textContent];
    const words = row.querySelector('.slabel > .shelp');
    if (at && words) words.dataset.docs = `appearance#${at}`;
  }
  const standalone = document.body.hasAttribute('data-standalone');
  for (const line of body.querySelectorAll('.section-line[data-section]')) {
    const id = line.dataset.section;
    line.dataset.docs = id;
    line.dataset.name = document.querySelector(`#sections-select option[value="${id}"]`)?.textContent || id;
    if (!standalone) line.dataset.scope = 'every PC and browser';
  }
  const style = document.createElement('style');
  style.textContent = TIP_CSS;
  document.head.append(style);
  TIP.enhance(body);
  TIP.bindTips();
  const root = document.getElementById('settings');
  const filter = document.getElementById('settings-filter');
  filter?.addEventListener('input', () => {
    const q = filter.value.trim().toLowerCase();
    body.querySelectorAll('.page-top').forEach((t) => { t.hidden = !!q || t.dataset.section !== root.dataset.section; });
    if (!q) { TIP.tipHitsOff(body); return; }
    body.querySelectorAll('.srow').forEach((r) => TIP.tipHit(r, r.closest('[hidden]') ? '' : q));
  });
};

// Advanced's Settings history in the window: this PC's changes, which the app keeps, among the
// server's by time, newest first. A server of this version says when its rows are in; a released
// one is watched for them, and its rows' times come from its own list, in the same order.
const settingsHistory = () => {
  const table = document.getElementById('history');
  const empty = document.getElementById('history-empty');
  const body = table?.querySelector('tbody');
  const call = window.__TAURI__?.core?.invoke;
  if (!body || !empty || !call || !window.__REEL_DESKTOP__) return;
  const NONE = empty.textContent;
  let mine = null, theirs = false, times = null;
  // WebKit reads a time to the millisecond alone; the server's and the app's go to the nanosecond
  const time = (at) => new Date(String(at).replace(/(\.\d{3})\d+/, '$1'));
  // as the server words its own rows' times
  const ago = (at) => {
    const t = time(at), secs = Math.max(0, (Date.now() - t) / 1000);
    if (Number.isNaN(secs)) return at;
    if (secs < 60) return 'just now';
    if (secs < 3600) return `${Math.floor(secs / 60)} min ago`;
    if (secs < 86400) return `${Math.floor(secs / 3600)} h ago`;
    const two = (n) => String(n).padStart(2, '0');
    const year = t.getFullYear() === new Date().getFullYear() ? '' : `${t.getFullYear()} `;
    return `${t.toLocaleString('en-US', { month: 'short' })} ${t.getDate()}, ${year}${two(t.getHours())}:${two(t.getMinutes())}`;
  };
  const say = (text, kind) => {
    const host = document.getElementById('toasts');
    if (!host) return;
    const el = document.createElement('div');
    el.className = `toast ${kind}`;
    el.setAttribute('role', 'status');
    const words = document.createElement('span');
    words.className = 'toast-words';
    words.textContent = text;
    const acts = document.createElement('span');
    acts.className = 'toast-acts';
    const x = document.createElement('button');
    x.className = 'quiet small';
    x.textContent = 'Dismiss';
    x.onclick = () => el.remove();
    acts.append(x);
    el.append(words, ' ', acts);
    host.replaceChildren(el);
    setTimeout(() => el.remove(), 4000);
  };
  const load = async () => { try { mine = await call('app_settings_history'); } catch { mine = { rows: [] }; } };
  const rowOf = (h) => {
    const tr = document.createElement('tr');
    tr.dataset.app = h.id;
    tr.dataset.at = h.at;
    tr.innerHTML = '<td class="clip"></td><td class="sub" data-l="was"></td><td class="sub" data-l="now"></td><td class="sub"><span class="badge">this PC</span> <span></span></td><td class="sub"></td><td class="right acts"></td>';
    const c = tr.children;
    c[0].textContent = h.name;
    c[1].textContent = h.was;
    c[2].textContent = h.now;
    c[3].firstChild.title = mine.device || '';
    c[3].lastChild.textContent = h.from;
    c[4].textContent = ago(h.at);
    c[4].title = time(h.at).toLocaleString();
    if (h.revert) {
      const b = document.createElement('button');
      b.className = 'quiet small';
      b.type = 'button';
      b.textContent = 'Revert';
      b.onclick = async () => {
        if (b.dataset.armed !== '1') {
          b.dataset.armed = '1';
          b.textContent = 'Sure?';
          b.classList.add('danger');
          setTimeout(() => { b.dataset.armed = ''; b.textContent = 'Revert'; b.classList.remove('danger'); }, 3000);
          return;
        }
        b.disabled = true;
        try {
          const d = await call('app_settings_revert', { id: h.id });
          say(d.back ? `${d.name} is back to ${d.now}.` : `${d.name} did not go back to ${d.now}; Settings says why.`, d.back ? 'ok' : 'err');
          await load();
          merge();
        } catch (err) { say(String(err?.message || err), 'err'); b.disabled = false; }
      };
      c[5].append(b);
    } else {
      // where it changes instead, kept narrow: the cell is the table's last
      const note = document.createElement('span');
      note.className = 'faint small';
      note.style.cssText = 'display: inline-block; width: 9.5em; white-space: normal; text-align: right; line-height: 1.35';
      note.textContent = h.note || '';
      c[5].append(note);
    }
    return tr;
  };
  const serverTimes = async () => {
    if (times) return times;
    try {
      const r = await fetch('/api/settings/history', { headers: { accept: 'application/json' }, credentials: 'same-origin' });
      times = r.ok ? ((await r.json()).history || []) : [];
    } catch { times = []; }
    return times;
  };
  const merge = async () => {
    if (!mine || !theirs) return;
    if (!mine.rows.length && !body.querySelector('tr[data-app]')) return;
    const server = [...body.children].filter((tr) => !tr.dataset.app);
    if (server.some((tr) => !tr.dataset.at)) {
      const list = await serverTimes();
      let last = new Date().toISOString();
      server.forEach((tr, i) => {
        const h = list[i];
        if (h && (h.label || h.key) === tr.children[0]?.textContent) last = h.at;
        tr.dataset.at ||= last;
      });
    }
    const all = [...server, ...mine.rows.map(rowOf)];
    all.sort((a, b) => time(b.dataset.at) - time(a.dataset.at));
    watch.disconnect();
    body.replaceChildren(...all);
    if (empty.textContent === NONE) empty.hidden = all.length > 0;
  };
  // the server's rows are in once it says so, a row is there, or its empty line shows
  const watch = new MutationObserver(() => {
    if (!theirs && (body.children.length || !empty.hidden)) { theirs = true; merge(); }
  });
  watch.observe(body, { childList: true });
  watch.observe(empty, { attributes: true, attributeFilter: ['hidden'] });
  document.addEventListener('reel:history', () => { theirs = true; merge(); });
  theirs = body.children.length > 0 || !empty.hidden;
  load().then(merge);
};

// before the page's scripts run, which show and bind the app's pages in the nav they find
document.addEventListener('readystatechange', () => { if (document.readyState === 'interactive') { frame(); fill(); choices(); } });
addEventListener('DOMContentLoaded', count);
// after them: a server that draws its own question marks has marked its words by then
addEventListener('DOMContentLoaded', tips);
addEventListener('DOMContentLoaded', settingsHistory);
addEventListener('pageshow', (e) => { if (e.persisted) count(); });
document.addEventListener('visibilitychange', () => { if (!document.hidden) count(); });
