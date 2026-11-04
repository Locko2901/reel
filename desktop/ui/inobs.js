// Words and options for what was read from OBS, values no option here names among them.

export function takenWords(taken) {
  return taken && taken.length ? `From OBS: ${taken.join('; ')}.` : 'Nothing was changed in OBS.';
}

function fpsWord([n, d]) {
  return d === 1 ? String(n) : (n / d).toFixed(3).replace(/0+$/, '').replace(/\.$/, '');
}

export function customVideo(s) {
  const v = s.custom_video;
  return v ? `In OBS: ${v.output[0]}x${v.output[1]} at ${fpsWord(v.fps)} fps` : 'As set in OBS';
}

export function ensureOption(select, value, label) {
  if (!value || [...select.options].some((o) => o.value === value)) return;
  const o = document.createElement('option');
  o.value = value;
  o.textContent = label || `${value}, as set in OBS`;
  select.append(o);
}

export function syncCustom(select, s) {
  const o = [...select.options].find((x) => x.value === 'custom');
  if (!o) return;
  o.hidden = s.quality !== 'custom';
  o.textContent = customVideo(s);
}

export function takeInto(draft, before, after) {
  const b = before.obs.setup, a = after.obs.setup;
  for (const k of Object.keys(a)) {
    if (JSON.stringify(a[k]) !== JSON.stringify(b[k])) draft.obs.setup[k] = a[k];
  }
  if (after.clips_dir !== before.clips_dir) draft.clips_dir = after.clips_dir;
  if (after.hotkeys.save !== before.hotkeys.save) draft.hotkeys.save = after.hotkeys.save;
}
