// The hotkey recorder; the app's hotkeys are off while a field listens, so a press is recorded, not run.
import { $, invoke } from './shell.js';

const TYPES_NOTHING = /^(F\d{1,2}|Pause|ScrollLock|Insert|PrintScreen|Media(PlayPause|Stop|TrackNext|TrackPrevious)|AudioVolume(Up|Down|Mute))$/;
const MOVES_CARET = /^(Arrow(Up|Down|Left|Right)|Home|End|PageUp|PageDown)$/;
const needsModifier = (code) => `F8 is fine alone; ${keyName(code)} ${MOVES_CARET.test(code) ? 'moves the caret' : 'types'}, so it needs Ctrl, Alt or Win`;
const KEY_NAMES = { ArrowUp: 'Up', ArrowDown: 'Down', ArrowLeft: 'Left', ArrowRight: 'Right', NumpadAdd: 'NumpadPlus' };
const TAKEN = /^(Key[A-Z]|Digit\d|F\d{1,2}|Numpad(\d|Add|Decimal|Divide|Enter|Equal|Multiply|Subtract)|Arrow(Up|Down|Left|Right)|Space|Enter|Tab|Backspace|Delete|End|Home|Insert|PageDown|PageUp|PrintScreen|ScrollLock|Pause|Backquote|Backslash|BracketLeft|BracketRight|Comma|Equal|Minus|Period|Quote|Semicolon|Slash|Media(PlayPause|Stop|TrackNext|TrackPrevious)|AudioVolume(Up|Down|Mute))$/;
const MODS = [['ctrlKey', 'Ctrl'], ['altKey', 'Alt'], ['shiftKey', 'Shift'], ['metaKey', 'Super']];
const MOD_KEYS = new Set(['Control', 'Alt', 'AltGraph', 'Shift', 'Meta', 'OS']);
const CAP_WORDS = { Super: 'Win' };

function keyName(code) {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit\d$/.test(code)) return code.slice(5);
  return KEY_NAMES[code] || code;
}

export function keycaps(btn, combo, unset = 'none, click to set') {
  const parts = combo.split('+').filter(Boolean);
  if (!parts.length) {
    const none = document.createElement('span');
    none.className = 'unset';
    none.textContent = unset;
    btn.replaceChildren(none);
    return;
  }
  btn.replaceChildren(...parts.map((k) => { const cap = document.createElement('kbd'); cap.textContent = CAP_WORDS[k] || k; return cap; }));
}

// Binds one row's field: `save(row)` keeps the combination it took (row.dataset.value),
// `fill(row)` shows what is kept again after Escape or a refusal.
export function recorder(row, { save, fill }) {
  const btn = $('button.hotkey', row), why = $('.why', row), err = $('.err', row);
  let listening = false, sawMods = '', quietUntil = 0;
  const note = (text) => { why.textContent = text; why.hidden = !text; };
  const refuse = (text) => { err.textContent = text; err.hidden = !text; row.classList.toggle('refused', !!text); };
  const resume = () => invoke('hotkeys_listen', { on: false }).catch(() => {});
  const stop = (andResume = true) => {
    if (!listening) return;
    listening = false;
    btn.classList.remove('listening');
    document.removeEventListener('keydown', onKey, true);
    document.removeEventListener('keyup', onUp, true);
    note('');
    if (andResume) resume();
  };
  const take = async (combo) => {
    stop(false);
    quietUntil = Date.now() + 400;
    keycaps(btn, combo);
    try {
      row.dataset.value = await invoke('hotkey_try', { key: row.dataset.key, combo });
      await save(row);
    } catch (e) {
      fill(row);
      refuse(String(e.message || e));
    }
    resume();
  };
  function onKey(e) {
    e.preventDefault();
    e.stopPropagation();
    const held = MODS.filter(([p]) => e[p]).map(([, n]) => n);
    if (MOD_KEYS.has(e.key)) {
      keycaps(btn, held.join('+'), 'press the keys');
      if (held.some((m) => m !== 'Shift')) sawMods = held.join('+');
      return;
    }
    sawMods = '';
    if (e.key === 'Escape' && !held.length) { stop(); fill(row); return; }
    if (e.code === 'Backspace' && !held.length) { take(''); return; }
    if (!TAKEN.test(e.code)) { refuse(`${e.key === ' ' ? 'Space' : e.key} cannot be part of a hotkey`); return; }
    if (!TYPES_NOTHING.test(e.code) && !held.some((m) => m !== 'Shift')) { refuse(needsModifier(e.code)); keycaps(btn, '', 'press the keys'); return; }
    take([...held, keyName(e.code)].join('+'));
  }
  function onUp(e) {
    e.preventDefault();
    e.stopPropagation();
    if (MODS.some(([p]) => e[p])) return;
    keycaps(btn, '', 'press the keys');
    if (sawMods) note(`Nothing came after ${sawMods}: if you pressed a key with them, Windows or another app holds that combination.`);
    sawMods = '';
  }
  btn.addEventListener('click', () => {
    if (listening || Date.now() < quietUntil) return;
    listening = true;
    sawMods = '';
    refuse('');
    btn.classList.add('listening');
    keycaps(btn, '', 'press the keys');
    note('Press the combination. Esc cancels, Backspace clears.');
    btn.focus();
    document.addEventListener('keydown', onKey, true);
    document.addEventListener('keyup', onUp, true);
    invoke('hotkeys_listen', { on: true }).catch(() => {});
  });
  btn.addEventListener('blur', () => { if (listening) { stop(); fill(row); } });
}

export function recordersOffOnLeave() {
  window.addEventListener('pagehide', () => invoke('hotkeys_listen', { on: false }).catch(() => {}));
}
