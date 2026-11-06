#!/usr/bin/env python3
"""Build the benchmark corpus in corpus/ from scikit-image's sample photographs.

Two encoders are used:
  * a small numpy PNG writer with an explicit filter policy (none/sub/up/avg/paeth/adaptive),
    so that each filter's cost can be measured in the full pipeline;
  * Pillow (its own libpng-free encoder) for "whatever a normal tool produces" files.
Writes corpus/manifest.tsv: name  encoder  width  height  colour  depth  filter  bytes
"""
import os, sys, zlib, struct, numpy as np
from PIL import Image
import skimage.data as skd
from skimage.transform import resize

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'corpus')
os.makedirs(OUT, exist_ok=True)
manifest = []

def chunk(tag, data):
    c = struct.pack('>I', len(data)) + tag + data
    return c + struct.pack('>I', zlib.crc32(tag + data) & 0xFFFFFFFF)

def filt_row(row, prev, bpp, f):
    """PNG filter f applied to a reconstructed row (int32 arrays)."""
    a = np.concatenate([np.zeros(bpp, np.int32), row[:-bpp]]) if bpp <= len(row) else np.zeros_like(row)
    b = prev
    c = np.concatenate([np.zeros(bpp, np.int32), prev[:-bpp]]) if bpp <= len(row) else np.zeros_like(row)
    if f == 0: return row
    if f == 1: return (row - a) & 255
    if f == 2: return (row - b) & 255
    if f == 3: return (row - ((a + b) >> 1)) & 255
    p = a + b - c
    pa, pb, pc = np.abs(p - a), np.abs(p - b), np.abs(p - c)
    pred = np.where((pa <= pb) & (pa <= pc), a, np.where(pb <= pc, b, c))
    return (row - pred) & 255

def encode_png(arr, depth, ctype, policy, level=6, palette=None, trns=None):
    """arr: uint8 or uint16 array (h, w[, ch]) already packed as PNG expects (big-endian 16-bit)."""
    h, w = arr.shape[:2]
    ch = 1 if arr.ndim == 2 else arr.shape[2]
    if depth == 16:
        raw = arr.astype('>u2').reshape(h, -1).view(np.uint8)   # big-endian bytes
    elif depth == 8:
        raw = arr.astype(np.uint8).reshape(h, -1)
    else:  # sub-byte packing
        per = 8 // depth
        flat = arr.reshape(h, w).astype(np.uint8)
        padw = (w + per - 1) // per * per
        padded = np.zeros((h, padw), np.uint8); padded[:, :w] = flat
        groups = padded.reshape(h, padw // per, per)
        shifts = np.array([8 - depth * (i + 1) for i in range(per)], np.uint8)
        raw = (groups << shifts).sum(axis=2).astype(np.uint8)
    bpp = max(1, ch * depth // 8)
    rows = raw.astype(np.int32)
    out = bytearray()
    prev = np.zeros(rows.shape[1], np.int32)
    for y in range(h):
        row = rows[y]
        if policy == 'adaptive':
            best, bestf = None, 0
            for f in range(5):
                cand = filt_row(row, prev, bpp, f)
                score = np.abs(((cand + 128) & 255) - 128).sum()   # libpng's "minimum sum of absolute differences"
                if best is None or score < best: best, bestf = score, f
            fr = filt_row(row, prev, bpp, bestf); f = bestf
        else:
            f = {'none': 0, 'sub': 1, 'up': 2, 'avg': 3, 'paeth': 4}[policy]
            fr = filt_row(row, prev, bpp, f)
        out.append(f); out += fr.astype(np.uint8).tobytes()
        prev = row
    idat = zlib.compress(bytes(out), level)
    png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, depth, ctype, 0, 0, 0))
    if palette is not None: png += chunk(b'PLTE', palette)
    if trns is not None: png += chunk(b'tRNS', trns)
    for i in range(0, len(idat), 32768):   # 32K IDAT chunks, a common encoder choice
        png += chunk(b'IDAT', idat[i:i + 32768])
    png += chunk(b'IEND', b'')
    return bytes(png)

def save(name, data, enc, w, h, colour, depth, policy):
    path = os.path.join(OUT, name + '.png')
    with open(path, 'wb') as f: f.write(data)
    manifest.append((name, enc, w, h, colour, depth, policy, len(data)))
    print(f'{name:40s} {w}x{h} {colour}{depth} {policy:8s} {len(data)//1024} KiB')

CT = {'gray': 0, 'rgb': 2, 'pal': 3, 'ga': 4, 'rgba': 6}

def up(a, k):
    return (resize(a, (a.shape[0] * k, a.shape[1] * k) + a.shape[2:], order=3, preserve_range=True, anti_aliasing=False)).round().astype(a.dtype)

photos_rgb = {n: getattr(skd, n)() for n in ['astronaut', 'cat', 'chelsea', 'coffee', 'colorwheel', 'hubble_deep_field',
                                                'immunohistochemistry', 'retina', 'rocket']}
photos_gray = {n: getattr(skd, n)() for n in ['brick', 'camera', 'cell', 'clock', 'coins', 'grass', 'gravel', 'moon', 'page', 'text']}
logo = skd.logo()

# --- RGB photos: adaptive filtering (the normal case), at sizes 1x and 2x ---
for n, a in photos_rgb.items():
    save(f'rgb8_{n}', encode_png(a, 8, CT['rgb'], 'adaptive'), 'np', a.shape[1], a.shape[0], 'rgb', 8, 'adaptive')
for n in ['astronaut', 'coffee', 'hubble_deep_field']:
    a = up(photos_rgb[n], 2)
    save(f'rgb8_{n}_2x', encode_png(a, 8, CT['rgb'], 'adaptive'), 'np', a.shape[1], a.shape[0], 'rgb', 8, 'adaptive')
a = up(photos_rgb['rocket'], 4)
save('rgb8_rocket_4x', encode_png(a, 8, CT['rgb'], 'adaptive'), 'np', a.shape[1], a.shape[0], 'rgb', 8, 'adaptive')

# --- one photo with every fixed filter policy (isolates per-filter cost) ---
for pol in ['none', 'sub', 'up', 'avg', 'paeth']:
    a = photos_rgb['hubble_deep_field']
    save(f'rgb8_hubble_{pol}', encode_png(a, 8, CT['rgb'], pol), 'np', a.shape[1], a.shape[0], 'rgb', 8, pol)
    g = photos_gray['moon']
    save(f'gray8_moon_{pol}', encode_png(g, 8, CT['gray'], pol), 'np', g.shape[1], g.shape[0], 'gray', 8, pol)

# --- gray 8 ---
for n, g in photos_gray.items():
    save(f'gray8_{n}', encode_png(g, 8, CT['gray'], 'adaptive'), 'np', g.shape[1], g.shape[0], 'gray', 8, 'adaptive')

# --- RGBA 8: the logo (real alpha) and photos with a synthetic alpha gradient ---
save('rgba8_logo', encode_png(logo, 8, CT['rgba'], 'adaptive'), 'np', logo.shape[1], logo.shape[0], 'rgba', 8, 'adaptive')
for n in ['astronaut', 'chelsea', 'retina']:
    a = photos_rgb[n]
    alpha = np.linspace(0, 255, a.shape[1]).astype(np.uint8)[None, :].repeat(a.shape[0], 0)
    rgba = np.dstack([a, alpha])
    save(f'rgba8_{n}', encode_png(rgba, 8, CT['rgba'], 'adaptive'), 'np', a.shape[1], a.shape[0], 'rgba', 8, 'adaptive')
for pol in ['sub', 'avg', 'paeth']:
    a = photos_rgb['astronaut']; rgba = np.dstack([a, np.full(a.shape[:2], 200, np.uint8)])
    save(f'rgba8_astronaut_{pol}', encode_png(rgba, 8, CT['rgba'], pol), 'np', a.shape[1], a.shape[0], 'rgba', 8, pol)

# --- gray+alpha 8 ---
g = photos_gray['camera']; ga = np.dstack([g, np.linspace(0, 255, g.shape[1]).astype(np.uint8)[None, :].repeat(g.shape[0], 0)])
save('ga8_camera', encode_png(ga, 8, CT['ga'], 'adaptive'), 'np', g.shape[1], g.shape[0], 'ga', 8, 'adaptive')

# --- 16-bit: from 8-bit photos with 16-bit noise in the low byte (so the low byte is not constant) ---
rng = np.random.default_rng(1)
for n in ['astronaut', 'rocket', 'hubble_deep_field']:
    a = photos_rgb[n].astype(np.uint16) * 257
    a = np.clip(a.astype(np.int32) + rng.integers(-200, 200, a.shape), 0, 65535).astype(np.uint16)
    save(f'rgb16_{n}', encode_png(a, 16, CT['rgb'], 'adaptive'), 'np', a.shape[1], a.shape[0], 'rgb', 16, 'adaptive')
for n in ['camera', 'moon']:
    g = photos_gray[n].astype(np.uint16) * 257
    g = np.clip(g.astype(np.int32) + rng.integers(-200, 200, g.shape), 0, 65535).astype(np.uint16)
    save(f'gray16_{n}', encode_png(g, 16, CT['gray'], 'adaptive'), 'np', g.shape[1], g.shape[0], 'gray', 16, 'adaptive')
a = logo.astype(np.uint16) * 257
save('rgba16_logo', encode_png(a, 16, CT['rgba'], 'adaptive'), 'np', a.shape[1], a.shape[0], 'rgba', 16, 'adaptive')

# --- palette 8 and sub-byte ---
for n in ['astronaut', 'coffee']:
    im = Image.fromarray(photos_rgb[n]).quantize(256, method=Image.Quantize.MEDIANCUT)
    pal = bytes(im.getpalette()[:768]); idx = np.array(im, np.uint8)
    save(f'pal8_{n}', encode_png(idx, 8, CT['pal'], 'adaptive', palette=pal), 'np', idx.shape[1], idx.shape[0], 'pal', 8, 'adaptive')
im = Image.fromarray(photos_rgb['chelsea']).quantize(16)
pal = bytes(im.getpalette()[:48]); idx = np.array(im, np.uint8)
save('pal4_chelsea', encode_png(idx, 4, CT['pal'], 'none', palette=pal), 'np', idx.shape[1], idx.shape[0], 'pal', 4, 'none')
h = skd.horse().astype(np.uint8)
save('gray1_horse', encode_png(h, 1, CT['gray'], 'none'), 'np', h.shape[1], h.shape[0], 'gray', 1, 'none')
g4 = (photos_gray['coins'] >> 4).astype(np.uint8)
save('gray4_coins', encode_png(g4, 4, CT['gray'], 'up'), 'np', g4.shape[1], g4.shape[0], 'gray', 4, 'up')

# --- Pillow-encoded variants (different encoder, different compression levels) ---
for n in ['astronaut', 'hubble_deep_field', 'retina']:
    for lvl in [1, 6, 9]:
        p = os.path.join(OUT, f'rgb8_{n}_pil{lvl}.png')
        Image.fromarray(photos_rgb[n]).save(p, compress_level=lvl)
        sz = os.path.getsize(p)
        manifest.append((f'rgb8_{n}_pil{lvl}', f'pillow-l{lvl}', photos_rgb[n].shape[1], photos_rgb[n].shape[0], 'rgb', 8, 'adaptive', sz))
        print(f'{f"rgb8_{n}_pil{lvl}":40s} pillow level {lvl} {sz//1024} KiB')

with open(os.path.join(OUT, 'manifest.tsv'), 'w') as f:
    f.write('name\tencoder\twidth\theight\tcolour\tdepth\tfilter\tbytes\n')
    for m in manifest: f.write('\t'.join(str(x) for x in m) + '\n')
print(f'{len(manifest)} files, {sum(m[-1] for m in manifest)//1024} KiB total')
