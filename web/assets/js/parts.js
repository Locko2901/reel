// reportError logs as an uncaught error would and fires the window's error event, which the
// desktop app's Log hears; the parts after a failed one still start.
const report = (err) => {
  if (typeof window.reportError === 'function') window.reportError(err);
  else setTimeout(() => { throw err; });
};

/** Starts each part of a page on its own, in order. */
export function startParts(...parts) {
  for (const part of parts) {
    try {
      const running = part();
      if (running && typeof running.catch === 'function') running.catch(report);
    } catch (err) {
      report(err);
    }
  }
}
