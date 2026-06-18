#!/usr/bin/env python3
"""Allocate a pseudo-terminal and run argv[1:] inside it.

The acceptance login spec drives `agentsfleet login` end-to-end, and the
device flow refuses to run unless stdin is a terminal: `resolveDirectToken`
fails fast on a non-TTY pipe (there is no human to type the verification
code). Bun's test runner spawns from a non-TTY parent, so this launcher
calls `pty.spawn` (forkpty): the child sees `isTTY == True` while the
launcher copies bytes between the parent's pipes and the pty master, then
propagates the child's exit code so the caller can assert on it.

Why not node-pty: its prebuilt binary fails `posix_spawnp` under both Bun
and Node on darwin-arm64, and a from-source rebuild is a native-toolchain
liability in CI. Python's stdlib `pty` tolerates a non-TTY parent (it
guards the `tcgetattr` on stdin) and ships on every macOS box and Linux CI
runner with no native build step.
"""

import os
import pty
import sys

MISSING_COMMAND_EXIT = 2
UNKNOWN_STATUS_EXIT = 1
SIGNAL_EXIT_BASE = 128


def main() -> int:
    if len(sys.argv) < 2:
        sys.stderr.write("pty-spawn: missing command to run\n")
        return MISSING_COMMAND_EXIT
    status = pty.spawn(sys.argv[1:])
    if os.WIFEXITED(status):
        return os.WEXITSTATUS(status)
    if os.WIFSIGNALED(status):
        return SIGNAL_EXIT_BASE + os.WTERMSIG(status)
    return UNKNOWN_STATUS_EXIT


if __name__ == "__main__":
    sys.exit(main())
