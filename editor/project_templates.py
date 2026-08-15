# -*- coding: utf-8 -*-
"""Built-in project starters made exclusively from the public Story schema."""

from __future__ import annotations

import copy
from dataclasses import dataclass

from schema_versions import STORY_SCHEMA


@dataclass(frozen=True)
class ProjectTemplateInfo:
    key: str
    name: str
    description: str
    placeholder_content: bool = False


TEMPLATES = (
    ProjectTemplateInfo(
        "empty", "空项目", "只有一个安全结束节点，适合从零搭建。"
    ),
    ProjectTemplateInfo(
        "linear_dialogue", "线性对白", "人物登场、角色对白、旁白和结束的最小线性流程。"
    ),
    ProjectTemplateInfo(
        "branching_story", "分支剧情", "一个双选项分支，两条路线汇合到同一结尾。"
    ),
    ProjectTemplateInfo(
        "custom_character_showcase",
        "自定义人物展示",
        "自定义人物登场、对白、移动和退场；需替换 user:template.hero。",
        True,
    ),
    ProjectTemplateInfo(
        "user_content_showcase",
        "用户内容展示",
        "自定义背景、BGM、音效、CG 和配音；需在内容库替换占位引用。",
        True,
    ),
)


def template_info(key: str) -> ProjectTemplateInfo:
    for item in TEMPLATES:
        if item.key == key:
            return item
    raise ValueError("未知项目模板：%s" % key)


def _story(title: str, nodes: list[dict]) -> dict:
    return {
        "story_schema": STORY_SCHEMA,
        "id": "main",
        "title": title,
        "mood": False,
        "start": nodes[0]["id"],
        "nodes": nodes,
    }


def _official_character(editor_data: dict | None) -> str:
    characters = (editor_data or {}).get("characters") or []
    if not characters:
        return "player"
    first = characters[0]
    if isinstance(first, dict):
        return str(first.get("id") or "player")
    return str(first or "player")


def create_project_template(key: str, editor_data: dict | None = None) -> dict:
    """Return {stories,current_story_id,manifest}; every story is ordinary schema."""
    template_info(key)
    character = _official_character(editor_data)
    if key == "empty":
        story = _story("空项目", [{"id": "end1", "type": "end"}])
    elif key == "linear_dialogue":
        story = _story(
            "线性对白",
            [
                {"id": "show1", "type": "show", "character": character,
                 "position": "M", "portrait": "normal", "facing": "right"},
                {"id": "say1", "type": "say", "character": character,
                 "portrait": "normal", "mode": "character",
                 "text": "这里写角色的第一句对白。"},
                {"id": "say2", "type": "say", "mode": "narrative",
                 "text": "这里写推动情节的旁白。"},
                {"id": "end1", "type": "end"},
            ],
        )
    elif key == "branching_story":
        story = _story(
            "分支剧情",
            [
                {"id": "intro", "type": "say", "mode": "narrative",
                 "text": "前方出现两条路。"},
                {"id": "choose", "type": "choice", "dialog": "Options",
                 "options": [
                     {"text": "走左边", "goto": "left"},
                     {"text": "走右边", "goto": "right"},
                 ]},
                {"id": "left", "type": "say", "mode": "narrative",
                 "text": "你沿着左边的山路前进。", "goto": "ending"},
                {"id": "right", "type": "say", "mode": "narrative",
                 "text": "你沿着右边的溪流前进。", "goto": "ending"},
                {"id": "ending", "type": "say", "mode": "narrative",
                 "text": "两条路最终在山门前汇合。"},
                {"id": "end1", "type": "end"},
            ],
        )
    elif key == "custom_character_showcase":
        hero = "user:template.hero"
        story = _story(
            "自定义人物展示",
            [
                {"id": "show1", "type": "show", "character": hero,
                 "position": "L1", "portrait": "normal", "facing": "right",
                 "fadeDuration": 0.3},
                {"id": "say1", "type": "say", "character": hero,
                 "portrait": "normal", "mode": "character",
                 "text": "请把占位人物替换成内容库里的自定义角色。"},
                {"id": "move1", "type": "move", "character": hero,
                 "from": "L1", "to": "M", "duration": 0.8},
                {"id": "say2", "type": "say", "character": hero,
                 "portrait": "normal", "mode": "character",
                 "text": "同一个 user: 引用可以用于登场、动作和对白。"},
                {"id": "hide1", "type": "hide", "character": hero,
                 "fadeDuration": 0.3},
                {"id": "end1", "type": "end"},
            ],
        )
    else:
        story = _story(
            "用户内容展示",
            [
                {"id": "bg1", "type": "background", "action": "show",
                 "image": "user:template.background", "fade": 0.5},
                {"id": "music1", "type": "music",
                 "name": "user:template.music", "op": "play"},
                {"id": "sound1", "type": "sound",
                 "name": "user:template.sound", "kind": "sound", "op": "play"},
                {"id": "cg1", "type": "custom_cg", "action": "show",
                 "image": "user:template.cg", "fade": 0.4,
                 "scale": 100, "x": 0, "y": 0},
                {"id": "say1", "type": "say", "mode": "narrative",
                 "text": "背景、音乐、音效、CG 和本句配音都来自用户内容库。",
                 "voice": "user:template.voice"},
                {"id": "cg2", "type": "custom_cg", "action": "hide", "fade": 0.3},
                {"id": "bg2", "type": "background", "action": "clear", "fade": 0.3},
                {"id": "end1", "type": "end"},
            ],
        )
    return copy.deepcopy({
        "stories": {"main": story},
        "current_story_id": "main",
        "manifest": {},
    })
