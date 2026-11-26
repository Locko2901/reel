// Usage: node ci/browser/check-settings-fit.mjs <server URL> [--shots dir]
// Settings, Advanced against a running server (the demo), signed in as session.mjs does. Its
// Settings history, twelve rows answered here in the server's shape (two long URLs, a secret, a
// long name, select words), never makes the page or its panel scroll sideways, its rows name the
// old and new value "was" and "now" (the table by its heads), and every Revert is on the screen
// where it scrolls to: at a phone's width, the app's dock's two, the page's own column and a wide
// window, as a browser and as the app's window with three of this PC's rows (a stand-in shell
// around the app's own scripts). And on a wide window, side by side panels of
// every Settings page start on one line with their names on one baseline. Nothing on the server
// changes: the history comes from here, and no Revert is pressed.
import { launch } from './browsers.mjs';
import { signInContext } from './session.mjs';
import { mkdirSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const args = process.argv.slice(2);
const flag = (f) => (args.includes(f) ? args[args.indexOf(f) + 1] : null);
const base = args[0];
if (!base || base.startsWith('--')) { console.error('usage: node ci/browser/check-settings-fit.mjs <server URL> [--shots dir]'); process.exit(2); }
const shots = flag('--shots');
if (shots) mkdirSync(shots, { recursive: true });
let failed = 0;
const check = (ok, what) => { console.log(ok ? 'ok  ' : 'FAIL', what); if (!ok) failed++; };

const SECRET = 'the value under the table';
const row = (id, key, label, old, now, words, user, at, when) => ({
  id, key, label, old, new: now, old_words: words ? words[0] : old, new_words: words ? words[1] : now, user, at, when,
});
const HISTORY = [
  row(12, 'notify.webhook_url', 'Webhook URL', '', 'https://hooks.example.com/services/T0A1B2C3D4E/B5F6G7H8I9J/kL0mN1oP2qR3sT4uV5wX6yZ7', null, 'sam', '2026-10-20T21:42:00Z', 'Oct 20, 21:42'),
  row(11, 'share.cleanup_days', 'Revoke after', '14', '7', null, 'alexandra.whitfield', '2026-10-16T10:01:30Z', 'Oct 16, 10:01'),
  row(10, 'external.new_days', 'New for', '2', '3', null, 'sam', '2026-10-16T10:01:03Z', 'Oct 16, 10:01'),
  row(9, 'external.protect_days', 'Protected window', '14', '7', null, 'sam', '2026-10-16T10:00:45Z', 'Oct 16, 10:00'),
  row(8, 'external.new_days', 'New for', '7', '2', null, 'sam', '2026-10-16T10:00:11Z', 'Oct 16, 10:00'),
  row(7, 'external.ahead', 'Made ahead', 'edited_new', 'edited', ['for edited and new clips', 'for edited clips'], 'sam', '2026-10-14T20:15:40Z', 'Oct 14, 20:15'),
  row(6, 'external.ahead', 'Made ahead', 'off', 'edited_new', ['none', 'for edited and new clips'], 'sam', '2026-10-14T20:15:01Z', 'Oct 14, 20:15'),
  row(5, 'share.cleanup_days', 'Revoke after', '30', '14', null, 'sam', '2026-10-14T20:14:05Z', 'Oct 14, 20:14'),
  row(4, 'external.endpoint', 'S3 endpoint', '', 'https://5f0e2c9a7b3d4e1f8a6c0b2d9e7f1a3c.r2.cloudflarestorage.com/reel-previews', null, 'sam', '2026-10-14T18:08:12Z', 'Oct 14, 18:08'),
  row(3, 'external.public_url', 'Public address', '', 'https://previews.example.com', null, 'sam', '2026-10-14T18:07:30Z', 'Oct 14, 18:07'),
  { ...row(2, 'external.secret_key', 'Secret key', null, null, null, 'sam', '2026-10-14T18:07:02Z', 'Oct 14, 18:07'), old_words: null, new_words: null },
  row(1, 'external.access_key', 'Access key', '', '5c1d7e0b2a9f4e3d8c6b1a0f7e2d9c4b', null, 'sam', '2026-10-14T18:06:41Z', 'Oct 14, 18:06'),
];
// this PC's, as app_settings_history gives them: a long Windows path, a password, one with a note
const MINE = {
  device: 'DESKTOP-7Q2M4KD',
  rows: [
    { id: '41', name: 'Recording folder', was: "in the app's data folder", now: 'D:\\Captures\\reel\\Replay buffer saves\\2026\\Clips that go up later', from: 'Settings', at: '2026-10-21T19:12:00Z', revert: true, note: null },
    { id: '40', name: 'OBS password', was: 'changed', now: 'changed', from: 'the setup', at: '2026-10-15T08:30:00Z', revert: false, note: 'change it under Capture' },
    { id: '39', name: 'Clip length', was: '60', now: '90', from: 'Settings', at: '2026-10-14T19:00:00Z', revert: true, note: null },
  ],
};

// The app's window around a server page, as shell.rs and panel.rs put its scripts together: its
// globals, an invoke that answers this PC's history, panel.js, then nav.js and tip.js without their
// exports before server_nav.js. In the dock the panel's page has PANEL true.
const read = (p) => readFileSync(join(root, p), 'utf8');
const unexport = (t) => t.split('\n').map((l) => l.replace(/^export /, '')).join('\n');
const appScripts = (panel) => [
  `window.__REEL_DESKTOP__ = { version: 'stand-in' };
window.__TAURI__ = { core: { invoke: async (cmd) => {
  if (cmd === 'app_settings_history') return ${JSON.stringify(MINE)};
  if (cmd === 'app_status') return { unsorted: 0 };
  throw new Error('not in the stand-in shell: ' + cmd);
} }, event: { listen: async () => () => {} } };`,
  `(() => { const PANEL = ${panel};\n${read('desktop/app/src/panel.js')}\n})();`,
  `(() => {\n${unexport(read('desktop/ui/nav.js'))}\nconst TIP = (() => {\n${unexport(read('web/assets/js/tip.js'))}\nreturn { enhance, bindTips, tipHit, tipHitsOff };\n})();\nconst TIP_CSS = '';\n${read('desktop/app/src/server_nav.js')}})();`,
];

const browser = await launch();
const open = async (width, kind, path) => {
  const ctx = await browser.newContext({ viewport: { width, height: 900 } });
  await signInContext(ctx, base);
  await ctx.route(/\/api\/settings\/history$/, (r) => r.fulfill({ json: { history: HISTORY } }));
  if (kind !== 'browser') for (const s of appScripts(kind === 'dock')) await ctx.addInitScript(s);
  const page = await ctx.newPage();
  page.on('pageerror', (e) => check(false, `${kind} ${width}: page error: ${e.message}`));
  await page.goto(`${base}${path}`);
  return { ctx, page };
};

// The history: as a browser at a phone's width, the dock's two and the page's column (from 1024
// the page has its own 760 px column; from 1600 a browser puts the groups in columns of 600 px or
// more), as the app's window full (below 1600) and in its dock, beside the library.
const CASES = [
  ...[390, 760, 980, 1280, 1600, 1920].map((w) => ['browser', w]),
  ['app', 1280], ['dock', 760], ['dock', 980],
];
for (const [kind, width] of CASES) {
  const what = `${kind} ${width} px:`;
  const { ctx, page } = await open(width, kind, '/settings/advanced');
  const rows = HISTORY.length + (kind === 'browser' ? 0 : MINE.rows.length);
  try {
    await page.waitForFunction((n) => document.querySelectorAll('#history tbody tr').length >= n, rows, { timeout: 20000 });
  } catch { check(false, `${what} the history's ${rows} rows came`); await ctx.close(); continue; }
  await page.waitForTimeout(300);
  const m = await page.evaluate(() => {
    const d = document.documentElement;
    const table = document.querySelector('#history');
    const panel = table.closest('.panel');
    const body = table.parentElement;
    const p = panel.getBoundingClientRect(), t = table.getBoundingClientRect();
    const form = getComputedStyle(table.tHead).display === 'none' ? 'rows' : 'table';
    const narrow = form === 'table'
      ? [...table.tBodies[0].rows].flatMap((tr) => [...tr.cells].slice(0, 3).map((td) => td.getBoundingClientRect().width)).filter((w) => w < 120).length
      : 0;
    const secret = [...table.tBodies[0].rows].filter((tr) => tr.cells[0].textContent === 'Secret key').map((tr) => [tr.cells[1].textContent, tr.cells[2].textContent]);
    // a value's label, by what shows: its word, and how far the value's text starts past the cell's
    // padding (a label before it pushes it on)
    const labels = [...table.tBodies[0].rows].map((tr) => [1, 2].map((i) => {
      const td = tr.cells[i], range = document.createRange();
      range.selectNodeContents(td);
      const text = [...range.getClientRects()].find((x) => x.width > 0);
      const shift = text ? text.left - td.getBoundingClientRect().left - parseFloat(getComputedStyle(td).paddingLeft) : 0;
      return { word: td.dataset.l || '', shift };
    }));
    return {
      page: [d.scrollWidth, d.clientWidth], panel: [panel.scrollWidth, panel.clientWidth], body: [body.scrollWidth, body.clientWidth],
      inside: t.left >= p.left - 0.5 && t.right <= p.right + 0.5, form, narrow, secret, labels, buttons: table.querySelectorAll('tbody button').length,
    };
  });
  check(m.page[0] <= m.page[1], `${what} the page does not scroll sideways (${m.page[0]} of ${m.page[1]} px)`);
  check(m.panel[0] <= m.panel[1] && m.body[0] <= m.body[1] && m.inside, `${what} the history stays inside its panel, as ${m.form === 'table' ? 'a table' : 'rows'} (${m.body[0]} of ${m.body[1]} px)`);
  check(m.narrow === 0, `${what} no Setting, Was or Now column under 120 px${m.narrow ? ` (${m.narrow} cells)` : ''}`);
  check(m.secret.length === 1 && m.secret[0].every((v) => v === SECRET), `${what} the secret's row says "${SECRET}" twice`);
  if (m.form === 'rows') {
    const named = m.labels.filter(([was, now]) => was.word === 'was' && now.word === 'now' && was.shift > 8 && now.shift > 8).length;
    check(named === m.labels.length, `${what} every row's old and new value have "was" and "now" before them (${named} of ${m.labels.length} rows${kind === 'browser' ? '' : ', this PC\'s too'})`);
  } else {
    const shown = m.labels.flat().filter((v) => v.shift > 2).length;
    check(shown === 0, `${what} the table's values show no labels, its heads name them${shown ? ` (${shown} labelled)` : ''}`);
  }
  let reached = 0;
  for (let i = 0; i < m.buttons; i++) {
    const b = page.locator('#history tbody button').nth(i);
    await b.evaluate((el) => el.scrollIntoView({ block: 'center', inline: 'nearest' }));
    const ok = await b.evaluate((el) => {
      const r = el.getBoundingClientRect(), d = document.documentElement;
      const x = r.left + r.width / 2, y = r.top + r.height / 2;
      const inView = r.left >= 0 && r.right <= d.clientWidth && r.top >= 0 && r.bottom <= innerHeight;
      return inView && el.contains(document.elementFromPoint(x, y));
    });
    if (ok) reached++;
  }
  const want = HISTORY.length + (kind === 'browser' ? 0 : MINE.rows.filter((r) => r.revert).length);
  check(m.buttons === want && reached === m.buttons, `${what} every Revert is on the screen and takes the pointer where it scrolls to (${reached} of ${m.buttons})`);
  if (shots) await page.screenshot({ path: join(shots, `history-${kind}-${width}.png`), fullPage: true });
  await ctx.close();
}

// Side by side panels: on a wide window every Settings page's groups go in columns; the first
// panel of each column starts on the same line and its name sits on the same baseline.
for (const width of [1600, 1920]) {
  const { ctx, page } = await open(width, 'browser', '/settings/advanced');
  await page.waitForFunction((n) => document.querySelectorAll('#history tbody tr').length >= n, HISTORY.length);
  const pages = await page.evaluate(() => [...new Set([...document.querySelectorAll('.sections a[href^="/settings/"]')].map((a) => a.getAttribute('href')))]);
  let sides = 0;
  for (const path of pages) {
    await page.goto(`${base}${path}`);
    await page.waitForTimeout(400);
    const firsts = await page.evaluate(() => {
      const tops = new Map();
      for (const p of document.querySelectorAll('#sections-body .panel')) {
        if (!p.offsetParent) continue;
        const r = p.getBoundingClientRect();
        const name = p.querySelector(':scope > header h2');
        let line = null;
        if (name) { const mark = document.createElement('span'); mark.style.cssText = 'display: inline-block; width: 0; height: 0'; name.append(mark); line = mark.getBoundingClientRect().top; mark.remove(); }
        const left = Math.round(r.left);
        if (!tops.has(left) || r.top < tops.get(left).top) tops.set(left, { top: r.top, line, name: name?.textContent.trim() || '' });
      }
      return [...tops.values()];
    });
    if (firsts.length < 2) continue;
    sides++;
    const top = (f) => f.top.toFixed(1), line = (f) => (f.line ?? 0).toFixed(1);
    const same = (k) => Math.max(...firsts.map(k)) - Math.min(...firsts.map(k)) < 1;
    check(same((f) => f.top) && same((f) => f.line ?? 0), `${width} px, ${path}: the columns' first panels start on one line, names on one baseline (${firsts.map((f) => `${f.name} at ${top(f)}, ${line(f)}`).join('; ')})`);
    if (shots && path === '/settings/advanced') await page.screenshot({ path: join(shots, `columns-${width}.png`) });
  }
  check(sides > 0, `${width} px: some Settings page shows panels side by side (${sides})`);
  await ctx.close();
}

await browser.close();
console.log(failed ? `${failed} failed` : 'all passed');
process.exit(failed ? 1 : 0);
