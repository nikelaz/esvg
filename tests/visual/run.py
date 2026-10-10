#!/usr/bin/env python3
"""End-to-end visual regression harness for esvg-rs.

For every SVG in the corpus directories this script:
  1. runs the optimizer binary (the full default pipeline),
  2. renders the original and the optimized SVG with two independent
     renderers (resvg and rsvg-convert) at a fixed width,
  3. decodes both PNGs (standard library only) and compares them pixel by
     pixel over a white background,
  4. reports byte sizes and visual differences, and exits non-zero if any
     file fails to optimize, fails to render, or looks visibly different.

Only the Python standard library is used. The renderers must be on PATH.

Usage:
    tests/visual/run.py [--width 256] [--bin target/release/esvg-rs] [files...]
"""

import argparse
import glob
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
CORPUS_DIRS = [os.path.join(ROOT, "tests", "cases"), os.path.join(ROOT, "tests", "corpus")]

# A pixel counts as "different" when its composited RGB channels differ by more
# than this (out of 255). Anti-aliasing noise between renderers is well below it.
PIXEL_TOLERANCE = 24
# A file passes when at most this fraction of pixels differ.
MAX_DIFF_FRACTION = 0.005


def read_png(path):
    """Decode an 8-bit, non-interlaced PNG into (width, height, RGBA bytes)."""
    with open(path, "rb") as f:
        data = f.read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("not a PNG")
    pos = 8
    idat = []
    width = height = bit_depth = color_type = interlace = None
    while pos < len(data):
        length, = struct.unpack(">I", data[pos:pos + 4])
        ctype = data[pos + 4:pos + 8]
        chunk = data[pos + 8:pos + 8 + length]
        pos += 12 + length
        if ctype == b"IHDR":
            width, height, bit_depth, color_type, _, _, interlace = struct.unpack(">IIBBBBB", chunk)
        elif ctype == b"IDAT":
            idat.append(chunk)
        elif ctype == b"IEND":
            break
    if bit_depth != 8 or interlace != 0:
        raise ValueError("unsupported PNG layout")
    channels = {0: 1, 2: 3, 4: 2, 6: 4}[color_type]
    bpp = channels
    stride = width * channels
    raw = zlib.decompress(b"".join(idat))

    rows = []
    prev = bytearray(stride)
    p = 0
    for _ in range(height):
        ftype = raw[p]
        line = bytearray(raw[p + 1:p + 1 + stride])
        p += 1 + stride
        if ftype == 1:
            for i in range(bpp, stride):
                line[i] = (line[i] + line[i - bpp]) & 0xFF
        elif ftype == 2:
            for i in range(stride):
                line[i] = (line[i] + prev[i]) & 0xFF
        elif ftype == 3:
            for i in range(stride):
                left = line[i - bpp] if i >= bpp else 0
                line[i] = (line[i] + ((left + prev[i]) >> 1)) & 0xFF
        elif ftype == 4:
            for i in range(stride):
                a = line[i - bpp] if i >= bpp else 0
                b = prev[i]
                c = prev[i - bpp] if i >= bpp else 0
                pa = abs(b - c)
                pb = abs(a - c)
                pc = abs(a + b - 2 * c)
                if pa <= pb and pa <= pc:
                    pred = a
                elif pb <= pc:
                    pred = b
                else:
                    pred = c
                line[i] = (line[i] + pred) & 0xFF
        rows.append(line)
        prev = line

    rgba = bytearray(width * height * 4)
    o = 0
    for line in rows:
        for x in range(width):
            i = x * channels
            if color_type == 6:
                rgba[o:o + 4] = line[i:i + 4]
            elif color_type == 2:
                rgba[o:o + 4] = bytes((line[i], line[i + 1], line[i + 2], 255))
            elif color_type == 4:
                rgba[o:o + 4] = bytes((line[i], line[i], line[i], line[i + 1]))
            else:
                rgba[o:o + 4] = bytes((line[i], line[i], line[i], 255))
            o += 4
    return width, height, rgba


def composite_over_white(rgba):
    out = bytearray(len(rgba) // 4 * 3)
    j = 0
    for i in range(0, len(rgba), 4):
        a = rgba[i + 3]
        inv = 255 - a
        out[j] = (rgba[i] * a + 255 * inv) // 255
        out[j + 1] = (rgba[i + 1] * a + 255 * inv) // 255
        out[j + 2] = (rgba[i + 2] * a + 255 * inv) // 255
        j += 3
    return out


def compare_pngs(path_a, path_b):
    wa, ha, a = read_png(path_a)
    wb, hb, b = read_png(path_b)
    if (wa, ha) != (wb, hb):
        return {"size_mismatch": (wa, ha, wb, hb), "diff_fraction": 1.0, "max_diff": 255}
    ca = composite_over_white(a)
    cb = composite_over_white(b)
    total = wa * ha
    bad = 0
    worst = 0
    for i in range(0, len(ca), 3):
        d = max(abs(ca[i] - cb[i]), abs(ca[i + 1] - cb[i + 1]), abs(ca[i + 2] - cb[i + 2]))
        if d > worst:
            worst = d
        if d > PIXEL_TOLERANCE:
            bad += 1
    return {"diff_fraction": bad / total if total else 0.0, "max_diff": worst}


def render(renderer, svg_path, png_path, width):
    if renderer == "resvg":
        cmd = ["resvg", "-w", str(width), svg_path, png_path]
    else:
        cmd = ["rsvg-convert", "-w", str(width), "-o", png_path, svg_path]
    res = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
    if res.returncode != 0 or not os.path.exists(png_path):
        raise RuntimeError(f"{renderer} failed: {res.stderr.strip()[:300]}")


def optimize(binary, src, dst):
    res = subprocess.run([binary, src, dst], capture_output=True, text=True, timeout=300)
    if res.returncode != 0:
        raise RuntimeError(f"optimizer failed: {res.stderr.strip()[:300] or 'exit ' + str(res.returncode)}")


def collect(paths):
    if paths:
        return sorted(os.path.abspath(p) for p in paths)
    files = []
    for d in CORPUS_DIRS:
        files += glob.glob(os.path.join(d, "*.svg"))
    return sorted(files)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("files", nargs="*")
    ap.add_argument("--width", type=int, default=256)
    ap.add_argument("--bin", default=os.path.join(ROOT, "target", "release", "esvg-rs"))
    args = ap.parse_args()

    for tool in ("resvg", "rsvg-convert"):
        if shutil.which(tool) is None:
            print(f"missing renderer on PATH: {tool}", file=sys.stderr)
            return 2
    if not os.path.exists(args.bin):
        print(f"optimizer binary not found: {args.bin} (run cargo build --release)", file=sys.stderr)
        return 2

    files = collect(args.files)
    work = tempfile.mkdtemp(prefix="esvg-visual-")
    rows = []
    failures = 0

    header = f"{'file':<46} {'orig':>8} {'opt':>8} {'saved':>7}  {'resvg':>8} {'rsvg':>8}  verdict"
    print(header)
    print("-" * len(header))

    for src in files:
        name = os.path.relpath(src, ROOT)
        base = os.path.basename(src)
        opt = os.path.join(work, base)
        orig_size = os.path.getsize(src)
        try:
            optimize(args.bin, src, opt)
        except Exception as e:  # noqa: BLE001 - report every failure, keep going
            failures += 1
            print(f"{name:<46} {orig_size:>8} {'-':>8} {'-':>7}  {'-':>8} {'-':>8}  OPTIMIZE FAILED: {e}")
            continue

        opt_size = os.path.getsize(opt)
        saved = 100.0 * (orig_size - opt_size) / orig_size if orig_size else 0.0

        diffs = {}
        error = None
        for renderer in ("resvg", "rsvg-convert"):
            try:
                png_a = os.path.join(work, f"a-{renderer}-{base}.png")
                png_b = os.path.join(work, f"b-{renderer}-{base}.png")
                render(renderer, src, png_a, args.width)
                render(renderer, opt, png_b, args.width)
                diffs[renderer] = compare_pngs(png_a, png_b)
            except Exception as e:  # noqa: BLE001
                error = f"{renderer}: {e}"
                break

        if error:
            failures += 1
            print(f"{name:<46} {orig_size:>8} {opt_size:>8} {saved:>6.1f}%  {'-':>8} {'-':>8}  RENDER FAILED: {error}")
            continue

        worst = max(d["diff_fraction"] for d in diffs.values())
        ok = worst <= MAX_DIFF_FRACTION
        if not ok:
            failures += 1
        verdict = "ok" if ok else "VISUAL DIFF"
        r = diffs["resvg"]["diff_fraction"] * 100
        s = diffs["rsvg-convert"]["diff_fraction"] * 100
        print(f"{name:<46} {orig_size:>8} {opt_size:>8} {saved:>6.1f}%  {r:>7.3f}% {s:>7.3f}%  {verdict}")
        rows.append((name, orig_size, opt_size, saved, r, s))

    if rows:
        total_o = sum(r[1] for r in rows)
        total_n = sum(r[2] for r in rows)
        print("-" * len(header))
        print(f"{'TOTAL (' + str(len(rows)) + ' files)':<46} {total_o:>8} {total_n:>8} "
              f"{100.0 * (total_o - total_n) / total_o:>6.1f}%")

    print(f"\nrender width: {args.width}px, pixel tolerance: {PIXEL_TOLERANCE}/255, "
          f"max differing pixels: {MAX_DIFF_FRACTION * 100:.2f}%")
    print(f"files: {len(files)}, failures: {failures}")
    shutil.rmtree(work, ignore_errors=True)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
