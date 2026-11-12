// In the app's window the page stays hidden until app-shell.js has drawn the app's parts, so its
// first paint is never the browser's page; a classic script in the head, it runs before that paint.
(() => {
  const t = window.__TAURI__;
  if (!window.__REEL_DESKTOP__ || !t || !t.core || typeof t.core.invoke !== 'function') return;
  document.documentElement.dataset.app = 'desktop';
  document.documentElement.dataset.wait = '';
})();
