// The body of a function the app runs at the start of every page in its window, after
// desktop/ui/nav.js without its exports (shell.rs). The nav of the server's Settings pages is the
// server's, and a server older than the app lacks the app's newer pages.
if (!/^https?:$/.test(location.protocol) || location.hostname === 'tauri.localhost') return;
const navOf = () => document.querySelector('#settings nav.sections');

const fill = () => {
  const nav = navOf();
  if (!nav || !nav.querySelector('a[data-app-section]')) return;
  const standalone = document.body.hasAttribute('data-standalone');
  const heads = [...nav.querySelectorAll('.sections-head')];
  let prev = null;
  for (const [words, pages] of NAV) {
    let head = heads.find((h) => h.textContent.trim() === words);
    if (head) prev = head;
    for (const page of pages) {
      const [kind, id] = page.split(' ');
      if ((kind.includes('s') && !standalone) || (kind.includes('p') && standalone)) continue;
      const there = nav.querySelector(kind[0] === 'a' ? `a[data-app-section="${id}"]` : `a[data-section="${id}"]`);
      if (there) { prev = there; continue; }
      if (kind[0] !== 'a') continue;
      if (!head) {
        head = document.createElement('span');
        head.className = 'sections-head app-link';
        head.hidden = true;
        head.textContent = words;
        if (prev) prev.after(head); else nav.prepend(head);
        prev = head;
      }
      // as the server draws the app's pages: the page's own script shows them and opens them
      const a = document.createElement('a');
      a.className = 'app-link';
      a.href = '#';
      a.dataset.appSection = id;
      a.hidden = true;
      a.textContent = (standalone && STANDALONE_LABELS[id]) || APP_SECTIONS.find(([k]) => k === id)[1];
      prev.after(a);
      prev = a;
    }
  }
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

// before the page's scripts run, which show and bind the app's pages in the nav they find
document.addEventListener('readystatechange', () => { if (document.readyState === 'interactive') fill(); });
addEventListener('DOMContentLoaded', count);
addEventListener('pageshow', (e) => { if (e.persisted) count(); });
document.addEventListener('visibilitychange', () => { if (!document.hidden) count(); });
