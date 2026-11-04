import { $, $$, api, arm, copyText, onEvent, settings, toast } from './lib.js';
import { header } from './header.js';
import { switches } from './switch.js';

export function prefs() { try { return JSON.parse(localStorage.getItem('reel-prefs') || '{}'); } catch { return {}; } }
export function applyPrefs() {
  const p = prefs(), html = document.documentElement;
  html.dataset.cards = ['small', 'large'].includes(p.cards) ? p.cards : 'medium';
  html.dataset.hover = p.hover === false ? 'off' : 'on';
  html.dataset.kbd = p.kbd === false ? 'off' : 'on';
  return p;
}
applyPrefs();

function bindCopyAndRevoke() {
  document.addEventListener('click', async (e) => {
    const copy = e.target.closest('.copy');
    if (copy) { e.preventDefault(); if (await copyText(copy.dataset.copy)) toast('Link copied. Go paste it.', 'ok'); return; }
    const revoke = e.target.closest('.revoke');
    if (!revoke) return;
    e.preventDefault();
    if (!arm(revoke, 'Sure?', 'Revoke')) return;
    try {
      await api('DELETE', `/api/shares/${revoke.dataset.sid}`);
      const row = revoke.closest('[data-sid]');
      if (row) row.remove();
      toast('Link revoked. It stopped working just now.');
    } catch (err) { toast(err.message, 'err'); }
  });
}

function jobsCount() {
  const els = $$('#jobs-count, #tab-jobs-count');
  if (!els.length) return;
  const render = (c) => els.forEach((el) => {
    const live = (c.running || 0) + (c.queued || 0);
    if (c.failed) { el.textContent = c.failed; el.className = 'count failed'; el.title = `${c.failed} failed`; }
    else if (live) { el.textContent = live; el.className = 'count live'; el.title = `${c.running} running, ${c.queued} queued`; }
    else { el.textContent = ''; el.className = 'count'; el.title = ''; }
  });
  if (settings.jobs) render(settings.jobs);
  let timer = null;
  onEvent((ev) => {
    if (ev.type !== 'job' && ev.type !== 'resumed') return;
    clearTimeout(timer);
    timer = setTimeout(async () => { try { render(await api('GET', '/api/jobs/counts')); } catch {} }, 500);
  });
}

function globalKeys() {
  let chord = '';
  document.addEventListener('keydown', (e) => {
    const inField = /^(INPUT|TEXTAREA|SELECT)$/.test(document.activeElement?.tagName) || document.activeElement?.isContentEditable;
    if (e.key === 'Escape') { $$('dialog[open]').forEach(d => d.close()); if (inField) document.activeElement.blur(); return; }
    if (inField || e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.key === '/') { e.preventDefault(); $('#search')?.focus(); $('#search')?.select(); return; }
    if (chord === 'g') { chord = ''; if (e.key === 's' && !document.body.hasAttribute('data-standalone')) location.href = '/shares'; if (e.key === 'l') location.href = '/'; if (e.key === 'j') location.href = '/jobs'; if (e.key === 't') location.href = '/status'; return; }
    if (e.key === ',') { location.href = '/settings'; return; }
    if (e.key === 'g') { chord = 'g'; setTimeout(() => chord = '', 900); return; }
    if (e.key === 'u') { location.href = '/upload'; }
  });
}

// A list of clips WebKit's page cache kept missed what changed meanwhile, so it is drawn again.
function freshOnReturn() {
  addEventListener('pageshow', (e) => { if (e.persisted && $('#grid, .trash-table')) location.reload(); });
}

bindCopyAndRevoke();
freshOnReturn();
header();
switches();
jobsCount();
globalKeys();
