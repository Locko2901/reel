// Shared by the server's pages and the app's own. A field that takes text marks the page as
// typing, so a phone's tab bar steps aside for the keyboard.
import { $, $$ } from './lib.js';

export function header() {
  const pops = [[$('#chip'), $('#chip-pop')], [$('#user-btn'), $('#user-menu')]].filter(([b, p]) => b && p);
  const close = () => pops.forEach(([b, p]) => { p.hidden = true; b.setAttribute('aria-expanded', 'false'); });
  for (const [btn, pop] of pops) {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const open = pop.hidden;
      close();
      if (open) { pop.hidden = false; btn.setAttribute('aria-expanded', 'true'); }
    });
    pop.addEventListener('click', (e) => e.stopPropagation());
  }
  document.addEventListener('click', close);
  document.addEventListener('keydown', (e) => { if (e.key === 'Escape') close(); });
  const typing = (el) => !!el && (el.isContentEditable || /^(TEXTAREA|SELECT)$/.test(el.tagName) || (el.tagName === 'INPUT' && !['checkbox', 'radio', 'range', 'button', 'submit', 'reset', 'file', 'color'].includes(el.type)));
  const keyboard = () => document.documentElement.classList.toggle('typing', typing(document.activeElement));
  document.addEventListener('focusin', keyboard);
  document.addEventListener('focusout', () => setTimeout(keyboard, 0));
  const toggle = $('#search-toggle'), form = $('#search-form');
  if (toggle && form) {
    toggle.addEventListener('click', (e) => {
      e.stopPropagation();
      const open = !form.classList.contains('open');
      form.classList.toggle('open', open);
      toggle.setAttribute('aria-expanded', String(open));
      if (open) $('#search')?.focus();
    });
  }
}

export const WORST = ['err', 'warn', 'active', 'ok', 'neutral'];
export function worst(states) {
  for (const s of WORST) if (states.includes(s)) return s;
  return 'ok';
}

export function setPart(id, state, word, line) {
  const dd = $(`#${id}`), dt = dd?.previousElementSibling;
  if (!dd) return;
  dd.hidden = false; if (dt) dt.hidden = false;
  $('.dot', dd).className = `dot ${state}`;
  $('b', dd).textContent = word || '';
  $('.muted', dd).textContent = line || '';
}
