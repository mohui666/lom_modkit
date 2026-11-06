"""Read selected vanilla scripts from the player's own installation, without writes."""
import re
import struct
from pathlib import Path


def resources(game):
    import UnityPy
    path = Path(game) / "Mortal_Data/globalgamemanagers"
    if not path.is_file():
        raise ValueError("请选择包含 Mortal_Data 的《活侠传》游戏目录")
    environment = UnityPy.load(str(path))
    manager = next((obj.read() for obj in environment.objects
                    if obj.type.name == "ResourceManager"), None)
    if manager is None:
        raise ValueError("没有找到游戏资源目录，请检查游戏版本")
    return environment, dict(manager.m_Container)


def script_text(pointer):
    if pointer.type.name != "TextAsset":
        raise ValueError("原版章节资源不是 Lua 文本")
    value = pointer.read().m_Script
    return value if isinstance(value, str) else value.decode("utf-8-sig")


def _string(raw, at):
    size = struct.unpack_from("<i", raw, at)[0]
    if size < 0 or at + 4 + size > len(raw):
        raise ValueError("原版剧情资源字符串结构已改变")
    return raw[at + 4:at + 4 + size].decode("utf-8"), (at + 4 + size + 3) & ~3


def position_routes(game, environment, available_scripts):
    """Read PositionResultData.List and its StoryMappingItem.Value candidates."""
    import UnityPy
    from UnityPy.classes import PPtr
    settings = next(obj.read() for obj in environment.objects if obj.type.name == "BuildSettings")
    scene = next((index for index, name in enumerate(settings.scenes)
                  if name.rsplit("/", 1)[-1] == "Story.unity"), None)
    if scene is None:
        raise ValueError("游戏场景目录中没有 Story 场景")
    assets = UnityPy.load(str(Path(game) / "Mortal_Data" / f"sharedassets{scene}.assets"))

    def pointer(raw, at, owner):
        file_id, path_id = struct.unpack_from("<iq", raw, at)
        return PPtr(m_FileID=file_id, m_PathID=path_id, assetsfile=owner.assets_file).deref()

    result = {}
    for obj in assets.objects:
        if obj.type.name != "MonoBehaviour":
            continue
        header = obj.read(check_read=False)
        script = header.m_Script.read()
        if script.m_Namespace != "Mortal.Story" or script.m_ClassName != "PositionResultData":
            continue
        raw = obj.get_raw_data()
        name, at = _string(raw, 28)
        count = struct.unpack_from("<i", raw, at)[0]
        at += 4
        if count < 0 or at + count * 12 > len(raw):
            raise ValueError("场所检定的事件列表结构已改变")
        events = [pointer(raw, at + index * 12, obj) for index in range(count)]
        _, at = _string(raw, at + count * 12)  # PositionResultData._devNote.
        prefix, at = _string(raw, at)
        candidates = set()
        default = prefix + "_001"  # PositionEvent.Get()'s confirmed default.
        if default.lower() in available_scripts:
            candidates.add(default)
        for event in events:
            event_raw = event.get_raw_data()
            _, at = _string(event_raw, 28)
            _, at = _string(event_raw, at)  # PositionEventData._devNote.
            mapping = pointer(event_raw, at, event)
            mapping_raw = mapping.get_raw_data()
            _, at = _string(mapping_raw, 28)
            _, at = _string(mapping_raw, at)  # StoryMappingItem.Key.
            value, at = _string(mapping_raw, at)
            candidates.add(prefix + "_" + value)
        result[name] = sorted(candidates)
    if not result:
        raise ValueError("没有读到原版场所检定配置")
    return result


def translations(game, source):
    """LeanLanguageCSV.Language and serialized Entries, checked against the live type.
    Duplicate Story_N TextAssets have identical names across languages; never guess
    their language from Chinese characters or object order.
    """
    import UnityPy
    keys = set(re.findall(r'GetStoryText\s*\(\s*["\']([A-Za-z0-9_-]+)["\']', source))
    option_references = set(re.findall(r'["\'](O_[A-Za-z0-9_]+(?:\|[^"\'\r\n]*)?)["\']', source))
    keys.update(reference.split("|", 1)[0] for reference in option_references)
    if not keys:
        return {}
    environment = UnityPy.load(str(Path(game) / "Mortal_Data/level0"))
    result = {}

    for obj in environment.objects:
        if obj.type.name != "MonoBehaviour":
            continue
        header = obj.read(check_read=False)
        script = header.m_Script.read()
        if script.m_ClassName != "LeanLanguageCSV" or script.m_Namespace != "Lean.Localization":
            continue
        raw = obj.get_raw_data()
        _, at = _string(raw, 28)  # MonoBehaviour name, after the confirmed base header.
        at += 12  # Source TextAsset PPtr.
        language, at = _string(raw, at)
        if language != "ChineseSimplified":
            continue
        for _ in range(3):  # Separator, NewLine, Comment.
            _, at = _string(raw, at)
        _, count = struct.unpack_from("<ii", raw, at)  # Cache, Entries.Count.
        at += 8
        if count < 0 or count > 1_000_000:
            raise ValueError("原版剧情语言表条目数无效")
        for _ in range(count):
            name, at = _string(raw, at)
            text, at = _string(raw, at)
            if name.startswith("Story/") and name[6:] in keys:
                result[name[6:]] = text
    if not result:
        raise ValueError("没有读到原版章节的简体文字，请核对当前游戏版本")
    # DiceOption.UpdateContent localizes the first pipe-separated field and
    # displays the second as its condition. Preserve the original suffix bytes.
    for reference in option_references:
        key, separator, suffix = reference.partition("|")
        if separator and key in result:
            result[reference] = result[key] + separator + suffix
    return result


def read(game, target=None):
    environment, entries = resources(game)
    prefix = "story/chinesetraditional/"  # LuaManager.ExecuteLuaScript in the installed game.
    if target is not None:
        if not re.fullmatch(r"[A-Za-z0-9_-]{1,64}", target) or target.startswith("MOD_"):
            raise ValueError("原版章节编号无效")
        pointer = entries.get(prefix + target.lower())
        if pointer is None:
            raise ValueError("当前游戏中没有原版章节：" + target)
        source = script_text(pointer)
        return {"target": target, "source": source, "translations": translations(game, source)}
    chapters = sorted(pointer.read().m_Name for key, pointer in entries.items() if key.startswith(prefix))
    if not chapters:
        raise ValueError("没有找到原版章节，请检查游戏资源结构")
    return {"chapters": chapters}
