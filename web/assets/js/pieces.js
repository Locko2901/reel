// The device API's upload in pieces, behind the session; after a reload the same file picked
// again goes on where it stopped.
import { Sha256 } from './sha256.js';

const READ = 4 << 20;

async function call(method, url, body) {
  const r = await fetch(url, {
    method,
    credentials: 'same-origin',
    headers: body ? { 'content-type': 'application/json' } : {},
    body: body ? JSON.stringify(body) : undefined,
  });
  let data = null;
  try { data = await r.json(); } catch { /* an answer without JSON */ }
  return { status: r.status, data };
}

function putPiece(url, blob, progress) {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.upload.onprogress = (e) => progress(e.loaded);
    xhr.onload = () => {
      let data = null;
      try { data = JSON.parse(xhr.responseText); } catch { /* an answer without JSON */ }
      resolve({ status: xhr.status, data });
    };
    xhr.onerror = () => reject(new Error('the connection broke off'));
    xhr.ontimeout = xhr.onerror;
    xhr.open('PUT', url);
    xhr.send(blob);
  });
}

// The file's SHA-256 over what the server has: fed each piece as it goes up, read again from
// the file where the server's end and the hash's part.
class FileHash {
  constructor(file) { this.file = file; this.reset(); }
  reset() { this.sha = new Sha256(); this.done = 0; }
  async feed(blob) {
    for (let at = 0; at < blob.size; at += READ) {
      this.sha.update(new Uint8Array(await blob.slice(at, at + READ).arrayBuffer()));
    }
    this.done += blob.size;
  }
  async piece(blob, at) {
    if (at === this.done) await this.feed(blob);
  }
  async to(end) {
    if (end < this.done) this.reset();
    if (end > this.done) await this.feed(this.file.slice(this.done, end));
  }
  async hex() { await this.to(this.file.size); return this.sha.hex(); }
}

export class Paused extends Error {}

const wait = (ms) => new Promise((r) => setTimeout(r, ms));

// `on.progress(sent, size, word)` as it goes; the clip ({status, clip}) at the end. Throws
// Paused when the connection keeps failing, for a Retry, and Error when the server refuses.
export async function sendInPieces(file, on) {
  const start = await call('POST', '/api/uploads', { file_name: file.name, size: file.size, file_key: `${file.size}-${file.lastModified}` });
  if (start.status >= 400 || !start.data) throw new Error((start.data && start.data.error) || `the upload was refused (${start.status})`);
  const id = start.data.upload_id;
  const piece = start.data.chunk_bytes || 32 << 20;
  let at = start.data.offset || 0;
  on.progress(at, file.size, at ? 'going on' : '');
  const hash = new FileHash(file);
  let tries = 0;
  for (;;) {
    while (at < file.size) {
      const blob = file.slice(at, Math.min(at + piece, file.size));
      const hashing = hash.piece(blob, at);
      let r = null;
      try { r = await putPiece(`/api/uploads/${id}?offset=${at}`, blob, (n) => on.progress(at + n, file.size)); } catch { r = null; }
      await hashing;
      if (r && r.status === 200 && r.data) {
        at = r.data.offset;
        tries = 0;
        on.progress(at, file.size);
        continue;
      }
      if (r && r.status === 409 && r.data && typeof r.data.offset === 'number') { at = r.data.offset; continue; }
      if (r && r.status < 500 && ![408, 429].includes(r.status)) throw new Error((r.data && r.data.error) || `a piece was refused (${r.status})`);
      if (++tries > 6) throw new Paused('the connection keeps breaking off');
      on.progress(at, file.size, 'trying again');
      await wait(Math.min(30000, 1000 * 2 ** tries));
      const s = await call('GET', `/api/uploads/${id}`).catch(() => null);
      if (s && s.status === 200 && s.data) at = s.data.offset;
    }
    on.progress(file.size, file.size, 'checking');
    await hash.to(at);
    const f = await call('POST', `/api/uploads/${id}/finish`, { sha256: await hash.hex() }).catch(() => null);
    if (!f) throw new Paused('the connection broke off');
    if (f.status === 200 && f.data) return f.data;
    if ([409, 422].includes(f.status) && f.data && typeof f.data.offset === 'number') { at = f.data.offset; continue; }
    throw new Error((f.data && f.data.error) || `the upload could not finish (${f.status})`);
  }
}
