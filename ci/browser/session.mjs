import { readFileSync, writeFileSync } from 'node:fs';

const user = process.env.REEL_DEMO_USER || 'sam';
// scripts/demo.sh sets the admin's password to "reel demo <port>" unless told otherwise.
const passwordFor = (base) => process.env.REEL_DEMO_PASSWORD || `reel demo ${new URL(base).port || '8746'}`;
// The session cookie is kept so a second run does not spend the sign-in limit.
const cache = process.env.REEL_DEMO_SESSION_FILE || '/tmp/reel-demo-session.json';

async function works(base, cookie) {
  const r = await fetch(`${base}/api/jobs/counts`, { headers: { cookie: `${cookie.name}=${cookie.value}` }, redirect: 'manual' });
  return r.status === 200;
}

export async function demoCookie(base) {
  try {
    const cached = JSON.parse(readFileSync(cache, 'utf8'));
    if (cached.base === base && await works(base, cached)) return cached;
  } catch {}
  const r = await fetch(`${base}/signin`, {
    method: 'POST',
    redirect: 'manual',
    headers: { 'content-type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams({ name: user, password: passwordFor(base), remember: '1' }),
  });
  const set = r.headers.get('set-cookie') || '';
  const m = /reel_session=([^;]+)/.exec(set);
  if (r.status !== 303 || !m) throw new Error(`the demo sign-in failed (${r.status}); is the demo admin made? scripts/demo.sh serve makes it`);
  const cookie = { name: 'reel_session', value: m[1], base };
  try { writeFileSync(cache, JSON.stringify(cookie)); } catch {}
  return cookie;
}

export async function signInContext(context, base) {
  const c = await demoCookie(base);
  await context.addCookies([{ name: c.name, value: c.value, url: base }]);
}
