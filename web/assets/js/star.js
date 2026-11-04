// The star on a card, the clip page's Star and the clip menu's item all go through here; the
// page hears "reel:star" to sieve its cards again.
import { $$, api, toast } from './lib.js';

export function showStar(id, on) {
  for (const card of $$(`.card[data-id="${id}"]`)) card.toggleAttribute('data-star', on);
  for (const b of $$(`.card[data-id="${id}"] .card-star, .star-btn[data-clip="${id}"]`)) {
    b.setAttribute('aria-pressed', String(on));
    b.title = on ? 'Starred' : 'Star';
    const word = b.querySelector('.word');
    if (word) word.textContent = on ? 'Starred' : 'Star';
  }
  document.dispatchEvent(new CustomEvent('reel:star', { detail: { id, on } }));
}

export async function setStar(id, on) {
  showStar(id, on);
  try { await api(on ? 'PUT' : 'DELETE', `/api/clips/${id}/star`); return true; }
  catch (err) { showStar(id, !on); toast(err.message, 'err'); return false; }
}

export function stars() {
  document.addEventListener('click', (e) => {
    const b = e.target.closest('.card-star, .star-btn');
    if (!b) return;
    e.preventDefault();
    const id = b.dataset.clip || b.closest('.card')?.dataset.id;
    if (id) setStar(id, b.getAttribute('aria-pressed') !== 'true');
  });
  // the clip page's s, outside a text field
  const page = document.querySelector('#star');
  if (!page) return;
  document.addEventListener('keydown', (e) => {
    if (e.key !== 's' || e.ctrlKey || e.metaKey || e.altKey || e.repeat) return;
    const el = document.activeElement;
    if (/^(INPUT|TEXTAREA|SELECT)$/.test(el?.tagName) || el?.isContentEditable || document.querySelector('dialog[open]')) return;
    e.preventDefault();
    page.click();
  });
}
