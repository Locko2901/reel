import { $, copyText, toast } from './js/lib.js';
import { isOn, setOn, switches } from './js/switch.js';
import { markNativePlay, rememberPlayback } from './js/playback.js';
import { playerKeys } from './js/player-keys.js';
import { clock, precise } from './js/format.js';
import { appDownloads } from './js/app-downloads.js';

appDownloads();

const video = $('#video'), player = $('#player'), overlay = $('#play-overlay');
markNativePlay();
rememberPlayback(video);
const loopBtn = $('#loop-btn'), speedBtn = $('#speed-btn'), momentBtn = $('#moment-btn'), moment = $('#moment');
const pageUrl = document.body.dataset.url || location.href.split('?')[0];
const fps = +document.body.dataset.fps || 60;

const start = parseFloat(new URLSearchParams(location.search).get('t'));
if (!isNaN(start) && start > 0) {
  const seek = () => { video.currentTime = Math.min(start, (video.duration || start) - 0.05); };
  if (video.readyState >= 1) seek(); else video.addEventListener('loadedmetadata', seek, { once: true });
}

const syncPlaying = () => player.classList.toggle('playing', !video.paused);
let byKeys = false;
const keyStep = () => { byKeys = true; player.classList.add('stepping'); };
video.addEventListener('play', () => player.classList.add('started'), { once: true });
video.addEventListener('play', () => { player.classList.remove('stepping'); syncPlaying(); });
video.addEventListener('pause', () => { if (!byKeys) player.classList.remove('stepping'); byKeys = false; syncPlaying(); });
video.addEventListener('ended', () => player.classList.remove('stepping'));
overlay.onclick = () => video.play().catch(() => {});
video.addEventListener('timeupdate', () => { moment.textContent = clock(video.currentTime); });

const setLoop = (on) => { video.loop = on; setOn(loopBtn, on); };
const setHalf = (on) => { video.playbackRate = on ? 0.5 : 1; setOn(speedBtn, on); };
switches();
loopBtn.addEventListener('change', () => setLoop(isOn(loopBtn)));
speedBtn.addEventListener('change', () => setHalf(isOn(speedBtn)));
momentBtn.onclick = async () => {
  const t = Math.round(video.currentTime * 10) / 10;
  const url = t > 0 ? `${pageUrl}?t=${t}` : pageUrl;
  if (await copyText(url)) toast(`Link copied, starts at ${precise(t)}.`, 'ok');
};

playerKeys(video, { box: player, fps, stepped: keyStep });
document.addEventListener('keydown', (e) => {
  if (e.ctrlKey || e.metaKey || e.altKey || /^(INPUT|TEXTAREA|SELECT)$/.test(document.activeElement?.tagName)) return;
  switch (e.key) {
    case 's': setHalf(video.playbackRate !== 0.5); break;
    case 'r': setLoop(!video.loop); break;
    case 'c': momentBtn.click(); break;
    case '?': $('#keys').classList.toggle('shown'); break;
  }
});

function awaitWebCopy() {
  const ask = video.dataset.web;
  if (!ask) return;
  let tries = 0;
  const swap = (d) => {
    const at = video.currentTime, playing = !video.paused, rate = video.playbackRate;
    video.addEventListener('loadedmetadata', () => {
      video.currentTime = Math.min(at, (video.duration || at) - 0.05);
      video.playbackRate = rate;
      if (playing) video.play().catch(() => {});
    }, { once: true });
    video.src = d.url;
    delete video.dataset.web;
    const shown = [...document.querySelectorAll('.meta span')].find((s) => /^\d+p( preview)?$/.test(s.textContent));
    if (shown && d.height) shown.textContent = `${d.height}p`;
  };
  const check = async () => {
    tries += 1;
    try {
      const d = await (await fetch(ask, { cache: 'no-store' })).json();
      if (d.ready && d.url) { swap(d); return; }
      if (d.gone) return;
    } catch {}
    if (tries < 120) setTimeout(check, tries < 12 ? 5000 : 15000);
  };
  setTimeout(check, 3000);
}
awaitWebCopy();
