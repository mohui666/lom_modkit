"""Read sprite dimensions for displaying legacy cropped portrait previews."""
import argparse
from functools import lru_cache
import json
from pathlib import Path

import UnityPy
from UnityPy.enums import ClassIDType, SpritePackingMode, SpritePackingRotation
from UnityPy.export.SpriteHelper import SpriteSettings
from UnityPy.helpers.MeshHelper import MeshHandler

import _probe_assets as probe


def geometry(sprite):
    atlas = None
    if sprite.m_SpriteAtlas:
        atlas = sprite.m_SpriteAtlas.read()
    elif sprite.m_AtlasTags:
        for obj in sprite.assets_file.objects.values():
            if obj.type == ClassIDType.SpriteAtlas:
                candidate = obj.read()
                if candidate.m_Name == sprite.m_AtlasTags[0]:
                    atlas = candidate
                    break
    data = (next(value for key, value in atlas.m_RenderDataMap if key == sprite.m_RenderDataKey)
            if atlas else sprite.m_RD)
    rect = data.textureRect
    width = round(rect.x + rect.width) - round(rect.x)
    height = round(rect.y + rect.height) - round(rect.y)
    settings = SpriteSettings(data.settingsRaw)
    if settings.packed and settings.packingRotation == SpritePackingRotation.kSPRRotate90:
        width, height = height, width
    if settings.packingMode == SpritePackingMode.kSPMTight:
        mesh = MeshHandler(sprite.m_RD, sprite.object_reader.version)
        mesh.process()
        if mesh.m_UV0 and any(u or v for u, v in mesh.m_UV0):
            width, height = sprite.image.size
    canvas = [round(sprite.m_Rect.width), round(sprite.m_Rect.height)]
    return {"canvas": canvas,
            "image": [round(data.textureRectOffset.x),
                      canvas[1] - round(data.textureRectOffset.y) - height,
                      width, height]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--game-dir", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    game_data = args.game_dir.resolve(strict=True) / "Mortal_Data"
    probe.SA2 = str(game_data / "sharedassets2.assets")
    addresses = probe.parse_catalog(str(game_data / "StreamingAssets/aa/catalog.json"))
    characters, _ = probe.load_sa2()
    bundles = Path(probe.bundle_directory(game_data))

    @lru_cache(maxsize=2)
    def bundle(name):
        return UnityPy.load(str(bundles / name))

    @lru_cache(maxsize=None)
    def sprite_geometry(address):
        env = bundle(addresses[address])
        sprite = probe.resolve_sprite(env, addresses.asset_paths.get(address, address)).read()
        return geometry(sprite)

    result = {}
    for cid, character in characters.items():
        portraits = dict(character["portraits"])
        portraits.setdefault("normal", character["first"])
        result[cid] = {emotion: sprite_geometry(address) for emotion, address in portraits.items()}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"characters": len(result), "portraits": sum(map(len, result.values())),
                      "output": str(args.output)}, ensure_ascii=False))


if __name__ == "__main__":
    main()
