// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Draws the add-in's ribbon and catalogue icons: a rounded square in the
// accent colour with a white "L" and a small branch, at each size the manifest
// asks for. PNGs written by hand (zlib from Node), so no image tooling is needed.
// Run: node scripts/make-icons.mjs

import { writeFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { deflateSync } from "node:zlib";

const out = fileURLToPath(new URL("../apps/office-addin/public/assets/", import.meta.url));
const ACCENT = [0x0e, 0x7c, 0x7b];
const WHITE = [0xff, 0xff, 0xff];

const table = new Int32Array(256).map((_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c;
});
function crc32(bytes) {
  let c = -1;
  for (const b of bytes) c = table[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ -1) >>> 0;
}
function chunk(type, data) {
  const head = Buffer.alloc(8);
  head.writeUInt32BE(data.length, 0);
  head.write(type, 4, "ascii");
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([head.subarray(4), data])), 0);
  return Buffer.concat([head, data, crc]);
}

function icon(size) {
  const radius = size * 0.2;
  const rows = [];
  for (let y = 0; y < size; y++) {
    const row = Buffer.alloc(1 + size * 4);
    for (let x = 0; x < size; x++) {
      const cx = x + 0.5;
      const cy = y + 0.5;
      // Rounded square.
      const dx = Math.max(radius - cx, 0, cx - (size - radius));
      const dy = Math.max(radius - cy, 0, cy - (size - radius));
      const inside = dx * dx + dy * dy <= radius * radius;
      // The "L": a stem and a foot.
      const stem = cx >= size * 0.3 && cx <= size * 0.44 && cy >= size * 0.2 && cy <= size * 0.8;
      const foot = cx >= size * 0.3 && cx <= size * 0.72 && cy >= size * 0.66 && cy <= size * 0.8;
      // A branch: a dot up and to the right of the stem.
      const bx = cx - size * 0.66;
      const by = cy - size * 0.36;
      const branch = bx * bx + by * by <= (size * 0.09) ** 2;
      const colour = inside && (stem || foot || branch) ? WHITE : ACCENT;
      const alpha = inside ? 255 : 0;
      row.set([...colour, alpha], 1 + x * 4);
    }
    rows.push(row);
  }
  const header = Buffer.alloc(13);
  header.writeUInt32BE(size, 0);
  header.writeUInt32BE(size, 4);
  header.set([8, 6, 0, 0, 0], 8);
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", header),
    chunk("IDAT", deflateSync(Buffer.concat(rows), { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

mkdirSync(out, { recursive: true });
for (const size of [16, 32, 64, 80, 128]) {
  writeFileSync(join(out, `icon-${size}.png`), icon(size));
}
console.log(`icons written to ${out}`);
