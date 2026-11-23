// Settings docked on the left of the app's window (panel.rs): the body of a function the app runs
// at the start of every page of both its webviews, the panel's (PANEL true) and the library's.
// Whatever the server's version, its pages get the panel's frame from here.
const CSS = `
html.in-panel header.top, html.in-panel footer.appbar, html.in-panel nav.tabbar { display: none !important; }
html.in-panel body, html.in-panel body.has-appbar { padding-bottom: 0; }
html.in-panel body::after { content: ''; position: fixed; top: 0; bottom: 0; right: 0; width: 1px; background: var(--line-2); z-index: 9; pointer-events: none; }
html.in-panel .page { padding: 14px 20px 40px; }
html.in-panel .page > .notice, html.in-panel .settings, html.in-panel .page-head:has(~ .settings) { max-width: none; margin-left: 0; margin-right: 0; }
html.in-panel .settings { grid-template-columns: 190px minmax(0, 1fr); }
html.in-panel .sections, html.in-panel body.has-appbar .sections { top: 14px; max-height: calc(100vh - 96px); }
html.in-panel .group, html.in-panel #log-panel { scroll-margin-top: 14px; }
html.in-panel .obs-bar { top: 0; }
html.in-panel .page-head .tools { gap: 6px; }
@media (max-width: 959px) { html.in-panel .settings { grid-template-columns: 1fr; } }
.panel-close { display: inline-flex; align-items: center; justify-content: center; flex: none; width: 30px; height: 30px; padding: 0; border: 0; background: none; border-radius: var(--radius); color: var(--text-2); cursor: pointer; }
.panel-close:hover { color: var(--text); background: var(--raised); }
.panel-close svg { width: 16px; height: 16px; stroke: currentColor; stroke-width: 1.6; fill: none; }
html.beside-panel main.page { margin-left: var(--panel-width); }
html.beside-panel .top nav a.tab-settings { color: var(--text); box-shadow: inset 0 -2px 0 var(--accent); border-radius: 0; }`;

const call = (cmd, args) => window.__TAURI__?.core?.invoke(cmd, args).catch(() => {});

// The page beside the panel moves right by the panel's width; a page loaded while it is open
// reads the width at its start, before it draws.
const beside = (w) => {
  const d = document.documentElement;
  if (!d || PANEL) return;
  d.style.setProperty('--panel-width', `${w}px`);
  d.classList.toggle('beside-panel', w > 0);
};

const start = () => {
  const d = document.documentElement;
  if (!d) return false;
  if (PANEL) d.classList.add('in-panel');
  const style = document.createElement('style');
  style.dataset.panel = '';
  style.textContent = CSS;
  (document.head || d).append(style);
  let w = 0;
  try { w = Number(sessionStorage.getItem('reel-panel')) || 0; } catch {}
  beside(w);
  return true;
};
if (!start()) document.addEventListener('readystatechange', start, { once: true });

window.__reelPanel = (w) => {
  try { sessionStorage.setItem('reel-panel', String(w)); } catch {}
  beside(w);
};

// Esc closes the panel, from it or from the page beside, once nothing on the page takes it: a
// tip, a menu, a dialog, a selection, a field with words in it.
const busy = () => {
  if (document.querySelector('.tip.open, .popover:not([hidden]), .menu:not([hidden]), dialog[open], body.selecting')) return true;
  if (document.fullscreenElement) return true;
  const a = document.activeElement;
  return !!(a && /^(INPUT|TEXTAREA|SELECT)$/.test(a.tagName) && a.value && a.type !== 'checkbox' && a.type !== 'radio');
};
addEventListener('keydown', (e) => {
  if (e.key !== 'Escape' || e.ctrlKey || e.altKey || e.metaKey || e.shiftKey) return;
  if (!PANEL && !document.documentElement.classList.contains('beside-panel')) return;
  if (!busy()) call('app_close_settings');
}, true);

if (PANEL) {
  const button = () => {
    const tools = document.querySelector('.page-head .tools');
    if (!tools || tools.querySelector('.panel-close')) return;
    const b = document.createElement('button');
    b.type = 'button';
    b.className = 'panel-close';
    b.title = 'Close Settings (Esc)';
    b.setAttribute('aria-label', 'Close Settings');
    b.innerHTML = '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8"/></svg>';
    b.addEventListener('click', () => call('app_close_settings'));
    tools.append(b);
  };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', button);
  else button();
}
