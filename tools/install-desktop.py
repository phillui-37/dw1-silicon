#!/usr/bin/env python3
"""Install a Linux application-menu entry for an existing dw1 binary; never opens a terminal."""
import argparse
import os
from pathlib import Path
import shutil
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", nargs="?", default="target/release/dw1")
    args = parser.parse_args()
    if not sys.platform.startswith("linux"):
        parser.error("this installer is for Linux; use tools/make-app.sh on macOS")
    binary = Path(args.binary).resolve(strict=True)
    if not binary.is_file() or not os.access(binary, os.X_OK):
        parser.error(f"not an executable file: {binary}")
    if any(c in str(binary) for c in "\r\n\x00"):
        parser.error("the binary path cannot contain a newline or NUL")

    # Desktop Entry value escaping is applied after Exec argument quoting. Unlike a shell,
    # Exec treats percent as a field code and does not perform shell variable expansion.
    quoted = str(binary).replace("\\", "\\\\").replace('"', '\\"')
    quoted = quoted.replace("`", "\\`").replace("$", "\\$").replace("%", "%%")
    executable = '"' + quoted + '"'
    executable = executable.replace("\\", "\\\\")
    data_home = Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local/share"))
    if not data_home.is_absolute():
        parser.error("XDG_DATA_HOME must be an absolute path")
    root = Path(__file__).resolve().parents[1]
    desktop = data_home / "applications/dw1-silicon.desktop"
    icon = data_home / "icons/hicolor/1024x1024/apps/dw1-silicon.png"
    desktop.parent.mkdir(parents=True, exist_ok=True)
    icon.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(root / "packaging/icon-1024.png", icon)
    template = (root / "packaging/dw1-silicon.desktop.in").read_text(encoding="utf-8")
    # env keeps a percent sign out of the first Exec token: GIO checks that token as a
    # literal filename before it expands %% in arguments. No shell or terminal is used.
    desktop.write_text(template.replace("@EXEC@", executable), encoding="utf-8")
    print(f"Installed {desktop}")
    print(f"Launch Digimon World from the application menu (binary: {binary}).")
    print("The menu entry refers to this binary; reinstall it if you move the binary.")


if __name__ == "__main__":
    main()
