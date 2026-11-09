// In the app on this PC, a file already in the library opens where it is, or shows in its
// folder, instead of downloading a copy. The page names the clip and the file, never a path.
import { $$, toast } from './lib.js';

function call() {
  const t = window.__TAURI__;
  if (!document.body.hasAttribute('data-standalone') || !window.__REEL_DESKTOP__) return null;
  return t && t.core && typeof t.core.invoke === 'function' ? t.core.invoke : null;
}

export const filesHere = () => !!call();

// file: "current", "raw" (the raw file kept), or a version's id; false where no app opens it
export function openFile(clip, file, reveal) {
  const invoke = call();
  if (!invoke) return false;
  invoke('app_library_open', { clip, file, reveal }).catch((err) => toast(String(err?.message || err), 'err'));
  return true;
}

export function libraryFiles(clip) {
  if (!filesHere()) return;
  $$('[data-library-file]').forEach((b) => {
    b.hidden = false;
    b.addEventListener('click', () => openFile(clip, b.dataset.libraryFile, b.hasAttribute('data-reveal')));
  });
  $$('[data-in-library]').forEach((a) => { a.hidden = true; });
}
