const KEY = 'reel-playback';

function read() {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) || '{}');
    return v && typeof v === 'object' ? v : {};
  } catch { return {}; }
}

function write(v) {
  try { localStorage.setItem(KEY, JSON.stringify(v)); } catch { /* the default next time */ }
}

const volumeOf = (p) => (typeof p.volume === 'number' && p.volume >= 0 && p.volume <= 1 ? p.volume : null);

// Browsers that draw their own play button over a video with controls (every one on iOS,
// Android's, Firefox): reel draws no overlay there, so one play button shows.
export function markNativePlay() {
  const ua = navigator.userAgent;
  const ipad = /Macintosh/.test(ua) && navigator.maxTouchPoints > 0;
  if (/iPhone|iPad|iPod|Android|Firefox\//.test(ua) || ipad) document.documentElement.dataset.nativePlay = '';
}

export function rememberPlayback(video) {
  const p = read();
  if (volumeOf(p) !== null) video.volume = volumeOf(p);
  if (typeof p.muted === 'boolean') video.muted = p.muted;
  let wasMuted = video.muted;
  video.addEventListener('volumechange', () => {
    const now = read();
    now.volume = video.volume;
    if (video.muted !== wasMuted) { now.muted = video.muted; wasMuted = video.muted; }
    write(now);
  });
}

export function hoverPlayback(video) {
  const p = read();
  if (volumeOf(p) !== null) video.volume = volumeOf(p);
  video.muted = p.muted !== false;
}
