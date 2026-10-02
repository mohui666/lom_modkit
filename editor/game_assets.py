# -*- coding: utf-8 -*-
"""从本机《活侠传》安装目录按需解析并导出立绘 / 背景预览图。

背景（为什么需要这个模块）
--------------------------
官方发布包与源码仓库都不含 ``data/assets/``（仓库 .gitignore 明确排除），
所以右侧「舞台预览」在没有素材时只能画占位方块。与其分发几百 MB 图片，
不如在用户已经装了游戏的前提下，直接从游戏资源里按需取图并本地缓存。

反编译接口依据（按 AGENTS.md 要求，只做本地实证解析，不猜接口）
------------------------------------------------------------
解析链路与字段布局沿用 ``tools/_probe_assets.py`` 中已经实证过的结论：

- ``CharacterPlaceholder.LoadCharacterAsset`` → ``StoryCharacterConfig.Get(id)``
  → ``StoryCharacterData.PortraitResourceList``（``{Mapping.Value=表情名,
  AddressKey=立绘地址}``）→ ``Addressables.LoadAssetAsync<Sprite>(addressKey)``
- ``ViewFlowchartController.LoadView`` → ``StoryViewImage.LoadAsset(view名)``
  → ``AddressableCollectionData(_StoryViewData).GetByKey`` → ``AddressKey``
  → ``Sprite``（``black`` / ``white`` 在客户端是硬编码纯色，没有贴图）

配置数据序列化在 ``Mortal_Data/sharedassets2.assets``，没有 typetree，按字段
布局手工解析：``StoryCharacterConfig._list`` → ``StoryCharacterData``
（``m_Name | _moodPosition | _mapping | _portraitResourceList``）；
``AddressableCollectionData`` → ``AddressableData``（``m_Name | _key | _addressKey``）；
``StoryMappingItem`` 提供 ``Value``。图片实体在 Addressables bundle 里，
``catalog.json`` 给出 addressKey → bundle 的映射。

本机实测（游戏 v250915 基础包）：解析出人物 427 个、背景 173 个映射，耗时约
0.15s；对编辑器清单的覆盖率是人物 406/410（99%）、背景 152/152（100%）。

设计约束
--------
- **不依赖 Qt**：便于离线测试，也让调用方只关心「拿到文件路径」。
- **优雅降级**：UnityPy / Pillow 未安装，或游戏目录无效时，``available()``
  返回 False，调用方照旧走占位图，绝不会因为预览素材而崩溃。
- **解析结果落盘缓存**：``data/assets/_cache/*.json``，二次启动直接秒开。
- **失败也记账**：缺失的 id 只提示一次，避免每次重绘都去读盘。
"""

from __future__ import annotations

import base64
import json
import os
import struct
import threading
from collections import OrderedDict
from pathlib import Path

# ---------------------------------------------------------------- 可选依赖

try:  # pragma: no cover - 环境相关
    import UnityPy
except Exception:  # noqa: BLE001 - 任何导入失败都视为不可用
    UnityPy = None

try:  # pragma: no cover - 环境相关
    from PIL import Image
except Exception:  # noqa: BLE001
    Image = None


CACHE_VERSION = 3
SOLID_SIZE = (1920, 1080)
MAX_LOADED_BUNDLES = 6  # 同时驻留的 bundle 上限（单个几十 MB，必须限量）


def unity_available() -> bool:
    """UnityPy 是否可用。"""
    return UnityPy is not None


def save_png_atomic(image, out_path: Path) -> None:
    """先写临时文件再原子替换。

    提取过程可能被用户中途关窗口/强杀。直接写目标路径会留下半张损坏的 PNG，
    而缓存判断只看「文件在不在」，坏图就会被永久当成已缓存。
    """
    temp = out_path.with_name(out_path.name + ".part")
    try:
        # 必须显式指定 format：PIL 靠扩展名推断格式，而临时文件叫 *.png.part，
        # 不指定就会以「unknown file extension: .part」直接失败。
        image.save(str(temp), format="PNG")
        temp.replace(out_path)
    finally:
        if temp.exists():
            try:
                temp.unlink()
            except OSError:
                pass


def resolve_asset_root(data_dir: Path) -> Path:
    """决定提取出来的图往哪放。

    优先放到 ``<项目根>/data/assets``（与 preview_map.json 的相对路径一致）。
    如果这个位置不可写（例如把编辑器装在 Program Files 下），退到
    ``%APPDATA%/lom_modkit/assets``，保证功能可用而不是直接失败。
    """
    candidate = Path(data_dir) / "assets"
    try:
        candidate.mkdir(parents=True, exist_ok=True)
        probe = candidate / ".write-test"
        probe.write_text("", encoding="utf-8")
        probe.unlink()
        return candidate
    except OSError:
        base = os.environ.get("APPDATA") or str(Path.home())
        fallback = Path(base) / "lom_modkit" / "assets"
        fallback.mkdir(parents=True, exist_ok=True)
        return fallback


class GameAssetLibrary:
    """按需从游戏安装目录导出立绘 / 背景。

    典型用法::

        lib = GameAssetLibrary(game_root, data_dir)
        if lib.ready:
            path = lib.portrait_file("girl4", "normal")   # 落盘后的绝对路径
    """

    def __init__(self, game_root: Path | str | None, data_dir: Path | str) -> None:
        self.game_root = Path(game_root) if game_root else None
        self.data_dir = Path(data_dir)
        self.asset_root = resolve_asset_root(self.data_dir)
        self.cache_dir = self.asset_root / "_cache"

        self._lock = threading.RLock()
        self._characters: dict | None = None
        self._views: dict | None = None
        self._addr2bundle: dict | None = None
        self._bundles: OrderedDict[str, object] = OrderedDict()
        self._failed: set[str] = set()
        self._unavailable_reason = ""
        self._solid_done: set[str] = set()
        self._last_error = ""

    @property
    def last_error(self) -> str:
        """最近一次取图失败的原因（成功时保留上一条，便于排查）。"""
        return self._last_error

    # ------------------------------------------------------------ 环境检查

    @property
    def game_data_dir(self) -> Path | None:
        return self.game_root / "Mortal_Data" if self.game_root else None

    @property
    def catalog_path(self) -> Path | None:
        base = self.game_data_dir
        return base / "StreamingAssets" / "aa" / "catalog.json" if base else None

    @property
    def bundle_dir(self) -> Path | None:
        base = self.game_data_dir
        return base / "StreamingAssets" / "aa" / "StandaloneWindows" if base else None

    @property
    def shared_assets_path(self) -> Path | None:
        base = self.game_data_dir
        return base / "sharedassets2.assets" if base else None

    def probe(self) -> tuple[bool, str]:
        """检查所有前置条件。返回 (是否可用, 人话原因)。"""
        if self.game_root is None:
            return False, "还没有设置《活侠传》游戏目录"
        for label, path in (
            ("catalog.json", self.catalog_path),
            ("资源包目录", self.bundle_dir),
            ("sharedassets2.assets", self.shared_assets_path),
        ):
            if path is None or not path.exists():
                return False, f"游戏目录里找不到 {label}"
        if not unity_available():
            return False, "缺少 UnityPy（无法解析 Unity 资源）"
        if Image is None:
            return False, "缺少 Pillow（无法写出 PNG）"
        return True, "可以从游戏目录提取立绘 / 背景"

    @property
    def ready(self) -> bool:
        ok, _ = self.probe()
        return ok

    @property
    def unavailable_reason(self) -> str:
        if self._unavailable_reason:
            return self._unavailable_reason
        _ok, reason = self.probe()
        return reason

    # ------------------------------------------------------------ 映射表解析

    def mappings(self) -> tuple[dict, dict]:
        """返回 (characters, views) 映射；不可用时返回 ({}, {})。"""
        ok, reason = self.probe()
        if not ok:
            self._unavailable_reason = reason
            return {}, {}
        with self._lock:
            if self._characters is None or self._views is None:
                cached = self._read_cache()
                if cached is None:
                    cached = self._parse_from_game()
                    self._write_cache(cached)
                self._characters = cached.get("characters") or {}
                self._views = cached.get("views") or {}
            return self._characters, self._views

    def _cache_file(self) -> Path:
        return self.cache_dir / "game_asset_map.json"

    def _cache_stamp(self) -> dict:
        catalog = self.catalog_path
        shared = self.shared_assets_path
        return {
            "cache_version": CACHE_VERSION,
            "catalog_size": catalog.stat().st_size if catalog and catalog.exists() else 0,
            "shared_size": shared.stat().st_size if shared and shared.exists() else 0,
        }

    def _read_cache(self) -> dict | None:
        try:
            raw = json.loads(self._cache_file().read_text(encoding="utf-8"))
        except (OSError, ValueError):
            return None
        if not isinstance(raw, dict):
            return None
        if raw.get("stamp") != self._cache_stamp():
            return None
        return raw

    def _write_cache(self, payload: dict) -> None:
        try:
            self.cache_dir.mkdir(parents=True, exist_ok=True)
            temp = self._cache_file().with_suffix(".tmp")
            temp.write_text(
                json.dumps(payload, ensure_ascii=False, indent=1), encoding="utf-8"
            )
            temp.replace(self._cache_file())
        except OSError:
            pass  # 缓存写不进去只影响启动速度，不影响功能

    def _parse_from_game(self) -> dict:
        addr2bundle = self._load_catalog()
        characters, views = self._load_shared_assets()
        return {
            "stamp": self._cache_stamp(),
            "characters": characters,
            "views": views,
            "addr2bundle": addr2bundle,
        }

    def _load_catalog(self) -> dict:
        """catalog.json -> {AddressKey: bundle 文件名}。

        二进制段格式参考 AddressablesToolsPy：桶表给出 key 偏移与条目下标，
        条目表的第 3 个字段是依赖 key 下标，指向 ``*.bundle``。
        """
        catalog = self.catalog_path
        if catalog is None:
            return {}
        try:
            cat = json.loads(catalog.read_text(encoding="utf-8"))
            kd = base64.b64decode(cat["m_KeyDataString"])
            bd = base64.b64decode(cat["m_BucketDataString"])
            ed = base64.b64decode(cat["m_EntryDataString"])
        except (OSError, ValueError, KeyError, TypeError):
            return {}

        buckets = []
        off = 4
        try:
            for _ in range(struct.unpack_from("<i", bd, 0)[0]):
                koff, ecnt = struct.unpack_from("<ii", bd, off)
                off += 8
                ents = struct.unpack_from("<%di" % ecnt, bd, off)
                off += 4 * ecnt
                buckets.append((koff, ents))
        except struct.error:
            return {}

        def read_key(koff: int) -> str | None:
            if koff >= len(kd):
                return None
            kind = kd[koff]
            if kind not in (0, 1):  # 0=ascii 1=utf16-le；其余是 GUID/int 等非字符串 key
                return None
            ln = struct.unpack_from("<i", kd, koff + 1)[0]
            raw = kd[koff + 5 : koff + 5 + ln]
            return raw.decode("ascii" if kind == 0 else "utf-16-le", "replace")

        keys = [read_key(b[0]) for b in buckets]

        entries = []
        off = 4
        try:
            for _ in range(struct.unpack_from("<i", ed, 0)[0]):
                entries.append(struct.unpack_from("<7i", ed, off))
                off += 28
        except struct.error:
            return {}

        addr2bundle: dict[str, str] = {}
        for i, key in enumerate(keys):
            if not isinstance(key, str):
                continue
            for ei in buckets[i][1]:
                if ei >= len(entries):
                    continue
                dep = entries[ei][2]
                if 0 <= dep < len(keys) and isinstance(keys[dep], str):
                    if keys[dep].endswith(".bundle"):
                        addr2bundle.setdefault(key, keys[dep])
        return addr2bundle

    def _load_shared_assets(self) -> tuple[dict, dict]:
        """解析 sharedassets2.assets 里的两套配置（无 typetree，按字段布局手工解析）。"""
        shared = self.shared_assets_path
        if shared is None or not unity_available():
            return {}, {}
        try:
            env = UnityPy.load(str(shared))
            raws = {
                o.path_id: o.get_raw_data()
                for o in env.objects
                if o.type.name == "MonoBehaviour"
            }
        except Exception:  # noqa: BLE001 - 资源格式不符时安静降级
            return {}, {}

        characters: dict[str, dict] = {}
        views: dict[str, dict] = {}

        def name_of(raw: bytes) -> str | None:
            try:
                return _read_str(raw, 0x1C)[0]
            except Exception:  # noqa: BLE001
                return None

        def mapping_value(pid: int) -> str | None:
            raw = raws.get(pid)
            if raw is None:
                return None
            try:
                _n, off = _read_str(raw, 0x1C)
                _n, off = _read_str(raw, off)
                return _read_str(raw, off)[0]
            except Exception:  # noqa: BLE001
                return None

        def parse_character_data(raw: bytes) -> tuple[str, dict]:
            name, off = _read_str(raw, 0x1C)
            mood = struct.unpack_from("<2f", raw, off)
            off += 8
            _m, mpid = struct.unpack_from("<iq", raw, off)
            off += 12
            cnt = struct.unpack_from("<i", raw, off)[0]
            off += 4
            if not (0 < cnt < 128):
                raise ValueError("bad portrait count")
            portraits: dict[str, str] = {}
            first = None
            for _ in range(cnt):
                _m, ipid = struct.unpack_from("<iq", raw, off)
                off += 12
                addr, off = _read_str(raw, off)
                if first is None:
                    first = addr
                emo = mapping_value(ipid)
                if emo:
                    portraits[emo] = addr
            cid = mapping_value(mpid)
            if not cid:
                raise ValueError("no character id")
            return cid, {
                "name": name,
                "mood": [mood[0], mood[1]],
                "first": first,
                "portraits": portraits,
            }

        def parse_addressable_data(raw: bytes) -> tuple[str, dict]:
            name, off = _read_str(raw, 0x1C)
            key, off = _read_str(raw, off)
            addr, off = _read_str(raw, off)
            if not key or not addr:
                raise ValueError("bad addressable entry")
            return key, {"name": name, "address": addr}

        for pid, raw in raws.items():
            nm = name_of(raw)
            if nm == "StoryCharacterConfig":
                off = _read_str(raw, 0x1C)[1]
                cnt = struct.unpack_from("<i", raw, off)[0]
                off += 4
                for i in range(cnt):
                    if off + 12 * i + 12 > len(raw):
                        break
                    dpid = struct.unpack_from("<iq", raw, off + 12 * i)[1]
                    if dpid in raws:
                        try:
                            cid, conf = parse_character_data(raws[dpid])
                            characters.setdefault(cid, conf)
                        except Exception:  # noqa: BLE001 - 单条脏数据不影响整体
                            pass
            elif nm is not None and len(raw) < 8000:
                # AddressableCollectionData：name + count + PPtr[]，目标都能解析成
                # AddressableData 才算命中，避免把别的集合类误当成 view 表。
                try:
                    off = _read_str(raw, 0x1C)[1]
                    cnt = struct.unpack_from("<i", raw, off)[0]
                    off += 4
                    if not (50 < cnt < 400) or off + 12 * cnt > len(raw):
                        continue
                    ok = 0
                    entries: dict[str, dict] = {}
                    for i in range(cnt):
                        dpid = struct.unpack_from("<iq", raw, off + 12 * i)[1]
                        if dpid == 0:
                            continue
                        if dpid not in raws:
                            break
                        key, entry = parse_addressable_data(raws[dpid])
                        entries[key] = entry
                        ok += 1
                    if ok >= 50 and ok >= cnt - 2:
                        views.update(entries)
                except Exception:  # noqa: BLE001
                    continue
        return characters, views

    # ------------------------------------------------------------ 图片导出

    def _bundle_for(self, address: str) -> str | None:
        if self._addr2bundle is None:
            cached = self._read_cache() or {}
            self._addr2bundle = cached.get("addr2bundle") or self._load_catalog()
        return self._addr2bundle.get(address)

    def _load_bundle(self, bundle_name: str):
        cached = self._bundles.get(bundle_name)
        if cached is not None:
            self._bundles.move_to_end(bundle_name)
            return cached
        path = self.bundle_dir / bundle_name if self.bundle_dir else None
        if path is None or not path.is_file():
            return None
        if not unity_available():
            return None
        env = UnityPy.load(str(path))
        self._bundles[bundle_name] = env
        while len(self._bundles) > MAX_LOADED_BUNDLES:
            self._bundles.popitem(last=False)
        return env

    def _export_sprite(self, address: str, out_path: Path) -> bool:
        """把 addressKey 指向的 Sprite 解码写到 out_path。成功返回 True。

        失败原因写进 ``last_error``：冻结版没有控制台，出问题时这是唯一的线索；
        图形界面里也用它告诉用户「为什么这张图取不到」。
        """
        bundle_name = self._bundle_for(address)
        if not bundle_name:
            self._last_error = f"资源包映射里没有 {address}"
            return False
        try:
            env = self._load_bundle(bundle_name)
        except Exception as exc:  # noqa: BLE001
            self._last_error = f"资源包解析失败 {bundle_name}：{exc!r}"
            return False
        if env is None:
            self._last_error = f"资源包不存在或不可读：{bundle_name}"
            return False
        sprite_name = Path(address).stem
        for obj in env.objects:
            if obj.type.name != "Sprite":
                continue
            try:
                data = obj.read()
            except Exception:  # noqa: BLE001
                continue
            if data.m_Name != sprite_name:
                continue
            try:
                image = data.image
            except Exception as exc:  # noqa: BLE001
                self._last_error = f"贴图解码失败 {sprite_name}：{exc!r}"
                return False
            try:
                out_path.parent.mkdir(parents=True, exist_ok=True)
                save_png_atomic(image, out_path)
                return True
            except Exception as exc:  # noqa: BLE001
                self._last_error = f"写入 PNG 失败 {out_path}：{exc!r}"
                return False
        self._last_error = f"资源包 {bundle_name} 内找不到 Sprite {sprite_name}"
        return False

    def _ensure_solid(self, name: str) -> Path | None:
        """black / white 在原版是硬编码纯色，没有贴图，本地生成。"""
        if name not in ("black", "white"):
            return None
        target = self.asset_root / "views" / f"{name}.png"
        if name in self._solid_done and target.is_file():
            return target
        if Image is None:
            return None
        try:
            target.parent.mkdir(parents=True, exist_ok=True)
            if not target.is_file():
                Image.new(
                    "RGB", SOLID_SIZE, (0, 0, 0) if name == "black" else (255, 255, 255)
                ).save(str(target))
            self._solid_done.add(name)
            return target
        except Exception:  # noqa: BLE001
            return None

    # ------------------------------------------------------------ 对外查询

    def cached_path(self, kind: str, item_id: str, sub: str = "") -> Path:
        """按素材类型算出本地缓存路径（不保证文件已存在）。"""
        if kind == "view":
            return self.asset_root / "views" / f"{item_id}.png"
        return self.asset_root / "portraits" / item_id / f"{sub or 'normal'}.png"

    def character_ids(self) -> list[str]:
        chars, _ = self.mappings()
        return sorted(chars)

    def view_ids(self) -> list[str]:
        _chars, views = self.mappings()
        return sorted(views)

    def emotions(self, char_id: str) -> list[str]:
        chars, _ = self.mappings()
        conf = chars.get(char_id) or {}
        portraits = conf.get("portraits") or {}
        if not portraits:
            return []
        first = conf.get("first")
        ordered = [e for e in portraits if e != first]
        return ([first] if first in portraits else []) + sorted(ordered)

    def portrait_file(self, char_id: str, emotion: str = "normal") -> Path | None:
        """取人物立绘；取不到返回 None（调用方走占位）。"""
        if not char_id:
            return None
        if isinstance(char_id, str) and char_id.startswith("user:"):
            return None  # 用户内容库有自己的解析路径
        chars, _ = self.mappings()
        conf = chars.get(char_id)
        if not conf:
            self._last_error = f"游戏配置里没有人物 {char_id}"
            return None
        portraits = conf.get("portraits") or {}
        # 表情缺失时按原版规则回退到人物第一张立绘，但文件名仍用请求的表情名，
        # 这样预览层拿到的路径稳定、可缓存。
        address = portraits.get(emotion) or conf.get("first")
        target = self.asset_root / "portraits" / char_id / f"{emotion}.png"
        if target.is_file():
            return target
        key = f"p:{char_id}:{emotion}"
        if key in self._failed or not address:
            self._last_error = f"人物 {char_id} 没有可用的立绘地址"
            return None
        with self._lock:
            if target.is_file():
                return target
            if self._export_sprite(address, target):
                return target
            self._failed.add(key)
        return None

    def view_file(self, view_id: str) -> Path | None:
        """取官方背景图；black/white 本地生成。"""
        if not view_id:
            return None
        solid = self._ensure_solid(view_id)
        if solid is not None:
            return solid
        _chars, views = self.mappings()
        conf = views.get(view_id)
        if not conf:
            self._last_error = f"游戏配置里没有背景 {view_id}"
            return None
        target = self.asset_root / "views" / f"{view_id}.png"
        if target.is_file():
            return target
        key = f"v:{view_id}"
        if key in self._failed:
            self._last_error = f"背景 {view_id} 之前已尝试过且失败"
            return None
        address = conf.get("address") or ""
        if not address:
            self._failed.add(key)
            self._last_error = f"背景 {view_id} 没有资源地址"
            return None
        with self._lock:
            if target.is_file():
                return target
            if self._export_sprite(address, target):
                return target
            self._failed.add(key)
        return None

    # ------------------------------------------------------------ 映射导出

    def preview_map_fragment(self) -> tuple[dict, dict]:
        """构造可合并进 preview_map.json 的 (characters, views) 片段。

        文件路径统一写成 ``assets/...`` 相对形式，与既有 preview_map 一致；
        真正落盘推迟到 ``*_file()`` 被调用时按需执行。
        """
        chars, views = self.mappings()
        out_chars: dict[str, dict] = {}
        for cid, conf in chars.items():
            portraits = conf.get("portraits") or {}
            first = conf.get("first")
            emotion_names = list(portraits)
            if not emotion_names:
                continue
            first_name = first if first in portraits else emotion_names[0]
            out_chars[cid] = {
                "name": conf.get("name") or cid,
                "first": first_name,
                "portraits": {
                    emo: f"assets/portraits/{cid}/{emo}.png" for emo in emotion_names
                },
            }
        out_views: dict[str, str] = {"black": "assets/views/black.png", "white": "assets/views/white.png"}
        for vid in views:
            out_views[vid] = f"assets/views/{vid}.png"
        return out_chars, out_views

    def total_counts(self) -> tuple[int, int]:
        chars, views = self.mappings()
        return len(chars), len(views)

    # ------------------------------------------------------------ 批量导出

    def extract_all(self, progress=None, should_stop=None) -> dict:
        """一次性把全部立绘 / 背景导出到本地缓存。

        按 bundle 分组后再导出：155 个资源包里很多张图共用同一个包，逐个
        「取一张图就加载一次包」会把同一个几十 MB 的包反复解压，这里改成
        「每个包只加载一次、把属于它的图一次抠完」。

        ``progress(done, total, text)`` 用于上报进度；``should_stop()`` 返回
        True 时中途退出。返回统计字典。
        """
        chars, views = self.mappings()
        if not chars and not views:
            return {"ok": False, "reason": self.unavailable_reason, "done": 0, "total": 0}

        # 目标清单： (address, 输出路径, 记账 key)
        jobs: list[tuple[str, Path, str]] = []
        for cid, conf in chars.items():
            portraits = conf.get("portraits") or {}
            first = conf.get("first")
            for emo in portraits:
                jobs.append(
                    (
                        portraits.get(emo) or first or "",
                        self.asset_root / "portraits" / cid / f"{emo}.png",
                        f"p:{cid}:{emo}",
                    )
                )
            # 表情名来自 editor_data 但游戏配置里没有时，仍回退到第一张立绘；
            # 这一步由调用方决定要哪些表情，这里只保证 first 一定有文件。
            if first and not portraits:
                jobs.append(
                    (
                        first,
                        self.asset_root / "portraits" / cid / "normal.png",
                        f"p:{cid}:normal",
                    )
                )
        for vid in views:
            jobs.append(
                (
                    (views[vid] or {}).get("address") or "",
                    self.asset_root / "views" / f"{vid}.png",
                    f"v:{vid}",
                )
            )
        self._ensure_solid("black")
        self._ensure_solid("white")

        pending = [j for j in jobs if not j[1].is_file() and j[0]]
        total = len(pending)
        done = 0
        failed: list[str] = []

        # 按 bundle 分组，保证每个包只加载一次
        groups: dict[str, list[tuple[str, Path, str]]] = {}
        for address, out_path, key in pending:
            bundle = self._bundle_for(address) or ""
            groups.setdefault(bundle, []).append((address, out_path, key))

        for bundle_name, items in groups.items():
            if should_stop is not None and should_stop():
                break
            env = None
            if bundle_name:
                try:
                    env = self._load_bundle(bundle_name)
                except Exception:  # noqa: BLE001
                    env = None
            by_name: dict[str, list] = {}
            for address, out_path, key in items:
                by_name.setdefault(Path(address).stem, []).append((out_path, key))

            found: set[str] = set()
            if env is not None:
                for obj in env.objects:
                    if obj.type.name != "Sprite":
                        continue
                    try:
                        data = obj.read()
                    except Exception:  # noqa: BLE001
                        continue
                    targets = by_name.get(data.m_Name)
                    if not targets:
                        continue
                    found.add(data.m_Name)
                    try:
                        image = data.image
                    except Exception:  # noqa: BLE001
                        continue
                    for out_path, _key in targets:
                        try:
                            out_path.parent.mkdir(parents=True, exist_ok=True)
                            save_png_atomic(image, out_path)
                        except Exception:  # noqa: BLE001
                            failed.append(str(out_path))

            for address, out_path, key in items:
                done += 1
                if out_path.is_file():
                    self._failed.discard(key)
                else:
                    self._failed.add(key)
                    failed.append(str(out_path))
                if progress is not None and (done % 20 == 0 or done == total):
                    progress(done, total, out_path.name)

        if progress is not None:
            progress(done, total, "完成")
        return {
            "ok": True,
            "done": done,
            "total": total,
            "failed": failed,
            "skipped": len(jobs) - total,
        }


def _read_str(raw: bytes, off: int) -> tuple[str, int]:
    """Unity 序列化字符串：4 字节长度 + UTF-8 + 4 字节对齐。"""
    if off + 4 > len(raw):
        raise ValueError("string offset out of range")
    ln = struct.unpack_from("<i", raw, off)[0]
    if ln < 0 or ln > 4096 or off + 4 + ln > len(raw):
        raise ValueError("bad string length")
    text = raw[off + 4 : off + 4 + ln].decode("utf-8", "replace")
    return text, (off + 4 + ln + 3) & ~3
