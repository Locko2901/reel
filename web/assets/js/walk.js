// The page cancels the back or forward button's mouseup and walks itself: on Linux wry walks
// only when it is not cancelled, and a webview that walks anyway leaves within the 150 ms.
export function mouseWalk() {
  let walk = null;
  const stop = () => clearTimeout(walk);
  addEventListener('mouseup', (e) => {
    if (e.button !== 3 && e.button !== 4) return;
    e.preventDefault();
    stop();
    const by = e.button === 3 ? -1 : 1;
    walk = setTimeout(() => history.go(by), 150);
  });
  addEventListener('pagehide', stop);
  addEventListener('popstate', stop);
}
