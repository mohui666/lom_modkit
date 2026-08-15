# -*- coding: utf-8 -*-
"""Voice coverage by project, story and character."""

from __future__ import annotations

from dataclasses import dataclass


NARRATOR_ID = "__narrator__"


@dataclass(frozen=True)
class CoverageRow:
    scope: str
    key: str
    label: str
    voiced: int
    unvoiced: int

    @property
    def total(self) -> int:
        return self.voiced + self.unvoiced

    @property
    def percent(self) -> float:
        return self.voiced / self.total * 100.0 if self.total else 0.0


@dataclass(frozen=True)
class UnvoicedDialogue:
    story_id: str
    story_title: str
    node_id: str
    character_id: str
    character_label: str
    text: str


@dataclass(frozen=True)
class VoiceCoverageReport:
    total: CoverageRow
    stories: tuple[CoverageRow, ...]
    characters: tuple[CoverageRow, ...]
    unvoiced_dialogues: tuple[UnvoicedDialogue, ...]


def _character(node: dict) -> tuple[str, str]:
    mode = node.get("mode", "character")
    if mode in ("narrative", "center"):
        return NARRATOR_ID, "旁白"
    value = node.get("character")
    if isinstance(value, str) and value:
        return value, value
    return "__unspecified__", "未指定人物"


def calculate_voice_coverage(stories: dict[str, dict]) -> VoiceCoverageReport:
    story_counts: dict[str, list[int]] = {}
    story_labels: dict[str, str] = {}
    character_counts: dict[str, list[int]] = {}
    character_labels: dict[str, str] = {}
    missing = []
    total_voiced = 0
    total_unvoiced = 0

    for raw_story_id, story in stories.items():
        if not isinstance(story, dict):
            continue
        story_id = str(raw_story_id)
        story_title = str(story.get("title") or story_id)
        story_labels[story_id] = story_title
        counts = story_counts.setdefault(story_id, [0, 0])
        for node in story.get("nodes") or []:
            if not isinstance(node, dict) or node.get("type") != "say":
                continue
            character_id, character_label = _character(node)
            character_labels[character_id] = character_label
            char_counts = character_counts.setdefault(character_id, [0, 0])
            voice = node.get("voice")
            voiced = isinstance(voice, str) and bool(voice.strip())
            if voiced:
                counts[0] += 1
                char_counts[0] += 1
                total_voiced += 1
            else:
                counts[1] += 1
                char_counts[1] += 1
                total_unvoiced += 1
                missing.append(UnvoicedDialogue(
                    story_id=story_id,
                    story_title=story_title,
                    node_id=str(node.get("id") or ""),
                    character_id=character_id,
                    character_label=character_label,
                    text=str(node.get("text") or ""),
                ))

    story_rows = tuple(
        CoverageRow("story", story_id, story_labels[story_id], *story_counts[story_id])
        for story_id in sorted(story_counts)
    )
    character_rows = tuple(
        CoverageRow(
            "character", character_id, character_labels[character_id],
            *character_counts[character_id]
        )
        for character_id in sorted(
            character_counts,
            key=lambda value: (value != NARRATOR_ID, character_labels[value].casefold()),
        )
    )
    return VoiceCoverageReport(
        CoverageRow("total", "total", "项目总计", total_voiced, total_unvoiced),
        story_rows,
        character_rows,
        tuple(missing),
    )
