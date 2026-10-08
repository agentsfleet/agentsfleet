"""One-shot live admission and truthful advisory measurement accounting."""

import datetime
import fcntl
import json
import os
from decimal import Decimal

from pilot_checks import (
    BATCHES,
    DEFECT_LABELS,
    LABELS,
    PREFIX,
    ROOT,
    digest,
    load,
    run_process,
    save,
    validate_inputs,
    verify_freeze,
)

MODEL = "jev-1.13.0"
REQUEST_CAP = 20
BATCH_CAP = 10
TOKEN_CAP = 65_536
INPUT_RATE = Decimal("0.042")
OUTPUT_RATE = Decimal(0)
SPENDING_CAP = Decimal("0.06")
MILLION = Decimal(1_000_000)
THRESHOLD = 0.8
ENGINE_VERSION = "0.13.0"
LIVE = "live"
COMPLETE = "complete"
INCOMPLETE = "incomplete"
NATIVE_ANSWER = "answer"
RESULTS = "results"
REQUESTS = "requests"
USAGE = "usage"
CONFIRMED_PRICING = "https://docs.typesafe.ai/models"
RUNNER_FILES = ("pilot.py", "pilot_checks.py", "pilot_measure.py", "pilot_test.py")


def runner_digest(root=ROOT):
    hashes = {name: digest((root / PREFIX / name).read_bytes()) for name in RUNNER_FILES}
    return digest(json.dumps(hashes, sort_keys=True).encode())


def approved(root, approval):
    frozen = verify_freeze(root)
    validate_inputs(root)
    required = {"frozen_digest": frozen, "runner_digest": runner_digest(root),
                "model": MODEL, "provider_requests_max": REQUEST_CAP,
                "ceiling_usd": str(SPENDING_CAP), "input_usd_per_million": str(INPUT_RATE),
                "output_usd_per_million": str(OUTPUT_RATE), "max_input_tokens_per_request": TOKEN_CAP}
    if any(approval.get(key) != value for key, value in required.items()):
        raise ValueError("Approval does not match frozen inputs, runner, model or budget")
    if not approval.get("owner_quote") or approval.get("approved_by") != "Kishore":
        raise ValueError("The owner's actual approval quote is required")
    if approval.get("pricing_checked_url") != CONFIRMED_PRICING or not approval.get("pricing_checked_at"):
        raise ValueError("Verify current model pricing before live admission")
    checked = datetime.datetime.fromisoformat(approval["pricing_checked_at"])
    if checked.tzinfo is None:
        raise ValueError("Pricing confirmation needs an explicit timezone")
    age = datetime.datetime.now(datetime.timezone.utc) - checked
    if not datetime.timedelta(0) <= age <= datetime.timedelta(hours=24):
        raise ValueError("Pricing confirmation is stale or in the future")
    code, output, _ = run_process(["orly", "--version"], cwd=root)
    if code or output.strip() != ENGINE_VERSION:
        raise ValueError("Installed orly does not match the frozen engine pin")
    return frozen


def reserve(root, batch, approval):
    if batch not in BATCHES:
        raise ValueError("Unknown batch")
    frozen = approved(root, approval)
    path = root / PREFIX / "receipts/reservations.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor = os.open(path, os.O_RDWR | os.O_CREAT, 0o600)
    with os.fdopen(descriptor, "r+", encoding="utf-8") as stream:
        fcntl.flock(stream, fcntl.LOCK_EX)
        text = stream.read()
        ledger = json.loads(text) if text else {"frozen_digest": frozen, "batches": []}
        validate_reservations(ledger, frozen)
        if any(row["batch"] == batch for row in ledger["batches"]):
            raise ValueError("Batch already reserved; retries are forbidden, including after crashes")
        requests = sum(row["request_cap"] for row in ledger["batches"]) + BATCH_CAP
        ceiling = INPUT_RATE * TOKEN_CAP * requests / MILLION
        if requests > REQUEST_CAP or ceiling > SPENDING_CAP:
            raise ValueError("Total request or conservative spending ceiling exceeded")
        ledger["batches"].append({"batch": batch, "request_cap": BATCH_CAP,
                                  "conservative_usd": str(INPUT_RATE * TOKEN_CAP * BATCH_CAP / MILLION)})
        stream.seek(0)
        stream.write(json.dumps(ledger, indent=2) + "\n")
        stream.truncate()
        stream.flush()
        os.fsync(stream.fileno())
        return ledger


def validate_reservations(ledger, frozen):
    if not isinstance(ledger, dict) or ledger.get("frozen_digest") != frozen:
        raise ValueError("Reservation ledger belongs to different frozen inputs")
    rows = ledger.get("batches")
    expected_cost = str(INPUT_RATE * TOKEN_CAP * BATCH_CAP / MILLION)
    if (not isinstance(rows, list) or len(rows) > len(BATCHES)
            or any(not isinstance(row, dict) or row.get("batch") not in BATCHES
                   or type(row.get("request_cap")) is not int or row["request_cap"] != BATCH_CAP
                   or row.get("conservative_usd") != expected_cost for row in rows)
            or len({row["batch"] for row in rows}) != len(rows)):
        raise ValueError("Reservation ledger is malformed; fail closed")


def unavailable(items, reason):
    return {"stage": "verify", "advisory": True, REQUESTS: None,
            RESULTS: [{"id": item["id"], "question": item["question"], "status": INCOMPLETE,
                       "reason": reason, REQUESTS: None, "elapsedMs": None, USAGE: None} for item in items]}


def validate_report(report, manifest, refresh):
    if not isinstance(report, dict) or report.get("stage") != "verify" or report.get("advisory") is not True:
        raise ValueError("Command report stage or advisory status differs")
    results = report[RESULTS]
    if (not isinstance(results, list) or any(not isinstance(r, dict) for r in results)
            or len(results) != BATCH_CAP or {r["id"] for r in results} != {i["id"] for i in manifest["items"]}):
        raise ValueError("Command report does not contain the frozen batch")
    for result in results:
        if (result["question"] != "verify.assertion" or result["status"] not in (COMPLETE, INCOMPLETE)
                or type(result[REQUESTS]) is not int or result[REQUESTS] not in ((0, 1) if refresh else (0,))):
            raise ValueError("Invalid native attempt or request count")
        usage = result[USAGE]
        if usage is not None and (not isinstance(usage, dict) or any(
                type(usage[k]) is not int or usage[k] < 0 for k in ("input_tokens", "output_tokens"))):
            raise ValueError("Invalid native token usage")
        if result["status"] == COMPLETE and (not isinstance(result.get("answer"), dict) or result["model"] != MODEL
                or result["answer"]["choice"] not in LABELS or result["mode"] != (LIVE if refresh else "replay")):
            raise ValueError("Native model, answer or execution mode differs")
    if type(report[REQUESTS]) is not int or report[REQUESTS] != sum(r[REQUESTS] for r in results):
        raise ValueError("Native request totals do not reconcile")


def judge_batch(root, batch, refresh=False, approval=None):
    verify_freeze(root)
    validate_inputs(root)
    pilot = root / PREFIX
    suffix = LIVE if refresh else ("replay" if (pilot / f"receipts/live-{batch}.json").exists() else "offline")
    destination = pilot / f"receipts/{suffix}-{batch}.json"
    if refresh:
        if destination.exists():
            raise ValueError("A retained live attempt already exists; it cannot be replaced")
        reserve(root, batch, approval or {})
    arguments = ["orly", "judge", "verify", "--input", str(PREFIX / f"manifests/{batch}.json")]
    if refresh:
        arguments.append("--refresh")
    arguments.append("--json")
    manifest = load(pilot / f"manifests/{batch}.json")
    code, stdout, stderr, raw_report, error_message = None, "", "", None, None
    report = unavailable(manifest["items"], "Command unavailable; retain the entire reservation and do not retry")
    try:
        code, stdout, stderr = run_process(arguments, cwd=root)
        raw_report = json.loads(stdout)
        validate_report(raw_report, manifest, refresh)
        report = raw_report
        if code not in (0, 2):
            error_message = "Unexpected command result; validated native answers retained"
    except (TimeoutError, OSError, KeyError, ValueError, TypeError) as error:
        error_message = str(error)
    receipt = {"mode": suffix, "command": arguments, "exit": code, "stdout": stdout, "stderr": stderr,
               "collection_error": error_message, "raw_report": raw_report,
               "frozen_digest": verify_freeze(root), "reserved_requests": BATCH_CAP if refresh else 0,
               "report": report}
    save(destination, receipt)
    return receipt


def case_measurement(case, result):
    complete = result.get("status") == COMPLETE
    answer = result.get(NATIVE_ANSWER, {})
    native = answer.get("choice") if complete else None
    assessment = result.get("assessment", {})
    strength = assessment.get("strength") if complete else None
    usage = result.get(USAGE)
    return {"id": case["id"], "origin": case["origin"], "expected": case["expected"],
            "status": result.get("status", INCOMPLETE), "native": native,
            "native_matches": native == case["expected"] if complete else None,
            "confidence": answer.get("confidence"), "probabilities": answer.get("probabilities"),
            "strength": strength, "low_strength": strength < THRESHOLD if strength is not None else None,
            "insufficient_requires_inspection": native == "insufficient" if complete else None,
            "uncertain": assessment.get("uncertain"), "concern": assessment.get("concern"),
            "requests": result.get(REQUESTS), "elapsed_ms": result.get("elapsedMs"), USAGE: usage,
            "known_usd": str(INPUT_RATE * usage["input_tokens"] / MILLION) if usage is not None else None}


def group_metrics(rows):
    complete = [row for row in rows if row["native"] is not None]
    positives = [row for row in complete if row.get("finding", row["native"] in DEFECT_LABELS)]
    confirmed = [row for row in positives if row["expected"] in DEFECT_LABELS]
    return {"cases": len(rows), "available": len(complete), "unavailable": len(rows) - len(complete),
            "native_matches": sum(row["native_matches"] is True for row in rows),
            "confirmed_findings": len(confirmed), "false_alarms": len(positives) - len(confirmed),
            "finding_misses": sum(row["expected"] in DEFECT_LABELS for row in complete) - len(confirmed),
            "native_misses": sum(row["expected"] in DEFECT_LABELS and row["native"] not in DEFECT_LABELS for row in complete),
            "unavailable_weaknesses": sum(row["expected"] in DEFECT_LABELS and row["native"] is None for row in rows),
            "low_strength": sum(row["low_strength"] is True for row in rows),
            "matching_native_withheld_by_strength": sum(row["native_matches"] is True and row["low_strength"] is True for row in rows),
            "confident_confirmed_findings": sum(row["concern"] is True for row in confirmed)}


def review_seal(pilot, paths, unaided, assisted):
    if assisted.get("unaided_sha256") != digest(paths[0].read_bytes()):
        raise ValueError("Assisted session does not bind the frozen unaided review")
    frozen_at = datetime.datetime.fromisoformat(unaided["frozen_at"])
    exposed_at = datetime.datetime.fromisoformat(assisted["advice_first_seen_at"])
    if frozen_at.tzinfo is None or exposed_at.tzinfo is None or frozen_at >= exposed_at:
        raise ValueError("Unaided review must be frozen before advice exposure")
    sources = assisted.get("advice_sources", {})
    for batch in BATCHES:
        path = pilot / f"receipts/live-{batch}.json"
        if not path.exists() or sources.get(batch) != digest(path.read_bytes()):
            raise ValueError("Assisted session needs the retained live advice digests")


def paired_reviews(pilot, cases):
    paths = [pilot / f"reviews/{condition}.json" for condition in ("unaided", "assisted")]
    if not all(path.exists() for path in paths):
        return {"status": "unmeasured", "reason": "Separate independent blinded review sessions are unavailable"}
    unaided, assisted = [load(path) for path in paths]
    if unaided["reviewer"] == assisted["reviewer"] or unaided["session"] == assisted["session"]:
        raise ValueError("Paired sessions require different reviewers and session identifiers")
    if unaided["advice_seen_before_freeze"] is not False:
        raise ValueError("Unaided review was contaminated or not frozen before advice")
    review_seal(pilot, paths, unaided, assisted)
    expected = {case["id"]: case["expected"] for case in cases}
    totals = []
    for session, condition in zip((unaided, assisted), ("unaided", "assisted")):
        if session.get("independent") is not True or session.get("blinded") is not True:
            raise ValueError("Reviewers must attest independent, label-blinded sessions")
        if (session["condition"] != condition or len(session["case_order"]) != len(expected)
                or set(session["case_order"]) != set(expected)):
            raise ValueError("Review session condition or order differs from the paired cases")
        decisions = session["decisions"]
        if len(decisions) != len(expected) or {d["id"] for d in decisions} != set(expected):
            raise ValueError("Review session omits or duplicates cases")
        if any(not isinstance(d["review_seconds"], (float, int)) or isinstance(d["review_seconds"], bool)
               or not 0 <= d["review_seconds"] < float("inf") or not d["evidence"]
               or d["classification"] not in LABELS or type(d["finding"]) is not bool for d in decisions):
            raise ValueError("Review timing and finding evidence must be measured")
        rows = [{"native": d["classification"], "expected": expected[d["id"]],
                 "native_matches": d["classification"] == expected[d["id"]],
                 "low_strength": False, "concern": d["finding"], "finding": d["finding"]} for d in decisions]
        totals.append({"condition": condition, **group_metrics(rows),
                       "review_seconds": sum(d["review_seconds"] for d in decisions)})
    pairs = [{"id": case["id"], "unaided": next(d for d in unaided["decisions"] if d["id"] == case["id"]),
              "assisted": next(d for d in assisted["decisions"] if d["id"] == case["id"])} for case in cases]
    return {"status": "measured", "sessions": totals, "pairs": pairs}


def live_results(pilot, frozen):
    reservations_path = pilot / "receipts/reservations.json"
    reservations = load(reservations_path) if reservations_path.exists() else {"frozen_digest": frozen, "batches": []}
    validate_reservations(reservations, frozen)
    reserved_batches = {row["batch"] for row in reservations["batches"]}
    receipts = []
    for batch in BATCHES:
        path = pilot / f"receipts/live-{batch}.json"
        if not path.exists():
            continue
        receipt = load(path)
        if (receipt["frozen_digest"] != frozen or receipt["mode"] != LIVE
                or receipt["reserved_requests"] != BATCH_CAP or batch not in reserved_batches):
            raise ValueError("Live receipt does not match its frozen reservation")
        manifest = load(pilot / f"manifests/{batch}.json")
        if receipt["report"][REQUESTS] is not None:
            validate_report(receipt["report"], manifest, True)
        receipts.append(receipt)
    return reservations, receipts


def summarize(root=ROOT):
    pilot = root / PREFIX
    frozen = verify_freeze(root)
    ledger, _ = validate_inputs(root)
    reservations, receipts = live_results(pilot, frozen)
    results = [result for receipt in receipts for result in receipt["report"][RESULTS]]
    by_id = {result["id"]: result for result in results}
    if len(by_id) != len(results):
        raise ValueError("Duplicate live attempts cannot be silently discarded")
    rows = [case_measurement(case, by_id.get(case["id"], {})) for case in ledger["cases"]]
    known = [row[USAGE] for row in rows if row[USAGE] is not None]
    reserved = sum(r["request_cap"] for r in reservations["batches"])
    request_counts = [r["report"][REQUESTS] for r in receipts]
    requests = sum(n for n in request_counts if n is not None)
    summary = {"frozen_digest": frozen, "model": MODEL, "frozen_cases": len(rows),
               "measurement_status": "measured" if len(results) == 20 and all(n is not None for n in request_counts) else "IN_PROGRESS",
               "reserved_requests": reserved, "reported_requests": requests,
               "unreconciled_request_reservations": reserved - requests,
               "live_case_slots_recorded": len(results), "native": group_metrics(rows),
               "by_origin": {origin: group_metrics([r for r in rows if r["origin"] == origin]) for origin in sorted({r["origin"] for r in rows})},
               "real_defects": [], "paired_review": paired_reviews(pilot, ledger["cases"]),
               "known_usage": {"input_tokens": sum(u["input_tokens"] for u in known), "output_tokens": sum(u["output_tokens"] for u in known)},
               "usage_unknown_slots": len(rows) - len(known),
               "known_cost_usd": str(sum((Decimal(row["known_usd"]) for row in rows if row["known_usd"] is not None), Decimal(0))),
               "reserved_cost_upper_usd": str(INPUT_RATE * TOKEN_CAP * reserved / MILLION),
               "ceiling_usd": str(SPENDING_CAP), "per_case": rows}
    save(pilot / "receipts/summary.json", summary)
    return summary
