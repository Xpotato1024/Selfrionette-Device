"""実機を接続せず、固定revisionのbridge binaryと照合用manifestを作る。"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile


REVISION = re.compile(r"[0-9a-f]{40}")
EVENT_SCHEMA = "selfrionette-bridge-event/v1"
COMMAND_SCHEMA = "selfrionette-bridge-command/v1"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def command_text(arguments: list[str], source: Path) -> str:
    result = subprocess.run(
        arguments, cwd=source, check=True, capture_output=True, text=True,
        encoding="utf-8", timeout=30,
    )
    return result.stdout.strip()


def inspect_source(source: Path, expected: str) -> dict[str, str]:
    if REVISION.fullmatch(expected) is None or expected == "0" * 40:
        raise ValueError("--revisionには40桁小文字hexの正確なcommit SHAを指定してください")
    actual = command_text(["git", "rev-parse", "HEAD"], source)
    if actual != expected:
        raise ValueError(f"source revisionが異なります: expected={expected}, actual={actual}")
    if command_text(["git", "status", "--porcelain", "--untracked-files=all"], source):
        raise ValueError("未commitまたは未追跡の変更があります。clean checkoutを指定してください")
    return {
        "revision": actual,
        "tree": command_text(["git", "rev-parse", "HEAD^{tree}"], source),
        "cargo_lock_sha256": sha256(source / "Cargo.lock"),
    }


def validate_smoke(stdout: str, revision: str) -> dict[str, object]:
    if len(stdout.encode("utf-8")) > 128 * 1024:
        raise ValueError("bridgeのstatus出力が上限を超えました")
    events = [json.loads(line) for line in stdout.splitlines()]
    if len(events) != 2:
        raise ValueError("起動statusと要求statusの2件だけが必要です")
    previous = 0
    for request_id, event in enumerate(events):
        if not isinstance(event, dict) or any(
            event.get(key) != value for key, value in {
                "schema": EVENT_SCHEMA,
                "event": "status",
                "bridge_revision": revision,
                "host_clock": "bridge_process_elapsed_us",
                "phase": "unconfigured",
                "request_id": request_id,
            }.items()
        ):
            raise ValueError("bridgeのrevision、schema、状態、request IDが一致しません")
        if type(event["request_id"]) is not int:
            raise ValueError("bridgeのrequest IDが整数ではありません")
        if event.get("ready") is not False or event.get("session_neutral_valid") is not False:
            raise ValueError("未接続のbridgeがreadyまたはneutral有効を報告しました")
        if any(key not in event or event[key] is not None for key in ("source", "profile", "operator", "diagnostics")):
            raise ValueError("未接続のbridgeに入力または個人profileの状態があります")
        emitted = event.get("host_emitted_us")
        if type(emitted) is not int or emitted < previous:
            raise ValueError("bridgeの単調時計が不正です")
        previous = emitted
    return {"status_events": 2, "sample_events": 0, "ready": False, "exit_code": 0}


def smoke(binary: Path, revision: str) -> dict[str, object]:
    # configure/connectを送らず、serial所有が発生しない起動・status・EOFだけを検証する。
    request = json.dumps({"schema": COMMAND_SCHEMA, "request_id": 1, "action": "status"}) + "\n"
    result = subprocess.run(
        [str(binary), "bridge"], input=request, capture_output=True,
        text=True, encoding="utf-8", timeout=10,
    )
    if result.returncode != 0:
        raise ValueError(f"bridgeの終了に失敗しました (exit={result.returncode}): {result.stderr[:2048]}")
    if result.stderr:
        raise ValueError(f"bridgeが診断エラーを出力しました: {result.stderr[:2048]}")
    return validate_smoke(result.stdout, revision)


def prepare(source: Path, revision: str, output: Path, cargo: str = "cargo") -> dict[str, object]:
    source = source.resolve(strict=True)
    output = output.absolute()
    if output.exists() or output.is_symlink():
        raise ValueError("出力先は存在しない新しいディレクトリを指定してください")
    if output.resolve().is_relative_to(source):
        raise ValueError("出力先はsource checkoutの外に指定してください")
    if not output.parent.is_dir():
        raise ValueError("出力先の親ディレクトリが存在しません")
    identity = inspect_source(source, revision)
    cargo_version = command_text([cargo, "--version"], source)
    rustc_version = command_text([os.environ.get("RUSTC", "rustc"), "--version"], source)
    environment = os.environ.copy()
    environment["SELFRIONETTE_BRIDGE_REVISION"] = revision
    # 別targetの既存binaryを拾わない。暗黙のcross buildも受け付けない。
    if environment.get("CARGO_BUILD_TARGET"):
        raise ValueError("CARGO_BUILD_TARGETを解除し、実行するPC向けにbuildしてください")
    with tempfile.TemporaryDirectory(prefix="selfrionette-bridge-") as temporary:
        target = Path(temporary) / "target"
        subprocess.run(
            [cargo, "build", "--locked", "--release", "-p", "selfrionettectl", "--target-dir", str(target)],
            cwd=source, env=environment, check=True,
        )
        binary = target / "release" / ("selfrionettectl.exe" if os.name == "nt" else "selfrionettectl")
        checked = smoke(binary, revision)
        if inspect_source(source, revision) != identity:
            raise ValueError("build中にsourceまたはCargo.lockが変わりました")
        manifest = {
            "schema": "selfrionette-bridge-build/v1",
            "bridge_revision": revision,
            "source_tree": identity["tree"],
            "cargo_lock_sha256": identity["cargo_lock_sha256"],
            "cargo_version": cargo_version,
            "rustc_version": rustc_version,
            "platform": sys.platform,
            "binary": {"name": binary.name, "sha256": sha256(binary), "bytes": binary.stat().st_size},
            "smoke": checked,
            "serial_opened": False,
            "hardware_acceptance": False,
        }
        output.mkdir(exist_ok=False)
        destination = output / binary.name
        with binary.open("rb") as reader, destination.open("xb") as writer:
            shutil.copyfileobj(reader, writer)
        destination.chmod(binary.stat().st_mode)
        if sha256(destination) != manifest["binary"]["sha256"]:
            raise ValueError("コピー後のbinary hashが一致しません")
        # manifestは成功時に最後に作る。不完全な出力を完成packageとして扱わない。
        manifest_bytes = (json.dumps(manifest, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
        pending = output / "bridge-build.json.pending"
        with pending.open("xb") as stream:
            stream.write(manifest_bytes)
            stream.flush()
            os.fsync(stream.fileno())
        if pending.read_bytes() != manifest_bytes:
            raise ValueError("manifestのread-backが一致しません")
        os.link(pending, output / "bridge-build.json")
        try:
            pending.unlink()
        except OSError:
            pass
    return manifest


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--revision", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--cargo", default="cargo")
    args = parser.parse_args()
    try:
        manifest = prepare(args.source, args.revision, args.output, args.cargo)
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"bridge準備に失敗しました: {error}", file=sys.stderr)
        return 1
    print(json.dumps(manifest, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
