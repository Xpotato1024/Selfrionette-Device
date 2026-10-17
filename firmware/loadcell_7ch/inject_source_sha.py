Import("env")

import os
import re


source_sha = os.environ.get("SELFRIONETTE_SOURCE_SHA", "")
if re.fullmatch(r"[0-9a-f]{40}", source_sha) is None or source_sha == "0" * 40:
    raise RuntimeError(
        "SELFRIONETTE_SOURCE_SHA must be a nonzero 40-character lowercase Git SHA"
    )

env.Append(CPPDEFINES=[("SELFRIONETTE_SOURCE_SHA", env.StringifyMacro(source_sha))])
