#!/usr/bin/env python3
"""Run Cargo with trusted machine build defaults and caller-owned overrides."""

import json
import os
from pathlib import Path
import subprocess
import sys


BUILD_SETTINGS = (
    "CARGO_PROFILE_DEV_DEBUG",
    "CARGO_PROFILE_TEST_DEBUG",
    "CARGO_INCREMENTAL",
    "RUSTC_WRAPPER",
    "SCCACHE_DIR",
    "SCCACHE_CACHE_SIZE",
)
EXPORT_SETTINGS = """import json, os, sys
print(json.dumps({k: os.environ[k] for k in sys.argv[1:] if k in os.environ}))
"""


def main():
    environment = dict(os.environ)
    config_home = Path(environment.get("XDG_CONFIG_HOME") or Path.home() / ".config")
    config = config_home / "agentsfleet" / ".env"
    if config.exists():
        # Source only the user's trusted file, never a checkout-controlled .env.
        # A child shell keeps service URLs, target directories and shell options
        # from escaping into a test lane. Capture output so config cannot leak it.
        loaded = subprocess.run(
            ["/bin/bash", "-e", "-c",
             'set -a; source "$1" >/dev/null 2>&1; shift; exec "$@"',
             "cargo-local", str(config), sys.executable, "-c", EXPORT_SETTINGS,
             *BUILD_SETTINGS],
            capture_output=True, text=True, check=False,
        )
        if loaded.returncode:
            print(f"cargo-local: cannot load build settings from {config}", file=sys.stderr)
            return 1
        try:
            defaults = json.loads(loaded.stdout)
        except json.JSONDecodeError:
            print(f"cargo-local: incomplete build settings from {config}", file=sys.stderr)
            return 1
        for name, value in defaults.items():
            environment.setdefault(name, value)
    try:
        os.execvpe("cargo", ["cargo", *sys.argv[1:]], environment)
    except OSError as error:
        print(f"cargo-local: cannot start cargo: {error.strerror}", file=sys.stderr)
        return 127


if __name__ == "__main__":
    sys.exit(main())
