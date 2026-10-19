"""Check image/profile identity for explicitly selected firmware build rates."""

import json
import os
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "firmware/loadcell_7ch/inject_source_sha.py"
REGISTRY = json.loads((ROOT / "firmware/editions.json").read_text(encoding="utf-8"))
VERSION = next(item["firmware_version"] for item in REGISTRY["editions"] if item["family"] == "modern")


class BuildEnvironment:
    def __init__(self, rate):
        self.rate = rate
        self.defines = {}

    def Append(self, *, CPPDEFINES):
        self.defines.update(CPPDEFINES)

    def StringifyMacro(self, value):
        return value

    def subst(self, value):
        assert value == "$PROJECT_DIR"
        return str(ROOT / "firmware/loadcell_7ch")

    def GetProjectOption(self, name, default):
        assert name == "custom_quality_stream_rate_hz" and default == "80"
        return self.rate


def inject(rate, source="a" * 40):
    environment = BuildEnvironment(rate)
    with patch.dict(os.environ, {"SELFRIONETTE_SOURCE_SHA": source}):
        runpy.run_path(str(SCRIPT), init_globals={"Import": lambda _: None, "env": environment})
    return environment.defines


class FirmwareBuildIdentityTest(unittest.TestCase):
    def test_rates_have_distinct_profile_binding_with_same_source(self):
        versions = set()
        for rate in ("20", "40", "80"):
            with self.subTest(rate=rate):
                definitions = inject(rate)
                version = definitions["SELFRIONETTE_FIRMWARE_VERSION"]
                expected = VERSION if rate == "80" else VERSION + ("." if "-" in VERSION else "-rate.") + rate + "hz"
                self.assertEqual(version, expected)
                self.assertLessEqual(len(version), 20)
                self.assertEqual(definitions["SELFRIONETTE_SOURCE_SHA"], "a" * 40)
                self.assertEqual(definitions["SELFRIONETTE_QUALITY_STREAM_RATE_HZ"], int(rate))
                versions.add(version)
        self.assertEqual(len(versions), 3)

    def test_invalid_rate_cannot_silently_select_an_image(self):
        for rate in ("", "0", "50", "81", "080", "80 "):
            with self.subTest(rate=rate), self.assertRaisesRegex(RuntimeError, "20/40/80"):
                inject(rate)

    def test_source_is_still_required(self):
        for source in ("", "0" * 40, "A" * 40, "a" * 39):
            with self.subTest(source=source), self.assertRaisesRegex(RuntimeError, "SOURCE_SHA"):
                inject("80", source)


if __name__ == "__main__":
    unittest.main()
