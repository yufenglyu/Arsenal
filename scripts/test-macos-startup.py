#!/usr/bin/env python3
"""Launch a packaged app against disposable legacy-link fixtures on macOS."""
import argparse
import os
from pathlib import Path
import platform
import subprocess
import tempfile
import time


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("app", type=Path)
    parser.add_argument("--expect-legacy-failure", action="store_true")
    args = parser.parse_args()
    if platform.system() != "Darwin":
        parser.error("This check requires macOS")
    app = args.app.resolve(strict=True)
    for scenario in ("dangling-installation", "dangling-config-root", "separate-legacy-data"):
        with tempfile.TemporaryDirectory(prefix="arsenal-startup-") as directory:
            root = Path(directory)
            bundle = root / "Arsenal.app"
            subprocess.run(["ditto", str(app), str(bundle)], check=True)
            current = root / ".arsenal"
            current.mkdir()
            legacy = root / ".skillshub"
            link = current / "old-installed-skill"
            target = legacy / "library" / "removed-skill"
            if scenario == "dangling-installation":
                link.symlink_to(target, target_is_directory=True)
            elif scenario == "dangling-config-root":
                legacy.symlink_to(root / "removed-config", target_is_directory=True)
            else:
                legacy.mkdir()
                (legacy / "config.json").write_text("legacy fixture")
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
                    if args.expect_legacy_failure:
                        expected = {
                            "dangling-installation": "An installed link points to missing data",
                            "dangling-config-root": "Both .arsenal and .skillshub exist",
                            "separate-legacy-data": "Both .arsenal and .skillshub exist",
                        }[scenario]
                        assert proc.poll() is not None and expected in output, (
                            "Published baseline did not reproduce the expected migration failure")
                        print(f"REPRODUCED {scenario}: {expected}", flush=True)
                        continue
                    assert proc.poll() is None, f"{scenario}: app exited during startup ({proc.returncode})"
                    assert (current / "store/manifest.json").is_file(), "Current store not initialized"
                    assert not (current / "migrations/arsenal-paths-v2").exists(), "Unexpected legacy migration"
                    if scenario == "dangling-installation":
                        assert link.readlink() == target and not target.exists()
                    elif scenario == "dangling-config-root":
                        assert legacy.is_symlink() and not legacy.exists()
                    else:
                        assert (legacy / "config.json").read_text() == "legacy fixture"
                    print(f"PASS {scenario}: app stays running and legacy fixture is unchanged", flush=True)
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
