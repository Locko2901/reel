// The same shapes as reel_core::format on the server.
export const size = (bytes) => {
  if (bytes == null) return '';
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  if (bytes >= 1e6) return `${(bytes / 1e6).toFixed(1)} MB`;
  if (bytes >= 1e3) return `${Math.round(bytes / 1e3)} kB`;
  return `${bytes} B`;
};
export const clock = (s) => { s = Math.max(0, Math.round(s)); const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), r = s % 60; return h ? `${h}:${String(m).padStart(2, '0')}:${String(r).padStart(2, '0')}` : `${m}:${String(r).padStart(2, '0')}`; };
export const precise = (s) => { s = Math.max(0, s); const m = Math.floor(s / 60), r = s - m * 60; return `${m}:${r.toFixed(2).padStart(5, '0')}`; };
export const seconds = (s) => `${Math.max(0, s).toFixed(1)} s`;
export const gain = (db) => { const n = Math.abs(db); const t = Number.isInteger(n) ? String(n) : n.toFixed(1); return db > 0 ? `+${t} dB` : db < 0 ? `-${t} dB` : '0 dB'; };
export const level = (v, unit) => Number.isFinite(v) ? `${v.toFixed(1)} ${unit}` : 'not measured';
export const parseTime = (t) => { const m = /^(?:(\d+):)?(\d+(?:\.\d+)?)$/.exec(t.trim()); return m ? (+(m[1] || 0)) * 60 + +m[2] : NaN; };
// Head and tail of a long value, so the pages cut the head and show the tail whole.
export const split = (value, keep = 6) => value.length <= keep ? [value, ''] : [value.slice(0, value.length - keep), value.slice(value.length - keep)];
