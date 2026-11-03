#!/usr/bin/env python3
"""Exercise release preparation in temporary repositories with mocked external tools.

Only Git against a temporary local bare repository is real. No network, application
build, real tag, release, or workflow dispatch is performed.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("release.sh")
APP = SCRIPT.parent.parent.name
if APP not in {"bref", "spot", "astrolabe"}:
    raise RuntimeError("run this test from one of the native application repositories")
GIT = shutil.which("git")
MOCK = r"""#!PYTHON
import json, os, pathlib, sys
name=pathlib.Path(sys.argv[0]).name
args=sys.argv[1:]
state=pathlib.Path(os.environ['MOCK_STATE'])
with (state/'calls').open('a') as f: f.write(json.dumps([name,*args])+'\n')
if name=='cargo':
    if args[0]=='update':
        pathlib.Path('Cargo.lock').write_text('updated lock\n')
    if args[0]=='test' and (state/'fail-tests').exists(): sys.exit(23)
elif name=='gh':
    if args[0]=='api':
        if (state/'fail-api').exists(): sys.exit(27)
        if (state/'release').exists(): print((state/'release').read_text().strip())
    elif args[:2]==['release','create']:
        assert '--draft' in args and '--verify-tag' in args
        (state/'release').write_text('true')
        notes=pathlib.Path(args[args.index('--notes-file')+1]).read_text()
        (state/'notes').write_text(notes)
    elif args[:2]==['workflow','run']:
        assert 'release-native.yml' in args and '--ref' in args and 'main' in args
        if (state/'fail-dispatch').exists(): sys.exit(29)
    else: raise RuntimeError('unexpected gh call: '+repr(args))
elif name=='curl':
    if (state/'fail-curl').exists(): sys.exit(22)
    pathlib.Path(args[args.index('-o')+1]).write_bytes(b'tagged source archive')
elif name=='makepkg':
    if (state/'fail-makepkg').exists(): sys.exit(31)
    print(pathlib.Path('PKGBUILD').read_text())
elif name=='sleep': pass
else: raise RuntimeError(name)
"""


class ReleaseTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix=f"{APP}-release-test-")
        self.base = Path(self.temp.name)
        self.repo = self.base / APP
        self.repo.mkdir()
        self.state = self.base / "state"
        self.state.mkdir()
        self.bin = self.base / "bin"
        self.bin.mkdir()
        self.env = os.environ.copy()
        self.env.update(HOME=str(self.base), GIT_CONFIG_GLOBAL=os.devnull,
                        GIT_CONFIG_SYSTEM=os.devnull, GIT_TERMINAL_PROMPT="0",
                        MOCK_STATE=str(self.state), PATH=f"{self.bin}:{self.env['PATH']}")
        self.env.pop("DRY_RUN", None)
        self.env.pop("RESUME", None)
        for name in ["cargo", "gh", "curl", "makepkg", "sleep"]:
            tool = self.bin / name
            tool.write_text(MOCK.replace("#!PYTHON", f"#!{sys.executable}", 1))
            tool.chmod(0o755)
        self.git("init", "-q", "-b", "main")
        self.git("config", "user.name", "Release Test")
        self.git("config", "user.email", "release-test@example.invalid")
        self.git("config", "commit.gpgsign", "false")
        (self.repo / "scripts").mkdir()
        shutil.copy2(SCRIPT, self.repo / "scripts/release.sh")
        (self.repo / "Cargo.toml").write_text(f'[package]\nname = "{APP}"\nversion = "0.1.0"\n')
        (self.repo / "Cargo.lock").write_text("original lock\n")
        (self.repo / "PKGBUILD").write_text("pkgver=0.1.0\npkgrel=3\nsha256sums=('SKIP')\n")
        (self.repo / ".SRCINFO").write_text("original source metadata\n")
        (self.repo / "CHANGELOG.md").write_text("# Changelog\n\n## [0.2.0]\n\n- User-facing change.\n")
        if APP == "spot":
            (self.repo / "aur/spot").mkdir(parents=True)
            shutil.copy2(self.repo / "PKGBUILD", self.repo / "aur/spot/PKGBUILD")
            shutil.copy2(self.repo / ".SRCINFO", self.repo / "aur/spot/.SRCINFO")
        if APP == "bref":
            (self.repo / "aur/bref-git").mkdir(parents=True)
            (self.repo / "aur/bref-git/PKGBUILD").write_text("pkgver=generated\n")
        self.git("add", ".")
        self.git("commit", "-qm", "feat: initial feature")
        self.original_head = self.git("rev-parse", "HEAD")
        self.remote = self.base / "remote.git"
        subprocess.run([GIT, "init", "-q", "--bare", str(self.remote)], env=self.env, check=True)
        self.git("remote", "add", "origin", str(self.remote))
        self.git("push", "-q", "origin", "main")
        self.original_files = {p.relative_to(self.repo): p.read_bytes()
                               for p in self.repo.rglob("*") if p.is_file() and ".git" not in p.parts}

    def tearDown(self):
        self.temp.cleanup()

    def git(self, *args):
        return subprocess.run([GIT, *args], cwd=self.repo, env=self.env, check=True,
                              capture_output=True, text=True).stdout.strip()

    def run_release(self, *args, dry=False, resume=False, success=True):
        env = dict(self.env, DRY_RUN="1" if dry else "0", RESUME="1" if resume else "0")
        result = subprocess.run(["bash", "scripts/release.sh", *(args or ["0.2.0"])],
                                cwd=self.repo, env=env, capture_output=True, text=True)
        self.assertEqual(result.returncode == 0, success, result.stdout + result.stderr)
        return result

    def calls(self):
        log = self.state / "calls"
        return [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []

    def assert_original(self):
        self.assertEqual(self.git("rev-parse", "HEAD"), self.original_head)
        self.assertEqual(self.git("status", "--porcelain"), "")
        for path, body in self.original_files.items():
            self.assertEqual((self.repo / path).read_bytes(), body, str(path))

    def test_dry_run_restores_successfully_without_network(self):
        self.run_release(dry=True)
        self.assert_original()
        self.assertEqual({call[0] for call in self.calls()}, {"cargo"})
        self.assertIn(["cargo", "test", "--locked", "--offline", "-q"], self.calls())
        self.assertEqual(self.git("tag"), "")

    def test_dry_run_restores_when_tests_fail(self):
        (self.state / "fail-tests").touch()
        self.run_release(dry=True, success=False)
        self.assert_original()
        self.assertEqual({call[0] for call in self.calls()}, {"cargo"})

    def test_normal_test_failure_restores_versions(self):
        (self.state / "fail-tests").touch()
        self.run_release(success=False)
        self.assert_original()
        self.assertFalse(any(call[1:3] == ["release", "create"] for call in self.calls()))

    def test_first_release_prepares_draft_and_dispatches(self):
        self.run_release()
        self.assertEqual(self.git("tag"), "v0.2.0")
        self.assertEqual(self.git("status", "--porcelain"), "")
        self.assertEqual((self.state / "release").read_text(), "true")
        self.assertIn("User-facing change", (self.state / "notes").read_text())
        self.assertEqual(sum(call[1:3] == ["release", "create"] for call in self.calls()), 1)
        self.assertEqual(sum(call[1:3] == ["workflow", "run"] for call in self.calls()), 1)
        self.assertIn("tag=v0.2.0", self.calls()[-1])
        self.assertFalse(any(call[1:3] == ["release", "edit"] for call in self.calls()))
        self.assertIn("pkgrel=1", (self.repo / "PKGBUILD").read_text())
        if APP == "spot":
            self.assertEqual((self.repo / "PKGBUILD").read_bytes(), (self.repo / "aur/spot/PKGBUILD").read_bytes())
            self.assertEqual((self.repo / ".SRCINFO").read_bytes(), (self.repo / "aur/spot/.SRCINFO").read_bytes())

    def test_first_release_without_changelog_includes_root_commit(self):
        (self.repo / "CHANGELOG.md").unlink()
        self.git("add", "CHANGELOG.md")
        self.git("commit", "-qm", "docs: remove changelog")
        self.git("push", "-q", "origin", "main")
        self.run_release()
        self.assertIn("initial feature", (self.state / "notes").read_text())

    def test_resume_dispatch_failure_preserves_draft_and_tag(self):
        (self.state / "fail-dispatch").touch()
        self.run_release(success=False)
        tag = self.git("rev-parse", "v0.2.0")
        head = self.git("rev-parse", "HEAD")
        (self.state / "fail-dispatch").unlink()
        self.run_release(resume=True)
        self.assertEqual(self.git("rev-parse", "v0.2.0"), tag)
        self.assertEqual(self.git("rev-parse", "HEAD"), head)
        self.assertEqual(sum(call[1:3] == ["release", "create"] for call in self.calls()), 1)
        self.assertEqual(sum(call[1:3] == ["workflow", "run"] for call in self.calls()), 2)

    def test_checksum_failure_is_clean_and_resumable(self):
        (self.state / "fail-makepkg").touch()
        self.run_release(success=False)
        self.assertEqual(self.git("status", "--porcelain"), "")
        self.assertFalse((self.state / "release").exists())
        (self.state / "fail-makepkg").unlink()
        self.run_release(resume=True)
        self.assertEqual((self.state / "release").read_text(), "true")

    def test_public_release_is_refused_before_local_mutation(self):
        (self.state / "release").write_text("false")
        self.run_release(success=False)
        self.assert_original()
        self.assertEqual({call[0] for call in self.calls()}, {"gh"})

    def test_public_release_is_refused_on_resume(self):
        self.run_release()
        head = self.git("rev-parse", "HEAD")
        tag = self.git("rev-parse", "v0.2.0")
        (self.state / "release").write_text("false")
        before = len(self.calls())
        self.run_release(resume=True, success=False)
        self.assertEqual(self.git("rev-parse", "HEAD"), head)
        self.assertEqual(self.git("rev-parse", "v0.2.0"), tag)
        self.assertEqual(self.git("status", "--porcelain"), "")
        self.assertEqual(len(self.calls()), before + 1)

    def test_remote_existing_tag_is_refused_before_version_mutation(self):
        self.git("tag", "v0.2.0")
        self.git("push", "-q", "origin", "v0.2.0")
        self.git("tag", "-d", "v0.2.0")
        self.run_release(success=False)
        self.assert_original()
        self.assertEqual({call[0] for call in self.calls()}, {"gh"})

    def test_curl_failure_does_not_hash_an_error_page(self):
        (self.state / "fail-curl").touch()
        self.run_release(success=False)
        self.assertEqual(self.git("status", "--porcelain"), "")
        self.assertFalse((self.state / "release").exists())
        self.assertEqual(sum(call[0] == "curl" for call in self.calls()), 5)
        (self.state / "fail-curl").unlink()
        self.run_release(resume=True)
        self.assertEqual((self.state / "release").read_text(), "true")

    def test_api_failure_is_refused_before_local_mutation(self):
        (self.state / "fail-api").touch()
        self.run_release(success=False)
        self.assert_original()
        self.assertEqual({call[0] for call in self.calls()}, {"gh"})

    def test_invalid_version_has_no_effect(self):
        self.run_release("v0.2.0;echo bad", success=False)
        self.assert_original()
        self.assertEqual(self.calls(), [])

    def test_existing_tag_requires_explicit_resume(self):
        self.run_release()
        head = self.git("rev-parse", "HEAD")
        before = len(self.calls())
        self.run_release(success=False)
        self.assertEqual(self.git("rev-parse", "HEAD"), head)
        self.assertEqual(len(self.calls()), before)

    @unittest.skipUnless(APP == "bref", "Bref-specific generated recipe")
    def test_bref_preserves_generated_git_recipe(self):
        recipe = self.repo / "aur/bref-git/PKGBUILD"
        recipe.write_text("pkgver=user-generated\n")
        self.run_release(dry=True)
        self.assertEqual(recipe.read_text(), "pkgver=user-generated\n")
        self.assertEqual(self.git("status", "--porcelain"), "M aur/bref-git/PKGBUILD")


if __name__ == "__main__":
    unittest.main()
