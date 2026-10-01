"""User-owned editor data; app bundles and the working directory stay read-only."""
import os
import sys
from pathlib import Path


def user_data_root() -> Path:
    if sys.platform == "darwin" and not os.environ.get("APPDATA"):
        return Path.home() / "Library" / "Application Support" / "lom_modkit"
    appdata = os.environ.get("APPDATA")
    base = Path(appdata) if appdata else Path.home() / "AppData" / "Roaming"
    return base / "lom_modkit"
