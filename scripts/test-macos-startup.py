#!/usr/bin/env python3
"""Launch a packaged app against isolated current-config fixtures on macOS."""
import argparse
import json
import os
from pathlib import Path
import platform
import subprocess
import tempfile
import time


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("app", type=Path)
    args = parser.parse_args()
    if platform.system() != "Darwin":
        parser.error("This check requires macOS")
    app = args.app.resolve(strict=True)
    for scenario in ("fresh", "existing-config", "dangling-installation"):
        with tempfile.TemporaryDirectory(prefix="arsenal-startup-") as directory:
            root = Path(directory)
            bundle = root / "Arsenal.app"
            subprocess.run(["ditto", str(app), str(bundle)], check=True)
            current = root / ".arsenal"
            current.mkdir()
            link = current / "unavailable-skill"
            target = root / "missing-library" / "removed-skill"
            if scenario == "dangling-installation":
                link.symlink_to(target, target_is_directory=True)
            elif scenario == "existing-config":
                (current / "config.json").write_text(json.dumps({
                    "platforms": [], "settings": {"language": "en"},
                    "scan_directories": [],
                }))
            logfile = root / "startup.log"
            with logfile.open("w+") as log:
                proc = subprocess.Popen([str(bundle / "Contents/MacOS/arsenal")],
                                        stdout=log, stderr=log,
                                        env={**os.environ, "RUST_BACKTRACE": "1"})
                try:
                    deadline = time.monotonic() + 15
                    while proc.poll() is None and time.monotonic() < deadline:
                        time.sleep(0.25)
                    log.flush()
                    log.seek(0)
                    output = log.read()
                    assert proc.poll() is None, f"{scenario}: app exited during startup ({proc.returncode})"
                    assert (current / "store/manifest.json").is_file(), "Current store not initialized"
                    if scenario == "dangling-installation":
                        assert link.readlink() == target and not target.exists()
                    elif scenario == "existing-config":
                        assert json.loads((current / "config.json").read_text())["settings"]["language"] == "en"
                    print(f"PASS {scenario}: app stays running with current configuration", flush=True)
                finally:
                    if proc.poll() is None:
                        proc.terminate()
                        try:
                            proc.wait(timeout=10)
                        except subprocess.TimeoutExpired:
                            proc.kill()
                            proc.wait(timeout=5)


if __name__ == "__main__":
    main()
