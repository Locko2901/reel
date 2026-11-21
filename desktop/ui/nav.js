// Settings' pages: the app's own pages draw their list from here, and the app puts this file,
// without its exports, in front of the server's Settings pages in its window
// (desktop/app/src/server_nav.js). crates/server/src/routes/settings.rs draws the same groups in
// its nav and its test reads this file.
export const APP_SECTIONS = [
  ['clips', 'Clips'], ['session', 'Session recording'], ['capture', 'Capture'], ['games', 'Games and apps'],
  ['upload', 'Uploads'], ['server', 'Connection'], ['startup', 'App'], ['status', 'Status'],
  ['library', 'Library folder'], ['encoder', 'Encoding'],
];
export const STANDALONE_ONLY = ['library', 'encoder'];
// The first group is this PC's, the app's own pages; the rest are reel's sections ('r') with the
// app's pages that belong among them ('a'), each always, in standalone alone ('s') or beside a
// server alone ('p').
export const NAV = [
  ['This PC', ['a clips', 'a session', 'a capture', 'a games', 'ap upload', 'a server', 'a startup', 'a status']],
  ['Library', ['r storage', 'as library', 'r ingest', 'as encoder', 'rs sharing', 'r imports']],
  ['Server', ['rp sharing', 'r devices', 'r access', 'r notifications']],
  ['System', ['r general', 'r appearance', 'r advanced']],
];

// Games and apps' count of programs not sorted yet, beside its name.
export function unsortedCount(n) {
  const span = document.createElement('span');
  span.className = 'n';
  span.id = 'games-n';
  span.textContent = n || '';
  span.title = n ? `${n} not sorted yet` : '';
  return span;
}
