// Trace the YSC logo PNG into simplified vector polygons (for laser engraving).
// Usage: node trace_logo.js <in.png> <out.json> <out.svg>
const fs = require('fs');
const zlib = require('zlib');

function decodePNG(buf) {
  if (buf.readUInt32BE(0) !== 0x89504e47) throw new Error('not a PNG');
  let pos = 8, w = 0, h = 0, colorType = 0, bitDepth = 0, interlace = 0;
  const idat = [];
  while (pos < buf.length) {
    const len = buf.readUInt32BE(pos);
    const type = buf.toString('ascii', pos + 4, pos + 8);
    const data = buf.subarray(pos + 8, pos + 8 + len);
    if (type === 'IHDR') {
      w = data.readUInt32BE(0); h = data.readUInt32BE(4);
      bitDepth = data[8]; colorType = data[9]; interlace = data[12];
    } else if (type === 'IDAT') idat.push(data);
    else if (type === 'IEND') break;
    pos += 12 + len;
  }
  if (bitDepth !== 8) throw new Error('bitDepth ' + bitDepth + ' unsupported');
  if (interlace !== 0) throw new Error('interlaced PNG unsupported');
  const bpp = { 0: 1, 2: 3, 3: 1, 4: 2, 6: 4 }[colorType];
  if (!bpp) throw new Error('colorType ' + colorType + ' unsupported');
  const raw = zlib.inflateSync(Buffer.concat(idat));
  const stride = w * bpp;
  const img = Buffer.alloc(w * h * bpp);
  let prev = Buffer.alloc(stride);
  for (let y = 0; y < h; y++) {
    const filter = raw[y * (stride + 1)];
    const line = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1));
    const cur = img.subarray(y * stride, (y + 1) * stride);
    for (let i = 0; i < stride; i++) {
      const a = i >= bpp ? cur[i - bpp] : 0;
      const b = prev[i];
      const c = i >= bpp ? prev[i - bpp] : 0;
      let v = line[i];
      if (filter === 1) v += a;
      else if (filter === 2) v += b;
      else if (filter === 3) v += (a + b) >> 1;
      else if (filter === 4) {
        const p = a + b - c, pa = Math.abs(p - a), pb = Math.abs(p - b), pc = Math.abs(p - c);
        v += pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      }
      cur[i] = v & 0xff;
    }
    prev = cur;
  }
  return { w, h, bpp, img };
}

function binarize(png) {
  const { w, h, bpp, img } = png;
  const grid = new Uint8Array(w * h); // 1 = ink (logo body)
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const o = (y * w + x) * bpp;
      const a = bpp === 4 ? img[o + 3] : 255;
      const lum = bpp >= 3 ? 0.299 * img[o] + 0.587 * img[o + 1] + 0.114 * img[o + 2] : img[o];
      // ink = visible AND dark; also treat solid dark-on-transparent as ink
      grid[y * w + x] = a > 127 && lum < 128 ? 1 : 0;
    }
  }
  let ink = 0;
  for (let i = 0; i < grid.length; i++) ink += grid[i];
  console.error('ink pixels:', ink, '/', w * h);
  return grid;
}

// Directed boundary edges so that each closed chain follows the shape outline.
function traceLoops(grid, w, h) {
  const at = (x, y) => (x >= 0 && y >= 0 && x < w && y < h) ? grid[y * w + x] : 0;
  const out = new Map(); // "x,y" -> [{x,y}] outgoing edges
  const add = (x1, y1, x2, y2) => {
    const k = x1 + ',' + y1;
    if (!out.has(k)) out.set(k, []);
    out.get(k).push({ x: x2, y: y2 });
  };
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      if (!at(x, y)) continue;
      if (!at(x, y - 1)) add(x, y, x + 1, y);       // top side ->
      if (!at(x, y + 1)) add(x + 1, y + 1, x, y + 1); // bottom side <-
      if (!at(x - 1, y)) add(x, y + 1, x, y);       // left side up
      if (!at(x + 1, y)) add(x + 1, y, x + 1, y + 1); // right side down
    }
  }
  const loops = [];
  for (const [k, edges] of out) {
    while (edges.length) {
      const [sx, sy] = k.split(',').map(Number);
      const start = { x: sx, y: sy };
      let cur = edges.pop();
      const loop = [start, { x: cur.x, y: cur.y }];
      let key = cur.x + ',' + cur.y;
      let guard = 4 * w * h;
      while (key !== k && guard-- > 0) {
        const nxt = out.get(key);
        if (!nxt || !nxt.length) break;
        cur = nxt.pop();
        loop.push({ x: cur.x, y: cur.y });
        key = cur.x + ',' + cur.y;
      }
      loops.push(loop);
    }
  }
  return loops;
}

// Ramer-Douglas-Peucker simplification (closed loop: anchor at two farthest-ish points)
function rdp(pts, eps) {
  if (pts.length < 4) return pts;
  const keep = new Uint8Array(pts.length);
  keep[0] = keep[pts.length - 1] = 1;
  const stack = [[0, pts.length - 1]];
  while (stack.length) {
    const [a, b] = stack.pop();
    const ax = pts[a].x, ay = pts[a].y, bx = pts[b].x, by = pts[b].y;
    const dx = bx - ax, dy = by - ay;
    const len2 = dx * dx + dy * dy;
    let maxD = -1, maxI = -1;
    for (let i = a + 1; i < b; i++) {
      const px = pts[i].x - ax, py = pts[i].y - ay;
      let t = len2 ? (px * dx + py * dy) / len2 : 0;
      t = Math.max(0, Math.min(1, t));
      const ex = px - t * dx, ey = py - t * dy;
      const d = ex * ex + ey * ey;
      if (d > maxD) { maxD = d; maxI = i; }
    }
    if (maxD > eps * eps) {
      keep[maxI] = 1;
      stack.push([a, maxI], [maxI, b]);
    }
  }
  return pts.filter((_, i) => keep[i]);
}

function simplifyLoop(loop, eps) {
  // drop closing dup; split the ring at top-most / bottom-most points into one open chain
  const pts = loop.slice(0, -1);
  if (pts.length < 8) return loop;
  let i0 = 0;
  for (let i = 1; i < pts.length; i++) if (pts[i].y < pts[i0].y) i0 = i; // topmost
  const rot = pts.slice(i0).concat(pts.slice(0, i0));
  let i1 = 0;
  for (let i = 1; i < pts.length; i++) if (rot[i].y > rot[i1].y) i1 = i; // bottommost
  if (i1 === 0) return pts; // degenerate ring, keep as-is
  const chain = rot.slice(i1).concat(rot.slice(0, i1 + 1)); // bottom → … → bottom
  return rdp(chain, eps).slice(0, -1);
}

const [, , inPng, outJson, outSvg] = process.argv;
const png = decodePNG(fs.readFileSync(inPng));
const grid = binarize(png);
let loops = traceLoops(grid, png.w, png.h).map(l => simplifyLoop(l, 1.7)).filter(l => l.length >= 4);

// bbox
let minX = 1e9, minY = 1e9, maxX = -1e9, maxY = -1e9;
for (const l of loops) for (const p of l) {
  if (p.x < minX) minX = p.x; if (p.y < minY) minY = p.y;
  if (p.x > maxX) maxX = p.x; if (p.y > maxY) maxY = p.y;
}
const d = loops.map(l => 'M' + l.map(p => p.x + ' ' + p.y).join('L') + 'Z').join('');
fs.writeFileSync(outJson, JSON.stringify({ w: png.w, h: png.h, bbox: [minX, minY, maxX, maxY], loops: loops.length, d }, null, 1));
const pad = 4;
fs.writeFileSync(outSvg,
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${minX - pad} ${minY - pad} ${maxX - minX + 2 * pad} ${maxY - minY + 2 * pad}">` +
  `<rect x="${minX - pad}" y="${minY - pad}" width="${maxX - minX + 2 * pad}" height="${maxY - minY + 2 * pad}" fill="#fff"/>` +
  `<path d="${d}" fill="#000" fill-rule="evenodd"/></svg>`);
console.log(`source ${png.w}x${png.h} | loops=${loops.length} | bbox=(${minX},${minY})-(${maxX},${maxY}) size=${maxX - minX}x${maxY - minY} | d.length=${d.length}`);
