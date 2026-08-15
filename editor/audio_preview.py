# -*- coding: utf-8 -*-
"""编辑器内试听用户音频。只读本机文件，不走游戏播放器或第二套 VoiceStore。"""

from __future__ import annotations

import os
import subprocess
import sys
import threading
from pathlib import Path

AUDIO_PREVIEW_EXTENSIONS = (".ogg", ".wav")


class AudioPreviewError(Exception):
    """试听失败（文件缺失、格式不支持、系统拒绝播放）。"""


_lock = threading.Lock()
_winsound_playing = False
_player_proc: subprocess.Popen | None = None


def stop_audio() -> None:
    """停掉当前试听。"""
    global _winsound_playing, _player_proc
    with _lock:
        if _winsound_playing:
            try:
                import winsound

                winsound.PlaySound(None, winsound.SND_PURGE)
            except Exception:
                pass
            _winsound_playing = False
        if _player_proc is not None:
            try:
                _player_proc.terminate()
            except Exception:
                pass
            _player_proc = None


def play_audio_file(path: Path) -> str:
    """试听一条本地音频。返回所用后端：winsound / system。"""
    src = Path(path)
    if not src.is_file():
        raise AudioPreviewError("找不到音频文件。")
    suffix = src.suffix.lower()
    if suffix not in AUDIO_PREVIEW_EXTENSIONS:
        raise AudioPreviewError("试听只支持 OGG / WAV。")
    stop_audio()
    if suffix == ".wav" and os.name == "nt":
        try:
            import winsound

            winsound.PlaySound(
                str(src),
                winsound.SND_FILENAME | winsound.SND_ASYNC,
            )
            global _winsound_playing
            _winsound_playing = True
            return "winsound"
        except Exception:
            pass
    return _play_with_system(src)


def _play_with_system(src: Path) -> str:
    """用系统默认播放器打开（OGG，或 winsound 不可用时）。"""
    global _player_proc
    try:
        if os.name == "nt":
            os.startfile(str(src))  # type: ignore[attr-defined]
            return "system"
        opener = "open" if sys.platform == "darwin" else "xdg-open"
        _player_proc = subprocess.Popen(
            [opener, str(src)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        return "system"
    except OSError as exc:
        raise AudioPreviewError("无法试听：%s" % exc) from exc
