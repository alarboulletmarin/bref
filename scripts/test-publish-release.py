#!/usr/bin/env python3
"""No-network regression tests for the final publication gate."""
import importlib.util
import json
import re
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True

SPEC = importlib.util.spec_from_file_location("release", Path(__file__).with_name("publish-release.py"))
release = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release)


class PublicationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.dist = self.root / "dist"
        self.dist.mkdir()
        self.app = "bref"
        self.calls = []
        self.remote = {}
        self.public = False
        self.upload_error = False
        self.remote_mutation = lambda assets: assets
        self.fixture(self.app)

    def fixture(self, app):
        self.app = app
        (self.root / "Cargo.toml").write_text(f'[package]\nname = "{app}"\nversion = "1.2.3"\n')
        (self.root / "PKGBUILD").write_text("pkgrel=1\n")
        for path in self.dist.iterdir():
            path.unlink()
        for name in release.expected_assets(self.root, "v1.2.3"):
            (self.dist / name).write_bytes(f"fixture: {name}".encode())

    def gh(self, *args):
        self.calls.append(args)
        if args[:2] == ("release", "view"):
            assets = [{"name": key, "size": size} for key, size in self.remote.items()]
            return subprocess.CompletedProcess(args, 0, json.dumps({"isDraft": not self.public, "assets": assets}))
        if args[:2] == ("release", "upload"):
            if self.upload_error:
                raise subprocess.CalledProcessError(1, args)
            self.remote = self.remote_mutation({Path(p).name: Path(p).stat().st_size for p in args[3:-1]})
        if args[:2] == ("release", "edit"):
            self.public = True
        return subprocess.CompletedProcess(args, 0, "")

    def publish(self):
        with patch.object(release, "gh", self.gh):
            release.publish(self.root, "v1.2.3", self.dist)

    def assert_unpublished(self):
        self.assertFalse(self.public)
        self.assertFalse(any(args[:2] == ("release", "edit") for args in self.calls))

    def test_all_native_profiles_publish_only_after_upload_and_inventory_check(self):
        for app, count in [("bref", 6), ("astrolabe", 6), ("spot", 3)]:
            with self.subTest(app=app):
                self.fixture(app)
                self.calls = []
                self.public = False
                self.publish()
                self.assertTrue(self.public)
                self.assertEqual(len(self.remote), count)
                self.assertEqual([a[1] for a in self.calls], ["view", "upload", "view", "edit"])
                self.assertIn("--verify-tag", self.calls[-1])
                self.assertIn("SHA256SUMS", self.remote)

    def test_missing_platform_download_never_contacts_github(self):
        (self.dist / "bref-macos.dmg").unlink()
        with self.assertRaises(ValueError):
            self.publish()
        self.assertEqual(self.calls, [])

    def test_empty_download_never_contacts_github(self):
        (self.dist / "bref-macos.dmg").write_bytes(b"")
        with self.assertRaises(ValueError):
            self.publish()
        self.assertEqual(self.calls, [])

    def test_unexpected_file_rejected(self):
        (self.dist / "another-project.exe").write_text("wrong project")
        with self.assertRaises(ValueError):
            self.publish()
        self.assertEqual(self.calls, [])

    def test_symlink_rejected(self):
        path = self.dist / "bref-macos.dmg"
        path.unlink()
        path.symlink_to(self.root / "Cargo.toml")
        with self.assertRaises(ValueError):
            self.publish()
        self.assertEqual(self.calls, [])

    def test_public_release_cannot_be_overwritten(self):
        self.public = True
        with self.assertRaises(ValueError):
            self.publish()
        self.assertEqual([a[1] for a in self.calls], ["view"])

    def test_failed_upload_keeps_draft_private(self):
        self.upload_error = True
        with self.assertRaises(subprocess.CalledProcessError):
            self.publish()
        self.assert_unpublished()

    def test_incomplete_or_changed_remote_inventory_keeps_draft_private(self):
        for change in [lambda a: {k: v for k, v in a.items() if not k.endswith(".dmg")},
                       lambda a: {**a, "bref-macos.dmg": 0},
                       lambda a: {**a, "obsolete.exe": 123}]:
            with self.subTest(change=change):
                self.calls = []
                self.remote_mutation = change
                with self.assertRaises(ValueError):
                    self.publish()
                self.assert_unpublished()

    def test_partial_upload_can_be_retried(self):
        self.remote = {"bref-macos.dmg": 1}
        self.publish()
        self.assertTrue(self.public)
        self.assertGreater(self.remote["bref-macos.dmg"], 1)

    def test_malformed_tag_and_version_mismatch_rejected(self):
        for tag in ["main", "v1.2.3; echo unsafe", "v1.2.4", "1.2.3"]:
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                release.expected_assets(self.root, tag)



class WorkflowContractTests(unittest.TestCase):
    """Check critical graph/permission invariants; this is not a YAML parser."""
    def setUp(self):
        self.root = Path(__file__).resolve().parent.parent
        self.app = self.root.name
        self.workflows = self.root / ".github/workflows"
        self.source = (self.workflows / "release-native.yml").read_text()

    def job(self, name):
        match = re.search(r"^  " + name + r":\n(.*?)(?=^  [a-z]+:|\Z)", self.source, re.M | re.S)
        self.assertIsNotNone(match)
        return match[1]

    def test_draft_visibility_and_publication_have_push_access(self):
        for name in ["validate", "publish"]:
            self.assertRegex(self.job(name), r"permissions:\n      contents: write")
        for name in ["arch-package.yml"] + ([] if self.app == "spot" else ["release-binaries.yml"]):
            source = (self.workflows / name).read_text()
            self.assertNotIn("contents: write", source)

    def test_publication_waits_for_every_platform_without_success_override(self):
        expected = "[validate, arch]" if self.app == "spot" else "[validate, arch, binaries]"
        publication = self.job("publish")
        self.assertIn("needs: " + expected, publication)
        self.assertNotIn("if:", publication)
        self.assertIn("pattern: native-*", publication)

    def test_packaging_cannot_publish_and_supports_artifact_retry(self):
        for name in ["arch-package.yml"] + ([] if self.app == "spot" else ["release-binaries.yml"]):
            source = (self.workflows / name).read_text()
            self.assertNotRegex(source, r"(?m)^  release:")
            self.assertNotIn("gh release", source)
            self.assertEqual(source.count("uses: actions/upload-artifact@"), source.count("overwrite: true"))


if __name__ == "__main__":
    unittest.main()
