// Usage: node ci/browser/app-pages.mjs <docs dir> [name ...]
//        node ci/browser/app-pages.mjs <out dir> --page 'index.html#first' [--flavour connected] [--width 1280]
// The desktop app's own pages (desktop/ui) in Chromium, the engine of Windows' WebView2, with a
// stand-in for the app's shell: a made-up PC on Windows, reel with OBS, paired with
// clips.example.com. For the docs' pictures in Windows' words, which the Linux app under Xvfb
// cannot take; desktop/ci/docs-shots.sh takes the rest from the real app. With no name, all.
// With REEL_E2E_SERVER (and REEL_E2E_USER, REEL_E2E_PASSWORD), the server's Pair a device too.
// FFMPEG (or desktop/app/binaries' ffmpeg) brings each picture to 256 colours, as the docs' are.
import { launch } from './browsers.mjs';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, renameSync, rmSync } from 'node:fs';
import { createServer } from 'node:http';
import { dirname, extname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const args = process.argv.slice(2);
const flag = (f, or) => (args.includes(f) ? args[args.indexOf(f) + 1] : or);
const [out, ...wanted] = args.filter((a, i) => !a.startsWith('--') && !(i > 0 && args[i - 1].startsWith('--')));
if (!out) { console.error('usage: node ci/browser/app-pages.mjs <out dir> [name ...]'); process.exit(2); }

// The stand-in shell, run in the page before its scripts: it holds nothing from outside. The
// flavour's words: standalone, linux, own (an OBS of one's own), choice (the setup's first page),
// first (a first setup), update (one ready to install).
const shell = (flavour) => {
  const standalone = flavour.includes('standalone');
  const ready = flavour.includes('update');
  const linux = flavour.includes('linux');
  const own = flavour.includes('own');
  const choice = flavour.includes('choice');
  const setup = {
    done: !choice && !flavour.includes('first'), clip_seconds: 60, quality: 'smooth', rate: 'bitrate', kbit: 0, level: 20,
    encoder: 'obs_nvenc_h264_tex', game: true, elevated: false, desktop_audio: 'default', mic: 'default', desktop: true,
    screen: { id: 'screen-1', name: 'LG ULTRAGEAR', width: 2560, height: 1440, hz: 144, main: true },
    game_audio: '', discord: false, own_tracks: false, format: 'hybrid_mp4', password: '', noise: 'rnnoise', gate: false,
    own_filters: [], own_filters_from: '', volumes: {}, custom_video: null, rec_tracks: 0, titles: [],
  };
  const cue = (enabled, volume = 1) => ({ enabled, volume, file: null });
  const settings = {
    mode: standalone ? 'standalone' : 'connected',
    standalone: { hwaccel: 'auto', cap_gb: 0, cap_keeps_unsent: true, copies_on_hardware: false, library_dir: '' },
    server_url: standalone ? '' : 'https://clips.example.com', device_name: 'GAMING-PC',
    obs: { enabled: true, host: '127.0.0.1', port: 4455, password: '', start_replay: true, replay_buffer: 'games', bundled: !choice && !own, setup, setup_later: false, own_setup_due: false },
    clips_follow_obs: true, clips_dir: 'C:\\Users\\sam\\Videos\\reel', extensions: ['mkv', 'mp4'], stable_seconds: 3,
    upload: { mode: 'auto', bandwidth_kbit: 0, wait_for_games: false, pause_only_on_wan: false, chunk_mb: 0, keep_last: 0 },
    cache: { gb: 0, days: 30 },
    hotkeys: { share_last: 'Ctrl+Alt+S', save_share: 'Ctrl+Alt+Shift+S', upload_last: 'Ctrl+Alt+U', save: 'Alt+F10', record: 'Alt+F9' },
    sounds: { captured: cue(true), uploaded: cue(true), link_copied: cue(true, 0.5), session: cue(true), no_clip: cue(true) },
    session: { remind_minutes: 30, record_by_itself: false, stop_with_game: true, grace_seconds: 60 },
    games: { steam: true }, discord: { show: false, name_game: true, button: true },
    startup: { autostart: true, elevated: false }, tray_icon: 'colour', updates: 'ask', log_level: 'info',
  };
  const versions = ready
    ? { app: '1.19.0', kind: 'reel with OBS', obs: '32.2.2', server: '1.19.1', newest: '1.19.1', standing: 'update', words: 'app 1.19.0, server 1.19.1' }
    : standalone
      ? { app: '1.19.0', kind: 'reel with OBS', obs: '32.2.2', server: null, newest: '1.19.0', standing: 'alone', words: '1.19.0' }
      : { app: '1.19.0', kind: 'reel with OBS', obs: '32.2.2', server: '1.19.0', newest: '1.19.0', standing: 'same', words: '1.19.0' };
  const status = {
    mode: settings.mode, paired: !standalone, offline: null, server_url: settings.server_url, server_name: standalone ? '' : 'clips.example.com',
    user: standalone ? '' : 'sam', device_name: 'GAMING-PC', server_admin: !standalone,
    state: 'idle', state_tone: 'ok', state_word: 'ready', state_detail: 'last 60 s', state_line: 'OBS keeps the last 60 s; Alt+F10 or Save clip below saves them.', state_fix: null,
    obs_row: { tone: 'ok', word: 'running', line: 'OBS 32.2.2, hidden' }, keys: { save: 'Alt+F10', save_share: standalone ? '' : 'Ctrl+Alt+Shift+S', share_last: standalone ? '' : 'Ctrl+Alt+S', record: 'Alt+F9' },
    paused_by_user: false, paused_fullscreen: false, recording_paused: false, unsorted: 0, playing: null, grace: null, presence: { line: '', sent: false }, newcomer: null,
    session: null, record: { ok: true, why: '' },
    obs: { connected: true, line: 'OBS 32.2.2, websocket 5.6.3' }, bundled: !own,
    obs_box: choice || own ? null : { state: 'running', line: 'running, hidden', shown: false, elevated: false, can_show: true, version: '32.2.2', without_rights: false },
    obs_task_problem: null, obs_copy: null, obs_step: null, obs_elevated: false, nvidia_noise: true, obs_which: linux ? 'the OBS installed on this PC (/usr/bin/obs)' : '', obs_linux: linux,
    obs_encoders: [{ id: 'obs_nvenc_h264_tex', ok: true, why: '' }, { id: 'obs_x264', ok: true, why: '' }], wayland: false, linux,
    obs_app_audio: null, obs_game_capture: null, server_upload_max_bytes: null, obs_version: '32.2.2', obs_source: 'https://github.com/obsproject/obs-studio/releases/tag/32.2.2',
    obs_state: { profile: 'reel', advanced: true, record_dir: 'C:\\Users\\sam\\Videos\\reel', format: 'hybrid_mp4', format_name: 'hybrid MP4', encoder: 'obs_nvenc_h264_tex', encoder_name: 'NVIDIA NVENC H.264', video_kbit: null, audio_kbit: 160, audio_tracks: [1], replay_enabled: true, replay_seconds: 60, replay_active: true, recording: false, quality: null, base: [2560, 1440], output: [1920, 1080], fps: 60, obs_version: '32.2.2', ws_version: '5.6.3' },
    watcher: { dir: 'C:\\Users\\sam\\Videos\\reel', ok: true, line: 'watching C:\\Users\\sam\\Videos\\reel, as OBS says', from_obs: true },
    queue: [], recent: [], hotkey_ok: true, hotkey_line: 'Alt+F10 saves a clip, Ctrl+Alt+S shares the last clip', hotkeys_how: '', tray_line: '', discord_note: '',
    save: { ok: true, why: '' }, save_share: { ok: !standalone, why: standalone ? '' : '' }, obs_checks: [],
    normalize_new: false, loudness_target: -16,
    update_line: ready ? 'reel 1.19.1 is ready to install' : 'up to date', update_version: ready ? '1.19.1' : null, update_ask: null, update_installing: false, update_notes: null,
    update_before: ready ? 'Install closes reel, its installer shows its progress, and reel 1.19.1 opens again by itself, in about a minute.' : null,
    update_check: standalone ? true : null, update_checked_at: Date.UTC(2026, 10, 21, 19, 42), updated_from: null,
    versions, update_mode: 'ask', releases_url: 'https://github.com/Locko2901/reel/releases', uninstaller: true, version: '1.19.0',
    config_dir: 'C:\\Users\\sam\\AppData\\Roaming\\reel', data_dir: 'C:\\Users\\sam\\AppData\\Local\\reel',
    local: standalone ? { url: 'http://127.0.0.1:52117', library_dir: 'D:\\reel', backend: 'nvenc', ffmpeg_version: '8.0', notes: [], ffmpeg: {} } : null,
    local_line: standalone ? 'reel on 127.0.0.1:52117 with nvenc' : '',
    library: { away: null }, panel: standalone ? { place: 'Library', place_state: null, pause: false, save_share: false, share_last: false, shares: false } : { place: 'Server', place_state: null, pause: true, save_share: true, share_last: true, shares: true },
    pages: {
      summary: standalone
        ? '**Alt+F10** saves the last **60 s** while a game runs, at **Smooth**. Each clip goes into the library on this PC.'
        : '**Alt+F10** saves the last **60 s** while a game runs, at **Smooth**. Each clip goes up to **clips.example.com**.',
    },
  };
  const sections = [
    ['storage', 'Library and storage'], ['ingest', 'Ingest'], ['sharing', standalone ? 'Copies under a size' : 'Sharing'], ['imports', 'Imports'],
    ...(standalone ? [] : [['devices', 'Devices'], ['access', 'Access'], ['notifications', 'Notifications']]),
    ['general', 'General'], ['appearance', 'Appearance'], ['advanced', 'Advanced'],
  ].map(([id, label]) => ({ id, label, line: '', changed: 0 }));
  const shape = { sections, who: { name: 'sam', line: 'admin', admin: true }, health: { state: 'ok', line: '' }, jobs: {} };
  const programs = { entries: [], every: { buffer_always: false, record: false, stop: true, wait: false }, not_again: [], steam: { found: true, games: 12, line: '' }, standalone, paired: !standalone, linux };
  const copy = (v) => JSON.parse(JSON.stringify(v));
  const video = (s) => {
    const sh = s.screen && s.screen.width ? s.screen : { width: 1920, height: 1080, hz: 60 };
    const [height, fps] = { light: [1080, 30], sharp: [1440, 60], native: [sh.height, Math.min(sh.hz, 240)] }[s.quality] || [1080, 60];
    const h = Math.min(height, sh.height), w = Math.round(sh.width * h / sh.height / 2) * 2;
    const kbit = s.kbit > 0 ? s.kbit : Math.max(1, Math.round(w * h * fps * 0.2 / 1000 / 500)) * 500;
    return { base: [sh.width, sh.height], output: [w, h], fps, kbit, buffer_mb: Math.max(64, Math.floor((kbit + 160) * Math.max(1, s.clip_seconds) * 5 / 4 / 8 / 1000)) };
  };
  const answers = {
    settings_get: () => copy(settings), status_get: () => copy(status), page_shape_get: () => ({ shape: copy(shape), base: '' }),
    settings_save: (a) => { Object.assign(settings, copy(a.settings)); return copy(settings); },
    obs_video: (a) => video(a.setup), system_memory: () => 32768, obs_own_mic: () => null, programs_get: () => copy(programs),
    audio_devices: () => ({ outputs: [], inputs: [{ value: '{headset}', name: 'Headset Microphone (USB)' }], discord: true }), audio_programs: () => [],
    obs_audio: (a) => ({ sources: [{ name: 'Desktop audio', kind: 'desktop', mixers: 1 }, ...(a.setup.mic ? [{ name: 'Microphone', kind: 'mic', mixers: 1 }] : [])], words: ['The full mix, which links play: desktop audio and the microphone'], apart: false }),
    screens_get: () => [setup.screen], obs_suggested_quality: () => null, encoders_test: () => [], steam_get: () => ({ found: true, games: [] }),
    log_tail: () => ['reel-desktop 1.19.0 starts', 'obs connected'], standalone_cap: () => ({ line: '' }),
  };
  const listeners = {};
  window.__TAURI__ = {
    core: { invoke: async (cmd, a = {}) => (answers[cmd] ? answers[cmd](a) : null) },
    event: { listen: async (name, f) => { (listeners[name] ||= []).push(f); return () => {}; } },
  };
};

// Each picture: the page, its flavour, the window, and the part of the page it shows.
const PICTURES = {
  'img/app-updates.png': { page: 'index.html#startup', flavour: 'update', size: [1280, 900], part: '#update-help' },
  'img/app-settings.png': { page: 'index.html', flavour: 'connected', size: [1280, 820] },
  'img/app-hotkeys.png': { page: 'index.html#hotkeys', flavour: 'connected', size: [1280, 820] },
  'setup/img/app-setup-0-choice.png': { page: 'setup.html', flavour: 'standalone choice', size: [1280, 800] },
  'setup/img/app-setup-9-new-versions.png': { page: 'setup.html#look', flavour: 'standalone first', size: [1280, 800] },
  'setup/img/app-linux-choice.png': { page: 'setup.html', flavour: 'standalone choice linux', size: [1280, 800] },
};

// One page as a whole, for a look at it in Windows' words: --page, its flavour and the width.
if (flag('--page')) {
  const w = +flag('--width', 1280);
  for (const k of Object.keys(PICTURES)) delete PICTURES[k];
  PICTURES[`${flag('--page').replace(/[^a-z0-9]+/gi, '-')}-${flag('--flavour', 'connected')}-${w}.png`] = { page: flag('--page'), flavour: flag('--flavour', 'connected'), size: [w, 900], full: true };
}

const ui = join(root, 'desktop/ui'), assets = join(root, 'web/assets');
const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.woff2': 'font/woff2', '.svg': 'image/svg+xml', '.png': 'image/png' };
const server = createServer((req, res) => {
  const want = decodeURIComponent(req.url.split('?')[0]);
  if (want === '/assets/tips.css') {
    // what build.rs writes into the app's assets: reel.css's tips block
    const css = readFileSync(join(assets, 'reel.css'), 'utf8');
    res.writeHead(200, { 'content-type': 'text/css' });
    res.end(css.slice(css.indexOf('/* tips:'), css.indexOf('/* tips end */')));
    return;
  }
  const file =want.startsWith('/assets/') ? resolve(assets, `.${want.slice('/assets'.length)}`) : resolve(ui, `.${want}`);
  if (!(file.startsWith(ui) || file.startsWith(assets)) || !existsSync(file)) { res.writeHead(404); res.end(); return; }
  res.writeHead(200, { 'content-type': types[extname(file)] || 'application/octet-stream' });
  res.end(readFileSync(file));
});
await new Promise((ok) => server.listen(0, '127.0.0.1', ok));
const base = `http://127.0.0.1:${server.address().port}`;

const ffmpeg = [process.env.FFMPEG, join(root, 'desktop/app/binaries/ffmpeg-x86_64-unknown-linux-gnu')].find((f) => f && existsSync(f));
let failed = 0;
// A screenshot at its place, in 256 colours when there is an ffmpeg to bring it there.
async function keep(name, take) {
  const file = join(out, name), raw = `${file}.raw.png`;
  mkdirSync(dirname(file), { recursive: true });
  await take(raw);
  if (!ffmpeg) { renameSync(raw, file); console.log('shot', file); return; }
  const r = spawnSync(ffmpeg, ['-y', '-v', 'error', '-i', raw, '-vf', 'split[a][b];[a]palettegen=max_colors=256:reserve_transparent=0[p];[b][p]paletteuse=dither=none', file]);
  rmSync(raw, { force: true });
  if (r.status !== 0) { console.log(`FAIL ${name}: ${r.stderr}`); failed++; return; }
  console.log('shot', file);
}

const browser = await launch('chromium');
for (const [name, p] of Object.entries(PICTURES)) {
  if (wanted.length && !wanted.some((w) => name.endsWith(w))) continue;
  const page = await browser.newPage({ viewport: { width: p.size[0], height: p.size[1] }, locale: 'en-US', timezoneId: 'UTC' });
  page.on('pageerror', (e) => { console.log(`FAIL ${name}: page error ${e.message}`); failed++; });
  await page.addInitScript(shell, p.flavour);
  await page.goto(`${base}/${p.page}`, { waitUntil: 'networkidle' });
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(600);
  await keep(name, async (raw) => {
    if (!p.part) { await page.screenshot({ path: raw, fullPage: !!p.full }); return; }
    const panel = page.locator(p.part).locator('xpath=ancestor::div[contains(@class,"panel")][1]');
    await panel.scrollIntoViewIfNeeded();
    await panel.screenshot({ path: raw });
  });
  await page.close();
}

// The server's Devices with Pair a device open, whose words name the app's page to pair on.
const pair = 'setup/img/server-pair-a-device.png';
if (process.env.REEL_E2E_SERVER && (!wanted.length || wanted.some((w) => pair.endsWith(w))) && !flag('--page')) {
  const at = process.env.REEL_E2E_SERVER.replace(/\/$/, '');
  const page = await browser.newPage({ viewport: { width: 1280, height: 820 } });
  await page.goto(`${at}/signin`);
  await page.fill('form:has(button:text("Sign in")) input[name="name"]', process.env.REEL_E2E_USER || 'sam');
  await page.fill('form:has(button:text("Sign in")) input[name="password"]', process.env.REEL_E2E_PASSWORD || '');
  await Promise.all([page.waitForURL((u) => !u.pathname.startsWith('/signin')), page.click('button:text("Sign in")')]);
  await page.goto(`${at}/settings/devices`);
  await page.waitForLoadState('networkidle');
  await page.click('#pair-device');
  await page.waitForSelector('#pair-dialog[open]');
  await page.waitForTimeout(500);
  await page.mouse.move(1270, 810);
  await keep(pair, (raw) => page.screenshot({ path: raw }));
}
await browser.close();
server.close();
process.exit(failed ? 1 : 0);
