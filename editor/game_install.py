# -*- coding: utf-8 -*-
"""活侠传目录配置、运行时安装与 .lommod 启停管理。

模块刻意不依赖 Qt，便于离线测试，也让图形界面只负责展示和确认。
启用的包位于 ``mods/``；停用的包移动到同级 ``mods_disabled/``，运行时不会扫描。
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
import zipfile
from dataclasses import dataclass
from pathlib import Path
from typing import Callable


APP_DIR_NAME = "lom_modkit"
PLUGIN_DIR_NAME = "MortalModHost"
RUNTIME_DLL_NAME = "MortalModHost.dll"
PREVIEW_PACKAGE_NAME = "__lom_modkit_preview.lommod"
PREVIEW_REQUEST_NAME = "preview-request.json"
BEPINEX_VERSION = "6.0.0-be.692"
BEPINEX_URL = (
    "https://builds.bepinex.dev/projects/bepinex_be/692/"
    "BepInEx-Unity.Mono-win-x86-6.0.0-be.692%2B851521c.zip"
)
BEPINEX_SHA256 = "97720c5f5c70abfb2ae19dba6000529049ae67f053303b3ce2b49e6ad6c0eca6"
BEPINEX_MAX_BYTES = 32 * 1024 * 1024

ProgressCallback = Callable[[str, int, int], None]


class GameInstallError(RuntimeError):
    """目录、安装包或文件操作不满足安全条件。"""


@dataclass(frozen=True)
class ModRecord:
    path: Path
    enabled: bool
    mod_id: str
    name: str
    version: str
    author: str
    description: str
    error: str = ""


def _settings_path() -> Path:
    override = os.environ.get("LOM_MODKIT_SETTINGS_PATH")
    if override:
        return Path(override)
    appdata = os.environ.get("APPDATA")
    base = Path(appdata) if appdata else Path.home() / "AppData" / "Roaming"
    return base / APP_DIR_NAME / "settings.json"


def _runtime_dll_path() -> Path:
    if getattr(sys, "frozen", False):
        return Path(getattr(sys, "_MEIPASS")) / "runtime" / RUNTIME_DLL_NAME
    return (
        Path(__file__).resolve().parent.parent
        / "runtime"
        / PLUGIN_DIR_NAME
        / "bin"
        / "Release"
        / "net48"
        / RUNTIME_DLL_NAME
    )


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


class GameInstallManager:
    def __init__(self, settings_path: Path | None = None, runtime_dll: Path | None = None):
        self.settings_path = settings_path or _settings_path()
        self.runtime_dll = runtime_dll or _runtime_dll_path()

    # ------------------------------------------------------------ 配置
    def load_game_dir(self) -> Path | None:
        try:
            raw = json.loads(self.settings_path.read_text(encoding="utf-8"))
            value = raw.get("game_dir")
            return Path(value) if isinstance(value, str) and value else None
        except (OSError, ValueError, TypeError):
            return None

    def save_game_dir(self, game_dir: Path) -> None:
        root = game_dir.resolve()
        self.validate_game_root(root)
        self.settings_path.parent.mkdir(parents=True, exist_ok=True)
        temp = self.settings_path.with_suffix(".tmp")
        temp.write_text(
            json.dumps({"game_dir": str(root)}, ensure_ascii=False, indent=2),
            encoding="utf-8",
        )
        temp.replace(self.settings_path)

    @staticmethod
    def validate_game_root(game_dir: Path) -> None:
        root = Path(game_dir)
        missing = []
        for rel in ("Mortal.exe", "Mortal_Data/Managed"):
            if not (root / rel).exists():
                missing.append(rel)
        if missing:
            raise GameInstallError("这不是《活侠传》游戏目录。缺少：" + "、".join(missing))

    @staticmethod
    def validate_bepinex(game_dir: Path) -> None:
        root = Path(game_dir)
        missing = []
        for rel in (
            "BepInEx/core/BepInEx.Core.dll",
            "BepInEx/core/BepInEx.Unity.Mono.dll",
        ):
            if not (root / rel).is_file():
                missing.append(rel)
        if missing:
            raise GameInstallError(
                "尚未安装兼容的 BepInEx 6。缺少：" + "、".join(missing)
            )

    @staticmethod
    def bepinex_installed(game_dir: Path) -> bool:
        try:
            GameInstallManager.validate_bepinex(game_dir)
            return True
        except GameInstallError:
            return False

    @staticmethod
    def game_architecture(game_dir: Path) -> str:
        """读取 PE 头，不启动游戏即可确认 BepInEx 所需位数。"""
        exe = Path(game_dir) / "Mortal.exe"
        try:
            with exe.open("rb") as stream:
                header = stream.read(64)
                if len(header) < 64 or header[:2] != b"MZ":
                    raise ValueError("无效的 DOS 头")
                pe_offset = struct.unpack_from("<I", header, 0x3C)[0]
                stream.seek(pe_offset)
                pe = stream.read(6)
            if len(pe) != 6 or pe[:4] != b"PE\0\0":
                raise ValueError("无效的 PE 头")
            machine = struct.unpack_from("<H", pe, 4)[0]
        except (OSError, ValueError, struct.error) as exc:
            raise GameInstallError(f"无法确认 Mortal.exe 位数：{exc}") from exc
        if machine == 0x014C:
            return "x86"
        if machine == 0x8664:
            return "x64"
        raise GameInstallError(f"不支持的 Mortal.exe 架构：0x{machine:04X}")

    @staticmethod
    def detect_game_dir() -> Path | None:
        candidates: list[Path] = []
        for key in ("ProgramFiles(x86)", "ProgramFiles"):
            value = os.environ.get(key)
            if value:
                candidates.append(
                    Path(value) / "Steam" / "steamapps" / "common" / "LegendOfMortal"
                )

        steam_root = Path(os.environ.get("ProgramFiles(x86)", "C:/Program Files (x86)")) / "Steam"
        library_file = steam_root / "steamapps" / "libraryfolders.vdf"
        try:
            text = library_file.read_text(encoding="utf-8", errors="ignore")
            for match in re.finditer(r'"path"\s+"([^"]+)"', text):
                library = Path(match.group(1).replace("\\\\", "\\"))
                candidates.append(
                    library / "steamapps" / "common" / "LegendOfMortal"
                )
        except OSError:
            pass

        seen: set[str] = set()
        for candidate in candidates:
            key = str(candidate).casefold()
            if key in seen:
                continue
            seen.add(key)
            try:
                GameInstallManager.validate_game_root(candidate)
                return candidate.resolve()
            except GameInstallError:
                continue
        return None

    # ------------------------------------------------------------ 路径
    def require_game_dir(self) -> Path:
        root = self.load_game_dir()
        if root is None:
            raise GameInstallError("尚未选择《活侠传》游戏文件夹。")
        self.validate_game_root(root)
        return root

    def plugin_dir(self) -> Path:
        return self.require_game_dir() / "BepInEx" / "plugins" / PLUGIN_DIR_NAME

    def mods_dir(self, enabled: bool = True) -> Path:
        return self.plugin_dir() / ("mods" if enabled else "mods_disabled")

    # ------------------------------------------------------------ 安装
    def install_runtime(self) -> tuple[Path, bool]:
        """安装内置 DLL；返回 (目标路径, 是否实际更新)。"""
        if not self.runtime_dll.is_file():
            raise GameInstallError(f"编辑器内置运行时不存在：{self.runtime_dll}")
        root = self.require_game_dir()
        self.validate_bepinex(root)
        target_dir = self.plugin_dir()
        target_dir.mkdir(parents=True, exist_ok=True)
        self.mods_dir(True).mkdir(parents=True, exist_ok=True)
        self.mods_dir(False).mkdir(parents=True, exist_ok=True)
        target = target_dir / RUNTIME_DLL_NAME
        changed = not target.exists() or _sha256(target) != _sha256(self.runtime_dll)
        if changed:
            try:
                shutil.copy2(self.runtime_dll, target)
            except OSError as exc:
                raise GameInstallError(
                    "无法安装运行时。请确认游戏已退出，并检查目录写入权限：" + str(exc)
                ) from exc
        return target, changed

    def install_bepinex(self, progress: ProgressCallback | None = None) -> tuple[str, str]:
        """下载并安装已验证的官方 BepInEx 6 Mono x86 版本。"""
        root = self.require_game_dir()
        architecture = self.game_architecture(root)
        if architecture != "x86":
            raise GameInstallError(
                f"当前 Mortal.exe 是 {architecture}，内置安装器只支持已验证的 x86 版本。"
            )

        if progress:
            progress("正在连接 BepInEx 官方下载站…", 0, 0)
        request = urllib.request.Request(
            BEPINEX_URL,
            headers={"User-Agent": f"lom_modkit/{BEPINEX_VERSION}"},
        )
        try:
            with tempfile.TemporaryDirectory(prefix="lom_modkit_bepinex_") as temp_dir:
                archive_path = Path(temp_dir) / "bepinex.zip"
                with urllib.request.urlopen(request, timeout=30) as response:
                    total = int(response.headers.get("Content-Length") or 0)
                    if total > BEPINEX_MAX_BYTES:
                        raise GameInstallError("BepInEx 下载文件异常大，已停止安装。")
                    current = 0
                    with archive_path.open("wb") as output:
                        while True:
                            chunk = response.read(64 * 1024)
                            if not chunk:
                                break
                            current += len(chunk)
                            if current > BEPINEX_MAX_BYTES:
                                raise GameInstallError("BepInEx 下载文件超过安全上限。")
                            output.write(chunk)
                            if progress:
                                progress("正在下载 BepInEx…", current, total)
                if _sha256(archive_path).lower() != BEPINEX_SHA256:
                    raise GameInstallError(
                        "BepInEx 下载文件校验失败。文件可能不完整，请稍后重试。"
                    )
                if progress:
                    progress("正在安装 BepInEx…", 0, 0)
                self._install_bepinex_archive(archive_path, root)
        except GameInstallError:
            raise
        except (OSError, urllib.error.URLError, zipfile.BadZipFile) as exc:
            raise GameInstallError(f"BepInEx 下载或安装失败：{exc}") from exc

        self.validate_bepinex(root)
        if progress:
            progress("BepInEx 安装完成", 1, 1)
        return BEPINEX_VERSION, BEPINEX_URL

    @staticmethod
    def _install_bepinex_archive(archive_path: Path, game_dir: Path) -> None:
        """安全解包；拒绝路径穿越，不删除现有配置、插件或 Mod。"""
        root = Path(game_dir).resolve()
        with tempfile.TemporaryDirectory(prefix="lom_modkit_bepinex_extract_") as temp_dir:
            staging = Path(temp_dir)
            with zipfile.ZipFile(archive_path, "r") as archive:
                files = []
                for info in archive.infolist():
                    name = info.filename.replace("\\", "/")
                    parts = [part for part in name.split("/") if part]
                    if (
                        name.startswith("/")
                        or ":" in (parts[0] if parts else "")
                        or any(part == ".." for part in parts)
                    ):
                        raise GameInstallError("BepInEx 压缩包包含不安全路径，已停止安装。")
                    if info.is_dir():
                        continue
                    files.append((info, Path(*parts)))
                names = {path.as_posix().casefold() for _info, path in files}
                required = {
                    "bepinex/core/bepinex.core.dll",
                    "bepinex/core/bepinex.unity.mono.dll",
                    "winhttp.dll",
                }
                if not required.issubset(names):
                    raise GameInstallError("BepInEx 压缩包结构不完整，已停止安装。")
                for info, relative in files:
                    output = staging / relative
                    output.parent.mkdir(parents=True, exist_ok=True)
                    with archive.open(info) as source, output.open("wb") as target:
                        shutil.copyfileobj(source, target)

            for source in staging.rglob("*"):
                if not source.is_file():
                    continue
                relative = source.relative_to(staging)
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, target)

    def install_mod(self, package: Path, enabled: bool = True) -> Path:
        source = Path(package)
        if source.suffix.lower() != ".lommod" or not source.is_file():
            raise GameInstallError("请选择有效的 .lommod 文件。")
        # 先读 manifest，坏包不能进入游戏扫描目录。
        self._read_manifest(source)
        self.install_runtime()
        target_dir = self.mods_dir(enabled)
        other_dir = self.mods_dir(not enabled)
        target_dir.mkdir(parents=True, exist_ok=True)
        other_dir.mkdir(parents=True, exist_ok=True)
        target = target_dir / source.name
        try:
            if source.resolve() != target.resolve():
                # 先复制到同目录临时文件再原子替换，避免游戏扫描到只写了一半的 zip。
                staging = target.with_suffix(target.suffix + ".tmp")
                shutil.copy2(source, staging)
                staging.replace(target)
            stale = other_dir / source.name
            if stale.exists():
                stale.unlink()
        except OSError as exc:
            raise GameInstallError(
                "无法复制 Mod。请确认游戏已退出，并检查目录写入权限：" + str(exc)
            ) from exc
        return target

    # ------------------------------------------------------------ 一键试玩
    @staticmethod
    def is_game_running() -> bool:
        """只读检查 Mortal.exe 是否正在运行；查询失败按未运行处理。"""
        if os.name != "nt":
            return False
        try:
            result = subprocess.run(
                ["tasklist", "/FI", "IMAGENAME eq Mortal.exe", "/FO", "CSV", "/NH"],
                capture_output=True,
                text=True,
                timeout=5,
                check=False,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
            )
            return any(
                line.lstrip().lower().startswith('"mortal.exe"')
                for line in result.stdout.splitlines()
            )
        except (OSError, subprocess.SubprocessError):
            return False

    def request_preview(self, mod_id: str, script_id: str, node_id: str) -> Path:
        """原子写入运行时试玩请求；游戏会在安全场景自动消费并删除。"""
        if not all(
            isinstance(value, str) and re.fullmatch(r"[A-Za-z0-9_-]+", value)
            for value in (mod_id, script_id, node_id)
        ):
            raise GameInstallError("试玩请求包含无效的 Mod、章节或步骤编号。")
        target_dir = self.plugin_dir()
        target_dir.mkdir(parents=True, exist_ok=True)
        target = target_dir / PREVIEW_REQUEST_NAME
        staging = target.with_suffix(".tmp")
        try:
            staging.write_text(
                json.dumps(
                    {
                        "format": 1,
                        "mod_id": mod_id,
                        "script_id": script_id,
                        "node_id": node_id,
                        "requested_at": int(time.time()),
                    },
                    ensure_ascii=False,
                    indent=2,
                ),
                encoding="utf-8",
            )
            staging.replace(target)
        except OSError as exc:
            raise GameInstallError(f"无法写入试玩请求：{exc}") from exc
        return target

    def launch_game(self) -> bool:
        """游戏未运行时启动 Mortal.exe；返回是否实际启动了新进程。"""
        if self.is_game_running():
            return False
        root = self.require_game_dir()
        exe = root / "Mortal.exe"
        try:
            subprocess.Popen(
                [str(exe)],
                cwd=str(root),
                close_fds=True,
                creationflags=getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0),
            )
        except OSError as exc:
            raise GameInstallError(f"无法启动游戏：{exc}") from exc
        return True

    # ------------------------------------------------------------ 管理
    @staticmethod
    def _read_manifest(path: Path) -> dict:
        try:
            with zipfile.ZipFile(path, "r") as archive:
                raw = archive.read("manifest.json")
            manifest = json.loads(raw.decode("utf-8-sig"))
            if not isinstance(manifest, dict):
                raise ValueError("manifest.json 不是对象")
            if not manifest.get("id"):
                raise ValueError("manifest.json 缺少 id")
            return manifest
        except (OSError, KeyError, ValueError, zipfile.BadZipFile) as exc:
            raise GameInstallError(f"{path.name} 不是有效的 .lommod：{exc}") from exc

    def list_mods(self) -> list[ModRecord]:
        records: list[ModRecord] = []
        for enabled in (True, False):
            directory = self.mods_dir(enabled)
            if not directory.exists():
                continue
            for path in sorted(directory.glob("*.lommod"), key=lambda p: p.name.casefold()):
                try:
                    m = self._read_manifest(path)
                    records.append(
                        ModRecord(
                            path=path,
                            enabled=enabled,
                            mod_id=str(m.get("id") or ""),
                            name=str(m.get("name") or m.get("id") or path.stem),
                            version=str(m.get("version") or ""),
                            author=str(m.get("author") or ""),
                            description=str(m.get("description") or ""),
                        )
                    )
                except GameInstallError as exc:
                    records.append(
                        ModRecord(path, enabled, path.stem, path.stem, "", "", "", str(exc))
                    )
        return sorted(records, key=lambda r: (not r.enabled, r.name.casefold(), r.path.name.casefold()))

    def set_enabled(self, path: Path, enabled: bool) -> Path:
        source = Path(path)
        expected_parent = self.mods_dir(not enabled).resolve()
        try:
            if source.resolve().parent != expected_parent:
                raise GameInstallError("Mod 当前状态已经改变，请刷新列表后重试。")
        except OSError as exc:
            raise GameInstallError(f"无法确认 Mod 路径：{exc}") from exc
        target_dir = self.mods_dir(enabled)
        target_dir.mkdir(parents=True, exist_ok=True)
        target = target_dir / source.name
        if target.exists():
            raise GameInstallError(f"目标目录已经有同名文件：{target.name}")
        try:
            source.replace(target)
        except OSError as exc:
            raise GameInstallError(
                "无法切换 Mod 状态。请确认游戏已退出，并检查目录写入权限：" + str(exc)
            ) from exc
        return target
