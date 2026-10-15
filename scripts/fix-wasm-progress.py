"""Use the final WASM size in Trunk 0.21.14's generated progress loader."""

import os
from pathlib import Path
import re


staging = Path(os.environ["TRUNK_STAGING_DIR"]).resolve()
index = staging / "index.html"
html = index.read_text(encoding="utf-8")


def final_size(match):
    wasm = (staging / match[2]).resolve()
    if not wasm.is_relative_to(staging):
        raise ValueError("WASM asset must be inside the build directory")
    return f"{match[1]}{wasm.stat().st_size}{match[3]}"


# Trunk captures sourceSize before wasm-opt. Update only the initializer call,
# after all asset pipelines finish, so compressed responses also report correctly.
html, count = re.subn(
    r"(__trunkInitializer\(init, '\./([^']+\.wasm)', )\d+(, initializer\(\), true\))",
    final_size,
    html,
)
if count != 1:
    raise RuntimeError("Expected one Trunk initializer; check the loader template")
index.write_text(html, encoding="utf-8")
