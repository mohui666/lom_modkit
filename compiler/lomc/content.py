# -*- coding: utf-8 -*-
"""用户内容引用协议与本地/包内解析。

契约见 docs/zh_CN/mod_format.md「用户内容」与 docs/zh_CN/user_content.md。

本模块是 Python 侧唯一的 user: 解析入口。编辑器仓库、编译校验、打包收集
都走这里，避免 editor / compiler / pack 各自实现一套字符串规则。

官方资源 ID（如 普通_001、brother4）保持原样，不迁移成 official: 前缀。
"""

from __future__ import annotations

import json
import os
import re

from .errors import LomcError

USER_PREFIX = "user:"
CONTENT_SCHEMA = 1
CONTENT_TYPES = ("audio", "character", "image")
AUDIO_KINDS = ("music", "sound", "env")
ART_FACINGS = ("left", "right")
ART_FACING_DEFAULT = "left"
CHARACTER_SCALE_DEFAULT = 100
CHARACTER_SCALE_MIN = 50
CHARACTER_SCALE_MAX = 130
AUDIO_EXTENSIONS = (".ogg", ".wav")
IMAGE_EXTENSIONS = (".png", ".jpg", ".jpeg")
MAX_AUDIO_BYTES = 20 * 1024 * 1024
MAX_IMAGE_BYTES = 8 * 1024 * 1024
PACKAGE_USER_ROOT = "assets/user"
CHARACTER_NODE_TYPES = (
    "show",
    "say",
    "hide",
    "move",
    "face",
    "focus",
    "offset",
    "shock",
    "dim",
    "rotate",
    "intro",
)
# affinity 会写入官方 CharacterData / 好感数值，不是纯舞台演出。自定义角色
# 没有官方好感数据槽，因此必须继续显式拒绝，不能把 user: id 传给原版 API。
UNSUPPORTED_USER_CHAR_TYPES = ("affinity",)

# 内容 ID：<namespace>.<local>，只允许小写字母、数字、下划线；禁止路径分隔。
# namespace 以字母开头，避免与纯数字/点号路径混淆。
CONTENT_ID_RE = re.compile(r"^[a-z][a-z0-9_]{0,31}\.[a-z0-9][a-z0-9_]{0,47}$")
CONTENT_TYPE_RE = re.compile(r"^[a-z][a-z0-9_]{0,15}$")
PORTRAIT_ID_RE = re.compile(r"^[A-Za-z][A-Za-z0-9_]{0,31}$")
SAFE_FILENAME_RE = re.compile(r"^[A-Za-z0-9_\-\u4e00-\u9fff][A-Za-z0-9_\-\.\u4e00-\u9fff]{0,79}$")
# 官方人物 id（无 user: 前缀）。仅作音频管理归属，不要求角色存在。
OFFICIAL_CHARACTER_ID_RE = re.compile(r"^[A-Za-z][A-Za-z0-9_]{0,47}$")


class ContentRef:
    """一条稳定的用户内容引用（story 里保存的值）。"""

    __slots__ = ("raw", "content_id", "namespace", "local_id")

    def __init__(self, content_id):
        self.content_id = content_id
        self.raw = USER_PREFIX + content_id
        ns, local = content_id.split(".", 1)
        self.namespace = ns
        self.local_id = local

    def __repr__(self):
        return "ContentRef(%r)" % self.raw

    def __eq__(self, other):
        return isinstance(other, ContentRef) and self.raw == other.raw

    def __hash__(self):
        return hash(self.raw)


def is_user_ref(value):
    """是否以 user: 开头（不校验后面是否合法）。"""
    return isinstance(value, str) and value.startswith(USER_PREFIX)


def validate_content_id(content_id, label="内容 ID"):
    """校验裸内容 ID（不含 user: 前缀）。不通过抛 LomcError。"""
    if not isinstance(content_id, str) or not content_id:
        raise LomcError("%s不能为空" % label)
    if ".." in content_id or "/" in content_id or "\\" in content_id or ":" in content_id:
        raise LomcError(
            "%s %r 含有非法路径字符。内容 ID 只能是 命名空间.名称，"
            "例如 mohui.boss_theme。" % (label, content_id)
        )
    if CONTENT_ID_RE.match(content_id) is None:
        raise LomcError(
            "%s %r 不合法。必须是 小写命名空间.名称（只含字母、数字、下划线），"
            "例如 mohui.boss_theme。" % (label, content_id)
        )
    return content_id


def parse_content_ref(value, label="用户内容"):
    """解析 user:... 引用。

    不是 user: 前缀时返回 None（官方 ID 走原逻辑）。
    是 user: 但格式非法时抛 LomcError。
    """
    if not is_user_ref(value):
        return None
    body = value[len(USER_PREFIX) :]
    validate_content_id(body, label=label)
    return ContentRef(body)


def make_content_ref(content_id):
    """由裸 ID 构造完整引用字符串。"""
    validate_content_id(content_id)
    return USER_PREFIX + content_id


def package_content_dir(content_type, content_id):
    """包内/仓库内该内容的相对目录（正斜杠）。"""
    if not isinstance(content_type, str) or CONTENT_TYPE_RE.match(content_type) is None:
        raise LomcError("不支持的内容类型 %r" % (content_type,))
    validate_content_id(content_id)
    return "%s/%s/%s" % (PACKAGE_USER_ROOT, content_type, content_id)


def safe_audio_filename(name):
    """把导入文件名收成可放进内容目录的安全名（保留扩展名）。"""
    return _safe_filename(name, AUDIO_EXTENSIONS, "audio", "音频")


def safe_image_filename(name):
    """把导入立绘文件名收成可放进内容目录的安全名（保留扩展名）。"""
    return _safe_filename(name, IMAGE_EXTENSIONS, "portrait", "立绘")


def _safe_filename(name, allowed_exts, fallback_stem, label):
    raw = os.path.basename(str(name or "")).replace("\\", "/")
    stem, ext = os.path.splitext(raw)
    ext = ext.lower()
    if ext not in allowed_exts:
        raise LomcError(
            "%s只支持 %s，实际为 %r"
            % (label, " / ".join(allowed_exts), raw or name)
        )
    stem = re.sub(r"[^A-Za-z0-9_\-\u4e00-\u9fff]+", "_", stem).strip("._") or fallback_stem
    filename = stem[:64] + ext
    if SAFE_FILENAME_RE.match(filename) is None or ".." in filename:
        raise LomcError("%s文件名不合法：%r" % (label, filename))
    return filename


def validate_portrait_id(portrait_id, label="表情"):
    if not isinstance(portrait_id, str) or not portrait_id:
        raise LomcError("%s不能为空" % label)
    if PORTRAIT_ID_RE.match(portrait_id) is None:
        raise LomcError(
            "%s %r 不合法。表情 id 只能是字母开头、后接字母数字下划线，"
            "例如 normal / happy / angry。" % (label, portrait_id)
        )
    return portrait_id


def normalize_audio_character(value, source="content.json"):
    """规范化音频上可选的角色归属。

    只是编辑器管理关系，不参与播放、校验 say.voice、也不导致打包。
    None / 空字符串 → None（旁白、系统语音、未关联）。
    ``user:`` 引用或裸内容 ID → ``user:<id>``。
    官方人物 id（如 player / brother4）保持原样，不生成用户角色对象。
    """
    if value is None:
        return None
    if not isinstance(value, str):
        raise LomcError("%s：character 必须是字符串，或省略" % source)
    text = value.strip()
    if not text:
        return None
    if is_user_ref(text):
        parse_content_ref(text, label="%s 的 character" % source)
        return text
    if CONTENT_ID_RE.match(text):
        return USER_PREFIX + text
    if ".." in text or "/" in text or "\\" in text or ":" in text:
        raise LomcError(
            "%s：character %r 含有非法路径字符。请填写 user:命名空间.名称，"
            "或官方人物 id（例如 player）。" % (source, text)
        )
    if OFFICIAL_CHARACTER_ID_RE.match(text) is None:
        raise LomcError(
            "%s：character %r 不合法。请填写 user:命名空间.名称，"
            "或官方人物 id（例如 player / brother4）。" % (source, text)
        )
    return text


def _safe_same_dir_filename(raw, label, allowed_exts):
    if not isinstance(raw, str) or not raw.strip():
        raise LomcError("%s必须是文件名字符串" % label)
    name = os.path.basename(raw.replace("\\", "/"))
    if not name or ".." in raw or "/" in raw.replace("\\", "/"):
        raise LomcError("%s必须是同目录下的文件名，不能含路径：%r" % (label, raw))
    ext = os.path.splitext(name)[1].lower()
    if allowed_exts and ext not in allowed_exts:
        raise LomcError(
            "%s必须是 %s，实际为 %r" % (label, " / ".join(allowed_exts), name)
        )
    return name


def expected_audio_kind(node):
    """节点期望的 audio_kind；非音频节点返回 None。"""
    ntype = node.get("type")
    if ntype == "music":
        return "music"
    if ntype == "sound":
        return "env" if node.get("kind", "sound") == "env" else "sound"
    return None


def collect_story_content_refs(story):
    """扫描一个 story，返回用户内容引用列表。

    每项：{"node_id", "node_type", "field", "raw", "ref", "expected_kind"}
    expected_kind 仅音频节点有值（music/sound/env）。
    """
    found = []
    nodes = story.get("nodes") if isinstance(story, dict) else None
    if not isinstance(nodes, list):
        return found
    for node in nodes:
        if not isinstance(node, dict):
            continue
        nid = node.get("id") if isinstance(node.get("id"), str) else "?"
        ntype = node.get("type")
        kind = expected_audio_kind(node)
        if kind is not None:
            raw = node.get("name")
            ref = parse_content_ref(raw, label='节点 "%s"(%s) 的 name' % (nid, ntype))
            if ref is not None:
                found.append(
                    {
                        "node_id": nid,
                        "node_type": ntype,
                        "field": "name",
                        "raw": ref.raw,
                        "ref": ref,
                        "expected_kind": kind,
                        "expected_type": "audio",
                    }
                )
        if ntype == "say" and node.get("voice"):
            ref = parse_content_ref(
                node.get("voice"), label='节点 "%s"(say) 的 voice' % nid
            )
            if ref is not None:
                found.append(
                    {
                        "node_id": nid,
                        "node_type": ntype,
                        "field": "voice",
                        "raw": ref.raw,
                        "ref": ref,
                        "expected_kind": None,
                        "expected_type": "audio",
                    }
                )
        if ntype in CHARACTER_NODE_TYPES:
            raw = node.get("character")
            if raw:
                ref = parse_content_ref(
                    raw, label='节点 "%s"(%s) 的 character' % (nid, ntype)
                )
                if ref is not None:
                    found.append(
                        {
                            "node_id": nid,
                            "node_type": ntype,
                            "field": "character",
                            "raw": ref.raw,
                            "ref": ref,
                            "expected_kind": None,
                            "expected_type": "character",
                            "portrait": node.get("portrait")
                            if ntype in ("show", "say")
                            else None,
                        }
                    )
        # 统一图片协议：所有图片型节点都把稳定引用放在 image 字段。
        # 当前及后续 background / CG / overlay 共用这一条收集路径，不各建 Store。
        image_is_active = not (
            (ntype == "custom_cg" and node.get("action", "show") == "hide")
            or (ntype == "overlay" and node.get("action", "show") == "hide")
            or (
                ntype == "background"
                and node.get("action", "show") in ("fadeout", "clear")
            )
        )
        raw_image = node.get("image") if image_is_active else None
        if raw_image:
            ref = parse_content_ref(
                raw_image, label='节点 "%s"(%s) 的 image' % (nid, ntype)
            )
            if ref is not None:
                found.append(
                    {
                        "node_id": nid,
                        "node_type": ntype,
                        "field": "image",
                        "raw": ref.raw,
                        "ref": ref,
                        "expected_kind": None,
                        "expected_type": "image",
                    }
                )
    return found


def collect_stories_content_refs(stories):
    """扫描多个 story（dict[id, story] 或 list）。"""
    found = []
    if isinstance(stories, dict):
        items = stories.items()
    else:
        items = ((s.get("id") if isinstance(s, dict) else "?", s) for s in stories)
    for sid, story in items:
        for item in collect_story_content_refs(story):
            item = dict(item)
            item["story_id"] = sid
            found.append(item)
    return found


def load_content_metadata(meta_path):
    """读取并校验 content.json，返回规范化 dict。损坏时抛 LomcError。"""
    try:
        with open(meta_path, "r", encoding="utf-8-sig") as f:
            data = json.load(f)
    except FileNotFoundError:
        raise LomcError("缺少 content.json：%s" % meta_path)
    except (OSError, UnicodeDecodeError) as exc:
        raise LomcError("无法读取 content.json（%s）：%s" % (meta_path, exc))
    except json.JSONDecodeError as exc:
        raise LomcError(
            "content.json 不是合法 JSON（%s 第 %d 行）：%s"
            % (meta_path, exc.lineno, exc.msg)
        )
    return normalize_content_metadata(data, source=meta_path)


def normalize_content_metadata(data, source="content.json"):
    """校验 content.json 对象并补齐缺省。"""
    if not isinstance(data, dict):
        raise LomcError("%s：顶层必须是 JSON 对象" % source)
    schema = data.get("schema")
    if schema != CONTENT_SCHEMA or isinstance(schema, bool):
        raise LomcError(
            "%s：schema 必须是 %d（当前用户内容格式版本），实际为 %r"
            % (source, CONTENT_SCHEMA, schema)
        )
    content_id = data.get("id")
    validate_content_id(content_id, label="%s 的 id" % source)
    ctype = data.get("type")
    if ctype not in CONTENT_TYPES:
        raise LomcError(
            "%s：type 必须是 %s 之一，实际为 %r"
            % (source, " / ".join(CONTENT_TYPES), ctype)
        )
    name = data.get("name")
    if not isinstance(name, str) or not name.strip():
        raise LomcError("%s：name（显示名称）必须是非空字符串" % source)
    files = data.get("files")
    if not isinstance(files, dict) or not isinstance(files.get("main"), str):
        raise LomcError('%s：files.main 必须是主文件名字符串' % source)
    allowed_main = AUDIO_EXTENSIONS if ctype == "audio" else IMAGE_EXTENSIONS
    main = _safe_same_dir_filename(
        files["main"], "%s 的 files.main" % source, allowed_main
    )
    audio_kind = data.get("audio_kind")
    portraits = None
    character = None
    if ctype == "audio":
        if audio_kind not in AUDIO_KINDS:
            raise LomcError(
                "%s：音频的 audio_kind 必须是 music / sound / env，实际为 %r"
                % (source, audio_kind)
            )
        character = normalize_audio_character(data.get("character"), source)
    elif ctype == "character":
        portraits = _normalize_portraits(data.get("portraits"), main, source)
        if portraits["normal"] != main and main not in portraits.values():
            portraits["normal"] = main
    intro = None
    character_title = None
    character_scale = None
    art_facing = None
    if ctype == "character":
        intro = normalize_character_intro(data.get("intro"), source)
        raw_title = data.get("title")
        if raw_title is None:
            raw_title = (intro or {}).get("title")
        if isinstance(raw_title, str) and raw_title.strip():
            character_title = raw_title.strip()
        elif raw_title not in (None, ""):
            raise LomcError("%s：title（对话称号）必须是字符串" % source)
        character_scale = normalize_character_scale(data.get("scale"), source)
        art_facing = normalize_art_facing(data.get("art_facing"), source)
    return {
        "schema": CONTENT_SCHEMA,
        "id": content_id,
        "type": ctype,
        "name": name.strip(),
        "audio_kind": audio_kind if ctype == "audio" else None,
        "files": {"main": main},
        "portraits": portraits,
        "character": character,
        "intro": intro,
        "title": character_title,
        "scale": character_scale,
        "art_facing": art_facing,
    }


def _normalize_portraits(raw, main_file, source):
    if raw is None:
        raw = {"normal": main_file}
    if not isinstance(raw, dict) or not raw:
        raise LomcError("%s：角色必须提供 portraits（表情 id -> 文件名）" % source)
    portraits = {}
    for key, filename in raw.items():
        if not isinstance(key, str):
            raise LomcError("%s：portraits 的键必须是字符串" % source)
        validate_portrait_id(key, label="%s 的表情" % source)
        fname = _safe_same_dir_filename(
            filename, "%s 的 portraits.%s" % (source, key), IMAGE_EXTENSIONS
        )
        portraits[key] = fname
    if "normal" not in portraits:
        portraits["normal"] = main_file
    return portraits


def normalize_character_scale(value, source="content.json"):
    """角色体型百分比。缺省 100；超出 50–130 时夹到边界。"""
    if value is None or value == "":
        return CHARACTER_SCALE_DEFAULT
    return _clamp_number(
        value, CHARACTER_SCALE_DEFAULT, CHARACTER_SCALE_MIN, CHARACTER_SCALE_MAX
    )


def normalize_art_facing(value, source="content.json"):
    """立绘原图朝向。缺省 left（与原版立绘一致）。"""
    if value is None or value == "":
        return ART_FACING_DEFAULT
    if not isinstance(value, str):
        raise LomcError("%s：art_facing 必须是 left 或 right" % source)
    text = value.strip().lower()
    if text not in ART_FACINGS:
        raise LomcError(
            "%s：art_facing 必须是 left 或 right，实际为 %r" % (source, value)
        )
    return text


def normalize_character_intro(raw, source="content.json"):
    """角色可选介绍卡。缺省或空对象视为没有介绍卡。"""
    if raw is None:
        return None
    if not isinstance(raw, dict):
        raise LomcError("%s：intro 必须是对象，或省略" % source)
    if not raw:
        return None
    name = raw.get("name")
    text = raw.get("text")
    if not isinstance(name, str) or not name.strip():
        raise LomcError("%s：介绍卡 name（姓名）必须是非空字符串" % source)
    if not isinstance(text, str) or not text.strip():
        raise LomcError("%s：介绍卡 text（介绍）必须是非空字符串" % source)
    title = raw.get("title")
    if title is None:
        title = ""
    if not isinstance(title, str):
        raise LomcError("%s：介绍卡 title 必须是字符串" % source)
    image = raw.get("image")
    if image is None or (isinstance(image, str) and not image.strip()):
        image = None
    else:
        image = _safe_same_dir_filename(
            image, "%s 的 intro.image" % source, IMAGE_EXTENSIONS
        )
    return {
        "title": title.strip(),
        "name": name.strip(),
        "text": text.strip(),
        "image": image,
        "image_scale": _clamp_number(raw.get("image_scale"), 100, 40, 160),
        "image_x": _clamp_number(raw.get("image_x"), 0, -30, 30),
        "image_y": _clamp_number(raw.get("image_y"), 0, -30, 30),
    }


def _clamp_number(value, default, lo, hi):
    if value is None or isinstance(value, bool):
        return default
    try:
        number = int(value)
    except (TypeError, ValueError):
        return default
    if number < lo:
        return lo
    if number > hi:
        return hi
    return number


def listed_content_files(meta):
    """content.json 声明的全部同目录文件名（去重，主文件在前）。"""
    names = []
    main = meta.get("files", {}).get("main")
    if main:
        names.append(main)
    for fname in (meta.get("portraits") or {}).values():
        if fname and fname not in names:
            names.append(fname)
    intro_image = (meta.get("intro") or {}).get("image")
    if intro_image and intro_image not in names:
        names.append(intro_image)
    return names


def write_content_metadata(meta_path, metadata):
    """把规范化 metadata 写成 content.json。"""
    normalized = normalize_content_metadata(metadata, source=meta_path)
    payload = {
        "schema": normalized["schema"],
        "id": normalized["id"],
        "type": normalized["type"],
        "name": normalized["name"],
        "files": normalized["files"],
    }
    if normalized["type"] == "audio":
        payload["audio_kind"] = normalized["audio_kind"]
        if normalized.get("character"):
            payload["character"] = normalized["character"]
    if normalized["type"] == "character" and normalized.get("portraits"):
        payload["portraits"] = dict(normalized["portraits"])
    if normalized["type"] == "character" and normalized.get("title"):
        payload["title"] = normalized["title"]
    if (
        normalized["type"] == "character"
        and normalized.get("scale") not in (None, CHARACTER_SCALE_DEFAULT)
    ):
        payload["scale"] = normalized["scale"]
    if (
        normalized["type"] == "character"
        and normalized.get("art_facing")
        and normalized["art_facing"] != ART_FACING_DEFAULT
    ):
        payload["art_facing"] = normalized["art_facing"]
    if normalized["type"] == "character" and normalized.get("intro"):
        payload["intro"] = dict(normalized["intro"])
    parent = os.path.dirname(meta_path)
    if parent:
        os.makedirs(parent, exist_ok=True)
    with open(meta_path, "w", encoding="utf-8", newline="\n") as f:
        json.dump(payload, f, ensure_ascii=False, indent=2)
        f.write("\n")
    return normalized


def resolve_content_dir(root, content_type, content_id):
    """在仓库或包根目录下定位内容目录。目录不存在返回 None。"""
    rel = package_content_dir(content_type, content_id).replace("/", os.sep)
    full = os.path.normpath(os.path.join(root, rel))
    root_norm = os.path.normpath(root)
    if full != root_norm and not full.startswith(root_norm + os.sep):
        raise LomcError("内容路径逃逸：%s / %s" % (content_type, content_id))
    return full if os.path.isdir(full) else None


def resolve_content(root, content_type, content_id):
    """解析一条内容：返回 (metadata, main_file_abs)。失败抛 LomcError。"""
    folder = resolve_content_dir(root, content_type, content_id)
    if folder is None:
        raise LomcError(
            "找不到用户内容 %s%s（类型 %s）。"
            "请确认已导入用户内容库，或包内 assets/user/ 含有该资源。"
            % (USER_PREFIX, content_id, content_type)
        )
    meta = load_content_metadata(os.path.join(folder, "content.json"))
    if meta["id"] != content_id:
        raise LomcError(
            "用户内容 %s 的 content.json id=%r 与目录名不一致"
            % (content_id, meta["id"])
        )
    if meta["type"] != content_type:
        raise LomcError(
            "用户内容 %s 的 type=%s，与目录类型 %s 不一致"
            % (content_id, meta["type"], content_type)
        )
    main_path = os.path.normpath(os.path.join(folder, meta["files"]["main"]))
    folder_norm = os.path.normpath(folder)
    if main_path != folder_norm and not main_path.startswith(folder_norm + os.sep):
        raise LomcError("用户内容 %s 的主文件路径逃逸" % content_id)
    if not os.path.isfile(main_path):
        raise LomcError(
            "用户内容 %s%s 的文件不存在：%s"
            % (USER_PREFIX, content_id, meta["files"]["main"])
        )
    if meta["type"] == "audio":
        _check_file_size(main_path, USER_PREFIX + content_id, MAX_AUDIO_BYTES, "音频", 20)
    elif meta["type"] == "character":
        for fname in listed_content_files(meta):
            image_path = os.path.normpath(os.path.join(folder, fname))
            if image_path != folder_norm and not image_path.startswith(folder_norm + os.sep):
                raise LomcError("用户内容 %s 的立绘路径逃逸：%s" % (content_id, fname))
            if not os.path.isfile(image_path):
                raise LomcError(
                    "自定义角色 %s%s 的立绘不存在：%s"
                    % (USER_PREFIX, content_id, fname)
                )
            _check_file_size(
                image_path, USER_PREFIX + content_id + "/" + fname, MAX_IMAGE_BYTES, "立绘", 8
            )
    elif meta["type"] == "image":
        _check_file_size(main_path, USER_PREFIX + content_id, MAX_IMAGE_BYTES, "图片", 8)
    return meta, main_path


def _check_file_size(path, label, limit, kind, limit_mb):
    size = os.path.getsize(path)
    if size <= 0:
        raise LomcError("用户内容 %s 的%s文件是空的" % (label, kind))
    if size > limit:
        raise LomcError(
            "用户内容 %s 的%s超过 %sMB（当前 %.1fMB），请压缩后再导入。"
            % (label, kind, limit_mb, size / (1024.0 * 1024.0))
        )


def scan_repository(root, content_type=None):
    """扫描仓库/包根下 assets/user/<type>/<id>/content.json。损坏的条目跳过。

    返回 [(metadata, folder_abs), ...]，按 id 排序。
    """
    results = []
    user_root = os.path.join(root, PACKAGE_USER_ROOT.replace("/", os.sep))
    if not os.path.isdir(user_root):
        return results
    try:
        type_names = sorted(os.listdir(user_root))
    except OSError:
        return results
    for tname in type_names:
        if content_type is not None and tname != content_type:
            continue
        if CONTENT_TYPE_RE.match(tname) is None:
            continue
        type_dir = os.path.join(user_root, tname)
        if not os.path.isdir(type_dir):
            continue
        try:
            id_names = sorted(os.listdir(type_dir))
        except OSError:
            continue
        for cid in id_names:
            try:
                validate_content_id(cid)
            except LomcError:
                continue
            folder = os.path.join(type_dir, cid)
            meta_path = os.path.join(folder, "content.json")
            if not os.path.isfile(meta_path):
                continue
            try:
                meta = load_content_metadata(meta_path)
            except LomcError:
                continue
            if meta["id"] != cid or meta["type"] != tname:
                continue
            results.append((meta, folder))
    return results


def default_repository_root():
    """开发环境全局仓库：%APPDATA%/lom_modkit/repository。"""
    appdata = os.environ.get("APPDATA")
    base = appdata if appdata else os.path.join(os.path.expanduser("~"), "AppData", "Roaming")
    return os.path.join(base, "lom_modkit", "repository")


def validate_story_content_refs(story, resolver, source="story.json"):
    """用 resolver(ref, expected_kind) 检查 story 里全部 user: 引用。

    resolver 必须在失败时抛 LomcError（面向作者的中文消息）。
    """
    try:
        for item in collect_story_content_refs(story):
            resolver(item["ref"], item["expected_kind"], item)
    except LomcError as exc:
        raise LomcError("%s: %s" % (source, exc))


def check_character_portrait(meta, portrait, raw):
    """角色是否包含该表情。portrait 为空时只检查 type。"""
    if meta.get("type") != "character":
        raise LomcError(
            "用户内容 %s 是 %s，不能用在人物步骤。请改用自定义角色。"
            % (raw, meta.get("type"))
        )
    if not portrait:
        return
    validate_portrait_id(portrait, label="表情")
    portraits = meta.get("portraits") or {}
    if portrait not in portraits:
        have = "、".join(sorted(portraits)) or "无"
        raise LomcError(
            '自定义角色 %s 没有表情 "%s"（已有：%s）。请改用清单内表情，'
            "或在用户内容库里补这张立绘。" % (raw, portrait, have)
        )


def check_content_matches_kind(meta, expected_kind, raw):
    """音频 type / audio_kind 是否匹配节点。"""
    if expected_kind is None:
        return
    if meta.get("type") != "audio":
        raise LomcError(
            "用户内容 %s 是 %s，不能用在音乐/音效步骤。请改用音频内容。"
            % (raw, meta.get("type"))
        )
    kind = meta.get("audio_kind")
    if kind != expected_kind:
        kind_cn = {"music": "音乐", "sound": "音效", "env": "环境音"}.get(kind, kind)
        need_cn = {"music": "音乐", "sound": "音效", "env": "环境音"}.get(
            expected_kind, expected_kind
        )
        raise LomcError(
            "用户内容 %s 是%s，不能用在%s步骤。请改用对应类型的步骤，"
            "或在用户内容库里把它标成%s。"
            % (raw, kind_cn, need_cn, need_cn)
        )


class PackageContentResolver:
    """从已经放进 mod 目录的 assets/user/ 解析。"""

    def __init__(self, mod_dir):
        self.mod_dir = mod_dir

    def __call__(self, ref, expected_kind, item):
        return _resolve_ref(self.mod_dir, ref, expected_kind, item)


class RepositoryContentResolver:
    """从开发环境全局仓库解析。"""

    def __init__(self, root=None):
        self.root = root or default_repository_root()

    def __call__(self, ref, expected_kind, item):
        return _resolve_ref(self.root, ref, expected_kind, item)


def _resolve_ref(root, ref, expected_kind, item):
    ctype = (item or {}).get("expected_type") or "audio"
    meta, _path = resolve_content(root, ctype, ref.content_id)
    if ctype == "character":
        check_character_portrait(meta, (item or {}).get("portrait"), ref.raw)
    else:
        check_content_matches_kind(meta, expected_kind, ref.raw)
    return meta
