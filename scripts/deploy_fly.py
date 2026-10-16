#!/usr/bin/env python3
"""Deploy after proving a fast release command's exit when Fly misses its start."""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import urllib.request

MACHINES_API = "https://api.machines.dev/v1/apps"
RELEASE_GROUP = "fly_app_release_command"
MACHINE_TIMEOUT_SECONDS = 15
ANSI_COLOR = re.compile(r"\x1b\[[0-9;]*m")
CREATED = re.compile(r"Created release_command machine ([0-9a-f]+)\b")
START_ERROR = "error starting release_command machine: failed to start VM {machine}: aborted: machine exited abruptly"


def release_machine(output: str) -> str | None:
    """Only the machine created by this deploy can justify continuing it."""
    output = ANSI_COLOR.sub("", output)
    created = CREATED.findall(output)
    if len(created) != 1:
        return None
    machine = created[0]
    return machine if START_ERROR.format(machine=machine) in output else None


def successful_release(machine: object, expected_id: str) -> bool:
    if not isinstance(machine, dict) or machine.get("id") != expected_id:
        return False
    if machine.get("state") not in ("stopped", "destroyed"):
        return False
    config = machine.get("config")
    metadata = config.get("metadata") if isinstance(config, dict) else None
    if not isinstance(metadata, dict) or metadata.get("fly_process_group") != RELEASE_GROUP:
        return False
    events = machine.get("events")
    if not isinstance(events, list) or not all(isinstance(event, dict) for event in events):
        return False
    lifecycle = [event for event in events if event.get("type") in ("start", "exit")]
    if not lifecycle or any(type(event.get("timestamp")) is not int for event in lifecycle):
        return False
    lifecycle.sort(key=lambda event: event["timestamp"], reverse=True)
    if len(lifecycle) < 2 or lifecycle[0].get("type") != "exit" or lifecycle[1].get("type") != "start":
        return False
    if lifecycle[0]["timestamp"] <= lifecycle[1]["timestamp"]:
        return False
    request = lifecycle[0].get("request")
    exit_event = request.get("exit_event") if isinstance(request, dict) else None
    if not isinstance(exit_event, dict):
        return False
    return (
        type(exit_event.get("exit_code")) is int
        and exit_event["exit_code"] == 0
        and type(exit_event.get("guest_exit_code")) is int
        and exit_event["guest_exit_code"] == 0
        and exit_event.get("oom_killed") is False
        and exit_event.get("requested_stop") is False
        and exit_event.get("restarting") is False
        and exit_event.get("error") == ""
        and exit_event.get("guest_error") == ""
        and exit_event.get("signal") == -1
        and exit_event.get("guest_signal") == -1
    )


def read_machine(app: str, machine: str) -> object:
    token = os.environ.get("FLY_API_TOKEN")
    if not token:
        raise ValueError("FLY_API_TOKEN is missing")
    request = urllib.request.Request(
        f"{MACHINES_API}/{app}/machines/{machine}",
        headers={"Authorization": f"Bearer {token}"},
    )
    with urllib.request.urlopen(request, timeout=MACHINE_TIMEOUT_SECONDS) as response:
        return json.load(response)


def run_logged(command: list[str]) -> subprocess.CompletedProcess[str]:
    with subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=1) as process:
        assert process.stdout is not None
        output = []
        for line in process.stdout:
            print(line, end="", flush=True)
            output.append(line)
        return subprocess.CompletedProcess(command, process.wait(), "".join(output))


def deploy(app: str, arguments: list[str]) -> int:
    command = ["flyctl", "deploy", "--app", app, *arguments]
    result = run_logged(command)
    if result.returncode == 0:
        return 0
    machine = release_machine(result.stdout)
    if machine is None:
        return result.returncode
    try:
        completed = successful_release(read_machine(app, machine), machine)
    except (OSError, ValueError):
        print(f"Cannot prove release machine {machine} completed; refusing rollout.", file=sys.stderr)
        return result.returncode
    if not completed:
        print(f"Release machine {machine} has no clean successful exit; refusing rollout.", file=sys.stderr)
        return result.returncode
    print(f"Release machine {machine} exited successfully before Fly observed its start; continuing rollout.", flush=True)
    # The migration already succeeded. Keep the same image and normal rollout
    # checks, and bypass only this completed release command on the second call.
    return subprocess.run([*command, "--skip-release-command"], check=False).returncode


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", required=True)
    parser.add_argument("--image", required=True)
    parser.add_argument("--config", required=True)
    parser.add_argument("--wait-timeout", default="120")
    args = parser.parse_args()
    if not re.fullmatch(r"[a-z0-9-]+", args.app):
        parser.error("--app must be a Fly app name")
    if not re.fullmatch(r"[^@\s]+@sha256:[0-9a-f]{64}", args.image):
        parser.error("--image must name an immutable sha256 digest")
    return deploy(args.app, ["--image", args.image, "--config", args.config, "--wait-timeout", args.wait_timeout])


if __name__ == "__main__":
    raise SystemExit(main())
