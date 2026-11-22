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

// before the page's scripts run, which show and bind the app's pages in the nav they find
document.addEventListener('readystatechange', () => { if (document.readyState === 'interactive') { frame(); fill(); choices(); } });
addEventListener('DOMContentLoaded', count);
addEventListener('pageshow', (e) => { if (e.persisted) count(); });
document.addEventListener('visibilitychange', () => { if (!document.hidden) count(); });
