# -*- coding: utf-8 -*-
"""story_api — 供 AI / 脚本调用方使用的剧情数据工具接口（纯数据 + 编译，无 UI 依赖）。

设计原则：
1. 固定规则转换：所有写操作都走固定规则——节点由 models 的契约默认值生成、
   字段按 NODE_SCHEMAS 做宽松类型校验，未知字段一律拒绝；调用方只提供语义值，
   不可能绕过规则手写 story JSON 或 Lua。
2. 防写坏：把游戏侧已知崩溃坑挡在写入口（choice 皮肤锁死 Options、dice 检查点
   必须命中官方元数据、say 模式与人物联动），编译期剩余问题交给 lomc 校验
   （check_story / compile_story），编辑器与 AI 共用同一套防线。
3. AI 友好：函数小而语义明确、错误消息全中文、错误与警告都是字符串列表；
   另带 argparse CLI（check / compile / pack / new-story），退出码 0/1，
   适合 AI 以子进程方式调用。
"""

from __future__ import annotations

import sys
from pathlib import Path

# 与 editor/lua_preview.py 相同的路径推导：无论从仓库根还是 editor/ 启动都成立
EDITOR_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = EDITOR_DIR.parent
COMPILER_DIR = PROJECT_ROOT / "compiler"

# models 与 story_api 同在 editor/ 内；确保从任意 cwd 都能 import
if str(EDITOR_DIR) not in sys.path:
    sys.path.insert(0, str(EDITOR_DIR))

import models  # noqa: E402

# ---------------------------------------------------------------------------
# lomc 懒加载（契约方式引入，见 editor/lua_preview.py 的 get_lomc 写法）
# ---------------------------------------------------------------------------
_lomc = None
_lomc_error: str | None = None


def get_lomc():
    """按契约方式引入 lomc（sys.path 插入 <项目根>/compiler）；不可用返回 (None, 原因)。"""
    global _lomc, _lomc_error
    if _lomc is not None or _lomc_error is not None:
        return _lomc, _lomc_error
    path = str(COMPILER_DIR)
    if path not in sys.path:
        sys.path.insert(0, path)
    try:
        import lomc  # noqa: F401
        import lomc.dice_data  # noqa: F401  显式加载骰子元数据子模块

        if not hasattr(lomc, "compile_story"):
            raise AttributeError("lomc 缺少 compile_story(story) 接口")
        _lomc = lomc
    except Exception as exc:  # lomc 尚未就绪/导入失败时给出可读原因
        _lomc_error = f"{type(exc).__name__}: {exc}"
    return _lomc, _lomc_error


def _require_lomc():
    """取 lomc 模块；不可用抛 ValueError（保持接口只抛 ValueError/LomcError 的约定）。"""
    lomc, err = get_lomc()
    if lomc is None:
        raise ValueError(f"lomc 编译器不可用（{err}）。预期位置：{COMPILER_DIR}")
    return lomc


# ---------------------------------------------------------------------------
# editor_data：公开接口每次从磁盘读取；内部默认值/摘要用进程级缓存
# ---------------------------------------------------------------------------
_ED_CACHE: tuple[dict, bool] | None = None


def load_editor_data() -> tuple[dict, bool]:
    """读取 <项目根>/data/editor_data.json，返回 (数据, 是否兜底)。

    每次调用都重新读取，保证拿到最新清单（schema 1/2/3 兼容，缺文件时兜底）。
    """
    return models.load_editor_data(PROJECT_ROOT)


def _get_ed() -> dict:
    """内部用：带进程级缓存的 editor_data（新节点默认值/列表摘要用）。"""
    global _ED_CACHE
    if _ED_CACHE is None:
        _ED_CACHE = models.load_editor_data(PROJECT_ROOT)
    return _ED_CACHE[0]


# ---------------------------------------------------------------------------
# 字段校验：所有写操作共用的"固定规则"层
# ---------------------------------------------------------------------------
# 任何节点都允许的通用字段
_COMMON_FIELDS = ("id", "type", "goto")
# kind 为数组型的字段
_LIST_KINDS = {"options", "cases", "vars", "dice_options"}


def _check_kind(kind: str, value) -> bool:
    """按 NODE_SCHEMAS 的 kind 做宽松类型校验（不修改值）。"""
    if kind in ("int", "float"):
        # bool 是 int 的子类，数值字段不接受 true/false（与编译器一致）
        return isinstance(value, (int, float)) and not isinstance(value, bool)
    if kind == "bool":
        return isinstance(value, bool)
    if kind in _LIST_KINDS:
        return isinstance(value, list)
    # character/portrait/position/view/music/stat/mode/facing/branch_source/
    # enum:*/menu_dialog/effect/camera/talent/game_flag/dice_check/item/
    # goto_scene_key/story_ref/line/multiline/code 等一律要字符串
    return isinstance(value, str)


def _check_fields(node_type: str, fields: dict | None) -> dict:
    """校验字段键（NODE_SCHEMAS 合法字段 + 通用字段）与类型；不通过抛 ValueError。"""
    if fields is None:
        return {}
    if not isinstance(fields, dict):
        raise ValueError(
            f"fields 必须是 dict（字段名→值），实际为 {type(fields).__name__}"
        )
    legal = {
        key for key, _label, _kind, _opt in models.NODE_SCHEMAS[node_type]["fields"]
    }
    legal |= set(_COMMON_FIELDS)
    bad = sorted(k for k in fields if k not in legal)
    if bad:
        raise ValueError(
            f"节点类型 {node_type} 不支持字段: {', '.join(bad)}"
            f"（允许: {', '.join(sorted(legal))}）"
        )
    # 通用字段类型
    for key in _COMMON_FIELDS:
        if key in fields and not isinstance(fields[key], str):
            raise ValueError(f'通用字段 "{key}" 必须是字符串，实际为 {fields[key]!r}')
    # 类型表字段类型
    for key, _label, kind, _opt in models.NODE_SCHEMAS[node_type]["fields"]:
        if key in fields and not _check_kind(kind, fields[key]):
            raise ValueError(
                f'节点类型 {node_type} 字段 "{key}" 类型不符（kind={kind}，'
                f"应为 {'数值' if kind in ('int', 'float') else ('布尔' if kind == 'bool' else ('数组' if kind in _LIST_KINDS else '字符串'))}），"
                f"实际为 {fields[key]!r}"
            )
    return dict(fields)


# ---------------------------------------------------------------------------
# 节点插入
# ---------------------------------------------------------------------------
def _insert_after(story: dict, node: dict, after: str | None) -> None:
    """把 node 插到 story["nodes"]：after 为节点 id 时插到其后，None 插到末尾。"""
    nodes = story.setdefault("nodes", [])
    if after is None:
        nodes.append(node)
        return
    if not isinstance(after, str):
        raise ValueError(f"after 必须是节点 id 或 None，实际为 {after!r}")
    for i, n in enumerate(nodes):
        if n.get("id") == after:
            nodes.insert(i + 1, node)
            return
    raise ValueError(f"after 指定的节点不存在: {after}")


def _make_node(story: dict, node_type: str, fields: dict, after: str | None) -> dict:
    """写操作共用：按契约默认值造节点、覆盖 fields、插到指定位置。"""
    node_id = models.make_node_id(story)
    node = models.new_node(node_type, node_id, _get_ed())
    node.update(fields)
    _insert_after(story, node, after)
    return node


# ---------------------------------------------------------------------------
# 公开接口（契约固定，勿改签名/语义）
# ---------------------------------------------------------------------------
def new_story(
    story_id: str = "main", title: str = "新剧情", mood: bool = False
) -> dict:
    """新建空剧情：含一个 say 起始节点 n1；mood=false 时每次 show/say
    前后自动发射 mod_hide_mood() 隐藏官方心情气泡。"""
    if not isinstance(story_id, str) or not models.ID_PATTERN.match(story_id):
        raise ValueError(f"剧情脚本 id 非法: {story_id!r}（规则 [a-zA-Z0-9_-]+）")
    if not isinstance(title, str):
        raise ValueError(f"title 必须是字符串，实际为 {title!r}")
    if not isinstance(mood, bool):
        raise ValueError(f"mood 必须是布尔值，实际为 {mood!r}")
    story = models.new_story(story_id=story_id, editor_data=_get_ed())
    story["title"] = title
    story["mood"] = mood
    return story


def get_node(story: dict, node_id: str) -> dict:
    """按 id 取节点；不存在抛 ValueError。"""
    for node in story.get("nodes", []):
        if node.get("id") == node_id:
            return node
    raise ValueError(f"节点不存在: {node_id}")


def list_nodes(story: dict) -> list[dict]:
    """列出全部节点，每项 {"id", "type", "summary"}（summary 为中文摘要）。"""
    ed = _get_ed()
    return [
        {
            "id": node.get("id"),
            "type": node.get("type"),
            "summary": models.node_summary(node, ed),
        }
        for node in story.get("nodes", [])
    ]


def add_node(
    story: dict, node_type: str, fields: dict | None = None, after: str | None = None
) -> dict:
    """新增节点：node_type 必须合法、fields 走字段校验、id 自动生成，返回新节点。"""
    if node_type not in models.NODE_TYPES:
        raise ValueError(
            f"未知节点类型: {node_type}（支持 {len(models.NODE_TYPES)} 种，"
            f"见 models.NODE_TYPES）"
        )
    checked = _check_fields(node_type, fields)
    return _make_node(story, node_type, checked, after)


def update_node(story: dict, node_id: str, fields: dict) -> dict:
    """更新节点字段（同 add_node 的字段校验），返回更新后的节点。"""
    node = get_node(story, node_id)
    checked = _check_fields(node["type"], fields)
    node.update(checked)
    return node


def delete_node(story: dict, node_id: str) -> dict:
    """删除节点并返回被删节点（悬空 goto 交给 check_story 报告）。"""
    node = get_node(story, node_id)
    story["nodes"].remove(node)
    return node


def move_node(story: dict, node_id: str, delta: int) -> dict:
    """节点在节点列表中前后移动（delta=±1），返回移动后的节点。"""
    if delta not in (-1, 1):
        raise ValueError(f"delta 只能是 ±1，实际为 {delta!r}")
    nodes = story.setdefault("nodes", [])
    for i, n in enumerate(nodes):
        if n.get("id") == node_id:
            j = i + delta
            if j < 0 or j >= len(nodes):
                raise ValueError(
                    f"节点 {node_id} 已在{'开头' if delta < 0 else '末尾'}，无法再移动"
                )
            nodes[i], nodes[j] = nodes[j], nodes[i]
            return n
    raise ValueError(f"节点不存在: {node_id}")


def set_start(story: dict, node_id: str) -> dict:
    """设置剧情起始节点（校验节点存在），返回该节点。"""
    node = get_node(story, node_id)
    story["start"] = node_id
    return node


def add_say(
    story: dict,
    text: str,
    character: str | None = None,
    mode: str = "character",
    portrait: str = "normal",
    after: str | None = None,
) -> dict:
    """新增对白/独白/旁白节点。

    mode=character/think 时 character 必填；narrative/center 忽略 character
    （不写入该字段）。text 按 kind=multiline 允许换行。
    """
    if not isinstance(text, str):
        raise ValueError(f"say 文本必须是字符串，实际为 {text!r}")
    if mode not in ("character", "think", "narrative", "center"):
        raise ValueError(
            f"say 模式非法: {mode!r}（允许 character/think/narrative/center）"
        )
    if not isinstance(portrait, str):
        raise ValueError(f"portrait 必须是表情 id 字符串，实际为 {portrait!r}")
    fields: dict = {"text": text, "mode": mode, "portrait": portrait}
    if mode in ("character", "think"):
        if not isinstance(character, str) or not character:
            raise ValueError(f'mode="{mode}" 时 character 必填（人物 id）')
        fields["character"] = character
    node = _make_node(story, "say", fields, after)
    # narrative/center 不写入 character（契约默认值里的兜底人物一并去掉）
    if mode in ("narrative", "center"):
        node.pop("character", None)
    return node


def add_death(
    story: dict, text: str, next: str = "Title", after: str | None = None
) -> dict:
    """新增死亡文本节点（第 39 种节点类型）：黑屏 + 居中旁白 + 场景跳转。

    text 必填非空（多行合法）；next 只能是 "Free"/"Title"（默认回标题画面）。
    """
    if not isinstance(text, str) or not text.strip():
        raise ValueError(f"death 文本必须是非空字符串，实际为 {text!r}")
    if next not in ("Free", "Title"):
        raise ValueError(f"death next 非法: {next!r}（允许 Free/Title）")
    return _make_node(story, "death", {"text": text, "next": next}, after)


def add_scene(story: dict, view: str, after: str | None = None) -> dict:
    """新增场景切换节点（view 为场景 id）。"""
    if not isinstance(view, str) or not view:
        raise ValueError(f"scene 场景 view 必须是非空字符串，实际为 {view!r}")
    return _make_node(story, "scene", {"view": view}, after)


def add_choice(story: dict, options, after: str | None = None) -> dict:
    """新增选项分支节点：options 为 [(text, goto), ...] 2~4 项，dialog 固定 "Options"。

    其它皮肤（Talk/Meet/...）是自由场景的 break 格式菜单，纯文本选项会在
    游戏内触发 BreakOptionButton 解析崩溃，因此一律锁死为 Options。
    """
    if not isinstance(options, (list, tuple)) or not (2 <= len(options) <= 4):
        raise ValueError(f"choice 选项必须是 2~4 项，实际 {len(options)} 项")
    opts: list[dict] = []
    for i, item in enumerate(options):
        if not isinstance(item, (list, tuple)) or len(item) != 2:
            raise ValueError(f"第 {i + 1} 个选项必须是 (text, goto) 二元组: {item!r}")
        text, goto = item
        if not isinstance(text, str) or not text:
            raise ValueError(f"第 {i + 1} 个选项 text 必须是非空字符串: {text!r}")
        if not isinstance(goto, str):
            raise ValueError(f"第 {i + 1} 个选项 goto 必须是节点 id 字符串: {goto!r}")
        opts.append({"text": text, "goto": goto})
    return _make_node(story, "choice", {"options": opts, "dialog": "Options"}, after)


def add_dice(
    story: dict,
    check: str,
    goto_成功: str,
    goto_失败: str,
    goto_大成功: str = "",
    after: str | None = None,
) -> dict:
    """新增骰子检定节点。

    check 必须在官方元数据表（lomc.dice_data.get_dice_meta，无元数据会在游戏内
    骰子菜单崩溃）。2 带检查点：goto_成功/goto_失败 必填，goto_大成功 可空
    （非空且 ≠ goto_成功 也接受，编译警告会提示被忽略）；3 带及以上三个都要填。
    """
    if not isinstance(check, str) or not check:
        raise ValueError(f"dice 检查点 check 必须是非空字符串，实际为 {check!r}")
    lomc = _require_lomc()
    meta = lomc.dice_data.get_dice_meta(check)
    if meta is None:
        raise ValueError(
            f'骰子检查点 "{check}" 无官方元数据，请在编辑器清单里选择'
            "（dice_meta 缺失时游戏内骰子菜单会崩溃：选项条数不足导致"
            "UpdateSelection 索引越界 NRE）"
        )
    n_bands = len(meta.get("bands") or [])
    if not n_bands:
        raise ValueError(f'骰子检查点 "{check}" 的官方元数据没有结果带（提取数据异常）')

    def need(name: str, value) -> str:
        if not isinstance(value, str) or not value:
            raise ValueError(f"dice {name} 必填（节点 id 字符串），实际为 {value!r}")
        return value

    goto_成功 = need("goto_成功", goto_成功)
    goto_失败 = need("goto_失败", goto_失败)
    if not isinstance(goto_大成功, str):
        raise ValueError(
            f"dice goto_大成功 必须是节点 id 字符串（2 带可留空），实际为 {goto_大成功!r}"
        )
    if n_bands >= 3 and not goto_大成功:
        raise ValueError(
            f'检查点 "{check}" 有 {n_bands} 个结果带，goto_大成功 必填（最优带）'
        )
    options = [
        {"goto_大成功": goto_大成功, "goto_成功": goto_成功, "goto_失败": goto_失败}
    ]
    return _make_node(story, "dice", {"check": check, "options": options}, after)


# ---------------------------------------------------------------------------
# 校验 / 编译
# ---------------------------------------------------------------------------
def _split_error_message(message: str) -> list[str]:
    """LomcError.message 拆成 errors 列表（按行拆，无换行则单条）。"""
    text = str(message).strip()
    parts = [line.strip() for line in text.splitlines() if line.strip()]
    return parts or ([text] if text else [])


def check_story(story: dict) -> tuple[list[str], list[str]]:
    """校验剧情：返回 (errors, warnings)；errors 非空即校验失败。"""
    lomc = _require_lomc()
    warnings: list[str] = []
    try:
        lomc.validate_story(story, source="story.json", warnings=warnings)
    except lomc.LomcError as exc:
        return _split_error_message(str(exc)), []
    return [], warnings


def compile_story(story: dict) -> tuple[str | None, list[str], list[str]]:
    """校验并编译：失败返回 (None, errors, [])；成功返回 (lua, [], warnings)。

    warnings 由 validate 单独收集；lua 头部已嵌 "-- lomc 警告：" 注释，
    warnings 列表同时给调用方（可读文本，便于直接展示）。
    """
    lomc = _require_lomc()
    warnings: list[str] = []
    try:
        lomc.validate_story(story, source="story.json", warnings=warnings)
    except lomc.LomcError as exc:
        return None, _split_error_message(str(exc)), []
    try:
        lua = lomc.compile_story(story)
    except lomc.LomcError as exc:
        return None, _split_error_message(str(exc)), []
    return lua, [], warnings


# ---------------------------------------------------------------------------
# 文件读写 / 打包
# ---------------------------------------------------------------------------
def load_story_json(path) -> dict:
    """读取 story.json（包 models.load_story）；读失败抛 ValueError（中文消息）。"""
    return models.load_story(path)


def save_story_json(story: dict, path) -> None:
    """写出 story.json（UTF-8、缩进 2、保留中文）。"""
    models.save_story(story, path)


def pack_mod(mod_dir, output=None) -> str:
    """打包 mod 目录为 .lommod 并返回其路径；失败抛 ValueError（保留中文消息）。"""
    lomc = _require_lomc()
    try:
        return lomc.pack_mod(mod_dir, output=output)
    except lomc.LomcError as exc:
        raise ValueError(str(exc)) from exc


# ---------------------------------------------------------------------------
# CLI（AI 子进程友好：退出码 0/1，错误与警告打 stderr）
# ---------------------------------------------------------------------------
def main(argv: list[str] | None = None) -> int:
    """argparse 子命令入口：check / compile / pack / new-story。"""
    import argparse

    parser = argparse.ArgumentParser(
        prog="story_api",
        description="活侠传 mod 剧情工具接口：校验/编译/打包/新建（AI 子进程友好）",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    p_check = sub.add_parser(
        "check", help="校验 story.json（退出码 0/1；错误与警告打 stderr）"
    )
    p_check.add_argument("story_json", help="story.json 路径")

    p_compile = sub.add_parser("compile", help="编译 story.json → Lua")
    p_compile.add_argument("story_json", help="story.json 路径")
    p_compile.add_argument(
        "-o",
        "--output",
        dest="output",
        default=None,
        help="输出 .lua 路径（默认与输入同目录、同名 .lua）",
    )

    p_pack = sub.add_parser("pack", help="打包 mod 目录 → .lommod")
    p_pack.add_argument("mod_dir", help="mod 目录（含 manifest.json 与 story/）")
    p_pack.add_argument(
        "-o",
        "--output",
        dest="output",
        default=None,
        help="输出 .lommod 路径（默认 <mod目录> 同名 .lommod）",
    )

    p_new = sub.add_parser("new-story", help="新建剧情脚本 story.json")
    p_new.add_argument("story_id", help="剧情脚本 id（[a-zA-Z0-9_-]+）")
    p_new.add_argument("--title", default="新剧情", help="标题（默认：新剧情）")
    p_new.add_argument(
        "-o", "--output", dest="output", required=True, help="输出 story.json 路径"
    )

    args = parser.parse_args(argv)

    try:
        if args.command == "check":
            story = load_story_json(args.story_json)
            errors, warnings = check_story(story)
            for w in warnings:
                print(f"警告：{w}", file=sys.stderr)
            if errors:
                for e in errors:
                    print(f"错误：{e}", file=sys.stderr)
                return 1
            return 0
        if args.command == "compile":
            story = load_story_json(args.story_json)
            lua, errors, warnings = compile_story(story)
            for w in warnings:
                print(f"警告：{w}", file=sys.stderr)
            if errors:
                for e in errors:
                    print(f"错误：{e}", file=sys.stderr)
                return 1
            if lua is None:
                return 1
            out = (
                Path(args.output)
                if args.output
                else Path(args.story_json).with_suffix(".lua")
            )
            out.write_text(lua, encoding="utf-8")
            print(str(out))
            return 0
        if args.command == "pack":
            out_path = pack_mod(args.mod_dir, args.output)
            print(out_path)
            return 0
        if args.command == "new-story":
            story = new_story(args.story_id, args.title)
            save_story_json(story, args.output)
            print(str(Path(args.output)))
            return 0
    except (ValueError, OSError) as exc:
        print(f"错误：{exc}", file=sys.stderr)
        return 1
    return 1


if __name__ == "__main__":
    # AI 子进程友好：统一 UTF-8 输出，避免 Windows 控制台编码问题
    for stream in (sys.stdout, sys.stderr):
        reconfigure = getattr(stream, "reconfigure", None)
        if reconfigure is not None:
            reconfigure(encoding="utf-8")
    sys.exit(main())
