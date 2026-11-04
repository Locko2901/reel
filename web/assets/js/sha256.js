// SHA-256 fed a piece at a time: the browser's own digest takes a whole buffer, and a
// clip of a few hundred MB read whole would not fit a phone's memory.
const K = new Uint32Array([
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
  0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
  0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
  0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
]);

export class Sha256 {
  constructor() {
    this.h = new Int32Array([0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19]);
    this.w = new Int32Array(64);
    this.tail = new Uint8Array(64);
    this.fill = 0;
    this.length = 0;
  }

  update(bytes) {
    let i = 0;
    this.length += bytes.length;
    if (this.fill) {
      const n = Math.min(64 - this.fill, bytes.length);
      this.tail.set(bytes.subarray(0, n), this.fill);
      this.fill += n;
      i = n;
      if (this.fill < 64) return this;
      this.block(this.tail, 0);
      this.fill = 0;
    }
    for (; i + 64 <= bytes.length; i += 64) this.block(bytes, i);
    if (i < bytes.length) {
      this.tail.set(bytes.subarray(i), 0);
      this.fill = bytes.length - i;
    }
    return this;
  }

  block(b, o) {
    const w = this.w, h = this.h;
    for (let t = 0; t < 16; t++, o += 4) w[t] = (b[o] << 24) | (b[o + 1] << 16) | (b[o + 2] << 8) | b[o + 3];
    for (let t = 16; t < 64; t++) {
      const x = w[t - 15], y = w[t - 2];
      const s0 = ((x >>> 7) | (x << 25)) ^ ((x >>> 18) | (x << 14)) ^ (x >>> 3);
      const s1 = ((y >>> 17) | (y << 15)) ^ ((y >>> 19) | (y << 13)) ^ (y >>> 10);
      w[t] = (w[t - 16] + s0 + w[t - 7] + s1) | 0;
    }
    let a = h[0], c1 = h[1], c2 = h[2], d = h[3], e = h[4], f = h[5], g = h[6], k = h[7];
    for (let t = 0; t < 64; t++) {
      const s1 = ((e >>> 6) | (e << 26)) ^ ((e >>> 11) | (e << 21)) ^ ((e >>> 25) | (e << 7));
      const t1 = (k + s1 + ((e & f) ^ (~e & g)) + K[t] + w[t]) | 0;
      const s0 = ((a >>> 2) | (a << 30)) ^ ((a >>> 13) | (a << 19)) ^ ((a >>> 22) | (a << 10));
      const t2 = (s0 + ((a & c1) ^ (a & c2) ^ (c1 & c2))) | 0;
      k = g; g = f; f = e; e = (d + t1) | 0; d = c2; c2 = c1; c1 = a; a = (t1 + t2) | 0;
    }
    h[0] += a; h[1] += c1; h[2] += c2; h[3] += d; h[4] += e; h[5] += f; h[6] += g; h[7] += k;
  }

  hex() {
    const bits = this.length * 8;
    const pad = new Uint8Array(((this.fill < 56 ? 56 : 120) - this.fill) + 8);
    pad[0] = 0x80;
    const v = new DataView(pad.buffer);
    v.setUint32(pad.length - 8, Math.floor(bits / 0x100000000));
    v.setUint32(pad.length - 4, bits >>> 0);
    this.update(pad);
    return Array.from(this.h, (x) => (x >>> 0).toString(16).padStart(8, '0')).join('');
  }
}
