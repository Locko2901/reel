// Settings, Games and apps, the Discord panel: the line under Show on Discord says what reel sent
// to Discord, or why it sent nothing.
import { $ } from './shell.js';

export function renderPresence(status) {
  const note = $('#presence-note');
  if (!note) return;
  const d = status.presence || {};
  const what = document.createElement('span');
  what.className = 'what';
  what.textContent = d.what || '';
  note.replaceChildren(d.words || '', d.what ? ' ' : '', what);
  note.classList.toggle('st-warn', d.tone === 'warn');
  note.classList.toggle('sent', d.tone === 'sent');
}
