# 一次性脚本：生成 src-tauri/icons 下的应用图标。
#
# 图标是按 ark-ui「endfield」家族几何（零圆角、1px 细线、信号黄）程序化绘制的
# 原创图形，不包含任何第三方素材。运行方式：
#
#     & $PY icons/generate_icons.py
#
# 依赖：载荷自带的 Pillow（见 load_workspace_dependencies 返回的 site-packages）。

from __future__ import annotations

import struct
from pathlib import Path

from PIL import Image, ImageDraw

INK = (25, 25, 25)
PAPER = (242, 242, 240)
SIGNAL = (255, 245, 0)

OUT_DIR = Path(__file__).resolve().parent


def render(size: int) -> Image.Image:
    """绘制一枚方形部署终端标记。"""
    # 4 倍超采样后缩放，保证 1px 细线在 16px 下仍然干净。
    scale = 4
    canvas = size * scale
    image = Image.new("RGBA", (canvas, canvas), INK)
    draw = ImageDraw.Draw(image)

    unit = canvas / 32.0

    # 外框：1px 米白细线
    draw.rectangle(
        [unit * 1, unit * 1, canvas - unit * 1 - 1, canvas - unit * 1 - 1],
        outline=PAPER,
        width=max(1, int(unit * 0.36)),
    )

    # 信号黄楔形（左上切入，指向右下）
    draw.polygon(
        [
            (unit * 4.2, unit * 4.2),
            (unit * 15.4, unit * 4.2),
            (unit * 4.2, unit * 15.4),
        ],
        fill=SIGNAL,
    )

    # 一条长引导线，穿过下半区
    draw.line(
        [(unit * 4.2, unit * 24.4), (unit * 27.8, unit * 24.4)],
        fill=PAPER,
        width=max(1, int(unit * 0.36)),
    )

    # 三个校准刻度
    for index in range(3):
        x = unit * (6.4 + index * 5.6)
        draw.line(
            [(x, unit * 24.4), (x, unit * 27.4)],
            fill=SIGNAL if index == 1 else PAPER,
            width=max(1, int(unit * 0.36)),
        )

    # 下半区的炭黑填充块，让信号黄刻度更清晰
    draw.rectangle(
        [unit * 4.2, unit * 27.6, unit * 15.4, unit * 27.6 + max(1, int(unit * 0.36))],
        fill=PAPER,
    )

    return image.resize((size, size), Image.LANCZOS)


def write_ico(path: Path, images: list[Image.Image]) -> None:
    """手写 ICO 容器，内嵌 PNG 数据（Vista 及以上支持）。"""
    import io

    buffers: list[bytes] = []
    for image in images:
        buffer = io.BytesIO()
        image.save(buffer, format="PNG")
        buffers.append(buffer.getvalue())

    count = len(buffers)
    header = struct.pack("<HHH", 0, 1, count)
    offset = 6 + 16 * count

    entries = bytearray()
    for image, data in zip(images, buffers):
        width = 0 if image.width >= 256 else image.width
        height = 0 if image.height >= 256 else image.height
        entries += struct.pack(
            "<BBBBHHII",
            width,
            height,
            0,  # 调色板数量
            0,  # 保留
            1,  # 颜色平面
            32,  # 位深
            len(data),
            offset,
        )
        offset += len(data)

    path.write_bytes(header + bytes(entries) + b"".join(buffers))


def main() -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    ico_sizes = [16, 24, 32, 48, 64, 128, 256]
    images = [render(size) for size in ico_sizes]

    # 多尺寸 .ico：Windows 资源文件与打包器都使用它
    write_ico(OUT_DIR / "icon.ico", images)

    # Tauri 打包器要求的独立 PNG
    render(32).save(OUT_DIR / "32x32.png")
    render(128).save(OUT_DIR / "128x128.png")
    render(256).save(OUT_DIR / "128x128@2x.png")
    render(512).save(OUT_DIR / "icon.png")

    # Windows Store 资源（tauri 模板约定）
    for size in (30, 44, 71, 89, 107, 142, 150, 284, 310):
        render(size).save(OUT_DIR / f"Square{size}x{size}Logo.png")
    render(50).save(OUT_DIR / "StoreLogo.png")

    print(f"已生成 {len(list(OUT_DIR.glob('*.png')))} 个 PNG 与 icon.ico")


if __name__ == "__main__":
    main()
