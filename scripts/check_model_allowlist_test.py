#!/usr/bin/env python3
"""The allowlist checker refuses each malformed row it exists to catch.

    python3 scripts/check_model_allowlist_test.py
"""

from __future__ import annotations

import contextlib
import copy
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True

import check_model_allowlist as checker

MANUAL_ROW = {
    "model_id": "fixture-model",
    "context_cap_tokens": 128000,
    "input": 3.0,
    "cached_input": 0.3,
    "output": 15.0,
}

CLEAN_DOCUMENT = {
    "unpriced_reasons": {reason: "fixture legend" for reason in checker.VALID_REASONS},
    "providers": {
        "fixture-manual": {"source": checker.MANUAL_SOURCE, "models": [MANUAL_ROW]},
        "fixture-api": {"source": checker.API_SOURCE, "models": ["fixture-model"]},
        "fixture-unpriced": {"source": checker.MANUAL_SOURCE, "unpriced_reason": "no_public_rates"},
    },
}


def with_row(**changes: object) -> dict:
    """The clean document with the manual provider's one row changed."""
    document = copy.deepcopy(CLEAN_DOCUMENT)
    row = document["providers"]["fixture-manual"]["models"][0]
    for key, value in changes.items():
        if value is None:
            row.pop(key)
        else:
            row[key] = value
    return document


class MovedRulesRejectSeededViolations(unittest.TestCase):
    def test_moved_rules_reject_seeded_violations(self) -> None:
        seeded = {
            "cached_input 9.0 exceeds input": with_row(cached_input=9.0),
            "missing required key context_cap_tokens": with_row(context_cap_tokens=None),
            "context_cap_tokens must be a positive integer": with_row(context_cap_tokens=0),
            "input must be a non-negative number": with_row(input="3"),
            "unexpected key price": with_row(price=1),
        }
        for expected, document in seeded.items():
            with self.subTest(expected=expected):
                problems = checker.check_document(document)
                self.assertTrue(any(expected in problem for problem in problems), problems)

    def test_a_row_in_the_other_sources_shape_is_refused(self) -> None:
        document = copy.deepcopy(CLEAN_DOCUMENT)
        document["providers"]["fixture-api"]["models"] = [MANUAL_ROW]
        document["providers"]["fixture-manual"]["models"] = ["fixture-model"]
        problems = checker.check_document(document)
        self.assertTrue(any("inline rate row under source 'api'" in p for p in problems), problems)
        self.assertTrue(any("bare model id under source 'manual'" in p for p in problems), problems)

    def test_a_duplicate_id_and_an_unknown_source_are_refused(self) -> None:
        document = copy.deepcopy(CLEAN_DOCUMENT)
        document["providers"]["fixture-api"]["models"] = ["twice", "twice"]
        document["providers"]["fixture-unpriced"]["source"] = "scraped"
        problems = checker.check_document(document)
        self.assertTrue(any("duplicate model id twice" in p for p in problems), problems)
        self.assertTrue(any("source must be" in p for p in problems), problems)

    def test_the_clean_document_passes_and_main_exits_on_findings(self) -> None:
        self.assertEqual([], checker.check_document(CLEAN_DOCUMENT))
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "allowlist.json"
            path.write_text(json.dumps(CLEAN_DOCUMENT), encoding="utf-8")
            with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(0, checker.main(path))
                path.write_text(json.dumps(with_row(cached_input=9.0)), encoding="utf-8")
                self.assertEqual(1, checker.main(path))


if __name__ == "__main__":
    unittest.main()
