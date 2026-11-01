"""Network-free installer routing checks. Run with Python 3.9+ on macOS/Linux."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class InstallRouting(unittest.TestCase):
    def run_fixture(self, os_name, arch, libc, **extra):
        with tempfile.TemporaryDirectory(prefix="probbit-target-") as folder:
            root = Path(folder)
            scripts = {
                "uname": '#!/bin/sh\ncase "$1" in -s) echo "$FIXTURE_OS";; -m) echo "$FIXTURE_ARCH";; esac\n',
                "ldd": '#!/bin/sh\necho "$FIXTURE_LIBC"\n',
                "sysctl": '#!/bin/sh\necho 0\n',
                "curl": '#!/bin/sh\necho "FIXTURE_FETCH $*" >&2\nexit 42\n',
            }
            for name, body in scripts.items():
                script = root / name
                script.write_text(body)
                script.chmod(0o755)
            env = dict(os.environ, PATH=str(root) + os.pathsep + os.environ["PATH"],
                       FIXTURE_OS=os_name, FIXTURE_ARCH=arch, FIXTURE_LIBC=libc,
                       PROBBIT_VERSION="v0.14.0", PROBBIT_INSTALL_DIR=str(root / "bin"))
            for key in ["PROBBIT_TARGET", "PROBBIT_DOWNLOAD_BASE"]:
                env.pop(key, None)
            env.update(extra)
            result = subprocess.run(["sh", str(ROOT / "install.sh")], env=env, text=True,
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=10)
            self.assertNotEqual(result.returncode, 0)  # the fake transport always declines
            self.assertFalse((root / "bin" / "probbit").exists())
            return result.stdout + result.stderr

    def test_supported_target_selection(self):
        for os_name, arch, libc, target in [
            ("Darwin", "arm64", "", "aarch64-apple-darwin"),
            ("Darwin", "x86_64", "", "x86_64-apple-darwin"),
            ("Linux", "x86_64", "glibc 2.35", "x86_64-unknown-linux-gnu"),
            ("Linux", "x86_64", "musl libc", "x86_64-unknown-linux-musl"),
            # the gnu build needs glibc >= 2.34; an older one gets the static musl build
            ("Linux", "x86_64", "ldd (Ubuntu GLIBC 2.35-0ubuntu3) 2.35", "x86_64-unknown-linux-gnu"),
            ("Linux", "x86_64", "ldd (GNU libc) 2.34", "x86_64-unknown-linux-gnu"),
            ("Linux", "x86_64", "ldd (Ubuntu GLIBC 2.31-0ubuntu9.16) 2.31", "x86_64-unknown-linux-musl"),
            ("Linux", "x86_64", "ldd (GNU libc) 2.28", "x86_64-unknown-linux-musl"),
            ("Linux", "x86_64", "ldd (GNU libc) 2.17", "x86_64-unknown-linux-musl"),
            ("Linux", "aarch64", "glibc 2.35", "aarch64-unknown-linux-musl"),
        ]:
            with self.subTest(target=target):
                output = self.run_fixture(os_name, arch, libc)
                self.assertIn("FIXTURE_FETCH", output)
                self.assertIn("probbit-v0.14.0-" + target + ".tar.gz", output)

    def test_unsupported_architecture_never_fetches(self):
        output = self.run_fixture("Linux", "riscv64", "glibc")
        self.assertIn("no prebuilt", output)
        self.assertNotIn("FIXTURE_FETCH", output)

    def test_invalid_release_and_target_never_fetch(self):
        for values in [{"PROBBIT_VERSION": "../bad"}, {"PROBBIT_TARGET": "../../bad"}]:
            with self.subTest(values=values):
                output = self.run_fixture("Darwin", "arm64", "", **values)
                self.assertIn("invalid", output)
                self.assertNotIn("FIXTURE_FETCH", output)


if __name__ == "__main__":
    unittest.main()
