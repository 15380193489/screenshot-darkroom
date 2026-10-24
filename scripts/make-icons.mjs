import { deflateSync } from "node:zlib";
import { writeFileSync, mkdirSync } from "node:fs";
let table = Array.from({ length: 256 }, (_, n) => {
  for (let k = 0; k < 8; k++) n = n & 1 ? 0xedb88320 ^ (n >>> 1) : n >>> 1;
  return n >>> 0;
});
function crc(b) {
  let c = 0xffffffff;
  for (const v of b) c = table[(c ^ v) & 255] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
function chunk(name, data) {
  const n = Buffer.from(name),
    len = Buffer.alloc(4),
    sum = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  sum.writeUInt32BE(crc(Buffer.concat([n, data])));
  return Buffer.concat([len, n, data, sum]);
}
function png(size) {
  const raw = Buffer.alloc(size * (size * 4 + 1));
  for (let y = 0; y < size; y++)
    for (let x = 0; x < size; x++) {
      let X = x / size,
        Y = y / size;
      let color = [226, 118, 94, 255];
      if (
        (X < 0.08 || X > 0.92) &&
        (Y < 0.08 || Y > 0.92) &&
        Math.hypot(
          Math.max(0.08 - X, X - 0.92, 0),
          Math.max(0.08 - Y, Y - 0.92, 0),
        ) > 0.08
      )
        color = [0, 0, 0, 0];
      if (X > 0.21 && X < 0.79 && Y > 0.26 && Y < 0.77)
        color = [255, 249, 234, 255];
      if (X > 0.27 && X < 0.73 && Y > 0.32 && Y < 0.63)
        color = [82, 111, 101, 255];
      if (Math.hypot(X - 0.5, Y - 0.475) < 0.105) color = [250, 208, 147, 255];
      if (Math.hypot(X - 0.67, Y - 0.37) < 0.025) color = [255, 247, 224, 255];
      if (X > 0.39 && X < 0.61 && Y > 0.68 && Y < 0.705)
        color = [226, 118, 94, 255];
      const p = y * (size * 4 + 1) + 1 + x * 4;
      raw.set(color, p);
    }
  let head = Buffer.alloc(13);
  head.writeUInt32BE(size);
  head.writeUInt32BE(size, 4);
  head[8] = 8;
  head[9] = 6;
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk("IHDR", head),
    chunk("IDAT", deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}
mkdirSync("src-tauri/icons", { recursive: true });
const images = [16, 32, 48, 256].map((s) => ({ size: s, data: png(s) }));
writeFileSync("src-tauri/icons/icon.png", png(64));
const hdr = Buffer.alloc(6);
hdr.writeUInt16LE(1, 2);
hdr.writeUInt16LE(images.length, 4);
let offset = 6 + images.length * 16;
const entries = images.map(({ size, data }) => {
  const e = Buffer.alloc(16);
  e[0] = e[1] = size === 256 ? 0 : size;
  e.writeUInt16LE(1, 4);
  e.writeUInt16LE(32, 6);
  e.writeUInt32LE(data.length, 8);
  e.writeUInt32LE(offset, 12);
  offset += data.length;
  return e;
});
writeFileSync(
  "src-tauri/icons/icon.ico",
  Buffer.concat([hdr, ...entries, ...images.map((x) => x.data)]),
);
