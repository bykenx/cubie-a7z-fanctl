"""Exercise the overlay installer with real DT tools and a temporary /boot."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


PROJECT = Path(__file__).resolve().parents[1]
NAME = "cubie-a7z-pwm-fan"
BASE_DTS = """/dts-v1/;
/ {
    model = "Radxa Cubie A7Z";
    compatible = "allwinner,a733";
    soc: soc {
        pwm1: pwm { #pwm-cells = <3>; status = "disabled"; };
        pwm1_9: channel { status = "disabled"; };
        pinctrl {
            pwm1_9_pins_active: active { pins = "PK5"; };
            pwm1_9_pins_sleep: sleep { pins = "PK5"; };
        };
        reg_dc1sw1: regulator { regulator-name = "fan"; };
    };
};
"""
MOCK_TOOL = """#!/usr/bin/env python3
import os
from pathlib import Path
import shutil
import sys

boot = Path(os.environ["TEST_BOOT"])
with (boot / "tool-calls").open("a") as calls:
    calls.write("install\\n")
mode = os.environ.get("TEST_MODE", "success")
if mode == "false_success":
    sys.exit(0)
name = Path(sys.argv[1]).stem
destination = boot / "overlay-user" / (name + ".dtbo")
destination.parent.mkdir(exist_ok=True)
shutil.copyfile(Path(sys.argv[1]).with_suffix(".dtbo"), destination)
env = boot / "armbianEnv.txt"
lines = env.read_text().splitlines()
for i, line in enumerate(lines):
    if line.startswith("user_overlays="):
        tokens = line.split("=", 1)[1].split()
        if name not in tokens:
            tokens.append(name)
        lines[i] = "user_overlays=" + " ".join(tokens)
        break
else:
    lines.append("user_overlays=" + name)
if mode == "corrupt_config":
    lines.append("unexpected=changed")
if mode == "corrupt_overlays":
    lines = [("user_overlays=" + name) if line.startswith("user_overlays=")
             else line for line in lines]
env.write_text("\\n".join(lines) + "\\n")
sys.exit(7 if mode == "failure" else 0)
"""


@unittest.skipUnless(all(shutil.which(tool) for tool in ("dtc", "fdtoverlay", "fdtget")),
                     "device-tree-compiler tools are required")
class ArmbianInstallerTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="fanctl-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.boot = self.root / "boot"
        self.env = self.boot / "armbianEnv.txt"
        self.destination = self.boot / "overlay-user" / (NAME + ".dtbo")
        self.dtb = self.boot / "dtb/allwinner/sun60i-a733-cubie-a7z.dtb"
        self.dtb.parent.mkdir(parents=True)
        (self.root / "etc").mkdir()
        self.release = self.root / "etc/armbian-release"
        self.release.write_text("BOARD=cubie-a7z\nLINUXFAMILY=sun60iw2\n")
        model = self.root / "sys/firmware/devicetree/base/model"
        model.parent.mkdir(parents=True)
        model.write_bytes(b"Radxa Cubie A7Z\0")
        self.env.write_text("verbosity=1\nfdtfile=allwinner/sun60i-a733-cubie-a7z.dtb\n"
                            "user_overlays=uart spi\n# keep this comment\n")
        (self.boot / "boot.cmd").write_text("setenv overlay_error false\n")
        self.source = self.root / "source with spaces"
        self.source.mkdir()
        shutil.copyfile(PROJECT / "contrib/armbian" / (NAME + ".dts"),
                        self.source / (NAME + ".dts"))
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.executable(self.bin / "uname", "#!/bin/sh\ncase $1 in\n"
                        "-s) echo Linux;;\n-r) echo \"$TEST_KERNEL\";;\nesac\n")
        self.executable(self.bin / "sudo", "#!/bin/sh\n[ \"$1\" != -- ] || shift\nexec \"$@\"\n")
        self.tool = self.bin / "armbian-add-overlay"
        self.executable(self.tool, MOCK_TOOL)
        self.process_env = dict(os.environ, PATH=str(self.bin) + os.pathsep + os.environ["PATH"],
                                TEST_BOOT=str(self.boot), TEST_KERNEL="6.6.98-vendor-sun60iw2")
        self.compile_base()

    @staticmethod
    def executable(path, content):
        path.write_text(content)
        path.chmod(0o755)

    def compile_base(self, source=BASE_DTS):
        subprocess.run(["dtc", "-@", "-I", "dts", "-O", "dtb", "-o", str(self.dtb)],
                       input=source, text=True, check=True, capture_output=True)

    def run_installer(self, mode="success"):
        return subprocess.run(["sh", "-c", '. "$1"; _armbian_install "$2" "$3" "$4"', "sh",
                               str(PROJECT / "contrib/armbian/install.sh"), str(self.source),
                               str(self.root), str(self.tool)],
                              env=dict(self.process_env, TEST_MODE=mode),
                              text=True, capture_output=True, timeout=20)

    def backups(self):
        return sorted(self.boot.glob("cubie-a7z-fanctl-backup.*"))

    def native_fan(self):
        hwmon = self.root / "sys/class/hwmon/hwmon3"
        hwmon.mkdir(parents=True)
        (hwmon / "name").write_text("pwmfan\n")
        (hwmon / "pwm1").write_text("60\n")

    def assert_rejected_before_install(self):
        original = self.env.read_bytes()
        result = self.run_installer()
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.env.read_bytes(), original)
        self.assertEqual(self.backups(), [])
        self.assertFalse((self.boot / "tool-calls").exists())

    def test_wrong_board(self):
        self.release.write_text("BOARD=another-board\nLINUXFAMILY=sun60iw2\n")
        self.assert_rejected_before_install()

    def test_wrong_running_model(self):
        (self.root / "sys/firmware/devicetree/base/model").write_bytes(b"Other board\0")
        self.assert_rejected_before_install()

    def test_wrong_kernel(self):
        self.process_env["TEST_KERNEL"] = "6.12.0-current-sunxi64"
        self.assert_rejected_before_install()

    def test_wrong_dtb(self):
        self.env.write_text("fdtfile=another.dtb\n")
        self.assert_rejected_before_install()

    def test_wrong_dtb_model(self):
        self.compile_base(BASE_DTS.replace("Radxa Cubie A7Z", "Other board"))
        self.assert_rejected_before_install()

    def test_duplicate_overlay_entries(self):
        with self.env.open("a") as env:
            env.write("user_overlays=another\n")
        self.assert_rejected_before_install()

    def test_compile_failure(self):
        (self.source / (NAME + ".dts")).write_text("invalid DTS")
        self.assert_rejected_before_install()

    def test_overlay_does_not_apply(self):
        self.compile_base(BASE_DTS.replace("pwm1_9_pins_active:", ""))
        self.assert_rejected_before_install()

    def test_symlink_destination_is_rejected(self):
        self.destination.parent.mkdir()
        target = self.root / "unrelated-file"
        target.write_text("untouched\n")
        self.destination.symlink_to(target)
        self.assert_rejected_before_install()
        self.assertEqual(target.read_text(), "untouched\n")

    def test_native_fan_skips_overlay(self):
        self.native_fan()
        result = self.run_installer()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("no overlay is needed", result.stdout)
        self.assertEqual(self.backups(), [])
        self.assertFalse((self.boot / "tool-calls").exists())

    def test_install_and_repeat(self):
        original = self.env.read_bytes()
        base = self.dtb.read_bytes()
        result = self.run_installer()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.backups()[0] / "armbianEnv.txt").read_bytes(), original)
        self.assertEqual(self.dtb.read_bytes(), base)
        self.assertIn("user_overlays=uart spi " + NAME, self.env.read_text())
        self.assertIn("# keep this comment", self.env.read_text())
        merged = self.root / "merged.dtb"
        subprocess.run(["fdtoverlay", "-i", str(self.dtb), "-o", str(merged),
                        str(self.destination)], check=True, capture_output=True)
        pins = subprocess.check_output(["fdtget", "-t", "s", str(merged),
                                        "/soc/pinctrl/active", "pins"], text=True)
        self.assertEqual(pins.strip(), "PJ27")
        pwm = subprocess.check_output(["fdtget", "-t", "u", str(merged),
                                       "/soc/pwm-fan", "pwms"], text=True)
        self.assertEqual([int(value) for value in pwm.split()][1:], [9, 25000, 0])
        for active in (False, True):
            if active:
                self.native_fan()
            result = self.run_installer()
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("already active" if active else "already installed", result.stdout)
        self.assertEqual(len(self.backups()), 1)
        self.assertEqual((self.boot / "tool-calls").read_text(), "install\n")

    def test_missing_user_overlays_is_added(self):
        self.env.write_text("fdtfile=allwinner/sun60i-a733-cubie-a7z.dtb\n")
        result = self.run_installer()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("user_overlays=" + NAME, self.env.read_text())

    def test_failed_and_incomplete_installs_restore_configuration(self):
        original = self.env.read_bytes()
        for mode in ("failure", "false_success", "corrupt_config", "corrupt_overlays"):
            with self.subTest(mode=mode):
                result = self.run_installer(mode)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("Restoring boot configuration", result.stderr)
                self.assertEqual(self.env.read_bytes(), original)
                self.assertFalse(self.destination.exists())

    def test_failed_update_restores_existing_overlay(self):
        with self.env.open("a") as env:
            env.write("# existing overlay is updated\n")
        self.env.write_text(self.env.read_text().replace("uart spi", "uart " + NAME + " spi"))
        original = self.env.read_bytes()
        self.destination.parent.mkdir()
        self.destination.write_bytes(b"previous overlay")
        result = self.run_installer("failure")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.env.read_bytes(), original)
        self.assertEqual(self.destination.read_bytes(), b"previous overlay")
        self.assertEqual((self.backups()[0] / (NAME + ".dtbo")).read_bytes(), b"previous overlay")
