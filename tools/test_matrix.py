# -*- coding: utf-8 -*-
"""One explicit, offline entry point for the repository test matrix."""

from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass
import json
import os
from pathlib import Path
import subprocess
import sys
import time


@dataclass(frozen=True)
class MatrixStep:
    name: str
    command: tuple[str, ...]
    cwd: Path
    coverage: tuple[str, ...]


@dataclass(frozen=True)
class StepResult:
    name: str
    command: tuple[str, ...]
    cwd: str
    coverage: tuple[str, ...]
    exit_code: int
    duration_seconds: float


def _editor_python(root: Path) -> Path:
    candidates = (
        root / "editor" / ".venv" / "Scripts" / "python.exe",
        root / "editor" / ".venv" / "bin" / "python",
    )
    return next((path for path in candidates if path.is_file()), Path(sys.executable))


def build_matrix(root: str | Path) -> tuple[MatrixStep, ...]:
    project = Path(root).resolve()
    python = str(_editor_python(project))
    runtime = project / "runtime" / "MortalModHost"
    return (
        MatrixStep(
            "compiler-tests",
            (python, "-m", "unittest", "discover", "-s", "tests", "-p", "test_*.py", "-v"),
            project / "compiler",
            ("compiler tests", "localization tests", "watermark tests"),
        ),
        MatrixStep(
            "editor-tests",
            (python, "-m", "unittest", "discover", "-s", "tests", "-p", "*_test.py", "-v"),
            project / "editor",
            (
                "story_api tests", "content tests", "package tests",
                "migration tests", "editor localization tests",
            ),
        ),
        MatrixStep(
            "editor-smoke",
            (python, "tests/smoke_test.py"),
            project / "editor",
            ("editor smoke",),
        ),
        MatrixStep(
            "editor-stress",
            (python, "tests/stress_test.py"),
            project / "editor",
            ("editor stress",),
        ),
        MatrixStep(
            "runtime-build",
            ("dotnet", "build", "-c", "Release"),
            runtime,
            ("Runtime build",),
        ),
        MatrixStep(
            "runtime-smoke",
            ("dotnet", "run", "--project", "test/SmokeTest", "-c", "Release"),
            runtime,
            ("Runtime smoke",),
        ),
    )


def run_steps(steps: tuple[MatrixStep, ...]) -> tuple[StepResult, ...]:
    results: list[StepResult] = []
    environment = os.environ.copy()
    environment.setdefault("QT_QPA_PLATFORM", "offscreen")
    environment.setdefault("PYTHONUTF8", "1")
    for index, step in enumerate(steps, 1):
        print("[%d/%d] %s" % (index, len(steps), step.name), flush=True)
        print("  cwd: %s" % step.cwd, flush=True)
        print("  cmd: %s" % subprocess.list2cmdline(step.command), flush=True)
        started = time.monotonic()
        try:
            completed = subprocess.run(
                list(step.command), cwd=step.cwd, env=environment, check=False
            )
            exit_code = completed.returncode
        except OSError as exc:
            print("  无法启动：%s" % exc, file=sys.stderr, flush=True)
            exit_code = 127
        duration = round(time.monotonic() - started, 3)
        results.append(StepResult(
            step.name, step.command, str(step.cwd), step.coverage, exit_code, duration
        ))
        print("  %s (%.3fs)" % ("PASS" if exit_code == 0 else "FAIL", duration), flush=True)
    return tuple(results)


def write_report(path: str | Path, results: tuple[StepResult, ...]) -> None:
    target = Path(path)
    target.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "format": 1,
        "passed": all(item.exit_code == 0 for item in results),
        "steps": [asdict(item) for item in results],
    }
    temporary = target.with_suffix(target.suffix + ".tmp")
    temporary.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    os.replace(temporary, target)


def _print_matrix(steps: tuple[MatrixStep, ...]) -> None:
    for step in steps:
        print("%s\n  coverage: %s\n  cwd: %s\n  cmd: %s" % (
            step.name,
            ", ".join(step.coverage),
            step.cwd,
            subprocess.list2cmdline(step.command),
        ))


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="lom_modkit 离线测试矩阵")
    selection = parser.add_mutually_exclusive_group()
    selection.add_argument("--full", action="store_true", help="运行完整矩阵")
    selection.add_argument("--step", action="append", help="只运行指定步骤；可重复")
    parser.add_argument("--report", help="把本次运行结果写成 JSON")
    args = parser.parse_args(argv)
    root = Path(__file__).resolve().parent.parent
    matrix = build_matrix(root)
    if not args.full and not args.step:
        _print_matrix(matrix)
        return 0
    selected = matrix
    if args.step:
        names = set(args.step)
        unknown = names - {step.name for step in matrix}
        if unknown:
            parser.error("未知步骤：" + "、".join(sorted(unknown)))
        selected = tuple(step for step in matrix if step.name in names)
    results = run_steps(selected)
    if args.report:
        write_report(args.report, results)
    return 0 if all(item.exit_code == 0 for item in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
