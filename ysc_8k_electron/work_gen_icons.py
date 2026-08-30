# -*- coding: utf-8 -*-
"""Regenerate the app icon set so the exe icon matches the driver UI brand:

dark rounded plate (#0f1117, same as BrowserWindow backgroundColor) +
white YSC wordmark traced as real vector paths from brand-white.png alpha.

Outputs (all under ysc_8k_electron/):
  resources/icon.png   512x512
  build/icon.png       512x512 (kept in sync, electron-builder convention)
  resources/icon.ico   16/24/32/48/64/128/256 frames (LANCZOS pre-resized)
  resources/icon.svg   vector source: plate + traced wordmark paths
"""
from PIL import Image, ImageDraw
import os

BASE = os.path.dirname(os.path.abspath(__file__))
ASSETS = os.path.join(BASE, '..', 'packages', 'ysc-core', 'src', 'ui', 'assets')

PLATE = (0x0f, 0x11, 0x17, 255)   # app window backgroundColor / old icon plate
WORDMARK = os.path.join(ASSETS, 'brand-white.png')

MASTER = 1024
PLATE_RADIUS = 192                # 48/256 of canvas, same ratio as old icon.svg
WORDMARK_WIDTH = int(MASTER * 0.71)

# ---------- 1. master raster ----------
word = Image.open(WORDMARK).convert('RGBA')
bbox = word.getchannel('A').getbbox()
word = word.crop(bbox)

scale = WORDMARK_WIDTH / word.width
word = word.resize((WORDMARK_WIDTH, max(1, round(word.height * scale))), Image.LANCZOS)

master = Image.new('RGBA', (MASTER, MASTER), (0, 0, 0, 0))
plate = Image.new('RGBA', (MASTER, MASTER), (0, 0, 0, 0))
ImageDraw.Draw(plate).rounded_rectangle(
    [0, 0, MASTER - 1, MASTER - 1], radius=PLATE_RADIUS, fill=PLATE)
master.alpha_composite(plate)
# optical center: wordmark sits at 49.5% height (slightly above geometric center)
mx, my = (MASTER - word.width) // 2, int(MASTER * 0.495) - word.height // 2
master.alpha_composite(word, (mx, my))

# ---------- 2. raster outputs ----------
png512 = master.resize((512, 512), Image.LANCZOS)
png512.save(os.path.join(BASE, 'resources', 'icon.png'))
png512.save(os.path.join(BASE, 'build', 'icon.png'))

ico_sizes = [16, 24, 32, 48, 64, 128, 256]
base256 = master.resize((256, 256), Image.LANCZOS)
frames = [base256] + [base256.resize((s, s), Image.LANCZOS) for s in ico_sizes[:-1]]
frames[0].save(
    os.path.join(BASE, 'resources', 'icon.ico'),
    format='ICO', append_images=frames[1:],
    sizes=[(s, s) for s in ico_sizes])

# ---------- 3. vector: trace wordmark alpha -> SVG paths ----------
# crack-following on the binary mask; foreground on the left of travel.
src = Image.open(WORDMARK).convert('RGBA')
a = src.getchannel('A')
W, H = src.size
px = a.load()
mask = [[1 if px[x, y] >= 128 else 0 for x in range(W)] for y in range(H)]

def bg(x, y):
    return x < 0 or y < 0 or x >= W or y >= H or mask[y][x] == 0

# directed cracks: fg kept on the left. dirs: (dx,dy) with y down.
# For pixel (x,y): top side bg -> travel +x; right side bg -> travel +y;
# bottom side bg -> travel -x; left side bg -> travel -y.
edges = {}
def add(p, q):
    edges.setdefault(p, []).append(q)

for y in range(H):
    for x in range(W):
        if not mask[y][x]:
            continue
        if bg(x, y - 1): add((x, y), (x + 1, y))
        if bg(x + 1, y): add((x + 1, y), (x + 1, y + 1))
        if bg(x, y + 1): add((x + 1, y + 1), (x, y + 1))
        if bg(x - 1, y): add((x, y + 1), (x, y))

loops = []
while edges:
    start = next(iter(edges))
    loop = [start]
    cur = start
    while True:
        nxts = edges[cur]
        nxt = nxts.pop()
        if not nxts:
            del edges[cur]
        cur = nxt
        if cur == start:
            break
        loop.append(cur)
    loops.append(loop)

def rdp(pts, eps):
    if len(pts) < 3:
        return pts
    ax, ay = pts[0]; bx, by = pts[-1]
    dx, dy = bx - ax, by - ay
    norm = (dx * dx + dy * dy) ** 0.5 or 1.0
    best, idx = 0.0, 0
    for i in range(1, len(pts) - 1):
        d = abs(dy * (pts[i][0] - ax) - dx * (pts[i][1] - ay)) / norm
        if d > best:
            best, idx = d, i
    if best <= eps:
        return [pts[0], pts[-1]]
    left = rdp(pts[:idx + 1], eps)
    return left[:-1] + rdp(pts[idx:], eps)

paths = []
for loop in loops:
    # loop is open (start not repeated); closing it before RDP would make the
    # p0->pN baseline degenerate and collapse everything to one point.
    simp = rdp(loop, 1.1)
    if len(simp) < 3:  # degenerate speck
        continue
    d = 'M' + ' '.join(f'{p[0]} {p[1]}' for p in simp) + 'Z'
    paths.append(d)

# position traced paths inside the 1024 plate exactly like the raster wordmark
sx = WORDMARK_WIDTH / (bbox[2] - bbox[0])
sy = sx
tx = mx - bbox[0] * sx
ty = my - bbox[1] * sy
wordmark_paths = ' '.join(paths)
svg = f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {MASTER} {MASTER}">
  <!-- YSC app icon: dark plate (#0f1117) + white wordmark, aligned with the
       driver UI titlebar brand (packages/ysc-core/src/ui/assets/brand-white.png).
       Wordmark paths traced from that PNG's alpha channel. -->
  <rect width="{MASTER}" height="{MASTER}" rx="{PLATE_RADIUS}" fill="#0f1117"/>
  <g transform="translate({tx:.2f} {ty:.2f}) scale({sx:.6f} {sy:.6f})">
    <path d="{wordmark_paths}" fill="#ffffff" fill-rule="evenodd"/>
  </g>
</svg>
'''
with open(os.path.join(BASE, 'resources', 'icon.svg'), 'w', encoding='utf-8') as f:
    f.write(svg)

# ---------- 4. verification previews ----------
prev = os.path.join(BASE, 'work_icon_previews.png')
sheet = Image.new('RGBA', (512 + 20 + 128 + 20 + 48 + 20 + 24, 512 + 40), (90, 90, 90, 255))
xoff = 20
for img in (png512, png512.resize((128, 128), Image.LANCZOS),
            png512.resize((48, 48), Image.LANCZOS), png512.resize((24, 24), Image.LANCZOS)):
    sheet.alpha_composite(img, (xoff, 20))
    xoff += img.width + 20
sheet.save(prev)
print('traced loops:', len(loops), 'kept:', len(paths))
print('done -> resources/icon.png, build/icon.png, resources/icon.ico, resources/icon.svg')
