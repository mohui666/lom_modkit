# -*- coding: utf-8 -*-
"""story_api — 供 AI / 脚本调用方使用的剧情数据工具接口（纯数据 + 编译，无 UI 依赖）。

设计原则：
1. 固定规则转换：所有写操作都走固定规则——节点由 models 的契约默认值生成、
   字段按 NODE_SCHEMAS 做宽松类型校验，未知字段一律拒绝；调用方只提供语义值，
   不可能绕过规则手写 story JSON 或 Lua。
2. 防写坏：把游戏侧已知崩溃坑挡在写入口（choice 皮肤锁死 Options、dice 检查点
   必须命中官方元数据、say 模式与人物联动、动作人物必须先登场——未登场/已退场
   时自动在该节点前插入 show 登场节点），编译期剩余问题交给 lomc 校验
   （check_story / compile_story），编辑器与 AI 共用同一套防线。
3. AI 友好：函数小而语义明确、错误消息全中文、错误与警告都是字符串列表；
   另带 argparse CLI（check / compile / pack / new-story，可加 --json 输出
   单行结构化 JSON），退出码 0/1，适合 AI 以子进程方式调用。
"""

from __future__ import annotations

import json
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
import stage_guard  # noqa: E402

# 冻结（PyInstaller）态：__file__ 指向解包目录，改用 models 的 _MEIPASS 推导；
# data/editor_data.json 由打包 spec 打进包内，import lomc 由冻结导入器解析（PYZ）
if models.FROZEN:
    EDITOR_DIR = models.editor_dir()
    PROJECT_ROOT = models.project_root()
    COMPILER_DIR = PROJECT_ROOT / "compiler"

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
_LIST_KINDS = {
    "options", "cases", "vars", "dice_options", "official_characters",
    "battle_faction_list",
    "combat_talents", "reward_entries", "reward_entries_optional", "custom_shop_items",
}
_NUMBER_KINDS = {
    "int", "float", "percent_scale", "percent_cg_scale", "percent_position",
    "percent_offset", "percent_opacity", "discount_toggle", "bool_int",
}


def _check_kind(kind: str, value) -> bool:
    """按 NODE_SCHEMAS 的 kind 做宽松类型校验（不修改值）。"""
    if kind in _NUMBER_KINDS:
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
                f"应为 {'数值' if kind in _NUMBER_KINDS else ('布尔' if kind == 'bool' else ('数组' if kind in _LIST_KINDS else '字符串'))}），"
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
    node_id = models.make_node_id(story, node_type)
    node = models.new_node(node_type, node_id, _get_ed())
    node.update(fields)
    _normalize_branch(node)
    _check_portrait_node(node)  # 角色表情校验（与编译器同一张表）
    _insert_after(story, node, after)
    _guard_stage(story, node)  # 登场防线：动作人物未登场时自动补 show
    return node


def _guard_stage(story: dict, node: dict) -> None:
    """线性登场防线：node 的动作人物在前面未登场/已退场时，紧挨它补一个 show。

    新节点 id 是新生成的，不存在指向它的显式跳转，直接前插即可（顺序衔接
    自动经过新 show）；图级多路径的精确兜底在 preflight 体检（find_stage_issues
    + ensure_stage）。
    """
    nodes = story.get("nodes") or []
    for index, item in enumerate(nodes):
        if item is node:
            cid = stage_guard.missing_stage_linear(nodes, index)
            if cid:
                nodes.insert(index, stage_guard.make_show_node(story, cid))
            return


def _normalize_branch(node: dict) -> None:
    """branch 键字段归一：source=stat 用 stat，其余来源用 flag。

    清掉与当前来源冲突的键（契约默认值自带 flag:""，stat 来源必须去 flag，
    否则编译器报「不支持字段 flag」）。
    """
    if node.get("type") != "branch":
        return
    if node.get("source", "mod") == "stat":
        node.pop("flag", None)
    else:
        node.pop("stat", None)


def _check_portrait_node(node: dict) -> None:
    """show/say 的 (character, portrait) 校验。

    官方角色：必须落在 editor_data 表情表内（表不可用/角色不在表 → 放行）。
    自定义 user: 角色：表情 id 格式合法；若仓库里已有该角色，表情必须在其定义里。
    """
    if node.get("type") not in ("show", "say"):
        return
    character = node.get("character")
    portrait = node.get("portrait")
    if not (isinstance(character, str) and character) or not (
        isinstance(portrait, str) and portrait
    ):
        return
    if character.startswith("user:"):
        lomc, _err = get_lomc()
        if lomc is not None:
            try:
                lomc.content.parse_content_ref(character, label="character")
                lomc.content.validate_portrait_id(portrait, label="portrait")
            except Exception as exc:
                raise ValueError(str(exc)) from exc
        try:
            import content_registry

            content_registry.resolve(
                character, expected_type="character", portrait=portrait
            )
        except Exception as exc:
            # 仓库没有这条角色时，只拦非法路径 / 表情；缺失留给预检/打包报。
            message = str(exc)
            if "非法" in message or "不合法" in message or "没有表情" in message:
                raise ValueError(message) from exc
        return
    lomc, _err = get_lomc()
    if lomc is None:
        return  # 编译器不可用时校验下沉到 check_story
    table = lomc.dice_data.load_portrait_table()
    msg = lomc.dice_data.check_portrait(table, character, portrait)
    if msg:
        raise ValueError(msg)


# ---------------------------------------------------------------------------
# 公开接口（契约固定，勿改签名/语义）
# ---------------------------------------------------------------------------
def new_story(
    story_id: str = "main", title: str = "新剧情", mood: bool = False
) -> dict:
    """新建空剧情：show 登场 + say 对白开场（先登场再动作，否则游戏黑屏）；
    mood=false 时每次 show/say 前后自动发射 mod_hide_mood() 隐藏官方心情气泡。"""
    if not isinstance(story_id, str) or not models.ID_PATTERN.fullmatch(story_id):
        raise ValueError(f"剧情脚本 id 非法: {story_id!r}（规则 [a-zA-Z0-9_-]{{1,64}}）")
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
    """更新节点字段（同 add_node 的字段校验），返回更新后的节点。

    登场防线：更新后若动作人物在前面未登场/已退场（线性判断），自动在该
    节点前插入 show 登场节点，并把指向它的 goto/选项/分支跳转改指新节点。
    """
    node = get_node(story, node_id)
    checked = _check_fields(node["type"], fields)
    node.update(checked)
    _normalize_branch(node)  # branch 键字段归一（合并后的最终状态）
    _check_portrait_node(node)  # 角色表情校验（合并默认值后的最终状态）
    nodes = story.get("nodes") or []
    for index, item in enumerate(nodes):
        if item is node:
            if stage_guard.missing_stage_linear(nodes, index):
                stage_guard.ensure_stage(story, node_id)
            break
    return node


def delete_node(story: dict, node_id: str) -> dict:
    """删除节点并返回被删节点（悬空 goto 交给 check_story 报告）。"""
    node = get_node(story, node_id)
    story["nodes"].remove(node)
    return node


def rename_node(story: dict, node_id: str, new_id: str) -> dict:
    """重命名节点 id，并同步 start 与全部跳转引用，返回改名后的节点。"""
    models.rename_node(story, node_id, new_id)
    return get_node(story, new_id.strip())


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
    voice: str | None = None,
    after: str | None = None,
) -> dict:
    """新增对白/独白/旁白节点。

    mode=character/think 时 character 必填；narrative/center 忽略 character
    （不写入该字段）。text 按 kind=multiline 允许换行。
    voice 可选，必须是 user: 用户音频引用；缺省则与旧对白完全一致。
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
    if voice is not None:
        if not isinstance(voice, str) or not voice.strip():
            raise ValueError("voice 必须是非空的 user: 音频引用，不要语音请省略该参数")
        fields["voice"] = voice
    node = _make_node(story, "say", fields, after)
    # narrative/center 不写入 character（契约默认值里的兜底人物一并去掉）
    if mode in ("narrative", "center"):
        node.pop("character", None)
    return node


def add_death(
    story: dict,
    text: str,
    death_id: str,
    next: str = "Title",
    title: str | None = None,
    after: str | None = None,
) -> dict:
    """新增死亡文本节点：黑屏 + 两段式死亡文本 + 官方死亡画面。

    text 必填非空（多行合法）；death_id 必填 ≥900000 的 mod 专属数字 id
    （约定 9+官方 id：官方 10021 乱战中被践踏而死 → 910021；官方 id 会触发
    官方结局解锁与记录，污染存档）；原版死亡画面固定提供读档/标题按钮，
    next 只接受 "Title"（保留参数是为了兼容既有调用签名）；
    title 可选（短标题，缺省/空串时用「勝敗乃兵家常事」）。
    """
    if not isinstance(text, str) or not text.strip():
        raise ValueError(f"death 文本必须是非空字符串，实际为 {text!r}")
    try:
        ok_id = (
            isinstance(death_id, str) and death_id.isdigit() and int(death_id) >= 900000
        )
    except ValueError:  # 超长数字串 int() 会抛（防御）
        ok_id = False
    if not ok_id:
        raise ValueError(
            f"death_id 必须是 ≥900000 的 mod 专属数字 id（约定 9+官方 id，"
            f"如官方 10021 → 910021；官方 id 会触发官方结局解锁与记录，污染存档），"
            f"实际为 {death_id!r}"
        )
    if next != "Title":
        raise ValueError(
            f"death next 非法: {next!r}（原版死亡画面固定返回标题，只允许 Title）"
        )
    fields: dict = {"text": text, "death_id": death_id, "next": next}
    if title is not None:
        if not isinstance(title, str):
            raise ValueError(f"death title 必须是字符串，实际为 {title!r}")
        if title:
            fields["title"] = title
    node = _make_node(story, "death", fields, after)
    if not node.get("title"):  # 空标题不写（codegen 用缺省「勝敗乃兵家常事」）
        node.pop("title", None)
    return node


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
    maximum: int,
    header: str,
    bands: list,
    bonus: int = 0,
    bonus_name: str = "",
    bonus_status: str = "",
    after: str | None = None,
) -> dict:
    """Add a fully author-configured dice roll without an original checkpoint ID.

    ``bands`` contains 2~4 dictionaries in low-to-high order. Every non-final
    row has ``upper``; every row has readable ``text`` and a destination ``goto``.
    """
    if isinstance(maximum, bool) or not isinstance(maximum, int) or not 1 <= maximum <= 9999:
        raise ValueError("dice maximum 必须是 1~9999 的整数")
    if not isinstance(header, str) or not header.strip() or len(header) > 80:
        raise ValueError("dice header 必须是 1~80 字符的非空标题")
    if isinstance(bonus, bool) or not isinstance(bonus, int) or not -9999 <= bonus <= 9999:
        raise ValueError("dice bonus 必须是 -9999~9999 的整数")
    for name, value in (("bonus_name", bonus_name), ("bonus_status", bonus_status)):
        if not isinstance(value, str) or len(value) > 80:
            raise ValueError("dice %s 必须是最多 80 字符的字符串" % name)
    if not isinstance(bands, list) or not 2 <= len(bands) <= 4:
        raise ValueError("dice bands 必须是 2~4 个结果分段")
    normalized = []
    previous = None
    for index, item in enumerate(bands):
        if not isinstance(item, dict):
            raise ValueError("dice bands[%d] 必须是对象" % index)
        allowed = {"upper", "text", "goto"} if index < len(bands) - 1 else {"text", "goto"}
        unknown = set(item) - allowed
        if unknown:
            raise ValueError("dice bands[%d] 含未知字段: %s" % (index, ", ".join(sorted(unknown))))
        text = item.get("text")
        target = item.get("goto")
        if not isinstance(text, str) or not text.strip():
            raise ValueError("dice bands[%d].text 必须是非空字符串" % index)
        if not isinstance(target, str) or not target.strip():
            raise ValueError("dice bands[%d].goto 必须是非空节点 id" % index)
        row = {"text": text, "goto": target}
        if index < len(bands) - 1:
            upper = item.get("upper")
            if isinstance(upper, bool) or not isinstance(upper, int):
                raise ValueError("dice bands[%d].upper 必须是整数" % index)
            if upper < bonus or upper >= maximum + bonus:
                raise ValueError(
                    "dice bands[%d].upper 必须在可投出的总点数 %d~%d 之间，且要给后一档留出点数"
                    % (index, bonus, maximum + bonus)
                )
            if previous is not None and upper <= previous:
                raise ValueError("dice bands[%d].upper 必须严格递增" % index)
            row["upper"] = upper
            previous = upper
        normalized.append(row)
    fields = {
        "max": maximum,
        "header": header,
        "bands": normalized,
        "bonus": bonus,
    }
    if bonus_name:
        fields["bonus_name"] = bonus_name
    if bonus_status:
        fields["bonus_status"] = bonus_status
    return _make_node(story, "dice", fields, after)


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
# CLI（AI 子进程友好：退出码 0/1；文本模式错误与警告打 stderr，
# --json 模式 stdout 打单行 JSON，stderr 保持干净）
# ---------------------------------------------------------------------------
def _print_json(payload: dict) -> None:
    """结构化结果：单行 JSON 直写 stdout 的 UTF-8 字节流（绕开控制台编码）。"""
    text = json.dumps(payload, ensure_ascii=False) + "\n"
    buf = getattr(sys.stdout, "buffer", None)
    if buf is not None:
        try:
            buf.write(text.encode("utf-8"))
            buf.flush()
            return
        except Exception:  # buffer 不可写时退回 print（防御，不影响结果语义）
            pass
    print(text)


def _add_json_flag(sub_parser) -> None:
    sub_parser.add_argument(
        "--json",
        dest="json",
        action="store_true",
        default=False,
        help=(
            "结构化 JSON 输出：stdout 打单行 {\"ok\": bool, ...}，"
            "errors/warnings 数组随附；退出码约定不变"
        ),
    )


def main(argv: list[str] | None = None) -> int:
    """argparse 子命令入口：check / compile / pack / new-story。"""
    import argparse

    parser = argparse.ArgumentParser(
        prog="story_api",
        description="活侠传 mod 剧情工具接口：校验/编译/打包/新建（AI 子进程友好）",
    )
    # 主解析器也接受 --json（子命令前使用同样生效）；与子命令的 dest 错开再合并
    parser.add_argument(
        "--json", dest="json_global", action="store_true",
        default=False, help=argparse.SUPPRESS,
    )
    sub = parser.add_subparsers(dest="command", required=True)

    p_check = sub.add_parser(
        "check", help="校验 story.json（退出码 0/1；错误与警告打 stderr）"
    )
    p_check.add_argument("story_json", help="story.json 路径")
    _add_json_flag(p_check)

    p_compile = sub.add_parser("compile", help="编译 story.json → Lua")
    p_compile.add_argument("story_json", help="story.json 路径")
    p_compile.add_argument(
        "-o",
        "--output",
        dest="output",
        default=None,
        help="输出 .lua 路径（默认与输入同目录、同名 .lua）",
    )
    _add_json_flag(p_compile)

    p_pack = sub.add_parser("pack", help="打包 mod 目录 → .lommod")
    p_pack.add_argument("mod_dir", help="mod 目录（含 manifest.json 与 story/）")
    p_pack.add_argument(
        "-o",
        "--output",
        dest="output",
        default=None,
        help="输出 .lommod 路径（默认 <mod目录> 同名 .lommod）",
    )
    _add_json_flag(p_pack)

    p_new = sub.add_parser("new-story", help="新建剧情脚本 story.json")
    p_new.add_argument("story_id", help="剧情脚本 id（[a-zA-Z0-9_-]{1,64}）")
    p_new.add_argument("--title", default="新剧情", help="标题（默认：新剧情）")
    p_new.add_argument(
        "-o", "--output", dest="output", required=True, help="输出 story.json 路径"
    )
    _add_json_flag(p_new)

    args = parser.parse_args(argv)
    use_json = bool(
        getattr(args, "json", False) or getattr(args, "json_global", False)
    )

    try:
        if args.command == "check":
            story = load_story_json(args.story_json)
            errors, warnings = check_story(story)
            if use_json:
                _print_json({"ok": not errors, "errors": errors, "warnings": warnings})
                return 0 if not errors else 1
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
            out = (
                Path(args.output)
                if args.output
                else Path(args.story_json).with_suffix(".lua")
            )
            if use_json:
                if errors or lua is None:
                    _print_json({"ok": False, "errors": errors or ["编译失败：无输出"]})
                    return 1
                out.write_text(lua, encoding="utf-8")
                _print_json({"ok": True, "output": str(out), "warnings": warnings})
                return 0
            for w in warnings:
                print(f"警告：{w}", file=sys.stderr)
            if errors:
                for e in errors:
                    print(f"错误：{e}", file=sys.stderr)
                return 1
            if lua is None:
                return 1
            out.write_text(lua, encoding="utf-8")
            print(str(out))
            return 0
        if args.command == "pack":
            out_path = pack_mod(args.mod_dir, args.output)
            if use_json:
                _print_json({"ok": True, "output": out_path})
            else:
                print(out_path)
            return 0
        if args.command == "new-story":
            story = new_story(args.story_id, args.title)
            save_story_json(story, args.output)
            if use_json:
                _print_json({"ok": True, "output": str(Path(args.output))})
            else:
                print(str(Path(args.output)))
            return 0
    except (ValueError, OSError) as exc:
        if use_json:
            _print_json({"ok": False, "errors": [str(exc)]})
        else:
            print(f"错误：{exc}", file=sys.stderr)
        return 1
    return 1


if __name__ == "__main__":
    # AI 子进程友好：统一 UTF-8 输出，避免 Windows 控制台编码问题
    for stream in (sys.stdout, sys.stderr):
        reconfigure = getattr(stream, "reconfigure", None)
        if reconfigure is not None:
            try:
                reconfigure(encoding="utf-8")
            except Exception:
                pass
    sys.exit(main())
