"""Node constructors for the native LoM Modkit CLI. Python 3.10+, stdlib only.

Export this package and its matching contract with `lom-editor.exe --cli sdk -o DIR`.
Every constructor is generated from the compiler's embedded schema.
"""
from __future__ import annotations

from copy import deepcopy
import inspect
import json
import keyword
import math
from pathlib import Path
import re
import subprocess
import tempfile

_CONTRACT = json.loads(Path(__file__).with_name("contract.json").read_text(encoding="utf-8"))
_SCHEMA = _CONTRACT["schema"]
VERSION = _CONTRACT["version"]


def _plain(value):
    if hasattr(value, "to_dict"):
        return value.to_dict()
    if isinstance(value, dict):
        return {k: _plain(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [_plain(v) for v in value]
    return value


def _identifier(value, label, *, chapter=False):
    pattern = r"[A-Za-z0-9_-]{1,64}" if chapter else r"[A-Za-z0-9_]+"
    if not isinstance(value, str) or not re.fullmatch(pattern, value):
        raise ValueError(f"{label}: expected {pattern}")


def _valid(tag, value):
    if tag == "str":
        return isinstance(value, str)
    if tag == "idstr":
        return isinstance(value, str) and bool(value)
    if tag == "script_id":
        return isinstance(value, str) and re.fullmatch(r"[A-Za-z0-9_-]{1,64}", value)
    if tag == "num":
        return type(value) in (int, float) and (type(value) is int or math.isfinite(value))
    if tag == "bool":
        return type(value) is bool
    if tag == "list":
        return isinstance(value, list)
    if tag == "expr":
        return isinstance(value, dict)
    if tag == "talent_level":
        return type(value) in (int, float) and value in (1, -1)
    if tag == "save_button":
        return type(value) in (int, float) and value in (0, 1)
    if tag == "say_font_size":
        return (type(value) is int and 0 <= value <= 200) or value in ("normal", "small", "large")
    if tag not in _SCHEMA["enums"]:
        raise RuntimeError(f"Unsupported compiler field type: {tag}; export a matching SDK")
    return value in _SCHEMA["enums"][tag]


class Node:
    """Base node. Use Say/Show/Choice/etc. or Node.from_dict(existing_node)."""
    kind = None

    def __init__(self, *, id=None, **fields):
        if self.kind not in _SCHEMA["nodes"]:
            raise TypeError("Use a named node constructor, e.g. Say(id='s', text='...')")
        spec = _SCHEMA["nodes"][self.kind]
        # Python keywords use a trailing underscore: Move(from_='M', to='R').
        normalized = {}
        for key, value in fields.items():
            key = key[:-1] if key.endswith("_") and keyword.iskeyword(key[:-1]) else key
            if key in normalized:
                raise TypeError(f"Duplicate field: {key}")
            normalized[key] = _plain(value)
        fields = normalized
        tags = {**spec["required"], **spec["optional"], "goto": "str", "_editor": "expr"}
        for key in fields:
            if key not in tags:
                raise TypeError(f"{type(self).__name__}: unknown field {key!r}; allowed: {', '.join(tags)}")
        for key in spec["required"]:
            if key not in fields:
                raise TypeError(f"{type(self).__name__}: missing required field {key!r}")
        for key, value in fields.items():
            if not _valid(tags[key], value):
                allowed = _SCHEMA["enums"].get(tags[key], tags[key])
                raise TypeError(f"{type(self).__name__}.{key}: expected {allowed}, got {value!r}")
        if id is not None:
            _identifier(id, f"{type(self).__name__}.id")
        json.dumps(fields, ensure_ascii=False, allow_nan=False)
        self.id = id
        self.fields = deepcopy(fields)

    def to_dict(self):
        value = {"type": self.kind, **deepcopy(self.fields)}
        if self.id is not None:
            value["id"] = self.id
        return value

    def action(self):
        """A parallel child has no flow ID/goto. Do not silently discard a goto."""
        if self.kind not in _CONTRACT["parallel_actions"]:
            raise ValueError(f"{self.kind} cannot run inside Parallel")
        if "goto" in self.fields:
            raise ValueError("A parallel action cannot have goto")
        value = self.to_dict()
        value.pop("id", None)
        return value

    def replace(self, **fields):
        value = self.to_dict()
        value.update(fields)
        return Node.from_dict(value)

    @staticmethod
    def from_dict(value):
        value = deepcopy(value)
        kind = value.pop("type")
        if kind not in NODE_TYPES:
            raise ValueError(f"Unknown node type: {kind}")
        return NODE_TYPES[kind](**value)


def _constructor(kind, spec):
    name = "".join(part.capitalize() for part in kind.split("_"))
    cls = type(name, (Node,), {"kind": kind, "__module__": __name__,
                              "__doc__": f"{kind} node. Required: {', '.join(spec['required']) or '(none)'}."})
    parameters = [inspect.Parameter("id", inspect.Parameter.KEYWORD_ONLY, default=None)]
    for key, tag in {**spec["required"], **spec["optional"], "goto": "str", "_editor": "expr"}.items():
        parameter = key + "_" if keyword.iskeyword(key) else key
        parameters.append(inspect.Parameter(parameter, inspect.Parameter.KEYWORD_ONLY,
                          default=inspect.Parameter.empty if key in spec["required"] else None,
                          annotation=tag))
    cls.__signature__ = inspect.Signature(parameters)
    return cls


NODE_TYPES = {kind: _constructor(kind, spec) for kind, spec in _SCHEMA["nodes"].items()}
globals().update({cls.__name__: cls for cls in NODE_TYPES.values()})


class Option:
    def __init__(self, *, text: str, goto: str):
        if not isinstance(text, str) or not text:
            raise ValueError("Option.text must not be empty")
        _identifier(goto, "Option.goto")
        self.text, self.goto = text, goto

    def to_dict(self):
        return {"text": self.text, "goto": self.goto}


class Chapter:
    def __init__(self, *, id: str, title: str, nodes, start=None, **metadata):
        _identifier(id, "Chapter.id", chapter=True)
        if not isinstance(title, str):
            raise TypeError("Chapter.title must be a string")
        nodes = [_plain(node) for node in nodes]
        ids = []
        for node in nodes:
            if not isinstance(node, dict):
                raise TypeError("Chapter.nodes must contain node objects")
            Node.from_dict(node)
            _identifier(node.get("id"), "Chapter node.id")
            ids.append(node["id"])
        if not ids or len(ids) != len(set(ids)):
            raise ValueError("Chapter needs nodes with unique IDs")
        start = start if start is not None else ids[0]
        if start not in ids:
            raise ValueError(f"Chapter.start refers to missing node: {start}")
        if any(k in metadata for k in ("id", "title", "nodes", "start", "story_schema")):
            raise TypeError("Reserved chapter metadata")
        self.id, self.title, self.nodes, self.start = id, title, nodes, start
        self.metadata = _plain(metadata)

    def to_dict(self):
        return {**_plain(self.metadata), "story_schema": 2, "id": self.id,
                "title": self.title, "start": self.start, "nodes": _plain(self.nodes)}


class Project:
    def __init__(self, *, id: str, name: str, chapters, author="作者", version="1.0.0",
                 entry=None, mode="story", target=None, scenario=None, assets=None, manifest=None):
        if not isinstance(id, str) or not re.fullmatch(r"[a-z0-9_-]{1,64}", id):
            raise ValueError("Project.id must use 1-64 lowercase letters, digits, underscores or hyphens")
        for key, value in (("name", name), ("author", author), ("version", version)):
            if not isinstance(value, str) or not value.strip():
                raise ValueError(f"Project.{key} must not be empty")
        if mode not in ("story", "addon", "sandbox", "vanilla_patch"):
            raise ValueError("Unknown project mode")
        chapters = [_plain(chapter) for chapter in chapters]
        stories = {chapter["id"]: chapter for chapter in chapters}
        if not stories or len(stories) != len(chapters):
            raise ValueError("Project needs chapters with unique IDs")
        entry = entry if entry is not None else chapters[0]["id"]
        if entry not in stories:
            raise ValueError(f"Project.entry refers to missing chapter: {entry}")
        self.recipe = {"mode": mode, "manifest": {**(manifest or {}), "id": id, "name": name,
                       "author": author, "version": version, "entry": entry}, "stories": stories}
        if target is not None:
            self.recipe["target"] = target
        if scenario is not None:
            self.recipe["scenario"] = _plain(scenario)
        if assets:
            if isinstance(assets, dict):
                self.recipe["assets"] = {k: str(Path(v).resolve()) if isinstance(v, (str, Path)) else _plain(v) for k, v in assets.items()}
            else:
                self.recipe["assets"] = {}
                for asset in assets:
                    additions = asset.package_files()
                    if set(additions) & set(self.recipe["assets"]):
                        raise ValueError("Duplicate asset ID or file")
                    self.recipe["assets"].update(additions)

    @property
    def stories(self):
        return self.recipe["stories"]

    @property
    def manifest(self):
        return self.recipe["manifest"]

    @classmethod
    def open(cls, cli, path):
        """Preserve original metadata/assets/identity using the native reader."""
        instance = cls.__new__(cls)
        instance.recipe = cli.run("project-read", path)["project"]
        return instance

    def to_dict(self):
        return _plain(self.recipe)

    def write(self, path):
        path = Path(path)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(self.to_dict(), ensure_ascii=False, indent=2, allow_nan=False) + "\n", encoding="utf-8")
        return path


class CLIError(RuntimeError):
    def __init__(self, result, returncode):
        self.result, self.returncode = result, returncode
        super().__init__(json.dumps(result, ensure_ascii=False, indent=2))


class CLI:
    def __init__(self, executable):
        path = Path(executable).resolve()
        self.command = [str(path)] + (["--cli"] if path.stem == "lom-editor" else [])

    def run(self, *args):
        completed = subprocess.run([*self.command, *map(str, args), "--json"],
                                   capture_output=True, encoding="utf-8")
        try:
            result = json.loads(completed.stdout)
        except json.JSONDecodeError as error:
            raise RuntimeError(f"CLI did not return JSON (exit {completed.returncode}): {completed.stderr or completed.stdout}") from error
        if completed.returncode or result.get("ok") is False:
            raise CLIError(result, completed.returncode)
        return result

    def build(self, project: Project, output):
        with tempfile.TemporaryDirectory(prefix="lom-author-") as directory:
            recipe = project.write(Path(directory) / "project.json")
            return self.run("project", recipe, "-o", output)

    def export(self, project_path, output):
        return self.run("export", project_path, "-o", output)

    def operation(self, command, **request):
        """Invoke editor operations; query automation-schema for contracts.

        Paths are made absolute before the temporary request file is created.
        playtest/capture-effects execute game operations only when explicitly called.
        """
        request = _plain(request)
        for key in ("project", "game", "runtime", "output"):
            if key in request:
                request[key] = str(Path(request[key]).resolve())
        with tempfile.TemporaryDirectory(prefix="lom-operation-") as directory:
            path = Path(directory) / "request.json"
            path.write_text(json.dumps(request, ensure_ascii=False, allow_nan=False), encoding="utf-8")
            return self.run(command, path)

    def open_editor(self, project, *, output=None, script=None, node=None):
        request = {"project": project}
        request.update({k: v for k, v in (("output", output), ("script", script), ("node", node)) if v is not None})
        return self.operation("open-editor", **request)


__all__ = ["CLI", "CLIError", "Node", "Chapter", "Project", "Option", "NODE_TYPES", "VERSION",
           *[cls.__name__ for cls in NODE_TYPES.values()]]
from .authoring import *
from .authoring import __all__ as _authoring_exports
__all__ += _authoring_exports
