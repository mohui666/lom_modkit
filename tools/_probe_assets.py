#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""临时探测脚本：验证演出预览所需的立绘/背景图解包链路。

链路（依据反编译代码）：
- CharacterPlaceholder.LoadCharacterAsset -> StoryCharacterConfig.Get(id)
  -> StoryCharacterData.PortraitResourceList: [{Mapping.Value=表情名, AddressKey=立绘地址}]
  -> Addressables.LoadAssetAsync<Sprite>(addressKey)   (Mortal.Story.decompiled.cs:231-271, 2663-2678)
- ViewFlowchartController.LoadView -> StoryViewImage.LoadAsset(view名)
  -> AddressableCollectionData(_StoryViewData).GetByKey -> AddressKey -> Sprite
  (Mortal.Story.decompiled.cs:466-523, 5317-5374)；black/white 为硬编码纯色，无图片。

配置数据序列化在 Mortal_Data/sharedassets2.assets（无 typetree，按字段布局手工解析）：
- StoryCharacterConfig(m_Name='StoryCharacterConfig'): _list = PPtr[] -> StoryCharacterData
- StoryCharacterData: m_Name | _moodPosition(Vector2) | _mapping(PPtr->StoryMappingItem, Value=人物id)
  | _portraitResourceList: count * { _mapping(PPtr->StoryMappingItem, Value=表情名), _addressKey(string) }
- AddressableCollectionData(m_Name='_StoryViewData'): _data = PPtr[] -> AddressableData
- AddressableData: m_Name | _key(=view名) | _addressKey
- StoryMappingItem: m_Name | Key | Value

图片实体在 Addressables bundle（StreamingAssets/aa/StandaloneWindows/*.bundle）。
catalog.json 提供 addressKey -> bundle 映射；addressKey 可能是完整资源路径
（Assets/__Project/Images/...）也可能是短 key（如 pic_look_001）。bundle 内
Sprite 名为内部资源文件名去扩展名，UnityPy sprite.image 已按 m_Rect 裁切。
"""
import base64
import json
import os
import struct
import sys
import io

import UnityPy

sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")

GAME_DATA = r"C:/Program Files (x86)/Steam/steamapps/common/LegendOfMortal/Mortal_Data"
CATALOG = os.path.join(GAME_DATA, "StreamingAssets", "aa", "catalog.json")
BUNDLE_DIR = os.path.join(GAME_DATA, "StreamingAssets", "aa", "StandaloneWindows")
SA2 = os.path.join(GAME_DATA, "sharedassets2.assets")
EDITOR_DATA = r"C:/Users/mohui666/lom_modkit/data/editor_data.json"
OUT_DIR = r"C:/Users/mohui666/lom_modkit/data/assets/_probe"


def parse_catalog(path):
    """catalog.json -> {addressKey: bundle文件名}。二进制段格式参考 AddressablesToolsPy。"""
    cat = json.load(open(path, encoding="utf-8"))
    kd = base64.b64decode(cat["m_KeyDataString"])
    bd = base64.b64decode(cat["m_BucketDataString"])
    ed = base64.b64decode(cat["m_EntryDataString"])

    buckets = []
    off = 4
    for _ in range(struct.unpack_from("<i", bd, 0)[0]):
        koff, ecnt = struct.unpack_from("<ii", bd, off)
        off += 8
        ents = struct.unpack_from("<%di" % ecnt, bd, off)
        off += 4 * ecnt
        buckets.append((koff, ents))

    def read_key(koff):
        t = kd[koff]
        if t in (0, 1):  # 0=ascii 1=utf16-le；其余为 GUID/int 等非字符串 key
            ln = struct.unpack_from("<i", kd, koff + 1)[0]
            return kd[koff + 5 : koff + 5 + ln].decode("ascii" if t == 0 else "utf-16-le", "replace")
        return None

    keys = [read_key(b[0]) for b in buckets]
    entries = []
    off = 4
    for _ in range(struct.unpack_from("<i", ed, 0)[0]):
        entries.append(struct.unpack_from("<7i", ed, off))  # (internalId, provider, depKey, depHash, data, primaryKey, resType)
        off += 28

    addr2bundle = {}
    for i, k in enumerate(keys):
        if not isinstance(k, str):
            continue
        for ei in buckets[i][1]:
            dep = entries[ei][2]
            if dep >= 0 and isinstance(keys[dep], str) and keys[dep].endswith(".bundle"):
                addr2bundle.setdefault(k, keys[dep])
    return addr2bundle


def _read_str(raw, off):
    ln = struct.unpack_from("<i", raw, off)[0]
    if ln < 0 or ln > 4096 or off + 4 + ln > len(raw):
        raise ValueError("bad string at %#x" % off)
    return raw[off + 4 : off + 4 + ln].decode("utf-8", "replace"), (off + 4 + ln + 3) & ~3


def load_sa2():
    """解析 sharedassets2.assets。

    返回 (characters, views)：
    characters: {人物id: {"name","mood","first","portraits":{表情名: addressKey}}}
    views:      {view名: {"name","address"}}
    """
    env = UnityPy.load(SA2)
    raws = {}
    for o in env.objects:
        if o.type.name == "MonoBehaviour":
            raws[o.path_id] = o.get_raw_data()

    def name_of(raw):
        try:
            return _read_str(raw, 0x1C)[0]
        except Exception:
            return None

    def mapping_value(pid):
        """StoryMappingItem(m_Name, Key, Value) -> Value"""
        raw = raws.get(pid)
        if raw is None:
            return None
        try:
            _, off = _read_str(raw, 0x1C)
            _, off = _read_str(raw, off)
            return _read_str(raw, off)[0]
        except Exception:
            return None

    def parse_character_data(raw):
        """StoryCharacterData -> (人物id, dict)"""
        name, off = _read_str(raw, 0x1C)
        mood = struct.unpack_from("<2f", raw, off)
        off += 8
        _, mpid = struct.unpack_from("<iq", raw, off)
        off += 12
        cnt = struct.unpack_from("<i", raw, off)[0]
        off += 4
        if not (0 < cnt < 128):
            raise ValueError("bad count")
        portraits = {}
        first = None
        for _ in range(cnt):
            _, ipid = struct.unpack_from("<iq", raw, off)
            off += 12
            addr, off = _read_str(raw, off)
            if first is None:
                first = addr
            emo = mapping_value(ipid)
            if emo:
                portraits[emo] = addr
        cid = mapping_value(mpid)
        if not cid:
            raise ValueError("no id")
        return cid, {"name": name, "mood": [mood[0], mood[1]], "first": first, "portraits": portraits}

    def parse_addressable_data(raw):
        """AddressableData -> (key, dict)"""
        name, off = _read_str(raw, 0x1C)
        key, off = _read_str(raw, off)
        addr, off = _read_str(raw, off)
        if not key or not addr:
            raise ValueError("bad entry")
        return key, {"name": name, "address": addr}

    characters = {}
    views = {}
    for pid, raw in raws.items():
        nm = name_of(raw)
        if nm == "StoryCharacterConfig":
            off = _read_str(raw, 0x1C)[1]
            cnt = struct.unpack_from("<i", raw, off)[0]
            off += 4
            for i in range(cnt):
                dpid = struct.unpack_from("<iq", raw, off + 12 * i)[1]
                if dpid in raws:
                    try:
                        cid, c = parse_character_data(raws[dpid])
                        characters.setdefault(cid, c)
                    except Exception:
                        pass
        elif nm is not None and len(raw) < 8000:
            # AddressableCollectionData：name + count + PPtr[]，目标全部可解析为 AddressableData
            try:
                off = _read_str(raw, 0x1C)[1]
                cnt = struct.unpack_from("<i", raw, off)[0]
                off += 4
                if not (50 < cnt < 400) or off + 12 * cnt > len(raw):
                    continue
                ok = 0
                entries = {}
                for i in range(cnt):
                    dpid = struct.unpack_from("<iq", raw, off + 12 * i)[1]
                    if dpid == 0:  # null PPtr
                        continue
                    if dpid not in raws:
                        break
                    k, e = parse_addressable_data(raws[dpid])
                    entries[k] = e
                    ok += 1
                if ok >= 50 and ok >= cnt - 2:  # 容忍个别 null/重复
                    views.update(entries)
            except Exception:
                continue
    return characters, views


_bundle_cache = {}


def export_sprite(addr2bundle, address, out_path):
    bundle = addr2bundle.get(address)
    if bundle is None:
        return "无 bundle 映射"
    if bundle not in _bundle_cache:
        _bundle_cache[bundle] = UnityPy.load(os.path.join(BUNDLE_DIR, bundle))
    sprite_name = os.path.splitext(os.path.basename(address))[0]
    for o in _bundle_cache[bundle].objects:
        if o.type.name == "Sprite" and o.read().m_Name == sprite_name:
            d = o.read()
            d.image.save(out_path)
            tex = d.m_RD.texture.read()
            return "ok (sprite=%s, 整图 %dx%d fmt=%s, bundle=%s)" % (
                sprite_name, tex.m_Width, tex.m_Height, tex.m_TextureFormat, bundle)
    return "bundle 内找不到 Sprite " + sprite_name


def main():
    os.makedirs(OUT_DIR, exist_ok=True)
    addr2bundle = parse_catalog(CATALOG)
    print("[catalog] addressKey->bundle 条目:", len(addr2bundle))

    characters, views = load_sa2()
    print("[sa2] 人物配置数=%d, view 映射数=%d" % (len(characters), len(views)))

    # 1) 验证导出 brother4 normal 立绘 + center 背景
    b4 = characters["brother4"]
    print("[brother4] 中文名=%s first=%s" % (b4["name"], b4["first"]))
    print("[brother4] 表情映射=%s" % json.dumps(b4["portraits"], ensure_ascii=False))
    print("[导出] brother4/normal:", export_sprite(
        addr2bundle, b4["portraits"]["normal"], os.path.join(OUT_DIR, "brother4_normal.png")))
    print("[导出] center:", export_sprite(
        addr2bundle, views["center"]["address"], os.path.join(OUT_DIR, "view_center.png")))

    # 2) 规模统计
    char_addrs = {a for c in characters.values() for a in c["portraits"].values()}
    bg_addrs = {v["address"] for v in views.values()}

    def dedup_size(addrs):
        bundles = {addr2bundle[a] for a in addrs if a in addr2bundle}
        return sum(os.path.getsize(os.path.join(BUNDLE_DIR, b)) for b in bundles) / 1048576

    print("[规模] 配置引用的唯一立绘=%d 张（所在 bundle 去重体积=%.1fMB，catalog 内 Characters 路径地址共 870 条）"
          % (len(char_addrs), dedup_size(char_addrs)))
    print("[规模] 配置引用的唯一背景=%d 张（所在 bundle 去重体积=%.1fMB）" % (len(bg_addrs), dedup_size(bg_addrs)))

    # 3) editor_data.json 覆盖率
    ed = json.load(open(EDITOR_DATA, encoding="utf-8"))
    need, fallback, missing_id = set(), [], []
    for ch in ed["characters"]:
        cid = ch["id"]
        if cid not in characters:
            missing_id.append(cid)
            continue
        c = characters[cid]
        for emo in ch["portraits"]:
            addr = c["portraits"].get(emo)
            if addr:
                need.add(addr)
            else:
                fallback.append((cid, emo))
                if c["first"]:
                    need.add(c["first"])
    print("[覆盖] editor 人物=%d, 配置缺失 id=%s" % (len(ed["characters"]), missing_id))
    print("[覆盖] 表情不在配置中（游戏内回退到该人物第一张立绘）=%d 项, 样例: %s" % (len(fallback), fallback[:10]))
    print("[覆盖] 预览需导出的唯一立绘贴图=%d 张" % len(need))

    need_bg, missing_view = set(), []
    for v in ed["views"]:
        if v in views:
            need_bg.add(views[v]["address"])
        else:
            missing_view.append(v)
    print("[覆盖] editor 场景=%d, 有映射=%d, 无映射=%s（black/white 为硬编码纯色）"
          % (len(ed["views"]), len(need_bg), missing_view))

    # 4) 全量映射落地
    with open(os.path.join(OUT_DIR, "character_map.json"), "w", encoding="utf-8") as f:
        json.dump(characters, f, ensure_ascii=False, indent=1)
    with open(os.path.join(OUT_DIR, "view_map.json"), "w", encoding="utf-8") as f:
        json.dump(views, f, ensure_ascii=False, indent=1)
    print("[写出] character_map.json / view_map.json ->", OUT_DIR)


if __name__ == "__main__":
    main()
