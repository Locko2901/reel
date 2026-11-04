// Usage: node ci/browser/check-pieces.mjs <proxy URL> <500 MB clip> <clip over 100 MB> <small clip> [--log sizes.log] [--shots dir]
import { launch } from './browsers.mjs';
import { signInContext } from './session.mjs';
import { mkdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const args = process.argv.slice(2);
const flag = (f) => (args.includes(f) ? args[args.indexOf(f) + 1] : null);
const [base, big, shared, small] = args;
const log = flag('--log'), shots = flag('--shots');
if (shots) mkdirSync(shots, { recursive: true });
let failed = 0;
const check = (ok, what) => { console.log(ok ? 'ok  ' : 'FAIL', what); if (!ok) failed++; };
const percent = (t) => +((/(\d+)%/.exec(t || '') || [])[1] || 0);
const LONG = { timeout: 900000 };

const browser = await launch();
const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 } });
await signInContext(ctx, base);
const page = await ctx.newPage();
page.on('pageerror', (e) => check(false, `page error: ${e.message}`));

await page.goto(`${base}/upload`);
await page.setInputFiles('#file', big);
await page.waitForFunction(() => /(\d+)%/.test(document.querySelector('#uploads .mono')?.textContent || '') && +/(\d+)%/.exec(document.querySelector('#uploads .mono').textContent)[1] >= 40, null, LONG);
const before = percent(await page.locator('#uploads .mono').textContent());
if (shots) await page.screenshot({ path: join(shots, 'upload-in-pieces.png') });
await page.reload();
let aborted = 0;
await page.route(/\/api\/uploads\/[^/?]+\?offset=/, (route) => (aborted++ === 0 ? route.abort('connectionreset') : route.continue()));
await page.setInputFiles('#file', big);
await page.waitForFunction(() => /going on/.test(document.querySelector('#uploads .mono')?.textContent || ''), null, LONG);
const resumed = percent(await page.locator('#uploads .mono').textContent());
check(resumed >= before - 10 && resumed >= 30, `after a reload at ${before}%, the same file picked again goes on at ${resumed}%, not from the start`);
if (shots) await page.screenshot({ path: join(shots, 'upload-goes-on.png') });
await page.waitForFunction(() => document.querySelector('#uploads .notice.ok, #uploads .notice.err'), null, LONG);
const line = await page.locator('#uploads .notice').first().textContent();
check(/uploaded, now encoding|already in the library/.test(line) && aborted >= 2, `the 500 MB clip is in the library, a piece broken off on the way sent again (${line.trim()})`);
if (shots) await page.screenshot({ path: join(shots, 'upload-done.png') });
await page.unroute(/\/api\/uploads\/[^/?]+\?offset=/);

await page.goto(`${base}/`);
await page.evaluate(async () => { await navigator.serviceWorker.ready; });
if (!(await page.evaluate(() => !!navigator.serviceWorker.controller))) await page.reload();
check(await page.evaluate(() => !!navigator.serviceWorker.controller), 'the service worker controls the page, as on a phone with reel installed');
await page.evaluate(() => {
  const f = document.createElement('form');
  f.method = 'post'; f.action = '/share-target'; f.enctype = 'multipart/form-data'; f.id = 'as-shared';
  f.innerHTML = '<input type="file" name="clip">';
  document.body.append(f);
});
await page.setInputFiles('#as-shared input', shared);
await Promise.all([page.waitForURL(/\/share-target$|\/clips\//, LONG), page.evaluate(() => document.querySelector('#as-shared').submit())]);
await page.waitForURL(/\/clips\/[0-9A-Z]+/, LONG);
check(true, `a clip of ${Math.round(readFileSync(shared).length / 1e6)} MB shared to reel goes up from the service worker's handed file, and its page opens (${new URL(page.url()).pathname})`);

await page.goto(`${base}/`);
const bytes = readFileSync(small).toString('base64');
const dt = await page.evaluateHandle((b64) => {
  const t = new DataTransfer();
  t.items.add(new File([Uint8Array.from(atob(b64), (c) => c.charCodeAt(0))], 'dropped on the library.mp4', { type: 'video/mp4' }));
  return t;
}, bytes);
await page.dispatchEvent('body', 'dragenter', { dataTransfer: dt });
check(await page.evaluate(() => document.body.classList.contains('over')), 'files over the library: "Drop to upload"');
await page.dispatchEvent('body', 'drop', { dataTransfer: dt });
await page.waitForFunction(() => document.querySelector('#uploads .notice.ok, #uploads .notice.err'), null, LONG);
check(await page.locator('#uploads .notice.ok').count() === 1 && !(await page.evaluate(() => document.body.classList.contains('over'))), `a clip dropped on the library goes up there: ${(await page.locator('#uploads .notice').first().textContent()).trim()}`);
if (shots) await page.screenshot({ path: join(shots, 'dropped-on-the-library.png') });

if (log) {
  const rows = readFileSync(log, 'utf8').trim().split('\n').map((l) => l.split(' '));
  const reached = rows.filter((r) => r[2] !== '413');
  const top = Math.max(...reached.map((r) => +r[3]));
  const pieces = rows.filter((r) => r[0] === 'PUT' && r[1].startsWith('/api/uploads/'));
  check(top < 100 * 1024 * 1024 && pieces.length >= 20, `the proxy let through ${reached.length} requests, the largest ${(top / 1048576).toFixed(1)} MiB; ${pieces.length} pieces of ${Math.max(...pieces.map((r) => +r[3])) / 1048576 | 0} MiB at most`);
}
await browser.close();
console.log(failed ? `${failed} failed` : 'all ok');
process.exit(failed ? 1 : 0);
