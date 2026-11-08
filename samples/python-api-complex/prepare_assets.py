"""Generate original placeholder artwork/audio using Python's standard library."""
from pathlib import Path
import argparse
import json
import math
import struct
import wave
import zlib


def png(path, width, height, pixel):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    data = b"".join(b"\x00" + bytes(v for x in range(width) for v in pixel(x, y)) for y in range(height))
    path.write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
                     + chunk(b"IDAT", zlib.compress(data)) + chunk(b"IEND", b""))


def create(output):
    output = Path(output).resolve()
    if output.exists():
        raise FileExistsError(f"Choose a new asset directory: {output}")
    output.mkdir(parents=True)
    assets = {}
    def content(kind, ident, name, files, extra=None):
        folder = output / kind / ident
        folder.mkdir(parents=True)
        meta = {"schema": 1, "content_schema": 1, "id": ident, "type": kind, "name": name, "files": files, **(extra or {})}
        (folder / "content.json").write_text(json.dumps(meta, ensure_ascii=False, indent=2), encoding="utf-8")
        return folder
    for ident, name, palette in [("mist.harbor", "雾渡夜港", (20, 38, 60)), ("mist.tower", "旧灯塔", (38, 27, 52)), ("mist.letter", "浸水账单", (175, 151, 110))]:
        folder = content("image", ident, name, {"main": "image.png"})
        def image(x, y, palette=palette, ident=ident):
            if ident == "mist.letter":
                return (55, 42, 34, 255) if 65 < x < 575 and y in range(80, 280) and y % 32 < 3 else (*palette, 255)
            if 450 < x < 495 and 90 < y < 260:
                return (244, 207, 114, 255) if y < 120 else (65, 60, 68, 255)
            if y > 260 and (y + x // 15) % 29 < 2:
                return (83, 125, 146, 255)
            return (*(max(0, min(255, v + int(16 * y / 360))) for v in palette), 255)
        png(folder / "image.png", 640, 360, image)
    folder = content("character", "mist.shen", "沈砚", {"main": "normal.png"}, {"portraits": {p: f"{p}.png" for p in ["normal", "happy", "angry"]}, "scale": 100, "art_facing": "left", "mood_x": 50, "mood_y": 90})
    for portrait, coat in [("normal", (45, 86, 99)), ("happy", (56, 116, 97)), ("angry", (103, 61, 77))]:
        def character(x, y, coat=coat):
            if (x - 100) ** 2 + (y - 64) ** 2 < 35 ** 2:
                if y < 44:
                    return (34, 30, 38, 255)
                if 64 < y < 69 and (82 < x < 91 or 108 < x < 117):
                    return (28, 27, 32, 255)
                return (221, 183, 150, 255)
            if 95 < y < 350 and abs(x - 100) < 30 + (y - 95) * .18:
                return (*coat, 255)
            return (0, 0, 0, 0)
        png(folder / f"{portrait}.png", 200, 360, character)
    for ident, name, kind, seconds, frequency in [("mist.theme", "雾港慢调", "music", 4, 220), ("mist.bell", "引航钟", "sound", .5, 660)]:
        folder = content("audio", ident, name, {"main": "audio.wav"}, {"audio_kind": kind})
        with wave.open(str(folder / "audio.wav"), "wb") as wav:
            wav.setparams((1, 2, 22050, 0, "NONE", "not compressed"))
            wav.writeframes(b"".join(struct.pack("<h", int(6000 * math.sin(2 * math.pi * frequency * n / 22050) * (1 - n / (seconds * 22050)))) for n in range(int(seconds * 22050))))
    for path in output.rglob("*"):
        if path.is_file():
            assets["assets/user/" + path.relative_to(output).as_posix()] = str(path)
    (output / "asset-map.json").write_text(json.dumps(assets, ensure_ascii=False, indent=2), encoding="utf-8")
    return assets


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    print(json.dumps(create(args.output), ensure_ascii=False))
