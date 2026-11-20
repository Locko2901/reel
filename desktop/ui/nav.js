// The Settings nav: the app's own pages draw it from here, and the app puts this file, without
// its exports, in front of the server's Settings pages in its window (desktop/app/src/server_nav.js).
// crates/server/src/routes/settings.rs draws the same nav and its test reads this file.
export const APP_SECTIONS = [
  ['status', 'Status'], ['server', 'Server'], ['obs', 'OBS'], ['clips', 'Clips folder and cache'], ['upload', 'Upload'],
  ['hotkeys', 'Hotkeys'], ['session', 'Session recording'], ['games', 'Games and apps'], ['sounds', 'Sounds'], ['startup', 'Startup'], ['log', 'Log'],
  ['library', 'Library folder'], ['encoder', 'Encoder'],
];
export const STANDALONE_ONLY = ['library', 'encoder'];
export const STANDALONE_LABELS = { clips: 'Clips folder', upload: 'New clips' };
// By what a person does: reel's sections ('r') and the app's pages ('a'), each always, in
// standalone alone ('s') or beside a server alone ('p').
export const NAV = [
  ['Recording', ['a obs', 'a clips', 'a upload', 'a hotkeys', 'a session', 'a games', 'a sounds']],
  ['Library', ['r storage', 'as library', 'r ingest', 'as encoder', 'rs sharing', 'r imports']],
  ['Server', ['a server', 'rp sharing', 'r devices', 'r access', 'r notifications']],
  ['System', ['r general', 'r appearance', 'a startup', 'a status', 'a log', 'r advanced']],
];

// Games and apps' count of programs not sorted yet, beside its name in the nav.
export function unsortedCount(n) {
  const span = document.createElement('span');
  span.className = 'n';
  span.id = 'games-n';
  span.textContent = n || '';
  span.title = n ? `${n} not sorted yet` : '';
  return span;
}
