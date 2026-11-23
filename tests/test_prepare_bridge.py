"""sourceとbinaryの取り違えを、serialなしの準備段階で拒否する。"""

import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("prepare_bridge", ROOT / "tools/prepare_bridge.py")
bridge = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(bridge)
REVISION = "1234567890abcdef1234567890abcdef12345678"


def status(request_id):
    return {
        "schema": bridge.EVENT_SCHEMA, "event": "status", "bridge_revision": REVISION,
        "host_clock": "bridge_process_elapsed_us", "host_emitted_us": request_id,
        "request_id": request_id, "phase": "unconfigured", "ready": False,
        "session_neutral_valid": False, "source": None, "profile": None,
        "operator": None, "diagnostics": None,
    }


class BridgeStatusTest(unittest.TestCase):
    def test_disconnected_status_requires_exact_revision_and_no_sample(self):
        output = "\n".join(json.dumps(status(request)) for request in range(2)) + "\n"
        self.assertEqual(bridge.validate_smoke(output, REVISION), {
            "status_events": 2, "sample_events": 0, "ready": False, "exit_code": 0,
        })
        with self.assertRaises(ValueError):
            bridge.validate_smoke(output, "a" * 40)
        with self.assertRaises(ValueError):
            bridge.validate_smoke(output + json.dumps({"event": "sample"}) + "\n", REVISION)

    def test_invalid_state_clock_types_or_missing_provenance_are_rejected(self):
        changes = [
            ("ready", True), ("ready", None), ("session_neutral_valid", True),
            ("source", {}), ("profile", {}), ("operator", {}), ("diagnostics", {}),
            ("host_clock", "unknown"), ("host_emitted_us", -1),
            ("host_emitted_us", 1.0), ("host_emitted_us", True),
            ("request_id", True), ("request_id", 1.0),
            ("phase", "streaming"), ("event", "error"), ("schema", "unknown"),
        ]
        for key, value in changes:
            with self.subTest(key=key, value=value):
                event = status(1)
                event[key] = value
                with self.assertRaises(ValueError):
                    bridge.validate_smoke(json.dumps(status(0)) + "\n" + json.dumps(event), REVISION)
        for key in status(1):
            event = copy.copy(status(1))
            del event[key]
            with self.subTest(missing=key), self.assertRaises(ValueError):
                bridge.validate_smoke(json.dumps(status(0)) + "\n" + json.dumps(event), REVISION)
        first, second = status(0), status(1)
        first["host_emitted_us"] = 10
        with self.assertRaises(ValueError):
            bridge.validate_smoke(json.dumps(first) + "\n" + json.dumps(second), REVISION)


class SourceIdentityTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.source = self.root / "source"
        self.source.mkdir()
        self.git("init", "-q")
        (self.source / "Cargo.lock").write_text("version = 4\n", encoding="utf-8")
        self.git("add", "Cargo.lock")
        self.git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "-qm", "fixture")
        self.revision = self.git("rev-parse", "HEAD").strip()

    def tearDown(self):
        self.temporary.cleanup()

    def git(self, *arguments):
        return subprocess.run(["git", *arguments], cwd=self.source, check=True, text=True, capture_output=True).stdout

    def test_clean_source_identity_and_changed_or_untracked_files(self):
        identity = bridge.inspect_source(self.source, self.revision)
        self.assertEqual(identity["revision"], self.revision)
        self.assertEqual(identity["cargo_lock_sha256"], bridge.sha256(self.source / "Cargo.lock"))
        for revision in ("unknown", "0" * 40, self.revision.upper(), "b" * 40):
            with self.subTest(revision=revision), self.assertRaises(ValueError):
                bridge.inspect_source(self.source, revision)
        untracked = self.source / "unexpected.rs"
        untracked.write_text("changed\n", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "未commit"):
            bridge.inspect_source(self.source, self.revision)
        untracked.unlink()
        (self.source / "Cargo.lock").write_text("version = 3\n", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "未commit"):
            bridge.inspect_source(self.source, self.revision)

    def test_existing_or_in_source_output_is_rejected_before_build(self):
        existing = self.root / "existing"
        existing.mkdir()
        marker = existing / "keep.txt"
        marker.write_text("keep", encoding="utf-8")
        for output in (existing, self.source / "build-result"):
            with self.subTest(output=output), self.assertRaises(ValueError):
                bridge.prepare(self.source, self.revision, output, cargo="must-not-run")
        self.assertEqual(marker.read_text(encoding="utf-8"), "keep")
        self.assertFalse((self.source / "build-result").exists())


class PackagePublicationTest(unittest.TestCase):
    def package(self, root, output):
        source = root / "source"
        source.mkdir()
        def build(arguments, **kwargs):
            target = Path(arguments[arguments.index("--target-dir") + 1]) / "release"
            target.mkdir(parents=True)
            name = "selfrionettectl.exe" if bridge.os.name == "nt" else "selfrionettectl"
            (target / name).write_bytes(b"software-package-fixture")
        identity = {"revision": REVISION, "tree": "a" * 40, "cargo_lock_sha256": "b" * 64}
        with patch.object(bridge, "inspect_source", return_value=identity), \
             patch.object(bridge, "command_text", return_value="fixture-toolchain"), \
             patch.object(bridge.subprocess, "run", side_effect=build), \
             patch.object(bridge, "smoke", return_value={"ready": False}), \
             patch.dict(bridge.os.environ, {"CARGO_BUILD_TARGET": ""}):
            return bridge.prepare(source, REVISION, output)

    def test_complete_manifest_matches_package_and_has_no_pending_file(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / "package"
            manifest = self.package(root, output)
            self.assertEqual(json.loads((output / "bridge-build.json").read_bytes()), manifest)
            self.assertEqual(bridge.sha256(output / manifest["binary"]["name"]), manifest["binary"]["sha256"])
            self.assertFalse((output / "bridge-build.json.pending").exists())

    def test_final_flush_failure_never_publishes_completed_manifest(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / "package"
            with patch.object(bridge.os, "fsync", side_effect=OSError("manifest disk failure")):
                with self.assertRaisesRegex(OSError, "manifest disk failure"):
                    self.package(root, output)
            self.assertTrue((output / "bridge-build.json.pending").exists())
            self.assertFalse((output / "bridge-build.json").exists())


if __name__ == "__main__":
    unittest.main()
