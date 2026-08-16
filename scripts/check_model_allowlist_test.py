#!/usr/bin/env python3
"""Self-tests for check_model_allowlist.py.

The gate is only worth having if it bites on the four states that actually
shipped or nearly shipped: a provider with neither rates nor a reason (the state
87 providers sat in), a zero rate reaching the cost path, a typo'd reason code
reading as a decision nobody made, and rates hanging off the wrong continent.
Most tests drive one check function against a crafted provider dict, so no repo
state is read. The last class runs the gate against the real file — that is the
regression guard for the drift it was born from.

Run: python3 -m unittest discover -s scripts -t scripts -p 'check_model_allowlist*_test.py'
"""
import json
import os
import unittest

import check_model_allowlist as gate

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

PRICED = {
    "base_url": "https://api.example.com/v1",
    "source": "manual",
    "models": [{"model_id": "m", "context_cap_tokens": 1000, "input": 1.0, "cached_input": 0.1, "output": 2.0}],
}
REASONED = {"base_url": "https://api.example.com/v1", "source": "manual", "models": [], "unpriced_reason": "subscription_plan"}


class PricedXorReasoned(unittest.TestCase):
    def test_priced_alone_is_clean(self):
        self.assertEqual(gate.check_priced_xor_reasoned("p", PRICED), [])

    def test_reasoned_alone_is_clean(self):
        self.assertEqual(gate.check_priced_xor_reasoned("p", REASONED), [])

    def test_neither_is_the_uncurated_gap(self):
        problems = gate.check_priced_xor_reasoned("p", {"models": []})
        self.assertEqual(len(problems), 1)
        self.assertIn("uncurated gap", problems[0])

    def test_both_is_a_stale_reason_beside_real_rates(self):
        both = {**PRICED, "unpriced_reason": "cn_endpoint"}
        problems = gate.check_priced_xor_reasoned("p", both)
        self.assertEqual(len(problems), 1)
        self.assertIn("AND unpriced_reason", problems[0])

    def test_missing_models_key_counts_as_unpriced(self):
        self.assertEqual(gate.check_priced_xor_reasoned("p", {"unpriced_reason": "no_public_rates"}), [])


class ReasonVocabulary(unittest.TestCase):
    def test_every_valid_code_passes(self):
        for code in gate.VALID_REASONS:
            self.assertEqual(gate.check_reason_vocabulary("p", {"unpriced_reason": code}), [], code)

    def test_typo_is_rejected(self):
        problems = gate.check_reason_vocabulary("p", {"unpriced_reason": "local-runtime"})
        self.assertEqual(len(problems), 1)
        self.assertIn("unknown unpriced_reason", problems[0])

    def test_absent_reason_is_not_this_check_s_business(self):
        self.assertEqual(gate.check_reason_vocabulary("p", PRICED), [])


class ZeroRates(unittest.TestCase):
    def test_clean_rates_pass(self):
        self.assertEqual(gate.check_no_zero_rates("p", PRICED), [])

    def test_zero_input_is_caught(self):
        cfg = {"models": [{"model_id": "m", "input": 0, "cached_input": 0.1, "output": 2.0}]}
        self.assertEqual(len(gate.check_no_zero_rates("p", cfg)), 1)

    def test_zero_cached_read_is_caught(self):
        cfg = {"models": [{"model_id": "m", "input": 1.0, "cached_input": 0, "output": 2.0}]}
        problems = gate.check_no_zero_rates("p", cfg)
        self.assertEqual(len(problems), 1)
        self.assertIn("cached_input", problems[0])

    def test_all_three_zero_reports_all_three(self):
        cfg = {"models": [{"model_id": "m", "input": 0, "cached_input": 0, "output": 0}]}
        self.assertEqual(len(gate.check_no_zero_rates("p", cfg)), 3)

    def test_api_source_bare_ids_are_skipped(self):
        # api providers list ids as plain strings; their rates arrive at run time.
        self.assertEqual(gate.check_no_zero_rates("p", {"models": ["some/model"]}), [])


class RegionAgreement(unittest.TestCase):
    def test_international_priced_provider_is_clean(self):
        self.assertEqual(gate.check_region_agreement("kimi", {**PRICED, "base_url": "https://api.moonshot.ai/v1"}), [])

    def test_cn_endpoint_carrying_rates_is_the_wrong_continent_bug(self):
        cfg = {**PRICED, "base_url": "https://api.moonshot.cn/v1"}
        problems = gate.check_region_agreement("kimi", cfg)
        self.assertEqual(len(problems), 1)
        self.assertIn("international rule", problems[0])

    def test_cn_endpoint_needs_the_cn_reason(self):
        cfg = {"base_url": "https://open.bigmodel.cn/api/paas/v4", "models": [], "unpriced_reason": "gateway_passthrough"}
        problems = gate.check_region_agreement("bigmodel", cfg)
        self.assertEqual(len(problems), 1)
        self.assertIn("expected 'cn_endpoint'", problems[0])

    def test_cn_endpoint_with_cn_reason_is_clean(self):
        cfg = {"base_url": "https://api.moonshot.cn/v1", "models": [], "unpriced_reason": "cn_endpoint"}
        self.assertEqual(gate.check_region_agreement("moonshot", cfg), [])

    def test_provider_without_base_url_is_skipped(self):
        self.assertEqual(gate.check_region_agreement("vertex", {"models": [], "unpriced_reason": "deployment_scoped"}), [])


class ActivationFloor(unittest.TestCase):
    def test_sentinel_rates_must_declare_the_basis(self):
        cfg = {"models": [{"model_id": "local", "input": 0.000001, "cached_input": 0.000001, "output": 0.000001}]}
        problems = gate.check_floor_is_marked("vllm", cfg)
        self.assertEqual(len(problems), 1)
        self.assertIn("rate_basis", problems[0])

    def test_declared_floor_is_clean(self):
        cfg = {
            "rate_basis": gate.RATE_BASIS_FLOOR,
            "models": [{"model_id": "local", "input": 0.000001, "cached_input": 0.000001, "output": 0.000001}],
        }
        self.assertEqual(gate.check_floor_is_marked("vllm", cfg), [])

    def test_real_rates_may_not_claim_to_be_a_floor(self):
        cfg = {**PRICED, "rate_basis": gate.RATE_BASIS_FLOOR}
        problems = gate.check_floor_is_marked("anthropic", cfg)
        self.assertEqual(len(problems), 1)
        self.assertIn("real-looking rates", problems[0])


class LegendParity(unittest.TestCase):
    def test_legend_matching_the_vocabulary_is_clean(self):
        doc = {"unpriced_reasons": {"_readme": ["ignored"], **{c: "why" for c in gate.VALID_REASONS}}}
        self.assertEqual(gate.check_legend_covers_vocabulary(doc), [])

    def test_missing_legend_entry_is_caught(self):
        codes = sorted(gate.VALID_REASONS)[1:]
        doc = {"unpriced_reasons": {c: "why" for c in codes}}
        problems = gate.check_legend_covers_vocabulary(doc)
        self.assertEqual(len(problems), 1)
        self.assertIn("missing", problems[0])

    def test_legend_documenting_an_unknown_code_is_caught(self):
        doc = {"unpriced_reasons": {**{c: "why" for c in gate.VALID_REASONS}, "invented": "why"}}
        problems = gate.check_legend_covers_vocabulary(doc)
        self.assertEqual(len(problems), 1)
        self.assertIn("unknown codes", problems[0])


class RealFile(unittest.TestCase):
    """The regression guard: the committed allowlist must satisfy every check."""

    @classmethod
    def setUpClass(cls):
        with open(os.path.join(REPO_ROOT, "scripts", "model-library-allowlist.json"), encoding="utf-8") as handle:
            cls.doc = json.load(handle)

    def test_every_provider_is_priced_or_reasoned(self):
        problems = []
        for name, cfg in self.doc["providers"].items():
            problems.extend(gate.check_priced_xor_reasoned(name, cfg))
        self.assertEqual(problems, [])

    def test_no_zero_rates_anywhere(self):
        problems = []
        for name, cfg in self.doc["providers"].items():
            problems.extend(gate.check_no_zero_rates(name, cfg))
        self.assertEqual(problems, [])

    def test_no_cn_endpoint_carries_rates(self):
        problems = []
        for name, cfg in self.doc["providers"].items():
            problems.extend(gate.check_region_agreement(name, cfg))
        self.assertEqual(problems, [])

    def test_legend_and_vocabulary_agree(self):
        self.assertEqual(gate.check_legend_covers_vocabulary(self.doc), [])

    def test_kimi_and_qwen_point_at_their_international_endpoints(self):
        # The rates come from the international price pages; before M168 the
        # skeleton generator overwrote both with the mainland-China endpoint.
        self.assertIn("moonshot.ai", self.doc["providers"]["kimi"]["base_url"])
        self.assertIn("dashscope-intl", self.doc["providers"]["qwen"]["base_url"])

    def test_no_provider_still_carries_the_old_uniform_note(self):
        stale = [n for n, c in self.doc["providers"].items() if "not yet priced" in (c.get("note") or "")]
        self.assertEqual(stale, [])


class SeededRates(unittest.TestCase):
    """Regression guard on the two silent seeder failures M168 fixed.

    Both are asserted against the committed SQL rather than against the helper
    functions, because the failure mode of each was *emitting the wrong row*,
    not returning the wrong number — and the emitted row is what bills someone.
    """

    SEED = os.path.join(REPO_ROOT, "samples", "fixtures", "model-library", "seed.sql")

    @classmethod
    def setUpClass(cls):
        with open(cls.SEED, encoding="utf-8") as handle:
            cls.sql = handle.read()

    def _rows(self, provider):
        """(model_id, input, cached, output) for every seeded row of a provider."""
        rows = []
        for line in self.sql.splitlines():
            marker = f"'{provider}', '"
            if marker not in line:
                continue
            _, _, rest = line.partition(marker)
            model_id, _, tail = rest.partition("', ")
            nums = [p.strip() for p in tail.split(",")]
            # context_cap_tokens, input, cached_input, output, created_at, updated_at
            rows.append((model_id, int(nums[1]), int(nums[2]), int(nums[3])))
        return rows

    def test_currency_prefixed_provider_seeds_every_allowlisted_model(self):
        # Synthetic ships "$0.000001". Number() read that as NaN, the model was
        # skipped with a warning, and the provider seeded short — silently.
        rows = self._rows("synthetic")
        self.assertEqual(len(rows), 4, f"expected 4 synthetic rows, got {[r[0] for r in rows]}")
        for model_id, inp, cached, out in rows:
            self.assertGreater(inp, 0, model_id)
            self.assertGreater(cached, 0, model_id)
            self.assertGreater(out, 0, model_id)

    def test_zero_cache_read_seeded_as_input_not_as_free(self):
        # OVHcloud publishes "0" for cache reads meaning "no discount offered".
        # Seeding that verbatim would zero-rate every cached read.
        rows = self._rows("ovh")
        self.assertTrue(rows, "no ovh rows seeded")
        for model_id, inp, cached, _ in rows:
            self.assertGreater(cached, 0, f"{model_id}: cached read seeded at zero")
            self.assertEqual(cached, inp, f"{model_id}: expected cache read to fall back to the input rate")

    def test_no_seeded_row_carries_a_zero_rate(self):
        zeros = []
        for line in self.sql.splitlines():
            if not line.startswith("  '"):
                continue
            parts = [p.strip() for p in line.split(",")]
            if len(parts) < 8:
                continue
            try:
                inp, cached, out = int(parts[4]), int(parts[5]), int(parts[6])
            except ValueError:
                continue
            if 0 in (inp, cached, out):
                zeros.append(parts[2])
        self.assertEqual(zeros, [], "zero rates must never enter the cost path")

    def test_local_runtime_floor_is_nonzero_after_nanos_rounding(self):
        # The floor is 1e-6 USD/Mtok; toNanos rounds it to 1000. A smaller
        # sentinel would round to 0 and trip the zero-rate invariant.
        for provider in ("vllm", "ollama", "sglang"):
            rows = self._rows(provider)
            self.assertEqual(len(rows), 1, provider)
            _, inp, cached, out = rows[0]
            self.assertEqual((inp, cached, out), (1000, 1000, 1000), provider)


if __name__ == "__main__":
    unittest.main()
