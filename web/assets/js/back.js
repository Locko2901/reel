// data-history: the server saw reel's own page right before this one, so Back walks the history.
import { $ } from './lib.js';

export function canWalkBack() {
  const a = $('#back');
  return !!a && a.hasAttribute('data-history') && history.length > 1;
}

export function goBack() {
  if (canWalkBack()) history.back();
  else location.href = $('#back')?.href || '/';
}

export function backLink() {
  const a = $('#back');
  if (!a) return;
  a.addEventListener('click', (e) => {
    if (e.button !== 0 || e.ctrlKey || e.metaKey || e.shiftKey || e.altKey || !canWalkBack()) return;
    e.preventDefault();
    history.back();
  });
}
