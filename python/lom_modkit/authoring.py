"""Typed authoring documents. These serialize to the editor's existing format."""
from copy import deepcopy
from pathlib import Path
import re


def plain(value):
    if hasattr(value, "to_dict"):
        return value.to_dict()
    if isinstance(value, dict):
        return {k: plain(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [plain(v) for v in value]
    return value


def ident(value):
    if not isinstance(value, str) or not re.fullmatch(r"[A-Za-z0-9_-]{1,64}", value):
        raise ValueError(f"Invalid ID: {value!r}")
    return value


def integer(value, low=-(2**31), high=2**31-1):
    if type(value) is not int or not low <= value <= high:
        raise ValueError(f"Expected integer {low}..{high}, got {value!r}")
    return value


def enum(value, values):
    if value not in values:
        raise ValueError(f"Expected one of {values}, got {value!r}")
    return value


class Document:
    def to_dict(self):
        return deepcopy(self.data)


class Condition(Document):
    def __init__(self, *, source, key, op, value):
        enum(source, ("time", "stat", "affinity", "flag", "game_flag", "variable", "task"))
        enum(op, ("eq", "ne") if source == "task" else ("eq", "ne", "lt", "le", "gt", "ge"))
        if source == "time":
            enum(key, ("year", "month", "stage", "action"))
        else:
            ident(key)
        ident(value) if source == "task" else integer(value)
        self.data = dict(source=source, key=key, op=op, value=value)


class All(Document):
    def __init__(self, *conditions):
        self.data = {"all": [plain(c) for c in conditions]}


class Any(Document):
    def __init__(self, *conditions):
        self.data = {"any": [plain(c) for c in conditions]}


class Not(Document):
    def __init__(self, condition):
        self.data = {"not": plain(condition)}


class TaskTransition(Document):
    def __init__(self, *, from_, to, when):
        ident(from_); ident(to)
        if from_ == to:
            raise ValueError("Task transition needs distinct states")
        self.data = {"from": from_, "to": to, "when": plain(when)}


class Task(Document):
    def __init__(self, *, id, name, states, initial, transitions=()):
        ident(id)
        states = [ident(state) for state in states]
        if not 1 <= len(states) <= 32 or len(set(states)) != len(states) or initial not in states:
            raise ValueError("Task needs unique states and an initial state from that list")
        transitions = [plain(t) for t in transitions]
        for transition in transitions:
            if transition["from"] not in states or transition["to"] not in states:
                raise ValueError("Transition references missing task state")
        self.data = dict(id=id, name=name, states=states, initial=initial)
        if transitions:
            self.data["transitions"] = transitions


class VariableChange(Document):
    def __init__(self, *, op, value):
        self.data = dict(op=enum(op, ("set", "add")), value=integer(value))


class OutcomeAction(Document):
    def __init__(self, *, type, key=None, op=None, value=None, kind=None, year=None, month=None, stage=None):
        enum(type, ("stat", "affinity", "flag", "game_flag", "item", "talent", "time"))
        self.data = {"type": type}
        if type == "time":
            if any(v is not None for v in (key, op, value, kind)):
                raise TypeError("Time outcome only accepts year/month/stage")
            self.data.update(year=integer(year, 1, 9), month=integer(month, 1, 12), stage=integer(stage, 1, 3))
            return
        if any(v is not None for v in (year, month, stage)):
            raise TypeError("Only time outcomes accept date fields")
        if not isinstance(key, str) or not key:
            raise ValueError("Outcome action needs key")
        self.data["key"] = key
        if type == "flag":
            if any(v is not None for v in (op, value, kind)):
                raise TypeError("Flag outcome only accepts key")
        elif type == "talent":
            if op is not None or kind is not None:
                raise TypeError("Talent outcome only accepts key/value")
            self.data["value"] = enum(integer(value), (-1, 1))
        else:
            self.data.update(op=enum(op, ("add", "remove") if type == "item" else ("set", "add")),
                             value=integer(value, 1 if type == "item" else -(2**31)))
            if type == "item":
                self.data["kind"] = enum(kind, ("book", "misc", "special"))
            elif kind is not None:
                raise TypeError("Only item outcomes accept kind")


class Outcome(Document):
    def __init__(self, *, tasks=None, variables=None, actions=()):
        tasks = dict(tasks or {})
        for key, state in tasks.items():
            ident(key); ident(state)
        self.data = dict(tasks=tasks, variables=plain(variables or {}), actions=plain(actions))
        for key, change in self.data["variables"].items():
            ident(key)
            VariableChange(**change)


class WeightModifier(Document):
    def __init__(self, *, when, add):
        self.data = dict(when=plain(when), add=integer(add, -100000, 100000))


class Event(Document):
    def __init__(self, *, id, name, script, trigger, outcomes=None, when=None, position=None,
                 once=True, enabled=True, check="stage", priority=0, chance=100, pool=None,
                 weight=1, weight_modifiers=()):
        ident(id); ident(script)
        enum(trigger, ("start", "auto", "position")); enum(check, ("month", "stage", "action"))
        if type(once) is not bool or type(enabled) is not bool:
            raise TypeError("Event once/enabled must be bool")
        if type(chance) not in (int, float) or not 0 <= chance <= 100:
            raise ValueError("Event chance must be 0..100")
        self.data = dict(id=id, name=name, script=script, trigger=trigger, once=once, enabled=enabled,
                         check=check, priority=integer(priority, -100000, 100000), chance=chance,
                         weight=integer(weight, 0, 100000), when=plain(when if when is not None else All()),
                         outcomes=plain(outcomes if outcomes is not None else {"completed": Outcome()}))
        if "completed" not in self.data["outcomes"]:
            raise ValueError("Event outcomes must include completed")
        for result in self.data["outcomes"]:
            ident(result)
        if trigger == "position":
            if not isinstance(position, str) or not position:
                raise ValueError("Position event needs position")
            self.data["position"] = position
        elif position is not None:
            raise ValueError("Only position events accept position")
        if pool is not None:
            if trigger == "start":
                raise ValueError("Start event cannot join a pool")
            self.data["pool"] = ident(pool)
        if trigger == "start" and (not enabled or chance != 100 or self.data["when"] != {"all": []}):
            raise ValueError("Start event must be enabled, chance=100 and have no conditions")
        if weight_modifiers:
            self.data["weight_modifiers"] = plain(weight_modifiers)


class Scenario(Document):
    def __init__(self, *, events, tasks=(), entry_event=None):
        events, tasks = plain(events), plain(tasks)
        for rows, label in [(events, "events"), (tasks, "tasks")]:
            ids = [row["id"] for row in rows]
            if len(ids) != len(set(ids)):
                raise ValueError(f"Duplicate {label} ID")
        states = {task["id"]: task["states"] for task in tasks}
        def check(condition):
            if "all" in condition or "any" in condition:
                for child in condition.get("all", condition.get("any")):
                    check(child)
            elif "not" in condition:
                check(condition["not"])
            elif condition.get("source") == "task" and condition["value"] not in states.get(condition["key"], []):
                raise ValueError("Condition references missing task/state")
        for task in tasks:
            for transition in task.get("transitions", []):
                check(transition["when"])
        for event in events:
            check(event["when"])
            for modifier in event.get("weight_modifiers", []):
                check(modifier["when"])
            for outcome in event["outcomes"].values():
                for key, state in outcome.get("tasks", {}).items():
                    if state not in states.get(key, []):
                        raise ValueError(f"Outcome references missing task/state: {key}/{state}")
        self.data = dict(schema=1, events=events, tasks=tasks)
        if entry_event is not None:
            if entry_event not in {e["id"] for e in events}:
                raise ValueError("Missing entry event")
            self.data["entry_event"] = entry_event


class Translation(Document):
    def __init__(self, *, locale, texts):
        enum(locale, ("chs", "cht", "ja", "ko"))
        if not isinstance(texts, dict) or any(not isinstance(v, str) or not v for v in texts.values()):
            raise ValueError("Translation texts must map text paths to nonempty strings")
        self.locale, self.data = locale, deepcopy(texts)


class Localization(Document):
    def __init__(self, *, default_locale="chs", fallback_locale=None, translations=()):
        enum(default_locale, ("chs", "cht", "ja", "ko"))
        fallback_locale = default_locale if fallback_locale is None else fallback_locale
        enum(fallback_locale, ("chs", "cht", "ja", "ko"))
        if isinstance(translations, dict):
            translations = [Translation(locale=k, texts=v) for k, v in translations.items()]
        catalogs = {}
        for item in translations:
            if not isinstance(item, Translation):
                raise TypeError("Use Translation(locale=..., texts=...)")
            if item.locale == default_locale or item.locale in catalogs:
                raise ValueError("Duplicate/default language in translations")
            catalogs[item.locale] = item.to_dict()
        self.data = dict(default_locale=default_locale, fallback_locale=fallback_locale, translations=catalogs)


class Asset:
    """Metadata is inline JSON; source media stays at its original file path."""
    def __init__(self, *, type, id, name, main, portraits=None, files=None, **settings):
        enum(type, ("audio", "character", "image", "item", "martial", "support", "skill"))
        if not isinstance(id, str) or not re.fullmatch(r"[a-z][a-z0-9_]{0,31}\.[a-z0-9][a-z0-9_]{0,47}", id):
            raise ValueError("Asset ID must be lowercase namespace.name")
        if not isinstance(name, str) or not name.strip():
            raise ValueError("Asset needs a display name")
        allowed = {"audio_kind", "character", "extends", "intro", "title", "scale", "art_facing", "mood_x", "mood_y", "portrait_layouts", "portrait_reference", "combat_idle", "combat_attack", "combat_hurt", "combat_defence", "description", "category", "cost", "levels", "trigger", "chance", "uses", "effects", "use_in", "action", "power", "stamina_cost", "accuracy", "cooldown", "ai_chance", "animation"}
        if set(settings) - allowed:
            raise TypeError(f"Unknown asset fields: {sorted(set(settings) - allowed)}")
        self.sources = {}
        def source(file):
            path = Path(file).resolve()
            if not path.is_file():
                raise FileNotFoundError(path)
            filename = path.name
            if filename in self.sources and self.sources[filename] != str(path):
                raise ValueError(f"Different files share filename: {filename}")
            self.sources[filename] = str(path)
            return filename
        self.data = dict(schema=1, content_schema=1, type=type, id=id, name=name, files={"main": source(main)}, **plain(settings))
        for filename, path in (files or {}).items():
            if Path(filename).name != filename or "/" in filename or "\\" in filename:
                raise ValueError("Asset files need simple filenames")
            path = Path(path).resolve()
            if not path.is_file():
                raise FileNotFoundError(path)
            if filename in self.sources and self.sources[filename] != str(path):
                raise ValueError(f"Conflicting file: {filename}")
            self.sources[filename] = str(path)
        if portraits is not None:
            if type != "character":
                raise ValueError("Only character assets have portraits")
            for portrait in portraits:
                if not re.fullmatch(r"[A-Za-z][A-Za-z0-9_]{0,31}", portrait):
                    raise ValueError(f"Invalid portrait ID: {portrait}")
            self.data["portraits"] = {key: source(path) for key, path in portraits.items()}
        self.ref = f"user:{id}"

    def to_dict(self):
        return deepcopy(self.data)

    def package_files(self):
        prefix = f"assets/user/{self.data['type']}/{self.data['id']}/"
        return {prefix + "content.json": {"json": self.to_dict()},
                **{prefix + name: path for name, path in self.sources.items()}}


class ImageAsset(Asset):
    def __init__(self, *, id, name, main):
        super().__init__(type="image", id=id, name=name, main=main)


class AudioAsset(Asset):
    def __init__(self, *, id, name, main, audio_kind, character=None):
        enum(audio_kind, ("music", "sound", "env"))
        settings = dict(audio_kind=audio_kind)
        if character is not None:
            settings["character"] = character
        super().__init__(type="audio", id=id, name=name, main=main, **settings)


class CharacterAsset(Asset):
    def __init__(self, *, id, name, main, portraits, **settings):
        super().__init__(type="character", id=id, name=name, main=main, portraits=portraits, **settings)

    @classmethod
    def from_folder(cls, *, id, name, folder, prefix="", **settings):
        paths = sorted(p for p in Path(folder).iterdir() if p.is_file() and p.name.startswith(prefix)
                       and p.suffix.lower() in (".png", ".jpg", ".jpeg"))
        if not paths:
            raise ValueError("No matching portraits in folder")
        portraits = {}
        for path in paths:
            stem = path.stem[len(prefix):].lstrip("_-") if prefix else path.stem
            if not re.fullmatch(r"[A-Za-z][A-Za-z0-9_]{0,31}", stem):
                stem = "portrait"
            key, number = stem, 2
            while key in portraits:
                key, number = f"{stem[:24]}_{number}", number + 1
            portraits[key] = path
        main = portraits.get("normal", paths[0])
        portraits.setdefault("normal", main)
        return cls(id=id, name=name, main=main, portraits=portraits, **settings)


class ChoiceStep(Document):
    def __init__(self, *, node, option):
        self.data = dict(node=ident(node), option=integer(option, 0))


class RouteTest(Document):
    def __init__(self, *, name, story, choices=(), reaches_node=(), reaches_ending=True,
                 flags=None, variables=None, initial_flags=None, initial_variables=None):
        if not isinstance(name, str) or not name.strip():
            raise ValueError("Route test needs name")
        ident(story)
        if isinstance(reaches_node, str):
            reaches_node = [reaches_node]
        if type(reaches_ending) is not bool:
            raise TypeError("reaches_ending must be bool")
        self.data = {"name": name, "story": story,
                     "initial": {"flags": plain(initial_flags or {}), "variables": plain(initial_variables or {})},
                     "actions": {"choices": plain(choices)},
                     "assert": {"reaches_node": [ident(node) for node in reaches_node],
                                "reaches_ending": reaches_ending, "flags": plain(flags or {}), "variables": plain(variables or {})}}


__all__ = ["Condition", "All", "Any", "Not", "TaskTransition", "Task", "VariableChange", "OutcomeAction", "Outcome", "WeightModifier", "Event", "Scenario", "Translation", "Localization", "Asset", "ImageAsset", "AudioAsset", "CharacterAsset", "ChoiceStep", "RouteTest"]
