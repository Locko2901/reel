// Chromium, and WebKit with the iPhone profiles. PLAYWRIGHT names the playwright module to use
// (default: the one Node finds); extra system libraries may sit under ~/.cache/ms-playwright/extra-libs.
const { chromium, firefox, webkit, devices } = await import(process.env.PLAYWRIGHT || 'playwright');
import { existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';

const extra = join(homedir(), '.cache', 'ms-playwright', 'extra-libs');
const libs = [join(extra, 'usr', 'lib', 'x86_64-linux-gnu'), join(extra, 'usr', 'lib', 'x86_64-linux-gnu', 'gbm')].filter(existsSync);
const env = { ...process.env, LD_LIBRARY_PATH: [...libs, process.env.LD_LIBRARY_PATH].filter(Boolean).join(':') };

export { devices };

export async function launch(kind = 'chromium') {
  if (kind === 'webkit') {
    const runner = join(extra, 'webkit-headless.sh');
    if (existsSync(runner)) process.env.PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS = '1';
    return webkit.launch({ env, ...(existsSync(runner) ? { executablePath: runner } : {}) });
  }
  if (kind === 'firefox') return firefox.launch({ env });
  return chromium.launch({ env });
}
