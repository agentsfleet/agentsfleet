"""Validate frozen evidence and execute correct/faulty isolated copies."""

import hashlib
import json
import os
import shutil
import signal
import subprocess
import tempfile
from collections import Counter
from pathlib import Path

PREFIX = Path("pilots/jev-assertions")
ROOT = Path(__file__).resolve().parents[2]
LABELS = {"exact", "weak", "wrong_target", "missing", "insufficient"}
DEFECT_LABELS = {"weak", "wrong_target", "missing"}
BATCHES = ("a", "b")
STATE_CAP = 24 * 1024
MANIFEST_CAP = 64 * 1024
FILE_CAP = 256 * 1024
PROOF_TIMEOUT = 120
COMMAND_TIMEOUT = 600
KILL_GRACE_SECONDS = 3


def load(path):
    with Path(path).open(encoding="utf-8") as stream:
        return json.load(stream)


def save(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as stream:
        json.dump(value, stream, indent=2, ensure_ascii=False)
        stream.write("\n")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def contained(root, relative):
    root = Path(root).resolve()
    path = (root / relative).resolve(strict=True)
    if not path.is_relative_to(root):
        raise ValueError(f"Evidence escapes the project: {relative}")
    return path


def freeze_payload(root=ROOT):
    pilot = root / PREFIX
    paths = [pilot / "cases.json", pilot / "reviews/unaided-author.json",
             pilot / "reviews/session-template.json", pilot / "vitest.config.ts"]
    for directory in ("fixtures", "helpers", "manifests"):
        paths.extend(sorted((pilot / directory).rglob("*")))
    files = {str(path.relative_to(root)): digest(path.read_bytes())
             for path in paths if path.is_file()}
    return {"schema_version": 1, "files": files,
            "source_revision": load(pilot / "cases.json")["source_revision"]}


def freeze_digest(frozen):
    return digest(json.dumps(frozen, sort_keys=True, separators=(",", ":")).encode())


def verify_freeze(root=ROOT):
    frozen = load(root / PREFIX / "freeze.json")
    for relative, expected in frozen["files"].items():
        if digest(contained(root, relative).read_bytes()) != expected:
            raise ValueError(f"Frozen bytes changed: {relative}")
    if freeze_payload(root) != frozen:
        raise ValueError("Frozen inventory changed")
    return freeze_digest(frozen)


def check_item(root, item, case):
    if set(item) != {"id", "question", "requirement", "evidence"}:
        raise ValueError("Unexpected model-input fields")
    if item["question"] != "verify.assertion" or item["requirement"] != case["requirement"]:
        raise ValueError("Question or requirement changed")
    if item["evidence"] != case["model_evidence"]:
        raise ValueError("Selected evidence changed or leaked answers")
    spans = []
    for ref in item["evidence"]:
        allowed = (str(PREFIX / "fixtures") + "/", str(PREFIX / "helpers") + "/")
        if not ref["path"].startswith(allowed) or ref["selector"] != {"kind": "file"}:
            raise ValueError("Upload path or selector outside the frozen scope")
        path = contained(root, ref["path"])
        raw = path.read_bytes()
        if len(raw) > FILE_CAP:
            raise ValueError("Evidence file exceeds its byte cap")
        spans.append({**ref, "startLine": 1, "endLine": len(raw.decode().splitlines()),
                      "text": raw.decode()})
    state = {"requirement": item["requirement"], "evidence": spans}
    size = len(json.dumps(state, ensure_ascii=False, separators=(",", ":")).encode())
    if size > STATE_CAP or len(item["evidence"]) > 8:
        raise ValueError("Complete selected evidence exceeds the command limits")
    selected = {ref["path"] for ref in item["evidence"]}
    if selected.intersection(case["omitted_context"]):
        raise ValueError("Deliberately omitted context was uploaded")
    return {"id": item["id"], "state_bytes": size,
            "files": [{"path": ref["path"], "sha256": digest(contained(root, ref["path"]).read_bytes()),
                       "start_line": 1, "end_line": span["endLine"]}
                      for ref, span in zip(item["evidence"], spans)]}


def validate_inputs(root=ROOT):
    pilot = root / PREFIX
    ledger = load(pilot / "cases.json")
    cases = ledger["cases"]
    if len(cases) != 20 or Counter(c["expected"] for c in cases) != Counter({k: 4 for k in LABELS}):
        raise ValueError("Expected twenty balanced cases")
    by_id = {case["id"]: case for case in cases}
    if len(by_id) != 20 or any(c["source_revision"] != ledger["source_revision"] for c in cases):
        raise ValueError("Duplicate cases or mismatched source revision")
    uploads = []
    seen = set()
    for batch in BATCHES:
        path = pilot / f"manifests/{batch}.json"
        manifest = load(path)
        if set(manifest) != {"stage", "items"} or manifest["stage"] != "verify":
            raise ValueError("Invalid manifest fields or stage")
        if len(manifest["items"]) != 10 or path.stat().st_size > MANIFEST_CAP:
            raise ValueError("Manifest must contain ten bounded items")
        for item in manifest["items"]:
            if item["id"] in seen or item["id"] not in by_id:
                raise ValueError("Unknown or repeated manifest identifier")
            seen.add(item["id"])
            uploads.append(check_item(root, item, by_id[item["id"]]))
    if seen != set(by_id):
        raise ValueError("Missing manifest case")
    review = load(pilot / "reviews/unaided-author.json")
    if review["advice_seen"] or review["independent"] or review["blinded"]:
        raise ValueError("Author inspection cannot be treated as blinded or independent")
    return ledger, uploads


def run_process(arguments, cwd=ROOT, env=None, timeout=COMMAND_TIMEOUT):
    with subprocess.Popen(arguments, cwd=cwd, env=env, text=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                          start_new_session=True) as process:
        try:
            stdout, stderr = process.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.communicate(timeout=KILL_GRACE_SECONDS)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.communicate()
            raise TimeoutError("Pilot command timed out; its process group was stopped") from None
        return process.returncode, stdout, stderr


def source_implementation(root, ledger, family, faulty):
    definition = ledger["families"][family]
    revision = ledger["source_revision"]
    code, text, error = run_process(["git", "show", f"{revision}:{definition['implementation']}"], cwd=root)
    if code:
        raise ValueError(f"Pinned source unavailable: {error.strip()}")
    source_case = next(case for case in ledger["cases"] if case["family"] == family)
    if digest(text.encode()) != source_case["source_evidence"][0]["sha256"]:
        raise ValueError("Pinned source digest differs")
    for old, new in definition["imports"]:
        if text.count(old) != 1:
            raise ValueError("Import adaptation is not unique")
        text = text.replace(old, new)
    if faulty:
        old, new = definition["fault"]
        if text.count(old) != 1:
            raise ValueError("Fault seed is not unique")
        text = text.replace(old, new)
    for old, new in ledger["mechanical_source_adaptations"]["families"][family]:
        if old not in text:
            raise ValueError("Recorded constant repair cannot be reconstructed")
        text = text.replace(old, new)
    return text


def run_proof(root, ledger, faulty):
    pilot = root / PREFIX
    mode = "faulty" if faulty else "correct"
    with tempfile.TemporaryDirectory(prefix="proof-", dir=pilot) as temporary:
        scratch = Path(temporary)
        for name in ("fixtures", "helpers"):
            shutil.copytree(pilot / name, scratch / name)
        shutil.copyfile(pilot / "vitest.config.ts", scratch / "vitest.config.ts")
        workspace_modules = root / "ui/packages/design-system/node_modules"
        if not workspace_modules.is_dir():
            raise ValueError("Install the design-system workspace's frozen dependencies first")
        (scratch / "node_modules").symlink_to(workspace_modules, target_is_directory=True)
        sources = {family: source_implementation(root, ledger, family, faulty)
                   for family in ledger["families"]}
        for case in ledger["cases"]:
            (scratch / "fixtures" / case["id"] / "implementation.tsx").write_text(sources[case["family"]])
        output = scratch / "result.json"
        arguments = ["node", "ui/packages/design-system/node_modules/vitest/vitest.mjs", "run", "--config",
                     str(scratch / "vitest.config.ts"), "--root", str(scratch),
                     "--reporter=json", "--outputFile", str(output)]
        code, stdout, stderr = run_process(arguments, root, timeout=PROOF_TIMEOUT)
        (pilot / "receipts").mkdir(exist_ok=True)
        (pilot / f"receipts/{mode}.txt").write_text(f"Command: {arguments}\nExit: {code}\n{stdout}\n{stderr}")
        if not output.exists():
            raise ValueError(f"{mode} proof produced no test report; inspect receipts/{mode}.txt")
        report = load(output)
        with (pilot / f"receipts/{mode}.txt").open("a") as stream:
            stream.write("\nTest report:\n" + json.dumps(report, indent=2) + "\n")
        return validate_proof(ledger, report, code, mode)


def verify_provenance(root, ledger):
    references = {ref["path"]: ref["sha256"] for case in ledger["cases"] for ref in case["source_evidence"]}
    helpers = ledger["helper_sources"]
    paths = set(references).union(helpers.values())
    sources = {}
    for path in sorted(paths):
        code, text, error = run_process(["git", "show", f"{ledger['source_revision']}:{path}"], cwd=root)
        if code or (path in references and digest(text.encode()) != references[path]):
            raise ValueError(f"Pinned evidence differs: {path}; {error.strip()}")
        sources[path] = text
    for name, path in helpers.items():
        text = sources[path]
        changes = ledger["helper_imports"].get(name, []) + ledger["mechanical_source_adaptations"]["helpers"].get(name, [])
        for old, new in changes:
            if old not in text:
                raise ValueError("Recorded helper repair cannot be reconstructed")
            text = text.replace(old, new)
        if text != (root / PREFIX / "helpers" / name).read_text():
            raise ValueError(f"Copied helper differs from the declared adaptations: {name}")
    return {path: digest(text.encode()) for path, text in sources.items()}


def validate_proof(ledger, report, code, mode):
    if report.get("numRuntimeErrorTestSuites", 0):
        raise ValueError("Runtime/import failures are not assertion evidence")
    results = {}
    for suite in report["testResults"]:
        ident = Path(suite["name"]).parent.name
        assertions = suite["assertionResults"]
        if len(assertions) != 1 or ident in results:
            raise ValueError("Expected one executed test per neutral case")
        result = assertions[0]
        results[ident] = {"status": result["status"], "failureMessages": result["failureMessages"]}
    if set(results) != {c["id"] for c in ledger["cases"]}:
        raise ValueError("Missing proof executions")
    for case in ledger["cases"]:
        result = results[case["id"]]
        if result["status"] != case["proof"][mode]:
            raise ValueError(f"Unexpected {mode} proof for {case['id']}: {result}")
        if result["status"] == "failed" and not any("AssertionError" in m for m in result["failureMessages"]):
            raise ValueError("A non-assertion failure cannot count as discriminating the fault")
    counts = Counter(r["status"] for r in results.values())
    if code != (1 if counts["failed"] else 0):
        raise ValueError("Proof exit disagrees with executed assertions")
    return {"mode": mode, "exit": code, "counts": dict(counts), "results": results}


def check(root=ROOT):
    frozen_digest = verify_freeze(root)
    ledger, uploads = validate_inputs(root)
    source_hashes = verify_provenance(root, ledger)
    for case in ledger["cases"]:
        wanted = source_implementation(root, ledger, case["family"], case["expected"] in DEFECT_LABELS)
        path = contained(root, str(PREFIX / "fixtures" / case["id"] / "implementation.tsx"))
        if path.read_text() != wanted:
            raise ValueError(f"Selected implementation differs from the proved copy: {case['id']}")
    correct = run_proof(root, ledger, False)
    faulty = run_proof(root, ledger, True)
    selected = [file for item in uploads for file in item["files"]]
    upload_summary = {"distinct_files": len({file["path"] for file in selected}),
                      "selected_source_bytes_including_repeats": sum(contained(root, file["path"]).stat().st_size for file in selected),
                      "state_bytes_total": sum(item["state_bytes"] for item in uploads),
                      "largest_state_bytes": max(item["state_bytes"] for item in uploads),
                      "manifest_bytes": {batch: (root / PREFIX / f"manifests/{batch}.json").stat().st_size for batch in BATCHES}}
    receipt = {"frozen_digest": frozen_digest, "source_revision": ledger["source_revision"],
               "uploads": uploads, "upload_summary": upload_summary, "correct": correct, "faulty": faulty,
               "source_hashes": source_hashes, "provider_requests": 0, "test_framework": "Vitest 5.0.3 / jsdom 30.1.2"}
    save(root / PREFIX / "receipts/checks.json", receipt)
    return receipt
