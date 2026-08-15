# -*- coding: utf-8 -*-
"""Small, deterministic project statistics model (no dashboard/UI dependency)."""

from __future__ import annotations

from dataclasses import dataclass

from lomc.content import collect_stories_content_refs
from story_graph import analyze_story


@dataclass(frozen=True)
class ProjectStatistics:
    stories: int
    nodes: int
    dialogue_count: int
    choice_nodes: int
    choice_options: int
    endings: int
    characters: int
    images: int
    audio: int
    voiced_dialogue: int
    unvoiced_dialogue: int
    voice_coverage: float
    unreachable_nodes: int
    unused_assets: int | None

    def rows(self) -> tuple[tuple[str, str, str], ...]:
        unused = "不可用" if self.unused_assets is None else str(self.unused_assets)
        return (
            ("剧情章节", str(self.stories), "Story 数量"),
            ("节点", str(self.nodes), "全部章节节点总数"),
            ("对白", str(self.dialogue_count), "say 节点"),
            ("选项", f"{self.choice_nodes} 节点 / {self.choice_options} 项", "choice 节点及选项"),
            ("结尾", str(self.endings), "end / death / goto_scene 终止节点"),
            ("人物", str(self.characters), "节点中引用的不同 character"),
            ("图片", str(self.images), "不同图片引用"),
            ("音频", str(self.audio), "不同 BGM / 音效 / 配音引用"),
            (
                "语音覆盖",
                f"{self.voiced_dialogue}/{self.dialogue_count}（{self.voice_coverage:.1f}%）",
                f"未配音对白 {self.unvoiced_dialogue}",
            ),
            ("不可达节点", str(self.unreachable_nodes), "按编译器一致的 Story CFG"),
            ("未使用资产", unused, "未保存项目无资产目录时显示不可用"),
        )


def _references(stories: dict[str, dict]):
    try:
        return collect_stories_content_refs(stories)
    except Exception:
        return []


def _referenced_asset_paths(stories: dict[str, dict], bundled: set[str]) -> set[str]:
    direct = set()
    for story in stories.values():
        for node in story.get("nodes") or []:
            if not isinstance(node, dict):
                continue
            image = node.get("image")
            if isinstance(image, str):
                normalized = image.replace("\\", "/")
                if normalized.startswith("assets/"):
                    direct.add(normalized)
    content_ids = {item["ref"].content_id for item in _references(stories)}
    referenced = {path for path in direct if path in bundled}
    for content_id in content_ids:
        marker = "/%s/" % content_id
        referenced.update(path for path in bundled if marker in "/" + path)
    return referenced


def calculate_project_statistics(
    stories: dict[str, dict], bundled_assets: list[str] | tuple[str, ...] | None = None
) -> ProjectStatistics:
    valid_stories = {
        str(key): story for key, story in stories.items() if isinstance(story, dict)
    }
    nodes = [
        node
        for story in valid_stories.values()
        for node in (story.get("nodes") or [])
        if isinstance(node, dict)
    ]
    dialogue = [node for node in nodes if node.get("type") == "say"]
    choices = [node for node in nodes if node.get("type") == "choice"]
    endings = sum(
        1 for node in nodes if node.get("type") in ("end", "death", "goto_scene")
    )
    characters = {
        node.get("character")
        for node in nodes
        if isinstance(node.get("character"), str) and node.get("character")
    }
    refs = _references(valid_stories)
    images = {item["raw"] for item in refs if item.get("expected_type") == "image"}
    audio = {item["raw"] for item in refs if item.get("expected_type") == "audio"}
    for node in nodes:
        image = node.get("image")
        if isinstance(image, str) and image and not image.startswith("user:"):
            images.add(image.replace("\\", "/"))
    voiced = sum(
        1 for node in dialogue
        if isinstance(node.get("voice"), str) and bool(node.get("voice").strip())
    )
    unreachable = sum(len(analyze_story(story).unreachable) for story in valid_stories.values())
    unused = None
    if bundled_assets is not None:
        bundled = {
            str(path).replace("\\", "/").lstrip("/") for path in bundled_assets
            if str(path).replace("\\", "/").lstrip("/").startswith("assets/")
        }
        unused = len(bundled - _referenced_asset_paths(valid_stories, bundled))
    coverage = (voiced / len(dialogue) * 100.0) if dialogue else 0.0
    return ProjectStatistics(
        stories=len(valid_stories),
        nodes=len(nodes),
        dialogue_count=len(dialogue),
        choice_nodes=len(choices),
        choice_options=sum(len(node.get("options") or []) for node in choices),
        endings=endings,
        characters=len(characters),
        images=len(images),
        audio=len(audio),
        voiced_dialogue=voiced,
        unvoiced_dialogue=len(dialogue) - voiced,
        voice_coverage=coverage,
        unreachable_nodes=unreachable,
        unused_assets=unused,
    )
