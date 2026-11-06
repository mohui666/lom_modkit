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


def translations(game, source):
    """LeanLanguageCSV.Language and serialized Entries, checked against the live type.
    Duplicate Story_N TextAssets have identical names across languages; never guess
    their language from Chinese characters or object order.
    """
    import UnityPy
    keys = set(re.findall(r'GetStoryText\s*\(\s*["\']([A-Za-z0-9_-]+)["\']', source))
    keys.update(re.findall(r'["\'](O_[A-Za-z0-9_]+)["\']', source))
    if not keys:
        return {}
    environment = UnityPy.load(str(Path(game) / "Mortal_Data/level0"))
    result = {}

    def string(raw, at):
        size = struct.unpack_from("<i", raw, at)[0]
        if size < 0 or at + 4 + size > len(raw):
            raise ValueError("原版剧情语言表结构已改变")
        return raw[at + 4:at + 4 + size].decode("utf-8"), (at + 4 + size + 3) & ~3

    for obj in environment.objects:
        if obj.type.name != "MonoBehaviour":
            continue
        header = obj.read(check_read=False)
        script = header.m_Script.read()
        if script.m_ClassName != "LeanLanguageCSV" or script.m_Namespace != "Lean.Localization":
            continue
        raw = obj.get_raw_data()
        _, at = string(raw, 28)  # MonoBehaviour name, after the confirmed base header.
        at += 12  # Source TextAsset PPtr.
        language, at = string(raw, at)
        if language != "ChineseSimplified":
            continue
        for _ in range(3):  # Separator, NewLine, Comment.
            _, at = string(raw, at)
        _, count = struct.unpack_from("<ii", raw, at)  # Cache, Entries.Count.
        at += 8
        if count < 0 or count > 1_000_000:
            raise ValueError("原版剧情语言表条目数无效")
        for _ in range(count):
            name, at = string(raw, at)
            text, at = string(raw, at)
            if name.startswith("Story/") and name[6:] in keys:
                result[name[6:]] = text
    if not result:
        raise ValueError("没有读到原版章节的简体文字，请核对当前游戏版本")
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
