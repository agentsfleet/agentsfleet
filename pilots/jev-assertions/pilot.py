"""Run the frozen assertion pilot without changing the product or its gates."""

import argparse
import json
import sys

sys.dont_write_bytecode = True

from pilot_checks import (
    BATCHES,
    PREFIX,
    ROOT,
    check,
    freeze_digest,
    freeze_payload,
    load,
    validate_inputs,
)
from pilot_measure import judge_batch, runner_digest, summarize

COMMANDS = ("check", "replay", "live", "summarize", "freeze")
APPROVAL = ROOT / PREFIX / "approval.json"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=COMMANDS)
    parser.add_argument("--batch", choices=BATCHES)
    arguments = parser.parse_args()
    if arguments.command == "freeze":
        validate_inputs()
        frozen = freeze_payload()
        path = ROOT / PREFIX / "freeze.json"
        with path.open("x", encoding="utf-8") as stream:
            json.dump(frozen, stream, indent=2)
            stream.write("\n")
        result = {"frozen_digest": freeze_digest(frozen), "runner_digest": runner_digest()}
    elif arguments.command == "check":
        receipt = check()
        result = {"frozen_cases": len(receipt["uploads"]), "correct": receipt["correct"]["counts"],
                  "faulty": receipt["faulty"]["counts"], "provider_requests": 0,
                  "largest_state_bytes": max(row["state_bytes"] for row in receipt["uploads"])}
    elif arguments.command == "summarize":
        summary = summarize()
        result = {key: value for key, value in summary.items() if key != "per_case"}
    else:
        refresh = arguments.command == "live"
        if refresh and not APPROVAL.exists():
            raise ValueError("Owner approval is missing; no live call was made")
        approval = load(APPROVAL) if refresh else None
        result = [judge_batch(ROOT, batch, refresh, approval) for batch in
                  ((arguments.batch,) if arguments.batch else BATCHES)]
    print(json.dumps(result, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, FileNotFoundError, TimeoutError) as error:
        print(f"Pilot refused: {error}", file=sys.stderr)
        sys.exit(2)
