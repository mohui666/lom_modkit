# -*- coding: utf-8 -*-
"""Byte-level linkage between packaged Story sources and generated Lua."""

from __future__ import annotations

import hashlib

from .errors import LomcError


STORY_LUA_INTEGRITY_ENTRY = "story-lua.sha256"
STORY_LUA_INTEGRITY_ALGORITHM = "lom-story-lua-sha256-v1"


def _sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest().upper()


def build_story_lua_integrity(pairs) -> bytes:
    """Build the deterministic four-column integrity record.

    ``pairs`` contains ``(story_path, story_bytes, lua_path, lua_bytes)``.
    Default and localized Lua variants each get a row and rows are sorted by
    Lua path using ordinal Unicode ordering, matching the Runtime.
    """
    rows = []
    seen = set()
    for story_path, story_data, lua_path, lua_data in pairs:
        if "\t" in story_path or "\t" in lua_path or "\n" in story_path or "\n" in lua_path:
            raise LomcError("Story/Lua 完整性路径含非法控制字符")
        if lua_path in seen:
            raise LomcError("Story/Lua 完整性记录包含重复 Lua 路径：%s" % lua_path)
        seen.add(lua_path)
        rows.append(
            (lua_path, "%s\t%s\t%s\t%s" % (
                story_path, _sha256(story_data), lua_path, _sha256(lua_data)
            ))
        )
    lines = ["algorithm=" + STORY_LUA_INTEGRITY_ALGORITHM]
    lines.extend(row for _path, row in sorted(rows, key=lambda item: item[0]))
    return ("\n".join(lines) + "\n").encode("utf-8")


def parse_story_lua_integrity(data: bytes):
    """Parse a record, returning ``lua_path -> (story_path, story_sha, lua_sha)``."""
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise LomcError("story-lua.sha256 不是合法 UTF-8") from exc
    lines = text.splitlines()
    if not lines or lines[0] != "algorithm=" + STORY_LUA_INTEGRITY_ALGORITHM:
        raise LomcError("story-lua.sha256 algorithm 无效")
    result = {}
    for number, line in enumerate(lines[1:], 2):
        if not line:
            continue
        fields = line.split("\t")
        if len(fields) != 4:
            raise LomcError("story-lua.sha256 第 %d 行必须为四列" % number)
        story_path, story_sha, lua_path, lua_sha = fields
        if (
            not story_path.startswith("story/")
            or not story_path.endswith(".json")
            or not lua_path.startswith("lua/")
            or not lua_path.endswith(".lua")
            or len(story_sha) != 64
            or len(lua_sha) != 64
        ):
            raise LomcError("story-lua.sha256 第 %d 行路径或 SHA-256 无效" % number)
        try:
            int(story_sha, 16)
            int(lua_sha, 16)
        except ValueError as exc:
            raise LomcError("story-lua.sha256 第 %d 行 SHA-256 无效" % number) from exc
        if story_sha != story_sha.upper() or lua_sha != lua_sha.upper():
            raise LomcError("story-lua.sha256 第 %d 行 SHA-256 必须大写" % number)
        if lua_path in result:
            raise LomcError("story-lua.sha256 重复记录 Lua 路径：%s" % lua_path)
        result[lua_path] = (story_path, story_sha, lua_sha)
    if not result:
        raise LomcError("story-lua.sha256 没有 Story/Lua 记录")
    return result


def verify_story_lua_integrity(data: bytes, entry_bytes: dict[str, bytes]) -> None:
    """Verify hashes and require every packaged Lua script to be linked."""
    records = parse_story_lua_integrity(data)
    expected_lua = {
        name for name in entry_bytes
        if name.startswith("lua/") and name.endswith(".lua")
    }
    if set(records) != expected_lua:
        missing = sorted(expected_lua - set(records))
        extra = sorted(set(records) - expected_lua)
        detail = "缺记录 %s；多余记录 %s" % (missing, extra)
        raise LomcError("story-lua.sha256 与 Lua 条目集合不一致：" + detail)
    for lua_path, (story_path, story_sha, lua_sha) in records.items():
        story_data = entry_bytes.get(story_path)
        lua_data = entry_bytes.get(lua_path)
        if story_data is None:
            raise LomcError("story-lua.sha256 引用了缺失 Story：%s" % story_path)
        if _sha256(story_data) != story_sha or _sha256(lua_data) != lua_sha:
            raise LomcError("Story/Lua 完整性校验失败：%s / %s" % (story_path, lua_path))
