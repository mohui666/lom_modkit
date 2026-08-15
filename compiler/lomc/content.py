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
CONTENT_TYPES = ("audio", "character")
AUDIO_KINDS = ("music", "sound", "env")
AUDIO_EXTENSIONS = (".ogg", ".wav")
MAX_AUDIO_BYTES = 20 * 1024 * 1024
PACKAGE_USER_ROOT = "assets/user"

# 内容 ID：<namespace>.<local>，只允许小写字母、数字、下划线；禁止路径分隔。
# namespace 以字母开头，避免与纯数字/点号路径混淆。
CONTENT_ID_RE = re.compile(r"^[a-z][a-z0-9_]{0,31}\.[a-z0-9][a-z0-9_]{0,47}$")
CONTENT_TYPE_RE = re.compile(r"^[a-z][a-z0-9_]{0,15}$")
SAFE_FILENAME_RE = re.compile(r"^[A-Za-z0-9_\-\u4e00-\u9fff][A-Za-z0-9_\-\.\u4e00-\u9fff]{0,79}$")


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
    raw = os.path.basename(str(name or "")).replace("\\", "/")
    stem, ext = os.path.splitext(raw)
    ext = ext.lower()
    if ext not in AUDIO_EXTENSIONS:
        raise LomcError(
            "音频只支持 %s，实际为 %r"
            % (" / ".join(AUDIO_EXTENSIONS), raw or name)
        )
    stem = re.sub(r"[^A-Za-z0-9_\-\u4e00-\u9fff]+", "_", stem).strip("._") or "audio"
    filename = stem[:64] + ext
    if SAFE_FILENAME_RE.match(filename) is None or ".." in filename:
        raise LomcError("音频文件名不合法：%r" % filename)
    return filename


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
    main = os.path.basename(files["main"].replace("\\", "/"))
    if not main or ".." in files["main"] or "/" in files["main"].replace("\\", "/"):
        raise LomcError(
            "%s：files.main 必须是同目录下的文件名，不能含路径：%r"
            % (source, files["main"])
        )
    audio_kind = data.get("audio_kind")
    if ctype == "audio":
        if audio_kind not in AUDIO_KINDS:
            raise LomcError(
                "%s：音频的 audio_kind 必须是 music / sound / env，实际为 %r"
                % (source, audio_kind)
            )
        ext = os.path.splitext(main)[1].lower()
        if ext not in AUDIO_EXTENSIONS:
            raise LomcError(
                "%s：音频主文件必须是 %s，实际为 %r"
                % (source, " / ".join(AUDIO_EXTENSIONS), main)
            )
    return {
        "schema": CONTENT_SCHEMA,
        "id": content_id,
        "type": ctype,
        "name": name.strip(),
        "audio_kind": audio_kind if ctype == "audio" else None,
        "files": {"main": main},
    }


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
        size = os.path.getsize(main_path)
        if size <= 0:
            raise LomcError("用户内容 %s%s 的音频文件是空的" % (USER_PREFIX, content_id))
        if size > MAX_AUDIO_BYTES:
            raise LomcError(
                "用户内容 %s%s 的音频超过 20MB（当前 %.1fMB），请压缩后再导入。"
                % (USER_PREFIX, content_id, size / (1024.0 * 1024.0))
            )
    return meta, main_path


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
        meta, _path = resolve_content(self.mod_dir, "audio", ref.content_id)
        check_content_matches_kind(meta, expected_kind, ref.raw)
        return meta


class RepositoryContentResolver:
    """从开发环境全局仓库解析。"""

    def __init__(self, root=None):
        self.root = root or default_repository_root()

    def __call__(self, ref, expected_kind, item):
        meta, _path = resolve_content(self.root, "audio", ref.content_id)
        check_content_matches_kind(meta, expected_kind, ref.raw)
        return meta
