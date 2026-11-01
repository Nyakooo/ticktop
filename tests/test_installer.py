"""Exercise the real installer with local release fixtures and a fake curl transport."""
import hashlib
import io
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="ticktop-installer-")
        self.root = Path(self.tmp.name)
        self.home = self.root / "user home"
        self.home.mkdir()
        self.bin = self.root / "tools"
        self.bin.mkdir()
        self.fixture = self.root / "release"
        self.fixture.mkdir()
        self.asset = "ticktop-x86_64-unknown-linux-musl.tar.gz"
        binary = b'#!/usr/bin/env bash\nprintf "ticktop 0.1.0\\n"\n'
        with tarfile.open(self.fixture / self.asset, "w:gz") as archive:
            info = tarfile.TarInfo("ticktop")
            info.size = len(binary)
            info.mode = 0o755
            archive.addfile(info, io.BytesIO(binary))
        digest = hashlib.sha256((self.fixture / self.asset).read_bytes()).hexdigest()
        (self.fixture / (self.asset + ".sha256")).write_text(f"{digest}  {self.asset}\n")
        self.tool("curl", '''#!/usr/bin/env bash
set -eu
url='' out=''
while [ "$#" -gt 0 ]; do
 case "$1" in
  https://*) url="$1"; shift ;;
  -o) out="$2"; shift 2 ;;
  *) shift ;;
 esac
done
if [ -n "$out" ]; then cp "$FIXTURE/${url##*/}" "$out"; else cat "$FIXTURE/${url##*/}"; fi
''')
        self.tool("uname", '#!/bin/sh\ncase "$1" in -s) echo Linux;; -m) echo x86_64;; esac\n')
        self.env = {**os.environ, "HOME": str(self.home), "PATH": f"{self.bin}:{os.environ['PATH']}", "SHELL": "/usr/bin/zsh", "FIXTURE": str(self.fixture)}
        self.env.pop("ZDOTDIR", None)
        self.env.pop("TICKTOP_INSTALL_DIR", None)

    def tearDown(self):
        self.tmp.cleanup()

    def tool(self, name, content):
        path = self.bin / name
        path.write_text(content)
        path.chmod(0o755)

    def install(self, *args, success=True):
        result = subprocess.run(["bash", str(ROOT / "scripts/install.sh"), *args], env=self.env, text=True, capture_output=True)
        self.assertEqual(result.returncode == 0, success, result.stdout + result.stderr)
        return result

    def test_piped_bash_uses_user_zsh_and_is_idempotent(self):
        rc = self.home / ".zshrc"
        rc.write_text("# existing oh-my-zsh config\n")
        self.install()
        self.install()
        text = rc.read_text()
        self.assertTrue(text.startswith("# existing oh-my-zsh config\n"))
        self.assertEqual(text.count("# >>> ticktop >>>"), 1)
        self.assertFalse((self.home / ".bashrc").exists())
        self.assertTrue(os.access(self.home / ".local/bin/ticktop", os.X_OK))
        subprocess.run(["zsh", "-n", str(rc)], check=True)
        out = subprocess.check_output(["bash", "-c", 'source "$HOME/.zshrc"; source "$HOME/.zshrc"; printf "%s" "$PATH"'], env=self.env, text=True)
        self.assertEqual(out.split(":").count(str(self.home / ".local/bin")), 1)

    def test_explicit_bash_override(self):
        self.install("--shell", "bash")
        self.assertTrue((self.home / ".bashrc").exists())
        self.assertFalse((self.home / ".zshrc").exists())

    def test_honors_zdotdir(self):
        self.env["ZDOTDIR"] = str(self.root / "zsh config")
        self.install()
        self.assertTrue((Path(self.env["ZDOTDIR"]) / ".zshrc").exists())
        self.assertFalse((self.home / ".zshrc").exists())

    def test_checksum_failure_keeps_existing_binary_and_rc(self):
        self.install()
        binary = self.home / ".local/bin/ticktop"
        before = binary.read_bytes()
        rc = (self.home / ".zshrc").read_bytes()
        (self.fixture / (self.asset + ".sha256")).write_text(f"{'0' * 64}  {self.asset}\n")
        self.install(success=False)
        self.assertEqual(binary.read_bytes(), before)
        self.assertEqual((self.home / ".zshrc").read_bytes(), rc)

    def test_no_path_does_not_touch_shell_config(self):
        self.install("--no-path")
        self.assertFalse((self.home / ".zshrc").exists())

    def test_shell_special_characters_in_directory_are_quoted(self):
        self.env["TICKTOP_INSTALL_DIR"] = str(self.home / "bin'$(touch NEVER_EXECUTE)")
        self.install()
        subprocess.run(["bash", "-c", 'source "$HOME/.zshrc"'], env=self.env, cwd=self.root, check=True)
        self.assertFalse((self.root / "NEVER_EXECUTE").exists())

    def test_rejects_unsupported_shell_and_version(self):
        self.install("--shell", "fish", success=False)
        self.install("--version", "../../oops", success=False)

    def test_git_bash_dispatches_to_native_powershell_installer(self):
        self.tool("uname", '#!/bin/sh\necho MINGW64_NT-10.0\n')
        (self.fixture / "install.ps1").write_text("# native installer fixture\n")
        self.tool("powershell.exe", '''#!/usr/bin/env bash
set -eu
cat > "$FIXTURE/powershell-input"
printf '%s/%s' "$TICKTOP_VERSION" "$TICKTOP_NO_PATH" > "$FIXTURE/windows-options"
''')
        self.install("--version", "v0.3.0", "--no-path")
        self.assertEqual((self.fixture / "powershell-input").read_text(), "# native installer fixture\n")
        self.assertEqual((self.fixture / "windows-options").read_text(), "v0.3.0/1")
        self.assertFalse((self.home / ".zshrc").exists())
        self.assertFalse((self.home / ".local/bin/ticktop").exists())

    def test_rejects_archive_links(self):
        with tarfile.open(self.fixture / self.asset, "w:gz") as archive:
            info = tarfile.TarInfo("ticktop")
            info.type = tarfile.SYMTYPE
            info.linkname = "/bin/sh"
            archive.addfile(info)
        digest = hashlib.sha256((self.fixture / self.asset).read_bytes()).hexdigest()
        (self.fixture / (self.asset + ".sha256")).write_text(f"{digest}  {self.asset}\n")
        self.install(success=False)
        self.assertFalse((self.home / ".local/bin/ticktop").exists())


if __name__ == "__main__":
    unittest.main()
