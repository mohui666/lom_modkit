# -*- coding: utf-8 -*-
"""Pure structured variable/flag analysis shared by editor tools."""

from __future__ import annotations

from collections import defaultdict
from dataclasses import dataclass

from story_graph import analyze_story


@dataclass(frozen=True)
class SymbolUse:
    kind: str
    name: str
    access: str  # read | write
    story_id: str
    node_id: str | None
    field: str
    order: int
    external_consumption: bool = False


@dataclass(frozen=True)
class SymbolReport:
    kind: str
    name: str
    reads: int
    writes: int
    first_write: SymbolUse | None
    unused: bool | None
    possibly_read_before_write: bool | None
    uses: tuple[SymbolUse, ...]


def _structured_uses(
    stories: dict[str, dict], manifest: dict | None = None
) -> list[SymbolUse]:
    uses: list[SymbolUse] = []
    order = 0
    for story_id in sorted(stories):
        story = stories[story_id]
        for node in story.get("nodes") or []:
            if not isinstance(node, dict):
                continue
            node_id = str(node.get("id") or "") or None
            node_type = node.get("type")
            if node_type == "flag" and isinstance(node.get("flag"), str) and node["flag"]:
                uses.append(SymbolUse("mod_flag", node["flag"], "write", story_id, node_id, "flag", order))
            elif node_type == "game_flag" and isinstance(node.get("flag"), str) and node["flag"]:
                uses.append(SymbolUse("game_flag", node["flag"], "write", story_id, node_id, "flag", order, True))
            elif node_type == "branch" and isinstance(node.get("flag"), str) and node["flag"]:
                source = str(node.get("source") or "mod")
                kind = {
                    "mod": "mod_flag",
                    "flag_value": "game_flag",
                    "game": "checkpoint",
                    "condition": "condition",
                }.get(source)
                if kind:
                    uses.append(SymbolUse(kind, node["flag"], "read", story_id, node_id, "flag", order, kind != "mod_flag"))
            elif node_type == "block":
                for index, variable in enumerate(node.get("vars") or []):
                    if not isinstance(variable, dict) or not isinstance(variable.get("name"), str) or not variable["name"]:
                        continue
                    uses.append(SymbolUse(
                        "flow_variable", variable["name"], "write", story_id, node_id,
                        "vars[%d].name" % index, order, True,
                    ))
            order += 1
    safe_manifest = manifest if isinstance(manifest, dict) else {}
    campaign = safe_manifest.get("campaign")
    triggers = campaign.get("triggers") if isinstance(campaign, dict) else []
    for index, trigger in enumerate(triggers or []):
        if not isinstance(trigger, dict):
            continue
        story_id = str(trigger.get("script") or "")
        for field in ("when_flag_set", "when_flag_clear"):
            value = trigger.get(field)
            if isinstance(value, str) and value:
                uses.append(SymbolUse("game_flag", value, "read", story_id, None, "manifest.campaign.triggers[%d].%s" % (index, field), order, True))
                order += 1
    return uses


def find_read_before_write(
    stories: dict[str, dict], symbol: str, manifest: dict | None = None
) -> tuple[str, str] | None:
    """Locate a project-reachable read that can occur before the first write."""
    adjacency: dict[tuple[str, str], set[tuple[str, str]]] = {}
    by_story: dict[str, dict[str, dict]] = {}
    starts: dict[str, str] = {}
    for story_id in sorted(stories):
        story = stories[story_id]
        nodes = [node for node in story.get("nodes") or [] if isinstance(node, dict)]
        by_id = {str(node.get("id")): node for node in nodes if node.get("id")}
        by_story[story_id] = by_id
        graph = analyze_story(story)
        for node_id in by_id:
            adjacency[(story_id, node_id)] = set()
        for edge in graph.edges:
            source = (story_id, edge.source)
            target = (story_id, edge.target)
            if not edge.missing and source in adjacency and target in adjacency:
                adjacency[source].add(target)
        start = str(story.get("start") or "")
        if start in by_id:
            starts[story_id] = start
    for story_id, by_id in by_story.items():
        for node_id, node in by_id.items():
            target_story = node.get("next_script") if node.get("type") == "end" else None
            if isinstance(target_story, str) and target_story in starts:
                adjacency[(story_id, node_id)].add((target_story, starts[target_story]))

    manifest = manifest if isinstance(manifest, dict) else {}
    entry = manifest.get("entry")
    root_stories: set[str]
    if isinstance(entry, str) and entry in starts:
        root_stories = {entry}
        campaign = manifest.get("campaign")
        triggers = campaign.get("triggers") if isinstance(campaign, dict) else []
        for trigger in triggers or []:
            if isinstance(trigger, dict) and trigger.get("script") in starts:
                root_stories.add(str(trigger["script"]))
    else:
        # Standalone analysis has no project entry contract, so any chapter may start.
        root_stories = set(starts)
    pending = [(sid, starts[sid]) for sid in sorted(root_stories)]
    reached_without_write: set[tuple[str, str]] = set()
    while pending:
        state = pending.pop()
        if state in reached_without_write:
            continue
        reached_without_write.add(state)
        story_id, node_id = state
        node = by_story[story_id][node_id]
        is_read = node.get("type") == "branch" and node.get("source", "mod") == "mod" and node.get("flag") == symbol
        if is_read:
            return story_id, node_id
        is_write = node.get("type") == "flag" and node.get("flag") == symbol
        if not is_write:
            pending.extend(adjacency.get(state, ()))
    return None


def analyze_symbols(
    stories: dict[str, dict], manifest: dict | None = None
) -> list[SymbolReport]:
    uses = _structured_uses(stories, manifest)
    grouped: dict[tuple[str, str], list[SymbolUse]] = defaultdict(list)
    for use in uses:
        grouped[(use.kind, use.name)].append(use)
    reports: list[SymbolReport] = []
    for (kind, name), symbol_uses in sorted(grouped.items()):
        reads = [use for use in symbol_uses if use.access == "read"]
        writes = [use for use in symbol_uses if use.access == "write"]
        first_write = min(writes, key=lambda use: use.order) if writes else None
        unused: bool | None = (not reads and bool(writes)) if kind == "mod_flag" else None
        read_before: bool | None = (
            find_read_before_write(stories, name, manifest) is not None
            if kind == "mod_flag" and reads else None
        )
        reports.append(SymbolReport(
            kind, name, len(reads), len(writes), first_write, unused,
            read_before, tuple(sorted(symbol_uses, key=lambda use: use.order)),
        ))
    return reports
