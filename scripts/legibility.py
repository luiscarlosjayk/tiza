#!/usr/bin/env python3
"""Legibility at 1080p after YouTube-like compression (t-072).

Pipeline: `tiza render` → PNG → scale to 1920×1080 (letterboxed) → H.264
at a YouTube-like bitrate (ffmpeg) → decode the frame → for every text color
the theme uses, compare the pixels that had that color before compression
with what came back, and report the WCAG contrast against the background.

    python3 scripts/legibility.py examples/ownership.rs.tiza --step 7
    python3 scripts/legibility.py doc.tiza --theme light --min 4.5 --json report.json

Exit code 1 when any measured role falls under --min. Needs python3 with
Pillow and numpy, ffmpeg on PATH, and the tiza binary (--bin or
target/debug/tiza).
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile

import numpy as np
from PIL import Image


def srgb_to_linear(c):
    c = c / 255.0
    return np.where(c <= 0.03928, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def luminance(rgb):
    r, g, b = (srgb_to_linear(np.asarray(rgb, dtype=float))[i] for i in range(3))
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def contrast(a, b):
    la, lb = luminance(a) + 0.05, luminance(b) + 0.05
    return la / lb if la > lb else lb / la


def run(cmd, **kw):
    return subprocess.run(cmd, check=True, capture_output=True, text=True, **kw)


def render(binary, doc, step, theme, width, png):
    args = [binary, "render", doc, "--out", png, "--width", str(width), "--scale", "2"]
    if step is not None:
        args += ["--step", str(step)]
    if theme:
        args += ["--theme", theme]
    run(args)


def youtube_like(src_png, dst_png, bitrate):
    """Scale to 1080p and round-trip through H.264 at `bitrate` (kbit/s)."""
    with tempfile.TemporaryDirectory() as td:
        mp4 = os.path.join(td, "clip.mp4")
        # 2 s of a still frame: the encoder spends its budget like on a slow screen share.
        run([
            "ffmpeg", "-y", "-loglevel", "error", "-loop", "1", "-t", "2", "-i", src_png,
            "-vf", "scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2,format=yuv420p",
            "-c:v", "libx264", "-preset", "medium", "-b:v", f"{bitrate}k", "-maxrate", f"{bitrate}k",
            "-bufsize", f"{bitrate * 2}k", "-g", "60", "-r", "30", mp4,
        ])
        run(["ffmpeg", "-y", "-loglevel", "error", "-i", mp4, "-vf", "select=eq(n\\,30)", "-frames:v", "1", dst_png])


def fit_like_ffmpeg(img):
    """The same scale + letterbox ffmpeg applied, so pixels line up."""
    w, h = img.size
    scale = min(1920 / w, 1080 / h)
    nw, nh = round(w * scale), round(h * scale)
    out = Image.new("RGB", (1920, 1080), (0, 0, 0))
    out.paste(img.resize((nw, nh), Image.BICUBIC), ((1920 - nw) // 2, (1080 - nh) // 2))
    return out


def roles_from_svg(binary, doc, step, theme, width):
    """Every text color the export uses: the distinct `fill` values of the
    SVG's <text> runs (the layout decides them; exporters share the canvas)."""
    import re
    args = [binary, "render", doc, "--out", "-", "--format", "svg", "--no-embed-fonts", "--width", str(width)]
    if step is not None:
        args += ["--step", str(step)]
    if theme:
        args += ["--theme", theme]
    svg = run(args).stdout
    fills = set(re.findall(r'<text [^>]*fill="(#[0-9a-f]{6})"', svg))
    return sorted(fills)


def hex_rgb(h):
    return np.array([int(h[1:3], 16), int(h[3:5], 16), int(h[5:7], 16)], dtype=np.int16)


def measure(before_path, after_path, bg, roles, min_pixels=40, tolerance=10):
    """For every role color: the pixels that held it (within `tolerance`, so
    anti-aliased shades count) before compression, and the contrast of what
    those pixels became against what the background became."""
    before = np.asarray(fit_like_ffmpeg(Image.open(before_path).convert("RGB")), dtype=np.int16)
    after = np.asarray(Image.open(after_path).convert("RGB"), dtype=np.int16)
    assert before.shape == after.shape, (before.shape, after.shape)
    bg = np.asarray(bg, dtype=np.int16)
    bg_after = after[np.all(np.abs(before - bg) < 8, axis=2)].mean(axis=0)
    report = []
    for h in roles:
        color = hex_rgb(h)
        c_before = contrast(color, bg)
        if c_before < 1.5:
            continue  # tints and fills, not text
        mask = np.all(np.abs(before - color) <= tolerance, axis=2)
        # Text cores: pixels whose 4 neighbours also carry the color survive
        # scaling; edges are blended with the background by design.
        core = mask.copy()
        core[1:, :] &= mask[:-1, :]
        core[:-1, :] &= mask[1:, :]
        core[:, 1:] &= mask[:, :-1]
        core[:, :-1] &= mask[:, 1:]
        sel = core if core.sum() >= min_pixels else mask
        if sel.sum() < min_pixels:
            continue  # the role is not drawn in this frame
        # Thin strokes blur into the background at the edges; what the eye
        # reads is the stroke core. Take the mean of the quarter of the
        # pixels that kept the most contrast (per-pixel luminance distance).
        px = after[sel]
        lum = 0.2126 * srgb_to_linear(px[:, 0]) + 0.7152 * srgb_to_linear(px[:, 1]) + 0.0722 * srgb_to_linear(px[:, 2])
        bg_lum = luminance(bg_after)
        order = np.argsort(-np.abs(lum - bg_lum))
        top = px[order[: max(min_pixels // 4, len(order) // 4)]]
        got = top.mean(axis=0)
        report.append({
            "color": h,
            "pixels": int(sel.sum()),
            "contrast_before": round(float(c_before), 2),
            "contrast_after": round(float(contrast(got, bg_after)), 2),
            "color_after": "#%02x%02x%02x" % tuple(int(round(v)) for v in got),
        })
    report.sort(key=lambda r: r["contrast_after"])
    return report


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("document")
    ap.add_argument("--step", type=int)
    ap.add_argument("--theme", choices=["dark", "light"])
    ap.add_argument("--width", type=int, default=100)
    ap.add_argument("--bitrate", type=int, default=8000, help="kbit/s (YouTube 1080p30 ≈ 8000)")
    ap.add_argument("--min", type=float, default=4.5, help="minimum contrast after compression")
    ap.add_argument("--bin", default=os.environ.get("TIZA_BIN", "target/debug/tiza"))
    ap.add_argument("--json")
    ap.add_argument("--keep", help="directory to keep the PNGs in")
    a = ap.parse_args()
    if not shutil.which("ffmpeg"):
        print("ffmpeg not found", file=sys.stderr)
        return 2
    out_dir = a.keep or tempfile.mkdtemp(prefix="tiza-legibility-")
    os.makedirs(out_dir, exist_ok=True)
    before = os.path.join(out_dir, "before.png")
    after = os.path.join(out_dir, "after.png")
    render(a.bin, a.document, a.step, a.theme, a.width, before)
    youtube_like(before, after, a.bitrate)
    img = Image.open(before).convert("RGB")
    bg = img.getpixel((img.width // 2, 4))  # inside the margin, away from the rounded corners
    roles = roles_from_svg(a.bin, a.document, a.step, a.theme, a.width)
    report = measure(before, after, bg, roles)
    worst = min((r["contrast_after"] for r in report), default=0)
    result = {
        "document": a.document, "step": a.step, "theme": a.theme or "document", "bitrate_kbps": a.bitrate,
        "background": "#%02x%02x%02x" % bg, "roles": report, "worst": worst, "min": a.min,
        "pass": worst >= a.min, "frames": {"before": before, "after": after},
    }
    if a.json:
        with open(a.json, "w") as f:
            json.dump(result, f, indent=1)
    print(f"theme {result['theme']}  bg {result['background']}  {a.bitrate} kbit/s  →  worst contrast {worst} (min {a.min})  {'OK' if result['pass'] else 'FAIL'}")
    for r in report:
        flag = "" if r["contrast_after"] >= a.min else "  <-- under"
        print(f"  {r['color']} → {r['color_after']}  {r['contrast_before']:>5} → {r['contrast_after']:>5}  ({r['pixels']} px){flag}")
    return 0 if result["pass"] else 1


if __name__ == "__main__":
    sys.exit(main())
