// The desktop app's bar at the window's foot: what this PC does in words, the fix for it,
// Record, and one menu that its words and its "..." both open. The server's pages and the app's
// own pages draw it from the app's status; `acts` does what each asks of the app there.
import { isOn, setOn } from './switch.js';
import { recordControl } from './record.js';

const $ = (sel, root = document) => root.querySelector(sel);
const $$ = (sel, root = document) => Array.from(root.querySelectorAll(sel));
const DOTS = ['ok', 'warn', 'err', 'active', 'rec', 'neutral'];

function setRow(dd, tone, word, line) {
  if (!dd) return;
  $('.dot', dd).className = `dot ${tone || ''}`;
  $('b', dd).textContent = word || '';
  $('.muted', dd).textContent = line || '';
}

// One menu, two buttons: it opens above the one pressed, a press on the same one closes it, a
// click outside or Esc closes it, and the keyboard lands in it and back on its button.
function menu(bar, pop, buttons) {
  let from = null;
  const close = (back) => {
    if (pop.hidden) return;
    pop.hidden = true;
    buttons.forEach(b => b.setAttribute('aria-expanded', 'false'));
    if (back && from) from.focus();
  };
  for (const b of buttons) {
    b.addEventListener('click', (e) => {
      const again = !pop.hidden && from === b;
      close(false);
      if (again) return;
      from = b;
      pop.dataset.from = b.id === 'appbar-words' ? 'words' : 'more';
      pop.hidden = false;
      b.setAttribute('aria-expanded', 'true');
      if (e.detail === 0) $('button:not([disabled]), a[href]', pop)?.focus();
    });
  }
  // capture: the header's menus stop their clicks before they would reach the document
  document.addEventListener('click', (e) => {
    if (!pop.hidden && !e.target.closest('#appbar-pop, #appbar-words, #appbar-more')) close(false);
  }, true);
  document.addEventListener('keydown', (e) => { if (e.key === 'Escape') close(true); });
  pop.addEventListener('keydown', (e) => {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    const items = $$('button:not([disabled]), a[href]', pop).filter(el => el.offsetParent);
    const at = items.indexOf(document.activeElement);
    // the library's own keys move between its cards
    e.preventDefault();
    e.stopPropagation();
    items[(at + (e.key === 'ArrowDown' ? 1 : items.length - 1)) % items.length]?.focus();
  });
  return close;
}

/**
 * Shows the bar and returns its render(status, server): `status` the app's, `server` the health
 * of the server or of the library on this PC as the page knows it ({ state, line }).
 * acts: fix(act), save(which), share(), pauseUploads(on), pauseClips(on), record(on), fail(words).
 */
export function appBar(acts) {
  const bar = $('#appbar');
  if (!bar) return () => {};
  bar.hidden = false;
  document.body.classList.add('has-appbar');
  const pop = $('#appbar-pop'), words = $('#appbar-words'), more = $('#appbar-more');
  const close = menu(bar, pop, [words, more]);
  const fix = $('#appbar-fix');
  let shownFix = null;
  fix.addEventListener('click', () => { if (shownFix) acts.fix(shownFix.act); });
  const record = recordControl($('#app-record'), null, acts.record, acts.fail);
  for (const [id, which] of [['#app-save', 'save'], ['#app-save-share', 'save_share']]) {
    $(id)?.addEventListener('click', () => { close(false); acts.save(which); });
  }
  $('#app-share')?.addEventListener('click', () => { close(false); acts.share(); });
  $('#app-recording')?.addEventListener('change', () => acts.pauseClips(isOn($('#app-recording'))));
  $('#app-pause')?.addEventListener('change', () => acts.pauseUploads(isOn($('#app-pause'))));

  return (st, server = { state: 'ok', line: '' }) => {
    if (!st) return;
    const here = st.mode === 'standalone';
    let tone = st.state_tone || 'ok', word = st.state_word || st.state, detail = st.state_detail || '';
    let line = st.state_line || '', fixNow = st.state_fix || null;
    let hint = st.state === 'idle' && detail && st.keys ? st.keys.save : '';
    // The server's health is the page's to know: said when nothing on this PC wants a word first.
    if (st.state === 'idle' && (server.state === 'warn' || server.state === 'err')) {
      hint = '';
      tone = server.state;
      word = `${here ? 'library' : 'server'} ${server.state === 'err' ? 'error' : 'warning'}`;
      detail = '';
      line = server.line ? `${server.line.charAt(0).toUpperCase()}${server.line.slice(1)}` : line;
      fixNow = { word: 'Status', act: 'status' };
    }
    if (tone === 'rec') detail = '';
    bar.classList.remove(...DOTS);
    bar.classList.add(tone);
    $('#appbar-dot').className = `dot ${tone}`;
    const w = $('#appbar-word');
    w.textContent = word;
    w.className = tone === 'ok' || tone === 'neutral' || tone === 'rec' ? '' : tone;
    $('#appbar-detail').textContent = detail;
    $('#appbar-hint').textContent = hint || '';
    words.setAttribute('aria-label', `${word}${detail ? `, ${detail}` : ''}: this PC's menu`);
    $('#appbar-say').textContent = line;
    shownFix = fixNow;
    fix.hidden = !fixNow;
    fix.textContent = fixNow ? fixNow.word : '';
    fix.className = `btn small appbar-fix ${tone === 'ok' || tone === 'neutral' ? '' : tone}`;
    if (fixNow) fix.setAttribute('aria-label', fixNow.act === 'status' ? 'Open the Status page' : `${fixNow.word}: ${word}`);
    for (const k of $$('#appbar [data-key]')) {
      k.textContent = (st.keys && st.keys[k.dataset.key]) || '';
      k.hidden = !k.textContent;
    }
    const what = {
      save: here ? 'OBS saves the replay, and it goes into the library on this PC' : 'OBS saves the replay and it goes up as any clip does',
      save_share: 'OBS saves the replay, the app uploads it and copies the link',
    };
    for (const [id, a, which] of [['#app-save', st.save, 'save'], ['#app-save-share', st.save_share, 'save_share']]) {
      const b = $(id);
      if (!b || !a) continue;
      b.disabled = !a.ok;
      b.title = a.ok ? what[which] : `Not now: ${a.why}`;
    }
    const share = $('#app-share');
    const canShare = st.can_share ?? st.paired;
    if (share) {
      share.disabled = !canShare;
      share.title = canShare ? 'Copies the link to the last clip that went up' : st.paired ? 'Not now: no clip has gone up yet' : 'Not now: the app is not paired with a server';
    }
    const note = $('#app-save-why');
    if (note && st.save) {
      const both = $('#app-save-share') && !$('#app-save-share').hidden;
      const why = !st.save.ok ? st.save.why : (!st.save_share?.ok && both ? `Save and share: ${st.save_share.why}` : '');
      note.textContent = why ? `Not now: ${why}` : '';
      note.hidden = !why;
    }
    setOn($('#app-recording'), !!st.recording_paused);
    setOn($('#app-pause'), !!(st.paused_by_user ?? st.paused));
    $$('#appbar .app-paired').forEach(el => { el.hidden = !(st.panel ? st.panel.pause : st.paired); });
    if (st.obs_row) setRow($('#part-obs'), st.obs_row.tone, st.obs_row.word, st.obs_row.line);
    const place = $('#bar-place');
    if (place && st.panel?.place_state) setRow(place, ...st.panel.place_state);
    else if (place && here) setRow(place, server.state, 'on this PC', server.line);
    else if (place) setRow(place, server.state, server.word || { ok: 'ok', warn: 'warning', err: 'error' }[server.state], server.line);
    record(st);
  };
}
