"""Keep CPU detection data without bundling the vendor's test fixtures."""
from PyInstaller.utils.hooks import collect_all

datas, binaries, hiddenimports = collect_all("archspec", exclude_datas=["json/tests/**"])
