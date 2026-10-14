#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""生成 mdedit 应用图标（多尺寸 ICO）：绿色圆角方块 + 白色 Markdown M。"""
from PIL import Image, ImageDraw, ImageFont

def make_icon(size, m=2):
    # 透明底
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    # 圆角矩形背景：深绿 -> 墨绿 垂直渐变
    r = int(size * 0.22)
    top = (46, 125, 98)     # #2e7d62
    bot = (20, 70, 55)      # #144637
    for y in range(size):
        t = y / max(size - 1, 1)
        c = tuple(int(top[i] + (bot[i] - top[i]) * t) for i in range(3)) + (255,)
        # 画一条水平的圆角矩形横带，用遮罩近似
        d.line([(0, y), (size, y)], fill=c)
    # 用圆角遮罩裁剪
    mask = Image.new("L", (size, size), 0)
    dm = ImageDraw.Draw(mask)
    dm.rounded_rectangle([0, 0, size - 1, size - 1], radius=r, fill=255)
    img.putalpha(mask)
    # 顶部高光
    hl = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    dhl = ImageDraw.Draw(hl)
    dhl.rounded_rectangle([0, 0, size - 1, int(size * 0.5)], radius=r, fill=(255, 255, 255, 26))
    dhl.rounded_rectangle([0, 0, size - 1, size - 1], radius=r, outline=(255, 255, 255, 60), width=max(1, size // 60))
    img = Image.alpha_composite(img, hl)
    # 白色 Markdown M（两条斜线 + 中间倒 V）
    d = ImageDraw.Draw(img)
    col = (255, 255, 255, 255)
    # M 由左竖、两条斜线和中间倒V构成；用多边形
    w = size * 0.56
    lx = size * 0.20
    tx = size * 0.50
    rx = size * 0.80
    yt = size * 0.28
    yb = size * 0.72
    lw = size * 0.075
    # 左竖条
    d.rectangle([lx - lw/2, yt, lx + lw/2, yb], fill=col)
    # 右竖条
    d.rectangle([rx - lw/2, yt, rx + lw/2, yb], fill=col)
    # 左斜线（从左上 M 顶部 到 中间倒V尖）
    d.polygon([(lx - lw/2, yt), (lx + lw/2, yt), (tx, yb)], fill=col)
    # 右斜线（从中间尖 到 右竖条顶部）
    d.polygon([(tx, yb), (rx - lw/2, yt), (rx + lw/2, yt)], fill=col)
    # 中间倒 V 尖（两条斜线在 yb 交汇已是尖）
    # 去掉中间底部多余：重绘中间倒 V 的上部缺口
    # 直接在最上层画两条斜线形成 M，删除竖条底部交叉多余部分已在上面
    # 简化：用粗线画 M
    d2 = ImageDraw.Draw(img)
    lw2 = size * 0.075
    # 用粗线重画 M 更清晰
    d2.line([(lx - lw2, yt), (lx - lw2, yb)], fill=col, width=int(lw2))  # 左竖
    d2.line([(rx + lw2, yt), (rx + lw2, yb)], fill=col, width=int(lw2))  # 右竖
    d2.line([(lx, yt), (tx, yb)], fill=col, width=int(lw2*1.1))         # 左斜
    d2.line([(tx, yb), (rx, yt)], fill=col, width=int(lw2*1.1))         # 右斜
    return img

# 生成多尺寸并保存 ICO
sizes = [256, 128, 64, 48, 32, 16]
imgs = [make_icon(s) for s in sizes]
imgs[0].save(
    "mdedit.ico",
    format="ICO",
    sizes=[(s, s) for s in sizes],
    append_images=imgs[1:],
)
print("icon saved, sizes:", sizes)
