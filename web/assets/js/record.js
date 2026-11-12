// The bar's Record, and a menu's item where there is one: whether a session recording runs, for
// how long on a clock of its own between the status reads, and a press that starts or stops it.

export function clock(ms) {
  const s = Math.max(0, Math.floor(ms / 1000));
  const two = (n) => String(n).padStart(2, '0');
  const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60);
  return h ? `${h}:${two(m)}:${two(s % 60)}` : `${m}:${two(s % 60)}`;
}

// `set(on)` asks the app and answers with the status after it; `fail(words)` says why not.
export function recordControl(button, item, set, fail) {
  let st = null;
  let timer = 0;
  let asking = false;
  const paint = () => {
    const s = st && st.session;
    const on = !!s && !s.starting;
    const busy = asking || (!!s && (s.starting || s.stopping));
    const can = st && st.record;
    button.classList.toggle('on', on);
    button.classList.toggle('busy', busy);
    button.querySelector('.rec-time').textContent = on ? clock(Date.now() - s.since_ms) : '';
    button.querySelector('.rec-word').textContent = !s ? 'Record' : s.starting ? 'Starting' : s.stopping ? 'Stopping' : 'Stop';
    button.disabled = !st || busy || (!s && !(can && can.ok));
    button.setAttribute('aria-label', on ? `Stop the session recording, ${s.length}` : 'Record a session');
    button.title = on ? `Recording a session: ${s.line}. Stop ends it; the recording goes into the library.`
      : can && !can.ok ? `Not now: ${can.why}`
      : 'Record a session: OBS records until you stop it, and the recording goes into the library like a clip';
    if (item) {
      item.textContent = on ? `Stop the session recording (${clock(Date.now() - s.since_ms)})` : 'Record a session';
      item.disabled = button.disabled;
    }
  };
  const press = async () => {
    if (asking || !st) return;
    asking = true;
    paint();
    try {
      const next = await set(!st.session);
      if (next) st = next;
    } catch (e) {
      fail(String(e && e.message ? e.message : e));
    }
    asking = false;
    paint();
  };
  button.addEventListener('click', press);
  if (item) item.addEventListener('click', press);
  return (next) => {
    st = next;
    paint();
    clearInterval(timer);
    if (st && st.session && !st.session.starting) timer = setInterval(paint, 1000);
  };
}
