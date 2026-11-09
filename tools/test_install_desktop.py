#!/usr/bin/env python3
"""Exercise the Linux desktop installer through the desktop launcher, not just its text."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest


@unittest.skipUnless(sys.platform.startswith("linux"), "Linux desktop integration")
class DesktopLaunchTests(unittest.TestCase):
    def test_terminal_false_launches_paths_with_desktop_exec_metacharacters(self):
        if not shutil.which("gio"):
            self.skipTest("gio is required to exercise the Desktop Entry launcher")
        with tempfile.TemporaryDirectory(prefix="dw1-desktop-") as temporary:
            root = Path(temporary)
            binary = root / 'game space "quote" $dollar %f \\backslash'
            marker = root / "launched.json"
            binary.write_text(
                f"#!{sys.executable}\nimport json, os, pathlib, sys\n"
                "pathlib.Path(os.environ['DW1_LAUNCH_MARKER']).write_text(json.dumps(sys.argv[1:]))\n",
                encoding="utf-8",
            )
            binary.chmod(0o755)
            data_home = root / "app data"
            env = {**os.environ, "XDG_DATA_HOME": str(data_home), "DW1_LAUNCH_MARKER": str(marker)}
            installer = Path(__file__).with_name("install-desktop.py")
            subprocess.run([sys.executable, str(installer), str(binary)], env=env, check=True, capture_output=True)
            desktop = data_home / "applications/dw1-silicon.desktop"
            entry = desktop.read_text(encoding="utf-8")
            self.assertIn("Terminal=false\n", entry)
            self.assertIn("[Desktop Action Settings]\n", entry)
            self.assertIn(" --settings\n", entry)
            self.assertTrue((data_home / "icons/hicolor/1024x1024/apps/dw1-silicon.png").is_file())
            run = subprocess.run(["gio", "launch", str(desktop)], env=env, capture_output=True, text=True)
            self.assertEqual(run.returncode, 0, run.stderr)
            deadline = time.monotonic() + 3
            while not marker.exists() and time.monotonic() < deadline:
                time.sleep(0.01)
            self.assertTrue(marker.exists(), "desktop launcher did not run the selected binary")
            self.assertEqual(json.loads(marker.read_text()), [])


if __name__ == "__main__":
    unittest.main()
