#!/usr/bin/env python3
"""Self-tests for the rate-parsing helpers in scripts/seed-models.mjs.

These three functions decide what a tenant is billed, and every one of them was
written in response to a real feed that broke the previous version:

  rate()          Synthetic ships "$0.000001"; Number() read that as NaN and the
                  model was silently skipped, seeding the provider short.
  cachedOrInput() OVHcloud publishes "0" for cache reads meaning "no discount".
                  Seeding that verbatim zero-rates every cached read.
  isBillable()    Number(null), Number("") and Number("0") are all 0 — finite,
                  so they pass a Number.isFinite check and seed a ZERO rate.
                  That is free inference under platform posture, silently.

They are exercised through `node` rather than reimplemented here, so the test
asserts the shipping code and not a Python copy of it. seed-models.mjs guards its
main block behind `import.meta.main`, which is what makes importing it free of
side effects (no allowlist read, no network, no psql).

Run: python3 -m unittest discover -s scripts -t scripts -p 'seed_models*_test.py'
"""
import json
import os
import subprocess
import unittest

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SEEDER = os.path.join(REPO_ROOT, "scripts", "seed-models.mjs")


def _call(fn: str, args: list) -> object:
    """Invoke one exported helper with JSON args and return its JSON result."""
    script = (
        f"import {{ {fn} }} from {json.dumps(SEEDER)};"
        f"const a = {json.dumps(args)};"
        f"const r = {fn}(...a);"
        "console.log(JSON.stringify(Number.isNaN(r) ? '__NaN__' : r));"
    )
    out = subprocess.run(
        ["node", "--input-type=module", "-e", script],
        capture_output=True,
        text=True,
        timeout=30,
        cwd=REPO_ROOT,
    )
    if out.returncode != 0:
        raise AssertionError(f"node failed for {fn}{args}: {out.stderr.strip()}")
    return json.loads(out.stdout.strip())


NAN = "__NaN__"


class RateParsing(unittest.TestCase):
    def test_plain_numeric_string(self):
        self.assertEqual(_call("rate", ["0.000001"]), 0.000001)

    def test_bare_number_passes_through(self):
        self.assertEqual(_call("rate", [1.5]), 1.5)

    def test_currency_prefix_is_stripped(self):
        # Synthetic's real shape. This is the one that silently seeded short.
        self.assertEqual(_call("rate", ["$0.000001"]), 0.000001)

    def test_other_currency_symbols_and_separators(self):
        self.assertEqual(_call("rate", ["£2.50"]), 2.5)
        self.assertEqual(_call("rate", ["€2.50"]), 2.5)
        self.assertEqual(_call("rate", ["1,250.5"]), 1250.5)
        self.assertEqual(_call("rate", [" 3.0 "]), 3.0)

    def test_non_numeric_stays_nan_so_the_caller_rejects_it(self):
        self.assertEqual(_call("rate", ["abc"]), NAN)

    def test_values_that_collapse_to_zero_are_caught_downstream_not_here(self):
        """rate() alone cannot reject these — isBillable() is the guard that does.

        Three real shapes collapse to 0 rather than NaN, because that is what
        JavaScript's Number() does: a bare currency symbol strips to the empty
        string, and null and "" are both 0. Each is finite, so a Number.isFinite
        check waves all three through. Seeding any of them means billing that
        model at nothing, which is why isBillable() exists.
        """
        for collapses_to_zero in ("$", "", None):
            self.assertEqual(_call("rate", [collapses_to_zero]), 0, repr(collapses_to_zero))
            self.assertFalse(
                _call("isBillable", [0, 15.0, 262144]),
                "a zero input rate must never be seeded",
            )


class CachedReadFallback(unittest.TestCase):
    def test_real_cache_rate_is_kept(self):
        self.assertEqual(_call("cachedOrInput", [0.3, 3.0]), 0.3)

    def test_published_zero_means_no_discount_not_free(self):
        # OVHcloud's real shape across its whole catalogue.
        self.assertEqual(_call("cachedOrInput", [0, 3.0]), 3.0)

    def test_absent_cache_rate_falls_back_to_input(self):
        self.assertEqual(_call("cachedOrInput", [NAN, 3.0]), 3.0)

    def test_negative_cache_rate_falls_back(self):
        self.assertEqual(_call("cachedOrInput", [-1, 3.0]), 3.0)

    def test_fallback_never_produces_a_free_cached_read(self):
        for cached in (0, -0.5):
            self.assertGreater(_call("cachedOrInput", [cached, 0.15]), 0)


class BillableGuard(unittest.TestCase):
    def test_normal_row_is_billable(self):
        self.assertTrue(_call("isBillable", [3.0, 15.0, 262144]))

    def test_zero_input_is_refused(self):
        self.assertFalse(_call("isBillable", [0, 15.0, 262144]))

    def test_zero_output_is_refused(self):
        self.assertFalse(_call("isBillable", [3.0, 0, 262144]))

    def test_zero_context_is_refused(self):
        self.assertFalse(_call("isBillable", [3.0, 15.0, 0]))

    def test_nan_is_refused(self):
        self.assertFalse(_call("isBillable", [NAN, 15.0, 262144]))
        self.assertFalse(_call("isBillable", [3.0, NAN, 262144]))

    def test_negative_is_refused(self):
        self.assertFalse(_call("isBillable", [-3.0, 15.0, 262144]))

    def test_the_activation_floor_is_still_billable(self):
        # Local runtimes seed 1e-6 USD/Mtok. The guard must not reject it, or
        # every local runtime silently loses its row.
        self.assertTrue(_call("isBillable", [0.000001, 0.000001, 262144]))


def _emit(no_transaction: bool) -> str:
    """Render the SQL `emit()` produces for one row, on either path."""
    row = {
        "provider": "acme",
        "model_id": "acme/m1",
        "context_cap_tokens": 128000,
        "input": 3_000_000_000,
        "cached": 300_000_000,
        "output": 15_000_000_000,
        "tier": None,
        "source_url": "https://example.invalid/pricing",
    }
    opts = {"no_transaction": True} if no_transaction else {}
    script = (
        f"import {{ emit }} from {json.dumps(SEEDER)};"
        f"const stamp = Object.assign('2026-08-17', {{ ms: 1755388800000 }});"
        f"process.stdout.write(emit([{json.dumps(row)}], {{ verified_at: '2026-08-17' }}, stamp, {json.dumps(opts)}));"
    )
    out = subprocess.run(
        ["node", "--input-type=module", "-e", script],
        capture_output=True, text=True, timeout=30, cwd=REPO_ROOT,
    )
    if out.returncode != 0:
        raise AssertionError(f"emit failed: {out.stderr.strip()}")
    return out.stdout


class GenerationBump(unittest.TestCase):
    """The apply path must move the catalogue generation with the rows.

    Without the bump, `rateAtRevision` keeps serving whatever a replica already
    cached — a CHANGED rate is never re-read, so every replica bills the old
    price until it restarts. New rows are unaffected (a miss loads), which is why
    this stayed invisible until someone changed a rate.
    """

    @classmethod
    def setUpClass(cls):
        cls.txn = _emit(no_transaction=False)
        cls.fixture = _emit(no_transaction=True)

    def test_transaction_path_locks_the_singleton_first(self):
        self.assertIn("BEGIN;", self.txn)
        self.assertIn("core.model_catalogue_revision WHERE id = 1 FOR UPDATE", self.txn)
        # Lock before any row write, per schema/410's documented protocol.
        self.assertLess(
            self.txn.index("FOR UPDATE"),
            self.txn.index("INSERT INTO core.model_library"),
            "the singleton lock must precede the catalogue writes",
        )

    def test_transaction_path_bumps_the_generation_after_the_rows(self):
        self.assertIn("SET revision = revision + 1", self.txn)
        self.assertLess(
            self.txn.index("INSERT INTO core.model_library"),
            self.txn.index("SET revision = revision + 1"),
            "the generation must be bumped after the rows it describes",
        )
        self.assertLess(self.txn.index("SET revision = revision + 1"), self.txn.index("COMMIT;"))

    def test_bump_stamps_the_same_timestamp_as_the_rows(self):
        self.assertIn("updated_at = 1755388800000", self.txn)

    def test_missing_singleton_raises_rather_than_updating_nothing(self):
        # Slot 410 seeds the row and nothing deletes it, so absence means the
        # schema was not applied — writing rates into a catalogue nothing can
        # invalidate is worse than failing.
        self.assertIn("IF NOT FOUND THEN", self.txn)
        self.assertIn("RAISE EXCEPTION", self.txn)
        self.assertIn("schema slot 410 not applied", self.txn)

    def test_fixture_path_omits_every_transactional_construct(self):
        # The Zig tests exec this file one statement at a time with no
        # surrounding transaction, where a FOR UPDATE lock is meaningless.
        for construct in ("BEGIN;", "COMMIT;", "FOR UPDATE", "DO $$", "RAISE EXCEPTION"):
            self.assertNotIn(construct, self.fixture, f"fixture path must not emit {construct!r}")

    def test_both_paths_still_write_the_row(self):
        for sql in (self.txn, self.fixture):
            self.assertIn("INSERT INTO core.model_library", sql)
            self.assertIn("ON CONFLICT (provider, model_id) DO UPDATE SET", sql)

    def test_committed_fixture_matches_the_no_transaction_shape(self):
        """Regression guard tying the assertion above to the real artifact."""
        path = os.path.join(REPO_ROOT, "samples", "fixtures", "model-library", "seed.sql")
        with open(path, encoding="utf-8") as handle:
            committed = handle.read()
        for construct in ("BEGIN;", "COMMIT;", "FOR UPDATE", "DO $$"):
            self.assertNotIn(construct, committed)


class ImportIsSideEffectFree(unittest.TestCase):
    def test_importing_the_seeder_does_not_run_main(self):
        """The `import.meta.main` guard is what makes every test above possible."""
        out = subprocess.run(
            ["node", "--input-type=module", "-e",
             f"const m = await import({json.dumps(SEEDER)});"
             "console.log(Object.keys(m).sort().join(','));"],
            capture_output=True, text=True, timeout=30, cwd=REPO_ROOT,
        )
        self.assertEqual(out.returncode, 0, out.stderr)
        self.assertEqual(out.stdout.strip(), "cachedOrInput,emit,isBillable,rate")
        # main would print "→ N allowlisted rows across ..." and hit the network.
        self.assertNotIn("allowlisted rows", out.stdout)


if __name__ == "__main__":
    unittest.main()
