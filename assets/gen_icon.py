"""Generates the Aegis app icon — an indigo shield with a white 'A' — at high resolution,
then writes a multi-size Windows .ico and a 256px .png. Reproducible: `python assets/gen_icon.py`.
Requires Pillow (`pip install pillow`). Palette matches the app theme (#7e97ff accent)."""
from PIL import Image, ImageDraw

SS = 8                      # supersampling factor (anti-aliasing via downscale)
N = 256                     # final master size
S = N * SS

ACCENT = (126, 151, 255, 255)   # #7e97ff
ACCENT_DK = (90, 112, 214, 255) # darker indigo for the lower bevel
WHITE = (240, 243, 248, 255)

img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(img)


def _bez(p0, c, p1, steps):
    out = []
    for i in range(steps + 1):
        t = i / steps
        out.append((
            (1 - t) ** 2 * p0[0] + 2 * (1 - t) * t * c[0] + t * t * p1[0],
            (1 - t) ** 2 * p0[1] + 2 * (1 - t) * t * c[1] + t * t * p1[1],
        ))
    return out


def shield_pts(cx, w, top, shoulder_y, tip_y, rad, steps=14):
    """Heraldic shield: rounded top corners (widest), sides staying vertical then curving
    (quadratic Bézier) to a soft point — a tall body with a gentle tip."""
    import math
    left, right = cx - w / 2, cx + w / 2
    pts = []
    tlx, tly = left + rad, top + rad          # top-left corner arc (180° → 270°)
    for i in range(steps + 1):
        a = math.pi + (i / steps) * (math.pi / 2)
        pts.append((tlx + rad * math.cos(a), tly + rad * math.sin(a)))
    trx, tryy = right - rad, top + rad         # top-right corner arc (270° → 360°)
    for i in range(steps + 1):
        a = -math.pi / 2 + (i / steps) * (math.pi / 2)
        pts.append((trx + rad * math.cos(a), tryy + rad * math.sin(a)))
    ctrl_y = shoulder_y + 0.62 * (tip_y - shoulder_y)
    pts += _bez((right, shoulder_y), (right, ctrl_y), (cx, tip_y), steps)   # right → tip
    pts += _bez((cx, tip_y), (left, ctrl_y), (left, shoulder_y), steps)     # tip → left
    return pts


cx = S / 2
W, TOP, SHOULDER, TIP, RAD = 0.62 * S, 0.15 * S, 0.50 * S, 0.83 * S, 0.10 * S
# Darker shield offset slightly down → a thin bottom bevel for depth.
d.polygon(shield_pts(cx, W, TOP + 0.012 * S, SHOULDER + 0.012 * S, TIP + 0.012 * S, RAD), fill=ACCENT_DK)
d.polygon(shield_pts(cx, W, TOP, SHOULDER, TIP, RAD), fill=ACCENT)

# White "A" monogram, drawn as three thick strokes with rounded joints.
apex = (cx, 0.30 * S)
lfoot = (0.40 * S, 0.585 * S)
rfoot = (0.60 * S, 0.585 * S)
lw = int(0.052 * S)
d.line([apex, lfoot], fill=WHITE, width=lw, joint="curve")
d.line([apex, rfoot], fill=WHITE, width=lw, joint="curve")
# Crossbar a bit below the middle of the legs.
bar_y = 0.50 * S
bar_l = (0.443 * S, bar_y)
bar_r = (0.557 * S, bar_y)
d.line([bar_l, bar_r], fill=WHITE, width=int(lw * 0.85))

master = img.resize((N, N), Image.LANCZOS)
master.save("assets/aegis.png")
master.save("assets/aegis.ico", sizes=[(256, 256), (64, 64), (48, 48), (32, 32), (16, 16)])
print("wrote assets/aegis.png and assets/aegis.ico")
