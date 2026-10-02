# -*- coding: utf-8 -*-
"""新增节点类型后，把文档里的节点数从 62 改成实际值（只跑一次的工具脚本）。

documentation_consistency_test 要求 README / 文档索引 / 能力清单共 12 个文件里
都出现当前节点数，所以加节点之后必须同步，否则测试直接失败。
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

DOCS = (
    ROOT / "README.md",
    ROOT / "README.cht.md",
    ROOT / "README.ja.md",
    ROOT / "README.ko.md",
) + tuple(
    path
    for path in sorted((ROOT / "docs").rglob("*.md"))
    # 反编译产物不入库，也不该被脚本改写
    if "decompiled" not in path.parts
)

# 只替换「节点数量」语境下的数字：后面紧跟量词，或前面是「共 / 目前共 / 当前共 /
# 現在 / 현재」这类统计词。避免误改版本号、行号等无关数字。
COUNT_WORDS = r"(?:种|種|종|개|個|ノード)"
PATTERNS = (
    re.compile(r"\b62(?=\s*" + COUNT_WORDS + r")"),
    re.compile(r"(?<=共 )62\b"),
    re.compile(r"(?<=目前共 )62\b"),
    re.compile(r"(?<=當前共 )62\b"),
    re.compile(r"(?<=現在 )62\b"),
    re.compile(r"(?<=현재 )62종"),
)


def main() -> int:
    old, new = "62", "63"
    total = 0
    for path in DOCS:
        if not path.is_file():
            continue
        text = path.read_text(encoding="utf-8")
        updated, hits = PATTERNS[0].subn(new, text)
        for pattern in PATTERNS[1:]:
            updated, more = pattern.subn(new, updated)
            hits += more
        if not hits:
            continue
        path.write_text(updated, encoding="utf-8")
        total += hits
        print(f"[OK ] {path.relative_to(ROOT)}：替换 {hits} 处")
    print(f"共替换 {total} 处")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
