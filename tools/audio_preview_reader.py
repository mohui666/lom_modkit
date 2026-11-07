"""Read one official Wwise media file from the user's installed game."""
import struct
import xml.etree.ElementTree as ET
from pathlib import Path

from vanilla_story_reader import _string, resources


def audio_key(game, kind, name):
    import UnityPy
    environment, _ = resources(game)
    settings = next(obj.read() for obj in environment.objects if obj.type.name == "BuildSettings")
    scene = next((i for i, path in enumerate(settings.scenes) if path.rsplit("/", 1)[-1] == "Story.unity"), None)
    if scene is None:
        raise ValueError("没有找到原版剧情场景")
    assets = UnityPy.load(str(Path(game) / "Mortal_Data" / f"sharedassets{scene}.assets"))
    expected = {"music": "MusicDataCollection", "sound": "SoundDataCollection", "env": "EnvSoundDataCollection"}[kind]
    for obj in assets.objects:
        if obj.type.name != "MonoBehaviour":
            continue
        script = obj.read(check_read=False).m_Script.read()
        if script.m_Namespace != "Mortal.Core" or script.m_ClassName != expected:
            continue
        from UnityPy.classes import PPtr
        raw = obj.get_raw_data()
        _, at = _string(raw, 28)
        count = struct.unpack_from("<i", raw, at)[0]
        at += 4
        if count < 0 or at + count * 12 > len(raw):
            raise ValueError("音频目录结构已改变")
        for index in range(count):
            file_id, path_id = struct.unpack_from("<iq", raw, at + index * 12)
            entry = PPtr(m_FileID=file_id, m_PathID=path_id, assetsfile=obj.assets_file).deref()
            entry_raw = entry.get_raw_data()
            asset_name, offset = _string(entry_raw, 28)
            if asset_name == name:
                key, _ = _string(entry_raw, offset)
                return key
    raise ValueError("当前游戏没有这个音频资源：" + name)


def extract_media(bank, media_id, destination):
    """DIDX contains (id, offset in DATA, length) entries; read only the selected media."""
    with Path(bank).open("rb") as stream:
        index = None
        while header := stream.read(8):
            if len(header) != 8:
                raise ValueError("音频包区块头不完整")
            kind, size = struct.unpack("<4sI", header)
            if kind == b"DIDX":
                if size % 12:
                    raise ValueError("音频包索引长度无效")
                index = stream.read(size)
                if len(index) != size:
                    raise ValueError("音频包索引不完整")
            elif kind == b"DATA" and index is not None:
                for ident, offset, length in struct.iter_unpack("<III", index):
                    if ident == media_id:
                        if length == 0 or offset + length > size:
                            raise ValueError("音频包媒体范围无效")
                        stream.seek(offset, 1)
                        data = stream.read(length)
                        if len(data) != length or data[:4] not in (b"RIFF", b"RIFX"):
                            raise ValueError("音频包媒体不完整或格式不支持")
                        Path(destination).write_bytes(data)
                        return
                raise ValueError("音频包中没有对应的媒体文件")
            else:
                stream.seek(size, 1)
    raise ValueError("音频包缺少媒体索引或数据")


def extract(game, kind, name, output):
    if kind not in ("music", "sound", "env"):
        raise ValueError("试听音频类型无效")
    game = Path(game).resolve(strict=True)
    output = Path(output).resolve()
    if game == output or game in output.parents:
        raise ValueError("试听缓存必须保存在游戏目录之外")
    if output.exists():
        raise ValueError("试听输出文件已存在")
    key = audio_key(game, kind, name)
    folder = game / "Mortal_Data/StreamingAssets/Audio/GeneratedSoundBanks/Windows"
    bank_name, prefix = {"music": ("Music", "Music_"), "sound": ("SFX", "Sound_"), "env": ("Env", "Env_")}[kind]
    info = ET.parse(folder / "SoundbanksInfo.xml")
    bank = next((node for node in info.findall("./SoundBanks/SoundBank") if node.findtext("ShortName") == bank_name), None)
    event = next((node for node in bank.findall("./IncludedEvents/Event") if node.get("Name") == prefix + key), None) if bank is not None else None
    if event is None:
        raise ValueError("没有找到音频事件：" + prefix + key)
    media = event.findall("./IncludedMemoryFiles/File")
    # A multi-source event has runtime selection/layering. Never substitute an arbitrary file.
    ids = {int(node.get("Id")) for node in media}
    if len(ids) != 1:
        raise ValueError("此音频事件包含多段或动态混音，目前不能作为单段试听；请在游戏中确认")
    extract_media(folder / (bank_name + ".bnk"), ids.pop(), output)
    return {"name": name, "kind": kind, "event_name": prefix + key}
