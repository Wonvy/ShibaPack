#!/usr/bin/env python3
"""安装程序用的图片：Windows 安装程序（NSIS、MSI）的横幅和侧边图，macOS 磁盘映像（DMG）的背景。

改了样子以后重新生成：

    python3 src-tauri/installer/make-art.py [Chromium 或 Edge 的可执行文件]

用无界面的 Chromium 按准确的像素大小把下面的 HTML 截成 PNG；Windows 安装程序要的是 24 位 BMP，
这里自己把 PNG 转过去（不依赖 Pillow）。生成的图片和这个脚本放在同一个文件夹，提交进仓库，打包时直接用。
"""

from __future__ import annotations

import os
import shutil
import struct
import subprocess
import sys
import tempfile
import zlib
from pathlib import Path

HERE = Path(__file__).resolve().parent

BLUE = "#2b74d9"
BLUE_DARK = "#1a5fc4"

# 和 src-tauri/app-icon.svg 一样的标志；`box` 为真时带蓝色圆角底，假时只画白色的盒子线条（放在蓝底上）
def logo(size: int, box: bool = True) -> str:
    rect = f'<rect x="64" y="64" width="896" height="896" rx="200" fill="{BLUE}"/>' if box else ""
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" viewBox="0 0 1024 1024">{rect}
  <g fill="none" stroke="#fff" stroke-width="56" stroke-linejoin="round" stroke-linecap="round">
    <path d="M512 248 L760 380 L760 644 L512 776 L264 644 L264 380 Z"/><path d="M264 380 L512 512 L760 380"/><path d="M512 512 L512 776"/>
  </g></svg>"""


FONT = "-apple-system, 'PingFang SC', 'Segoe UI', 'Microsoft YaHei', sans-serif"
BASE = f"*{{margin:0;padding:0;box-sizing:border-box}} html,body{{width:100%;height:100%;overflow:hidden;font-family:{FONT};-webkit-font-smoothing:antialiased}}"

# 欢迎页、完成页左边的竖条：蓝色渐变，标志在上，名字在下
BRAND_PANEL = f"""
<div style="position:absolute;inset:0;background:linear-gradient(160deg,{BLUE} 0%,{BLUE_DARK} 100%)"></div>
<div style="position:absolute;left:-60px;bottom:-80px;width:260px;height:260px;border-radius:50%;background:rgba(255,255,255,.06)"></div>
<div style="position:absolute;right:-70px;top:-50px;width:200px;height:200px;border-radius:50%;background:rgba(255,255,255,.05)"></div>
<div style="position:absolute;inset:0;display:flex;flex-direction:column;align-items:center;padding-top:74px;color:#fff">
  <div style="width:84px;height:84px;border-radius:22px;background:rgba(255,255,255,.14);display:flex;align-items:center;justify-content:center">{logo(66, box=False)}</div>
  <div style="margin-top:18px;font-size:19px;font-weight:600;letter-spacing:.2px">ShibaPack</div>
  <div style="margin-top:6px;font-size:11px;opacity:.78">软件安装包管理器</div>
  <div style="margin-top:2px;font-size:10px;opacity:.62">Installer library</div>
</div>"""

ART = {
    # NSIS：欢迎页、完成页左边的图，164×314
    "nsis-sidebar.bmp": (164, 314, f"<body style='position:relative'>{BRAND_PANEL}</body>"),
    # NSIS：中间各页右上角的图，150×57，白底上放标志
    "nsis-header.bmp": (
        150,
        57,
        f"<body style='background:#fff;display:flex;align-items:center;justify-content:flex-end;padding-right:12px'>{logo(40)}</body>",
    ),
    # MSI：中间各页顶上的横幅，493×58。左边留给安装程序写标题，标志放右边
    "wix-banner.bmp": (
        493,
        58,
        f"<body style='background:#fff;display:flex;align-items:center;justify-content:flex-end;padding-right:14px'>{logo(40)}</body>",
    ),
    # MSI：欢迎页、完成页的底图，493×312。左边 164 像素是图，右边留白给安装程序写字
    "wix-dialog.bmp": (
        493,
        312,
        f"<body style='background:#fff;position:relative'><div style='position:absolute;left:0;top:0;width:164px;height:312px;overflow:hidden'>{BRAND_PANEL.replace('padding-top:74px', 'padding-top:72px')}</div></body>",
    ),
    # DMG 窗口的背景，660×400：左边是应用，右边是“应用程序”文件夹，中间一个箭头，下面一行说明
    "dmg-background.png": (
        660,
        400,
        f"""<body style="background:linear-gradient(180deg,#fbfbfc 0%,#eef1f5 100%);position:relative;color:#1b1c1f">
  <div style="position:absolute;top:34px;left:0;right:0;display:flex;align-items:center;justify-content:center;gap:10px">
    {logo(28)}<span style="font-size:18px;font-weight:600">ShibaPack</span>
  </div>
  <svg style="position:absolute;left:262px;top:168px" width="136" height="44" viewBox="0 0 136 44">
    <path d="M6 22 H118" stroke="{BLUE}" stroke-width="5" stroke-linecap="round" stroke-dasharray="1 13" fill="none" opacity=".55"/>
    <path d="M108 9 L126 22 L108 35" stroke="{BLUE}" stroke-width="5" stroke-linecap="round" stroke-linejoin="round" fill="none"/>
  </svg>
  <div style="position:absolute;left:0;right:0;bottom:44px;text-align:center">
    <div style="font-size:14px;color:#3b3e45">把 ShibaPack 拖到“应用程序”文件夹完成安装</div>
    <div style="margin-top:5px;font-size:12px;color:#8a8d95">Drag ShibaPack to the Applications folder to install</div>
  </div>
</body>""",
    ),
}


def find_chrome(arg: str | None) -> str:
    candidates = [
        arg,
        os.environ.get("CHROME"),
        *sorted(Path.home().glob("Library/Caches/ms-playwright/chromium_headless_shell-*/chrome-headless-shell-mac-*/chrome-headless-shell")),
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        shutil.which("chromium"),
        shutil.which("google-chrome"),
    ]
    for candidate in candidates:
        if candidate and Path(candidate).exists():
            return str(candidate)
    sys.exit("找不到 Chromium：把 Chrome、Edge 或 chrome-headless-shell 的路径作为参数传进来")


def render(chrome: str, width: int, height: int, html: str, out: Path, work: Path) -> None:
    page = work / f"{out.stem}.html"
    page.write_text(f"<!doctype html><meta charset='utf-8'><style>{BASE}</style>{html}", encoding="utf-8")
    subprocess.run(
        [
            chrome,
            "--headless",
            "--disable-gpu",
            "--hide-scrollbars",
            "--force-device-scale-factor=1",
            f"--user-data-dir={work / 'profile'}",
            f"--window-size={width},{height}",
            "--default-background-color=ffffffff",
            f"--screenshot={out}",
            page.as_uri(),
        ],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def read_png(path: Path) -> tuple[int, int, list[bytes]]:
    """读 8 位、不交错的 RGB / RGBA PNG，返回宽、高和每行的 RGB 字节。"""
    data = path.read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", "不是 PNG"
    pos, idat = 8, b""
    width = height = channels = 0
    while pos < len(data):
        length = struct.unpack(">I", data[pos : pos + 4])[0]
        kind = data[pos + 4 : pos + 8]
        body = data[pos + 8 : pos + 8 + length]
        if kind == b"IHDR":
            width, height, depth, color, _, _, interlace = struct.unpack(">IIBBBBB", body)
            assert depth == 8 and interlace == 0 and color in (2, 6), "只认 8 位、不交错的 RGB / RGBA"
            channels = 3 if color == 2 else 4
        elif kind == b"IDAT":
            idat += body
        pos += 12 + length
    raw = zlib.decompress(idat)
    stride = width * channels
    rows, previous = [], bytearray(stride)
    for y in range(height):
        start = y * (stride + 1)
        kind, line = raw[start], bytearray(raw[start + 1 : start + 1 + stride])
        for x in range(stride):
            left = line[x - channels] if x >= channels else 0
            up = previous[x]
            corner = previous[x - channels] if x >= channels else 0
            if kind == 1:
                line[x] = (line[x] + left) & 255
            elif kind == 2:
                line[x] = (line[x] + up) & 255
            elif kind == 3:
                line[x] = (line[x] + (left + up) // 2) & 255
            elif kind == 4:
                p = left + up - corner
                pa, pb, pc = abs(p - left), abs(p - up), abs(p - corner)
                line[x] = (line[x] + (left if pa <= pb and pa <= pc else up if pb <= pc else corner)) & 255
        previous = line
        rgb = bytearray()
        for x in range(width):
            r, g, b = line[x * channels : x * channels + 3]
            # 截图的背景是不透明白色，有透明度也按白底混合
            a = line[x * channels + 3] if channels == 4 else 255
            rgb += bytes(((c * a + 255 * (255 - a)) // 255) for c in (r, g, b))
        rows.append(bytes(rgb))
    return width, height, rows


def write_bmp(path: Path, width: int, height: int, rows: list[bytes]) -> None:
    """24 位、从下往上存的 BMP（Windows 安装程序认这个）。"""
    pad = (4 - (width * 3) % 4) % 4
    # 每个像素 RGB → BGR，每行补齐到 4 字节
    pixels = b"".join(b"".join(row[i : i + 3][::-1] for i in range(0, len(row), 3)) + b"\0" * pad for row in reversed(rows))
    header = struct.pack("<2sIHHI", b"BM", 54 + len(pixels), 0, 0, 54)
    info = struct.pack("<IiiHHIIiiII", 40, width, height, 1, 24, 0, len(pixels), 2835, 2835, 0, 0)
    path.write_bytes(header + info + pixels)


def main() -> None:
    chrome = find_chrome(sys.argv[1] if len(sys.argv) > 1 else None)
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        for name, (width, height, html) in ART.items():
            out = HERE / name
            png = work / f"{out.stem}.png"
            render(chrome, width, height, html, png, work)
            w, h, rows = read_png(png)
            assert (w, h) == (width, height), f"{name}：截出来是 {w}×{h}，要的是 {width}×{height}"
            if out.suffix == ".bmp":
                write_bmp(out, w, h, rows)
            else:
                shutil.copyfile(png, out)
            print(f"{name}  {w}×{h}")


if __name__ == "__main__":
    main()
