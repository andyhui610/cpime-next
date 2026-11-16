"""CPIME-Next 品牌 Logo 與跨平台應用程式圖示產生器

設計概念：「鍵帽 + 粵」
  - 品牌藍漸層底（#3B82F6 → #1D4ED8，中段即品牌色 #2563EB），延續系統列藍底「粵」圖示識別。
  - 白色立體圓角鍵帽（含底部厚度與投影），鍵帽中央為深品牌藍「粵」字。
  - 鍵帽右上角琥珀色小字「A」，呼應鍵盤上滑副標記與「打英文直出中文」雙軌檢索（英文反查標籤色 #F59E0B）。
  - 16 ~ 32 px 小尺寸改用「藍底白粵」簡化版，確保工作列與檔案總管小圖示依然清晰。

所有圖形皆以 108 x 108 的 Android 自適應圖示座標系定義，同一份場景描述同時輸出：
  - Android VectorDrawable（自適應圖示 background / foreground / monochrome）
  - Android 各密度 mipmap PNG 與 Google Play 512 px 圖示
  - Windows 多尺寸 `.ico`（嵌入 `cpime-win.exe`）
  - 品牌主檔 SVG / 1024 px PNG 與預覽圖

字形來源：Noto Sans TC（SIL Open Font License 1.1，允許用於標誌與圖形作品）。
用法：
    tools/.venv/Scripts/python.exe tools/generate_icons.py [--preview PATH]
"""

from __future__ import annotations

import argparse
import math
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Union

from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont
from PIL import Image, ImageChops, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
CACHE_DIR = ROOT / "tools" / "cache"
BRANDING_DIR = ROOT / "assets" / "branding"
ANDROID_RES = ROOT / "android" / "app" / "src" / "main" / "res"
WINDOWS_ICO = ROOT / "windows" / "assets" / "cpime.ico"

FONT_SOURCES = {
    "black": (
        "NotoSansTC-Black.otf",
        "https://github.com/notofonts/noto-cjk/raw/main/Sans/SubsetOTF/TC/NotoSansTC-Black.otf",
    ),
    "bold": (
        "NotoSansTC-Bold.otf",
        "https://github.com/notofonts/noto-cjk/raw/main/Sans/SubsetOTF/TC/NotoSansTC-Bold.otf",
    ),
}

CANVAS = 108.0
CENTER = CANVAS / 2.0

# 品牌色票
BRAND_LIGHT = "#3B82F6"
BRAND = "#2563EB"
BRAND_DEEP = "#1D4ED8"
BRAND_SHADOW = "#172554"
KEY_FACE_TOP = "#FFFFFF"
KEY_FACE_BOTTOM = "#EEF2FF"
KEY_SIDE_TOP = "#C7D2FE"
KEY_SIDE_BOTTOM = "#A5B4FC"
AMBER = "#F59E0B"
WHITE = "#FFFFFF"


# ---------------------------------------------------------------------------
# 場景描述（Scene Graph）
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class Solid:
    color: str
    alpha: float = 1.0


@dataclass(frozen=True)
class Diagonal:
    """沿外框左上 → 右下的線性漸層"""

    start: str
    end: str


@dataclass(frozen=True)
class Vertical:
    """沿外框上 → 下的線性漸層"""

    start: str
    end: str


Paint = Union[Solid, Diagonal, Vertical]


@dataclass(frozen=True)
class RoundRect:
    x0: float
    y0: float
    x1: float
    y1: float
    radius: float
    paint: Paint
    stroke: float | None = None


@dataclass(frozen=True)
class Circle:
    cx: float
    cy: float
    r: float
    paint: Paint
    stroke: float | None = None


@dataclass(frozen=True)
class Glyph:
    char: str
    weight: str
    cx: float
    cy: float
    size: float  # 字形外框最長邊（畫布單位）
    paint: Solid


Shape = Union[RoundRect, Circle, Glyph]
Scene = list[Shape]


def _scaler(scale: float):
    def pos(v: float) -> float:
        return CENTER + (v - CENTER) * scale

    def length(v: float) -> float:
        return v * scale

    return pos, length


def keycap_layer(scale: float = 1.0, with_sub_label: bool = True) -> Scene:
    """白色立體鍵帽 + 深藍「粵」字 + 琥珀色「A」副標記（Android 安全區內 66dp 圓內）"""
    p, s = _scaler(scale)
    shapes: Scene = [
        RoundRect(p(29), p(33), p(79), p(82), s(11), Solid(BRAND_SHADOW, 0.30)),
        RoundRect(p(28), p(30), p(80), p(80), s(11), Vertical(KEY_SIDE_TOP, KEY_SIDE_BOTTOM)),
        RoundRect(p(28), p(26), p(80), p(76), s(11), Vertical(KEY_FACE_TOP, KEY_FACE_BOTTOM)),
        Glyph("粵", "black", p(52.6), p(52.2), s(30), Solid(BRAND_DEEP)),
    ]
    if with_sub_label:
        shapes.append(Glyph("A", "bold", p(73.0), p(32.6), s(7.0), Solid(AMBER)))
    return shapes


def scene_adaptive_background() -> Scene:
    return [RoundRect(0, 0, CANVAS, CANVAS, 0, Diagonal(BRAND_LIGHT, BRAND_DEEP))]


def scene_adaptive_foreground() -> Scene:
    return keycap_layer(1.0)


def scene_adaptive_monochrome(color: str = WHITE) -> Scene:
    return [
        RoundRect(28, 26, 80, 76, 11, Solid(color), stroke=3.2),
        Glyph("粵", "black", 52.6, 52.2, 30, Solid(color)),
    ]


def scene_tile(with_sub_label: bool = True) -> Scene:
    """圓角方塊完整圖示（Windows 48px 以上 / Android 舊版 mipmap / 品牌主檔）"""
    return [RoundRect(6, 6, 102, 102, 22, Diagonal(BRAND_LIGHT, BRAND_DEEP))] + keycap_layer(1.24, with_sub_label)


def scene_round(with_sub_label: bool = True) -> Scene:
    return [Circle(CENTER, CENTER, 48, Diagonal(BRAND_LIGHT, BRAND_DEEP))] + keycap_layer(1.12, with_sub_label)


def scene_small() -> Scene:
    """16 ~ 32 px 簡化版：藍底白「粵」"""
    return [
        RoundRect(2, 2, 106, 106, 22, Diagonal(BRAND_LIGHT, BRAND_DEEP)),
        Glyph("粵", "black", CENTER, CENTER + 0.5, 78, Solid(WHITE)),
    ]


def scene_play_store() -> Scene:
    return scene_adaptive_background() + keycap_layer(1.3)


# ---------------------------------------------------------------------------
# 字形來源
# ---------------------------------------------------------------------------


def _fmt(v: float) -> str:
    text = f"{v:.2f}".rstrip("0").rstrip(".")
    return "0" if text in ("-0", "") else text


class GlyphSource:
    def __init__(self) -> None:
        self._fonts: dict[str, TTFont] = {}
        self._paths: dict[str, Path] = {}

    def font_path(self, weight: str) -> Path:
        if weight not in self._paths:
            filename, url = FONT_SOURCES[weight]
            path = CACHE_DIR / filename
            if not path.exists():
                CACHE_DIR.mkdir(parents=True, exist_ok=True)
                print(f"[fonts] 下載 {filename} ...")
                urllib.request.urlretrieve(url, path)
            self._paths[weight] = path
        return self._paths[weight]

    def font(self, weight: str) -> TTFont:
        if weight not in self._fonts:
            self._fonts[weight] = TTFont(self.font_path(weight))
        return self._fonts[weight]

    def layout(self, g: Glyph) -> tuple[float, float, float, int]:
        """回傳 (k, origin_x, origin_y, upm)：字形單位 (x, y) → 畫布 (ox + kx, oy - ky)"""
        font = self.font(g.weight)
        glyph_set = font.getGlyphSet()
        name = font.getBestCmap()[ord(g.char)]
        bounds_pen = BoundsPen(glyph_set)
        glyph_set[name].draw(bounds_pen)
        xmin, ymin, xmax, ymax = bounds_pen.bounds
        k = g.size / max(xmax - xmin, ymax - ymin)
        ox = g.cx - k * (xmin + xmax) / 2.0
        oy = g.cy + k * (ymin + ymax) / 2.0
        return k, ox, oy, font["head"].unitsPerEm

    def svg_path(self, g: Glyph) -> str:
        font = self.font(g.weight)
        glyph_set = font.getGlyphSet()
        name = font.getBestCmap()[ord(g.char)]
        k, ox, oy, _ = self.layout(g)
        svg_pen = SVGPathPen(glyph_set, ntos=_fmt)
        glyph_set[name].draw(TransformPen(svg_pen, (k, 0, 0, -k, ox, oy)))
        return svg_pen.getCommands()


GLYPHS = GlyphSource()


# ---------------------------------------------------------------------------
# 向量輸出（SVG / Android VectorDrawable）
# ---------------------------------------------------------------------------


def rrect_path(x0: float, y0: float, x1: float, y1: float, r: float) -> str:
    if r <= 0:
        return f"M{_fmt(x0)},{_fmt(y0)}H{_fmt(x1)}V{_fmt(y1)}H{_fmt(x0)}Z"
    f = _fmt
    return (
        f"M{f(x0 + r)},{f(y0)}H{f(x1 - r)}"
        f"A{f(r)},{f(r)} 0 0 1 {f(x1)},{f(y0 + r)}V{f(y1 - r)}"
        f"A{f(r)},{f(r)} 0 0 1 {f(x1 - r)},{f(y1)}H{f(x0 + r)}"
        f"A{f(r)},{f(r)} 0 0 1 {f(x0)},{f(y1 - r)}V{f(y0 + r)}"
        f"A{f(r)},{f(r)} 0 0 1 {f(x0 + r)},{f(y0)}Z"
    )


def circle_path(cx: float, cy: float, r: float) -> str:
    f = _fmt
    return (
        f"M{f(cx - r)},{f(cy)}A{f(r)},{f(r)} 0 1 0 {f(cx + r)},{f(cy)}"
        f"A{f(r)},{f(r)} 0 1 0 {f(cx - r)},{f(cy)}Z"
    )


def shape_path(shape: Shape) -> str:
    if isinstance(shape, RoundRect):
        return rrect_path(shape.x0, shape.y0, shape.x1, shape.y1, shape.radius)
    if isinstance(shape, Circle):
        return circle_path(shape.cx, shape.cy, shape.r)
    return GLYPHS.svg_path(shape)


def shape_bbox(shape: Shape) -> tuple[float, float, float, float]:
    if isinstance(shape, RoundRect):
        return shape.x0, shape.y0, shape.x1, shape.y1
    if isinstance(shape, Circle):
        return shape.cx - shape.r, shape.cy - shape.r, shape.cx + shape.r, shape.cy + shape.r
    half = shape.size / 2.0
    return shape.cx - half, shape.cy - half, shape.cx + half, shape.cy + half


def gradient_coords(shape: Shape, paint: Paint) -> tuple[float, float, float, float]:
    x0, y0, x1, y1 = shape_bbox(shape)
    if isinstance(paint, Vertical):
        return x0, y0, x0, y1
    return x0, y0, x1, y1


def _argb(color: str, alpha: float = 1.0) -> str:
    return f"#{round(alpha * 255):02X}{color.lstrip('#').upper()}"


def to_svg(scene: Scene, pixel_size: int = 1024) -> str:
    defs: list[str] = []
    body: list[str] = []
    for idx, shape in enumerate(scene):
        d = shape_path(shape)
        paint = shape.paint
        stroke = getattr(shape, "stroke", None)
        if isinstance(paint, Solid):
            color_attr = f'"{paint.color}"'
            opacity = f' {"stroke" if stroke else "fill"}-opacity="{_fmt(paint.alpha)}"' if paint.alpha < 1 else ""
        else:
            gid = f"g{idx}"
            x1, y1, x2, y2 = gradient_coords(shape, paint)
            defs.append(
                f'    <linearGradient id="{gid}" gradientUnits="userSpaceOnUse" '
                f'x1="{_fmt(x1)}" y1="{_fmt(y1)}" x2="{_fmt(x2)}" y2="{_fmt(y2)}">\n'
                f'      <stop offset="0" stop-color="{paint.start}"/>\n'
                f'      <stop offset="1" stop-color="{paint.end}"/>\n'
                f"    </linearGradient>"
            )
            color_attr = f'"url(#{gid})"'
            opacity = ""
        if stroke:
            body.append(
                f'  <path d="{d}" fill="none" stroke={color_attr} stroke-width="{_fmt(stroke)}"{opacity}/>'
            )
        else:
            body.append(f'  <path d="{d}" fill={color_attr}{opacity}/>')
    defs_block = "  <defs>\n" + "\n".join(defs) + "\n  </defs>\n" if defs else ""
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{pixel_size}" height="{pixel_size}" '
        f'viewBox="0 0 {_fmt(CANVAS)} {_fmt(CANVAS)}">\n'
        f"  <title>CPIME-Next</title>\n"
        f"{defs_block}" + "\n".join(body) + "\n</svg>\n"
    )


def to_vector_drawable(scene: Scene) -> str:
    lines = [
        '<?xml version="1.0" encoding="utf-8"?>',
        "<!-- 由 tools/generate_icons.py 自動產生，請勿手動修改 -->",
        '<vector xmlns:android="http://schemas.android.com/apk/res/android"',
        '    xmlns:aapt="http://schemas.android.com/aapt"',
        '    android:width="108dp"',
        '    android:height="108dp"',
        f'    android:viewportWidth="{_fmt(CANVAS)}"',
        f'    android:viewportHeight="{_fmt(CANVAS)}">',
    ]
    for shape in scene:
        d = shape_path(shape)
        paint = shape.paint
        stroke = getattr(shape, "stroke", None)
        if isinstance(paint, Solid):
            if stroke:
                lines.append(
                    f'    <path\n        android:pathData="{d}"\n'
                    f'        android:strokeColor="{_argb(paint.color, paint.alpha)}"\n'
                    f'        android:strokeWidth="{_fmt(stroke)}" />'
                )
            else:
                lines.append(
                    f'    <path\n        android:pathData="{d}"\n'
                    f'        android:fillColor="{_argb(paint.color, paint.alpha)}" />'
                )
        else:
            x1, y1, x2, y2 = gradient_coords(shape, paint)
            lines += [
                f'    <path android:pathData="{d}">',
                '        <aapt:attr name="android:fillColor">',
                f'            <gradient\n                android:type="linear"\n'
                f'                android:startX="{_fmt(x1)}"\n                android:startY="{_fmt(y1)}"\n'
                f'                android:endX="{_fmt(x2)}"\n                android:endY="{_fmt(y2)}">',
                f'                <item android:offset="0" android:color="{_argb(paint.start)}" />',
                f'                <item android:offset="1" android:color="{_argb(paint.end)}" />',
                "            </gradient>",
                "        </aapt:attr>",
                "    </path>",
            ]
    lines.append("</vector>")
    return "\n".join(lines) + "\n"


# ---------------------------------------------------------------------------
# 點陣輸出（Pillow，4 倍超取樣抗鋸齒）
# ---------------------------------------------------------------------------


def _hex_rgb(color: str) -> tuple[int, int, int]:
    c = color.lstrip("#")
    return int(c[0:2], 16), int(c[2:4], 16), int(c[4:6], 16)


def _ramp(length: int, start: float, end: float, horizontal: bool) -> Image.Image:
    span = max(end - start, 1e-6)
    values = [max(0, min(255, round((i + 0.5 - start) / span * 255))) for i in range(length)]
    if horizontal:
        img = Image.new("L", (length, 1))
    else:
        img = Image.new("L", (1, length))
    img.putdata(values)
    return img.resize((length, length), Image.NEAREST)


def _paint_layer(shape: Shape, paint: Paint, w: int, unit: float) -> Image.Image:
    if isinstance(paint, Solid):
        return Image.new("RGBA", (w, w), _hex_rgb(paint.color) + (round(paint.alpha * 255),))
    x0, y0, x1, y1 = (v * unit for v in shape_bbox(shape))
    vertical = _ramp(w, y0, y1, horizontal=False)
    if isinstance(paint, Vertical):
        t = vertical
    else:
        t = ImageChops.add(vertical, _ramp(w, x0, x1, horizontal=True), scale=2.0)
    start = Image.new("RGB", (w, w), _hex_rgb(paint.start))
    end = Image.new("RGB", (w, w), _hex_rgb(paint.end))
    return Image.composite(end, start, t).convert("RGBA")


def _shape_mask(shape: Shape, w: int, unit: float) -> Image.Image:
    mask = Image.new("L", (w, w), 0)
    draw = ImageDraw.Draw(mask)
    stroke = getattr(shape, "stroke", None)
    if isinstance(shape, Glyph):
        k, ox, oy, upm = GLYPHS.layout(shape)
        font = ImageFont.truetype(str(GLYPHS.font_path(shape.weight)), size=k * upm * unit)
        draw.text((ox * unit, oy * unit), shape.char, font=font, fill=255, anchor="ls")
        return mask
    if isinstance(shape, RoundRect):
        box = [shape.x0, shape.y0, shape.x1, shape.y1]
    else:
        box = [shape.cx - shape.r, shape.cy - shape.r, shape.cx + shape.r, shape.cy + shape.r]
    if stroke:
        half = stroke / 2.0
        box = [box[0] - half, box[1] - half, box[2] + half, box[3] + half]
    px = [box[0] * unit, box[1] * unit, box[2] * unit - 1, box[3] * unit - 1]
    width = round(stroke * unit) if stroke else 0
    if isinstance(shape, RoundRect):
        radius = (shape.radius + (stroke / 2.0 if stroke else 0)) * unit
        if stroke:
            draw.rounded_rectangle(px, radius=radius, outline=255, width=width)
        else:
            draw.rounded_rectangle(px, radius=radius, fill=255)
    else:
        if stroke:
            draw.ellipse(px, outline=255, width=width)
        else:
            draw.ellipse(px, fill=255)
    return mask


def render(scene: Scene, size: int, supersample: int = 4) -> Image.Image:
    w = size * supersample
    unit = w / CANVAS
    canvas = Image.new("RGBA", (w, w), (0, 0, 0, 0))
    for shape in scene:
        layer = _paint_layer(shape, shape.paint, w, unit)
        mask = _shape_mask(shape, w, unit)
        layer.putalpha(ImageChops.multiply(mask, layer.getchannel("A")))
        canvas = Image.alpha_composite(canvas, layer)
    return canvas.resize((size, size), Image.LANCZOS)


# ---------------------------------------------------------------------------
# 輸出流程
# ---------------------------------------------------------------------------

ANDROID_DENSITIES = {"mdpi": 48, "hdpi": 72, "xhdpi": 96, "xxhdpi": 144, "xxxhdpi": 192}
ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]
SMALL_ICON_MAX = 32
SUB_LABEL_MIN = 96  # 小於此尺寸時「A」副標記只剩 2~3 px 雜點，予以省略


def raster_icon(size: int, round_shape: bool = False) -> Image.Image:
    """依尺寸自動挑選細節層級：<=32 簡化藍底白粵、<96 省略副標記、>=96 完整版"""
    if size <= SMALL_ICON_MAX and not round_shape:
        return render(scene_small(), size)
    with_sub = size >= SUB_LABEL_MIN
    return render(scene_round(with_sub) if round_shape else scene_tile(with_sub), size)

ADAPTIVE_ICON_XML = """<?xml version="1.0" encoding="utf-8"?>
<!-- 由 tools/generate_icons.py 自動產生，請勿手動修改 -->
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@drawable/ic_launcher_background" />
    <foreground android:drawable="@drawable/ic_launcher_foreground" />
    <monochrome android:drawable="@drawable/ic_launcher_monochrome" />
</adaptive-icon>
"""


def write_text(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8", newline="\n")
    print(f"  ✓ {path.relative_to(ROOT)}")


def save_png(img: Image.Image, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    img.save(path, format="PNG", optimize=True)
    print(f"  ✓ {path.relative_to(ROOT)}")


def generate_android() -> None:
    print("[android] 自適應圖示 (VectorDrawable) 與各密度 mipmap")
    write_text(ANDROID_RES / "drawable" / "ic_launcher_background.xml", to_vector_drawable(scene_adaptive_background()))
    write_text(ANDROID_RES / "drawable" / "ic_launcher_foreground.xml", to_vector_drawable(scene_adaptive_foreground()))
    write_text(ANDROID_RES / "drawable" / "ic_launcher_monochrome.xml", to_vector_drawable(scene_adaptive_monochrome()))
    write_text(ANDROID_RES / "mipmap-anydpi-v26" / "ic_launcher.xml", ADAPTIVE_ICON_XML)
    write_text(ANDROID_RES / "mipmap-anydpi-v26" / "ic_launcher_round.xml", ADAPTIVE_ICON_XML)
    for density, size in ANDROID_DENSITIES.items():
        save_png(raster_icon(size), ANDROID_RES / f"mipmap-{density}" / "ic_launcher.png")
        save_png(raster_icon(size, round_shape=True), ANDROID_RES / f"mipmap-{density}" / "ic_launcher_round.png")


def generate_windows() -> None:
    print("[windows] 多尺寸 .ico")
    frames = [raster_icon(s) for s in ICO_SIZES]
    WINDOWS_ICO.parent.mkdir(parents=True, exist_ok=True)
    largest = frames[-1]
    largest.save(WINDOWS_ICO, format="ICO", sizes=[(s, s) for s in ICO_SIZES], append_images=frames[:-1])
    print(f"  ✓ {WINDOWS_ICO.relative_to(ROOT)}")


def generate_branding() -> None:
    print("[branding] 品牌主檔")
    write_text(BRANDING_DIR / "cpime-logo.svg", to_svg(scene_tile()))
    write_text(BRANDING_DIR / "cpime-logo-small.svg", to_svg(scene_small()))
    save_png(render(scene_tile(), 1024, supersample=2), BRANDING_DIR / "cpime-logo-1024.png")
    save_png(render(scene_play_store(), 512), BRANDING_DIR / "cpime-playstore-512.png")


def _superellipse_mask(size: int, n: float = 4.0, supersample: int = 4) -> Image.Image:
    w = size * supersample
    r = w / 2.0
    pts = []
    for i in range(720):
        t = 2 * math.pi * i / 720
        c, s = math.cos(t), math.sin(t)
        x = r + r * math.copysign(abs(c) ** (2 / n), c)
        y = r + r * math.copysign(abs(s) ** (2 / n), s)
        pts.append((x, y))
    mask = Image.new("L", (w, w), 0)
    ImageDraw.Draw(mask).polygon(pts, fill=255)
    return mask.resize((size, size), Image.LANCZOS)


def _masked_adaptive(size: int, mask_kind: str, mono: bool = False) -> Image.Image:
    full = round(size * CANVAS / 72.0)
    if mono:
        bg = Image.new("RGBA", (full, full), _hex_rgb("#D3E3FD") + (255,))
        fg = render(scene_adaptive_monochrome("#0B3B8C"), full)
        composed = Image.alpha_composite(bg, fg)
    else:
        composed = Image.alpha_composite(render(scene_adaptive_background(), full), render(scene_adaptive_foreground(), full))
    offset = (full - size) // 2
    icon = composed.crop((offset, offset, offset + size, offset + size))
    if mask_kind == "circle":
        m = Image.new("L", (size * 4, size * 4), 0)
        ImageDraw.Draw(m).ellipse([0, 0, size * 4 - 1, size * 4 - 1], fill=255)
        mask = m.resize((size, size), Image.LANCZOS)
    elif mask_kind == "squircle":
        mask = _superellipse_mask(size)
    else:
        m = Image.new("L", (size * 4, size * 4), 0)
        ImageDraw.Draw(m).rounded_rectangle([0, 0, size * 4 - 1, size * 4 - 1], radius=size * 4 * 0.22, fill=255)
        mask = m.resize((size, size), Image.LANCZOS)
    icon.putalpha(ImageChops.multiply(mask, icon.getchannel("A")))
    return icon


def generate_preview(path: Path) -> None:
    print("[preview] 預覽圖")
    label_font = ImageFont.truetype(str(GLYPHS.font_path("bold")), 20)
    title_font = ImageFont.truetype(str(GLYPHS.font_path("black")), 30)
    width, row_h = 1560, 330
    sheet = Image.new("RGBA", (width, 80 + row_h * 2), (255, 255, 255, 255))
    draw = ImageDraw.Draw(sheet)
    draw.rectangle([0, 0, width, 80], fill=(20, 20, 27, 255))
    draw.text((32, 40), "CPIME-Next 應用程式圖示 · 鍵帽＋粵", font=title_font, fill=(243, 244, 246, 255), anchor="lm")

    items = [
        ("Android 圓形", _masked_adaptive(180, "circle")),
        ("Android Squircle", _masked_adaptive(180, "squircle")),
        ("Android 圓角方形", _masked_adaptive(180, "rounded")),
        ("Android 13 主題圖示", _masked_adaptive(180, "circle", mono=True)),
        ("Windows 256", render(scene_tile(), 180)),
    ]
    small_items = [(f"{s}px", raster_icon(s)) for s in (64, 48, 32, 24, 16)]

    for row, (bg, fg) in enumerate([((241, 245, 249, 255), (51, 65, 85, 255)), ((20, 20, 27, 255), (203, 213, 225, 255))]):
        top = 80 + row * row_h
        draw.rectangle([0, top, width, top + row_h], fill=bg)
        x = 40
        for label, img in items:
            sheet.alpha_composite(img, (x, top + 40))
            draw.text((x + 90, top + 250), label, font=label_font, fill=fg, anchor="mm")
            x += 220
        x += 10
        for label, img in small_items:
            y = top + 40 + (180 - img.height) // 2 + 30
            sheet.alpha_composite(img, (x, y))
            draw.text((x + img.width // 2, top + 250), label, font=label_font, fill=fg, anchor="mm")
            x += max(img.width, 40) + 26
    path.parent.mkdir(parents=True, exist_ok=True)
    sheet.convert("RGB").save(path, format="PNG")
    print(f"  ✓ {path}")


def main() -> None:
    parser = argparse.ArgumentParser(description="產生 CPIME-Next 品牌 Logo 與 Windows / Android 應用程式圖示")
    parser.add_argument("--preview", type=Path, default=CACHE_DIR / "icon_preview.png", help="預覽圖輸出路徑")
    args = parser.parse_args()
    generate_branding()
    generate_android()
    generate_windows()
    generate_preview(args.preview)


if __name__ == "__main__":
    main()
