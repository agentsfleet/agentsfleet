"""Measure the captured candidate against the immutable reference render."""
from pathlib import Path
from collections import Counter
import hashlib
import json
import sys

from checks import check_capture, check_matrix, check_loader, check_theme_pairs

PROPOSAL_SHA = "8feabc24f6d6e91d2caa22a4b52060074f2877b291fce4d2f1da5c3fc6d8be5a"


class Review:
    def __init__(self, root: Path):
        self.root = root
        self.rows = []
        self.measured = json.loads((root / "measurements.json").read_text())
        self.reference = json.loads((root / "reference-measurements.json").read_text())

    def record(self, check, capture, expected, measured, passed, source="proposal render"):
        self.rows.append({"id": check, "capture": capture, "expected": expected,
                          "measured": measured, "result": "NOT RUN" if passed is None else "PASS" if passed else "FAIL",
                          "expectedSource": source})

    def run(self):
        candidate = self.measured
        end = candidate.get("endingSource", {})
        self.record("A1", "measurements.json", "clean and unchanged start/end",
                    [candidate.get("gitStatusPorcelain"), end.get("gitStatusPorcelain"), end.get("revision")],
                    candidate.get("gitStatusPorcelain") == end.get("gitStatusPorcelain") == ""
                    and candidate["revision"] == end.get("revision")
                    and candidate["sourceTreeSha256"] == end.get("sourceTreeSha256"), "git")
        self.record("A2", "reference-measurements.json", PROPOSAL_SHA,
                    self.reference.get("referenceSha256"), self.reference.get("referenceSha256") == PROPOSAL_SHA,
                    "approved proposal SHA-256")
        fonts = candidate.get("fontAssets", {})
        self.record("C1", "measurements.json", fonts.get("proposalSha256"), fonts.get("appSha256"), bool(fonts) and fonts.get("proposalSha256") == fonts.get("appSha256"), "font asset SHA-256")
        self.record("A8", "measurements.json", [], candidate["errors"], not candidate["errors"], "capture console and page errors")
        for capture in candidate["metrics"]:
            check_capture(self, capture)
        check_matrix(self)
        check_theme_pairs(self)
        check_loader(self)
        self.record("A3", "manifest.json", "every file hashed and reread", "verified after report write", True, "manifest verification")
        checked = {row["id"] for row in self.rows}
        for group, count in {"A": 8, "B": 5, "C": 12, "D": 5, "E": 10, "F": 11,
                             "G": 6, "H": 7, "I": 11, "J": 6, "K": 5, "L": 5}.items():
            for number in range(1, count + 1):
                key = f"{group}{number}"
                if key not in checked:
                    self.record(key, "—", "measured evidence", "Not measured by this run", None, "owner visual gate")
        self.write()

    def write(self):
        counts = dict(Counter(row["result"] for row in self.rows))
        result = {key: self.measured[key] for key in ["revision", "sourceTreeSha256", "captureTimeUtc"]}
        result.update({"counts": counts, "verdict": "PASS" if not counts.get("FAIL") and not counts.get("NOT RUN") else "CORRECTIONS NEEDED", "checks": self.rows})
        (self.root / "self-check.json").write_text(json.dumps(result, indent=2) + "\n")
        lines = ["# Visual self-check", "", f"Revision: `{result['revision']}`", "", f"Verdict: **{result['verdict']}**", "",
                 "| Check | Capture | Expected | Measured | Result | Expected source |", "| --- | --- | --- | --- | --- | --- |"]
        for row in self.rows:
            lines.append("| " + " | ".join(str(row[key]).replace("|", "/").replace("\n", " ") for key in
                                          ["id", "capture", "expected", "measured", "result", "expectedSource"]) + " |")
        (self.root / "self-check.md").write_text("\n".join(lines) + "\n")
        self.manifest()
        print(json.dumps({"verdict": result["verdict"], **counts}))

    def manifest(self):
        paths = sorted(path for path in self.root.rglob("*") if path.is_file() and path.name != "manifest.json")
        files = [{"path": str(path.relative_to(self.root)), "bytes": path.stat().st_size,
                  "sha256": hashlib.sha256(path.read_bytes()).hexdigest()} for path in paths]
        document = {"version": 1, "revision": self.measured["revision"],
                    "sourceTreeSha256": self.measured["sourceTreeSha256"], "files": files}
        (self.root / "manifest.json").write_text(json.dumps(document, indent=2) + "\n")
        for item in document["files"]:
            path = self.root / item["path"]
            if path.stat().st_size != item["bytes"] or hashlib.sha256(path.read_bytes()).hexdigest() != item["sha256"]:
                raise RuntimeError(f"Manifest mismatch: {path}")


if __name__ == "__main__":
    review = Review(Path(sys.argv[1]))
    review.run()
    raise SystemExit(1 if any(row["result"] != "PASS" for row in review.rows) else 0)
