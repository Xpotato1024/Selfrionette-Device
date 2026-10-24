Import("env")

import os
import re
import json
from pathlib import Path


source_sha = os.environ.get("SELFRIONETTE_SOURCE_SHA", "")
if re.fullmatch(r"[0-9a-f]{40}", source_sha) is None or source_sha == "0" * 40:
    raise RuntimeError(
        "SELFRIONETTE_SOURCE_SHA must be a nonzero 40-character lowercase Git SHA"
    )

env.Append(CPPDEFINES=[("SELFRIONETTE_SOURCE_SHA", env.StringifyMacro(source_sha))])

# Firmware版はアプリ版・通信Protocolから独立した配布定義を使用する。
registry_path = Path(env.subst("$PROJECT_DIR")).parent / "editions.json"
registry = json.loads(registry_path.read_text(encoding="utf-8"))
modern = next(item for item in registry["editions"] if item["family"] == "modern")
version = modern["firmware_version"]
if re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:-[a-z0-9.]+)?", version) is None:
    raise RuntimeError("Invalid Managed firmware version")

# Comparison/fallback images must not share a profile binding with the 80 Hz
# candidate. The wire shape and source revision stay unchanged.
rate = env.GetProjectOption("custom_quality_stream_rate_hz", "80")
if rate not in ("20", "40", "80"):
    raise RuntimeError("Managed quality stream supports 20/40/80 Hz only")
if rate != "80":
    version += ("." if "-" in version else "-rate.") + rate + "hz"
if len(version) > 20:
    raise RuntimeError("Managed firmware version exceeds info3 reply reserve")
env.Append(CPPDEFINES=[("SELFRIONETTE_QUALITY_STREAM_RATE_HZ", int(rate))])
env.Append(CPPDEFINES=[("SELFRIONETTE_FIRMWARE_VERSION", env.StringifyMacro(version))])
