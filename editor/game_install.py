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
import unicodedata
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
MOD_PACKAGE_MAX_BYTES = 160 * 1024 * 1024
MOD_MANIFEST_MAX_BYTES = 4 * 1024 * 1024
MOD_ID_RE = re.compile(r"[a-z0-9_-]{1,64}")
SCRIPT_ID_RE = re.compile(r"[A-Za-z0-9_-]{1,64}")
MANIFEST_TEXT_LIMITS = {
    "name": 80,
    "version": 32,
    "author": 80,
    "description": 500,
}

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


_IGNORE_DISABLE_RE = re.compile(
    r"(?im)^([ \t]*ignore_disable_switch[ \t]*=[ \t]*)(\S+)"
)


def _bundled_doorstop_dll() -> Path:
    name = "win-x86-doorstop.dll"
    if getattr(sys, "frozen", False):
        return Path(getattr(sys, "_MEIPASS")) / "assets" / "doorstop" / name
    return Path(__file__).resolve().parent / "assets" / "doorstop" / name


def _system_version_dll() -> Path | None:
    windir = Path(os.environ.get("SystemRoot", r"C:\Windows"))
    for rel in ("SysWOW64/version.dll", "System32/version.dll"):
        candidate = windir / rel
        if candidate.is_file():
            return candidate
    return None


def _ensure_ignore_disable_switch(text: str) -> tuple[str, bool]:
    """Return (new_text, changed). Forces ignore_disable_switch = true."""
    match = _IGNORE_DISABLE_RE.search(text)
    if match:
        if match.group(2).strip().lower() == "true":
            return text, False
        new = _IGNORE_DISABLE_RE.sub(r"\1true", text, count=1)
        return new, True
    suffix = "" if not text or text.endswith("\n") else "\n"
    return text + suffix + "ignore_disable_switch = true\n", True


class GameInstallManager:
    def __init__(
        self,
        settings_path: Path | None = None,
        runtime_dll: Path | None = None,
        doorstop_dll: Path | None = None,
    ):
        self.settings_path = settings_path or _settings_path()
        self.runtime_dll = runtime_dll or _runtime_dll_path()
        self.doorstop_dll = doorstop_dll if doorstop_dll is not None else _bundled_doorstop_dll()

    # ------------------------------------------------------------ 配置
    def load_game_dir(self) -> Path | None:
        try:
            raw = json.loads(self.settings_path.read_text(encoding="utf-8"))
            value = raw.get("game_dir")
            return Path(value) if isinstance(value, str) and value else None
        except (OSError, ValueError, TypeError):
            return None

    def load_pref(self, key: str) -> str | None:
        """读取任意偏好键（如 last_open_dir）；不存在返回 None。"""
        try:
            raw = json.loads(self.settings_path.read_text(encoding="utf-8"))
            value = raw.get(key)
            return str(value) if value else None
        except (OSError, ValueError, TypeError):
            return None

    def save_pref(self, key: str, value: str) -> None:
        """合并写入偏好键（不动 game_dir 等其它键）。失败静默——偏好不关键。"""
        try:
            raw = json.loads(self.settings_path.read_text(encoding="utf-8"))
            if not isinstance(raw, dict):
                raw = {}
        except (OSError, ValueError, TypeError):
            raw = {}
        raw[key] = value
        try:
            self.settings_path.parent.mkdir(parents=True, exist_ok=True)
            temp = self.settings_path.with_suffix(".tmp")
            temp.write_text(
                json.dumps(raw, ensure_ascii=False, indent=2), encoding="utf-8"
            )
            temp.replace(self.settings_path)
        except OSError:
            pass

    def save_game_dir(self, game_dir: Path) -> None:
        root = game_dir.resolve()
        self.validate_game_root(root)
        try:
            raw = json.loads(self.settings_path.read_text(encoding="utf-8"))
            if not isinstance(raw, dict):
                raw = {}
        except (OSError, ValueError, TypeError):
            raw = {}
        raw["game_dir"] = str(root)
        self.settings_path.parent.mkdir(parents=True, exist_ok=True)
        temp = self.settings_path.with_suffix(".tmp")
        temp.write_text(
            json.dumps(raw, ensure_ascii=False, indent=2),
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
        extras = [
            src
            for src in self.runtime_dll.parent.glob("*.dll")
            if src.name != RUNTIME_DLL_NAME
        ]
        if not changed:
            for src in extras:
                dest = target_dir / src.name
                if not dest.exists() or _sha256(dest) != _sha256(src):
                    changed = True
                    break
        if changed:
            try:
                shutil.copy2(self.runtime_dll, target)
                for src in extras:
                    shutil.copy2(src, target_dir / src.name)
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
        self.apply_steam_launch_fix(root)
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

    def apply_steam_launch_fix(self, game_dir: Path | None = None) -> list[str]:
        """让 Steam 普通启动也能加载 Doorstop / BepInEx。

        1. doorstop_config.ini 打开 ignore_disable_switch（忽略继承的
           DOORSTOP_DISABLE / 配合补丁后的 DOORSTOP_INITIALIZED）。
        2. 代理改为 version.dll，避免 Steam overlay 抢系统 winhttp.dll。
        3. 若编辑器自带修补过的 Doorstop，覆盖 version.dll。
        """
        root = Path(game_dir) if game_dir is not None else self.require_game_dir()
        self.validate_game_root(root)
        self.validate_bepinex(root)
        if self.is_game_running():
            raise GameInstallError("游戏正在运行。请先退出《活侠传》，再应用修复。")

        actions: list[str] = []
        try:
            ini = root / "doorstop_config.ini"
            if ini.is_file():
                original = ini.read_text(encoding="utf-8", errors="replace")
            else:
                original = (
                    "[General]\n"
                    "enabled = true\n"
                    "target_assembly = BepInEx\\core\\BepInEx.Unity.Mono.Preloader.dll\n"
                )
                actions.append("已新建 doorstop_config.ini")
            patched, changed = _ensure_ignore_disable_switch(original)
            if changed:
                ini.write_text(patched, encoding="utf-8")
                actions.append("已打开 ignore_disable_switch（忽略 Steam 传入的 Doorstop 环境变量）")

            winhttp = root / "winhttp.dll"
            version = root / "version.dll"
            patched_doorstop = self.doorstop_dll if self.doorstop_dll.is_file() else None
            if patched_doorstop is not None:
                if not version.is_file() or _sha256(version) != _sha256(patched_doorstop):
                    shutil.copy2(patched_doorstop, version)
                    actions.append("已安装修补过的 Doorstop 为 version.dll")
            elif winhttp.is_file() and not version.is_file():
                shutil.copy2(winhttp, version)
                actions.append("已将 winhttp.dll 复制为 version.dll（Steam overlay 不会抢这个名字）")
            elif not version.is_file():
                raise GameInstallError(
                    "游戏目录没有 version.dll / winhttp.dll，也没有内置 Doorstop 补丁。"
                    "请先点击“安装 BepInEx”。"
                )

            if winhttp.is_file():
                bak = root / "winhttp.dll.lom_bak"
                if not bak.exists():
                    winhttp.replace(bak)
                    actions.append("已移走 winhttp.dll（避免与 Steam overlay 冲突）")
                else:
                    winhttp.unlink()
                    actions.append("已删除多余的 winhttp.dll")

            alt = root / "version_alt.dll"
            if not alt.is_file():
                system_ver = _system_version_dll()
                if system_ver is not None:
                    shutil.copy2(system_ver, alt)
                    actions.append("已复制系统 VERSION.dll 为 version_alt.dll")
        except OSError as exc:
            raise GameInstallError(
                "无法写入游戏目录。请确认游戏已退出，并检查目录写入权限：" + str(exc)
            ) from exc

        if not actions:
            actions.append("Steam 启动修复已经就绪，无需再改。")
        return actions

    def steam_launch_fix_applied(self, game_dir: Path | None = None) -> bool:
        root = Path(game_dir) if game_dir is not None else self.require_game_dir()
        ini = root / "doorstop_config.ini"
        if not (root / "version.dll").is_file() or not ini.is_file():
            return False
        try:
            text = ini.read_text(encoding="utf-8", errors="replace")
        except OSError:
            return False
        match = _IGNORE_DISABLE_RE.search(text)
        return bool(match and match.group(2).strip().lower() == "true")

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
                encoding="utf-8",
                errors="replace",
                timeout=5,
                check=False,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
            )
            stdout = result.stdout or ""
            return any(
                line.lstrip().lower().startswith('"mortal.exe"')
                for line in stdout.splitlines()
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
            if path.stat().st_size > MOD_PACKAGE_MAX_BYTES:
                raise ValueError("Mod 包文件超过 160 MiB 上限")
            with zipfile.ZipFile(path, "r") as archive:
                info = archive.getinfo("manifest.json")
                if info.file_size < 0 or info.file_size > MOD_MANIFEST_MAX_BYTES:
                    raise ValueError("manifest.json 超过 4 MiB 上限")
                with archive.open(info, "r") as stream:
                    raw = stream.read(MOD_MANIFEST_MAX_BYTES + 1)
                if len(raw) > MOD_MANIFEST_MAX_BYTES:
                    raise ValueError("manifest.json 超过 4 MiB 上限")
            manifest = json.loads(raw.decode("utf-8-sig"))
            if not isinstance(manifest, dict):
                raise ValueError("manifest.json 不是对象")
            mod_id = manifest.get("id")
            if not isinstance(mod_id, str) or MOD_ID_RE.fullmatch(mod_id) is None:
                raise ValueError("manifest.id 必须匹配 [a-z0-9_-]{1,64}")
            entry = manifest.get("entry")
            if not isinstance(entry, str) or SCRIPT_ID_RE.fullmatch(entry) is None:
                raise ValueError("manifest.entry 必须匹配 [A-Za-z0-9_-]{1,64}")
            for field, limit in MANIFEST_TEXT_LIMITS.items():
                value = manifest.get(field)
                if value is None:
                    continue
                if not isinstance(value, str) or len(value) > limit:
                    raise ValueError(f"manifest.{field} 必须是不超过 {limit} 字符的文本")
                if any(
                    char in "\r\n"
                    or unicodedata.category(char) in ("Cc", "Cf", "Zl", "Zp")
                    for char in value
                ):
                    raise ValueError(
                        f"manifest.{field} 不能含换行、控制、零宽或双向格式字符"
                    )
            return manifest
        except (OSError, KeyError, ValueError, RuntimeError, zipfile.BadZipFile) as exc:
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


# ------------------------------------------------------------ 剧情已读状态重置
def find_universe_saves() -> list[Path]:
    """定位全局存档 Save_universe.dat（已读文本清单所在）。

    路径：%USERPROFILE%\\AppData\\LocalLow\\Obb Studio\\Mortal\\<steamid>\\Save_universe.dat。
    返回全部命中（多账号时逐个处理）。同目录下的 Save_universe.json 由
    reset_story_read_state 作为 dat 的兄弟文件一并处理。
    """
    base = Path.home() / "AppData" / "LocalLow" / "Obb Studio" / "Mortal"
    if not base.is_dir():
        return []
    return sorted(base.glob("*/Save_universe.dat"))


def _iter_zombie_prefixes():
    """等长 4 字符僵尸前缀（含尾部下划线），避开运行时 live 前缀 MOD_。"""
    seen: set[str] = set()
    for prefix in ("mod_", "xod_", "yod_", "zod_", "wod_", "vod_"):
        seen.add(prefix)
        yield prefix
    for digit in "0123456789":
        prefix = f"{digit}od_"
        if prefix not in seen:
            seen.add(prefix)
            yield prefix
    alphabet = "abcdefghijklmnopqrstuvwxyz0123456789"
    for a in alphabet:
        for b in alphabet:
            for c in alphabet:
                prefix = f"{a}{b}{c}_"
                if prefix == "MOD_" or prefix in seen:
                    continue
                yield prefix


def _choose_zombie_prefix(raw: bytes, mod_id: str) -> bytes:
    """选一个文件中尚未出现为 ``{prefix}{modid}_`` 的等长 4 字符前缀。"""
    for prefix in _iter_zombie_prefixes():
        candidate = f"{prefix}{mod_id}_".encode("utf-8")
        if candidate not in raw:
            return candidate
    raise GameInstallError(
        f"无法为 mod「{mod_id}」找到可用的等长僵尸前缀（存档中变体重名过多）"
    )


def _validated_read_story_keys(mod_id: str, read_keys: list[str]) -> tuple[str, ...]:
    """校验并去重由当前项目实际生成的完整已读 key。"""
    live_prefix = f"MOD_{mod_id}_"
    valid: set[str] = set()
    for key in read_keys:
        if (
            not isinstance(key, str)
            or not key.startswith(live_prefix)
            or not re.fullmatch(r"[A-Za-z0-9_\-]+", key[len(live_prefix) :])
        ):
            raise GameInstallError(
                f"已读记录键 {key!r} 不属于 mod「{mod_id}」或格式非法"
            )
        valid.add(key)
    # 长 key 优先，配合末尾边界检查避免 n1 抢先命中 n10。
    return tuple(sorted(valid, key=lambda value: (-len(value), value)))


def build_story_read_keys(mod_id: str, stories: dict[str, dict]) -> list[str]:
    """从当前项目的 say 节点生成该 mod 会写入存档的完整已读 key。"""
    keys: list[str] = []
    for fallback_id, story in stories.items():
        if not isinstance(story, dict):
            continue
        script_id = str(story.get("id") or fallback_id)
        for node in story.get("nodes") or []:
            if not isinstance(node, dict) or node.get("type") != "say":
                continue
            node_id = node.get("id")
            if isinstance(node_id, str) and node_id:
                keys.append(f"MOD_{mod_id}_{script_id}_{node_id}")
    return list(_validated_read_story_keys(mod_id, keys))


def _is_mod_read_story_key(
    key: str, mod_id: str, read_keys: tuple[str, ...] | list[str]
) -> bool:
    """是否与给定完整 key 相同（允许 MOD_ 被等长僵尸前缀替换）。"""
    if not isinstance(key, str):
        return False
    if len(key) < 5 or key[3] != "_":
        return False
    suffix = key[4:].casefold()
    return any(
        candidate.startswith(f"MOD_{mod_id}_")
        and suffix == candidate[4:].casefold()
        for candidate in read_keys
    )


def _backup_once(path: Path, data: bytes) -> None:
    backup = path.with_suffix(path.suffix + ".lomkit_bak")
    if not backup.exists():
        backup.write_bytes(data)


def _atomic_write_bytes(path: Path, data: bytes) -> None:
    """同目录落盘并原子替换，失败时保留原存档。"""
    temp_path: Path | None = None
    try:
        fd, raw_temp = tempfile.mkstemp(
            prefix=path.name + ".", suffix=".tmp", dir=str(path.parent)
        )
        temp_path = Path(raw_temp)
        with os.fdopen(fd, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temp_path, path)
        temp_path = None
    except OSError as exc:
        raise GameInstallError(f"写入存档失败：{exc}") from exc
    finally:
        if temp_path is not None:
            try:
                temp_path.unlink()
            except OSError:
                pass


def _has_binary_formatter_string_prefix(raw: bytes, start: int, length: int) -> bool:
    """识别 BinaryObjectString 的 ``record + objectId + 7-bit 长度`` 前缀。"""
    for width in range(1, min(5, start) + 1):
        prefix_start = start - width
        encoded = raw[prefix_start:start]
        if any(byte & 0x80 == 0 for byte in encoded[:-1]) or encoded[-1] & 0x80:
            continue
        value = 0
        for index, byte in enumerate(encoded):
            value |= (byte & 0x7F) << (7 * index)
        # BinaryFormatter BinaryObjectString：record type 6 + Int32 object id。
        if (
            value == length
            and prefix_start >= 5
            and raw[prefix_start - 5] == 6
        ):
            return True
    return False


def _exact_key_offsets(raw: bytes, needle: bytes) -> list[int]:
    """返回完整字符串 key 的偏移，拒绝任何字符串内部的同名子串。"""
    pattern = re.compile(re.escape(needle))
    offsets: list[int] = []
    for match in pattern.finditer(raw):
        start = match.start()
        end = match.end()
        # 真实 Save_universe.dat 使用 BinaryFormatter BinaryObjectString；只有
        # 声明的 UTF-8 字节长度恰好等于 key，才能证明命中的是完整字符串。
        binary_formatter_string = _has_binary_formatter_string_prefix(
            raw, start, len(needle)
        )
        # 保留旧测试夹具/简单转储的兼容路径，但边界必须是明确的 NUL（或文件
        # 首尾），不能把标点、Unicode 尾字节等任意非标识符字节当成字符串边界。
        nul_delimited = (start == 0 or raw[start - 1] == 0) and (
            end == len(raw) or raw[end] == 0
        )
        if binary_formatter_string or nul_delimited:
            offsets.append(start)
    return offsets


def _reset_universe_dat(
    save: Path, mod_id: str, read_keys: tuple[str, ...]
) -> int:
    """仅等长改写 .dat 内给定的完整 live key，返回改写次数。"""
    try:
        raw = save.read_bytes()
    except OSError:
        return 0
    offsets: set[int] = set()
    for key in read_keys:
        needle = key.encode("utf-8")
        offsets.update(_exact_key_offsets(raw, needle))
    count = len(offsets)
    if count == 0:
        return 0
    zombie = _choose_zombie_prefix(raw, mod_id)
    updated = bytearray(raw)
    for start in offsets:
        updated[start : start + 4] = zombie[:4]
    try:
        _backup_once(save, raw)
        _atomic_write_bytes(save, bytes(updated))
    except OSError as exc:
        raise GameInstallError(f"写入存档失败：{exc}") from exc
    return count


def _reset_universe_json(
    path: Path, mod_id: str, read_keys: tuple[str, ...]
) -> int:
    """从 Save_universe.json 的 ReadStoryData 中移除该 mod 的已读条目。返回移除条数。"""
    if not path.is_file():
        return 0
    try:
        original = path.read_bytes()
        data = json.loads(original.decode("utf-8-sig"))
    except (OSError, UnicodeError, ValueError, TypeError):
        return 0
    if not isinstance(data, dict):
        return 0
    stories = data.get("ReadStoryData")
    if not isinstance(stories, list):
        return 0
    kept: list = []
    removed = 0
    for item in stories:
        if _is_mod_read_story_key(item, mod_id, read_keys):
            removed += 1
        else:
            kept.append(item)
    if removed == 0:
        return 0
    data["ReadStoryData"] = kept
    try:
        _backup_once(path, original)
        _atomic_write_bytes(
            path, json.dumps(data, ensure_ascii=False).encode("utf-8")
        )
    except OSError as exc:
        raise GameInstallError(f"写入存档失败：{exc}") from exc
    return removed


def reset_story_read_state(
    mod_id: str,
    saves: list[Path] | None = None,
    extra_ids: list[str] | None = None,
    read_keys_by_id: dict[str, list[str]] | None = None,
) -> list[tuple[Path, int]]:
    """把指定 mod 的全部已读文本记录重置为未读（对话不再变黄/可快进）。

    实现：
    - ``Save_universe.dat``（.NET BinaryFormatter）：已读 key 形如
      ``MOD_<modid>_<script>_<node>``。仅把 ``read_keys_by_id`` 给出的完整 live key
      等长改写为文件中尚未出现的 4 字符僵尸前缀（``mod_`` / ``xod_`` / ``yod_`` …）。
      BinaryFormatter 声明长度校验确保只改完整字符串，避免误伤内嵌或更长的 key。
    - 同目录 ``Save_universe.json``（JsonUtility 转储）：从 ``ReadStoryData`` 列表中
       **移除** 与完整 key 匹配的条目（``MOD_`` 及任意僵尸前缀，大小写不敏感）。

    写入前各写一次 ``.lomkit_bak`` 备份（已存在则不覆盖）。

    ``saves`` 可注入 dat 路径列表（测试用）；默认 ``find_universe_saves()``。
    每个 dat 的兄弟 json 会一并处理。

    extra_ids：额外一并清理的 mod id（编辑器 F5 试玩包 ``lom_modkit_preview``）。

    read_keys_by_id：每个 mod id 对应的完整已读 key；这是防止前缀碰撞所必需的。

    注意：必须在游戏关闭后调用——运行中的游戏会在下次存档时把内存里的旧清单写回。
    返回 [(路径, 重置/移除条数)]，含 dat 与 json；无改动时返回空列表。
    """
    ids: list[str] = []
    for candidate in [mod_id, *(extra_ids or [])]:
        if not candidate or candidate in ids:
            continue
        if not re.fullmatch(r"[a-z0-9_\-]+", candidate):
            raise GameInstallError(
                f"Mod 标识 {candidate!r} 不可用（只允许小写英文、数字、_ 和 -）"
            )
        ids.append(candidate)
    if not ids:
        raise GameInstallError("Mod 标识不可用（只允许小写英文、数字、_ 和 -）")
    if read_keys_by_id is None:
        raise GameInstallError("重置已读状态需要当前项目的完整对白 key 清单")
    validated: dict[str, tuple[str, ...]] = {}
    for mid in ids:
        if mid not in read_keys_by_id:
            raise GameInstallError(f"缺少 mod「{mid}」的完整对白 key 清单")
        validated[mid] = _validated_read_story_keys(mid, read_keys_by_id[mid])
    results: list[tuple[Path, int]] = []
    dat_paths = saves if saves is not None else find_universe_saves()
    for mid in ids:
        read_keys = validated[mid]
        if not read_keys:
            continue
        for save in dat_paths:
            dat_count = _reset_universe_dat(save, mid, read_keys)
            if dat_count:
                results.append((save, dat_count))
            json_path = save.with_suffix(".json")
            json_count = _reset_universe_json(json_path, mid, read_keys)
            if json_count:
                results.append((json_path, json_count))
    return results
