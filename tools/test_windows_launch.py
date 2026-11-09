#!/usr/bin/env python3
"""Check the built Windows executable's subsystem and redirected command diagnostics."""
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest


@unittest.skipUnless(sys.platform == "win32", "Windows executable integration")
class WindowsLaunchTests(unittest.TestCase):
    def setUp(self):
        self.binary = Path(os.environ.get("DW1_TEST_BINARY", "target/debug/dw1.exe")).resolve(strict=True)

    def test_windows_executable_uses_gui_subsystem(self):
        image = self.binary.read_bytes()
        pe = struct.unpack_from("<I", image, 0x3C)[0]
        self.assertEqual(image[pe:pe + 4], b"PE\x00\x00")
        subsystem = struct.unpack_from("<H", image, pe + 24 + 68)[0]
        self.assertEqual(subsystem, 2, "GUI launch must not allocate a Windows console")

    def test_card_command_keeps_redirected_error_output(self):
        with tempfile.TemporaryDirectory(prefix="dw1-cli-") as temporary:
            missing = Path(temporary) / "missing-card.mcd"
            result = subprocess.run(
                [str(self.binary), "import-card", str(missing)],
                capture_output=True, text=True, timeout=15,
            )
            self.assertEqual(result.returncode, 1)
            self.assertIn("dw1:", result.stderr)
            self.assertIn("missing-card.mcd", result.stderr)


if __name__ == "__main__":
    unittest.main()
