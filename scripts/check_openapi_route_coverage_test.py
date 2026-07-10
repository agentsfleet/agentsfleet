#!/usr/bin/env python3
"""Self-tests for check_openapi_route_coverage.py.

The gate is only worth having if it actually fails on an undocumented route,
refuses to silently skip a variant it cannot parse, and notices when a
carve-out goes dead. Most tests drive `collect_violations` against a crafted
routes.zig body with injected carve-out tables, so no repo state is mutated
and the real allowlists never bleed in. The last class runs the gate against
the real tree — that is the regression guard for the drift it was born from.

Run: python3 -m unittest discover -s scripts -t scripts -p 'check_openapi_route_coverage*_test.py'
"""
import json
import os
import unittest

import check_openapi_route_coverage as gate

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Empty tables: a crafted registry serves none of the real runner-plane routes,
# so the real INTERNAL_ROUTE_ALLOW would (correctly) report every entry stale.
EMPTY = gate.CarveOuts(internal={}, pathless={}, non_v1={})


def routes_zig(*variant_lines: str) -> str:
    """A minimal routes.zig whose union body carries exactly these lines."""
    return gate.ROUTE_UNION_HEADER + "\n" + "\n".join(variant_lines) + "\n" + gate.ROUTE_UNION_FOOTER + "\n"


def spec(*paths: str) -> dict:
    return {"paths": {p: {"get": {}} for p in paths}}


class TestUndocumentedRoutes(unittest.TestCase):
    def test_flags_a_served_route_missing_from_the_spec(self):
        text = routes_zig("    admin_models, // GET + POST /v1/admin/models")
        violations, _ = gate.collect_violations(text, spec("/v1/healthz"), EMPTY)
        self.assertEqual(len(violations), 1)
        self.assertIn("UNDOCUMENTED ROUTE", violations[0])
        self.assertIn("/v1/admin/models", violations[0])
        self.assertIn("admin_models", violations[0])

    def test_passes_when_the_served_route_is_documented(self):
        text = routes_zig("    admin_models, // GET + POST /v1/admin/models")
        violations, served = gate.collect_violations(text, spec("/v1/admin/models"), EMPTY)
        self.assertEqual(violations, [])
        self.assertEqual(served, 1)

    def test_param_names_need_not_match_between_routes_and_spec(self):
        text = routes_zig("    f: []const u8, // GET /v1/workspaces/{ws}/fleets/{id}")
        violations, _ = gate.collect_violations(
            text, spec("/v1/workspaces/{workspace_id}/fleets/{fleet_id}"), EMPTY
        )
        self.assertEqual(violations, [])

    def test_allowlisted_internal_route_is_not_flagged(self):
        carve = gate.CarveOuts(internal={"/v1/runners/me/reports": "runner self-plane"}, pathless={}, non_v1={})
        text = routes_zig("    runner_report, // POST /v1/runners/me/reports")
        violations, _ = gate.collect_violations(text, spec(), carve)
        self.assertEqual(violations, [])

    def test_colon_op_alternation_expands_to_both_paths(self):
        text = routes_zig("    a: R, // POST /v1/workspaces/{ws}/approvals/{gate_id}:approve|:deny")
        violations, served = gate.collect_violations(text, spec("/v1/workspaces/{w}/approvals/{g}:approve"), EMPTY)
        self.assertEqual(served, 2)
        self.assertEqual(len(violations), 1)
        self.assertIn(":deny", violations[0])

    def test_route_family_reference_in_prose_is_not_a_served_path(self):
        """`kept out of /v1/webhooks/` names a family, not a route."""
        text = routes_zig(
            "    // Clerk signup event — /v1/auth/identity-events/clerk. Kept out of /v1/webhooks/.",
            "    auth_identity_event_clerk,",
        )
        violations, served = gate.collect_violations(text, spec("/v1/auth/identity-events/clerk"), EMPTY)
        self.assertEqual(violations, [])
        self.assertEqual(served, 1)


class TestUnaccountedVariants(unittest.TestCase):
    def test_variant_with_no_path_comment_fails_rather_than_being_skipped(self):
        text = routes_zig("    brand_new_route, // does something, path unstated")
        violations, _ = gate.collect_violations(text, spec(), EMPTY)
        self.assertEqual(len(violations), 1)
        self.assertIn("UNACCOUNTED VARIANT", violations[0])
        self.assertIn("brand_new_route", violations[0])

    def test_pathless_variant_is_checked_against_its_mapped_path(self):
        carve = gate.CarveOuts(internal={}, pathless={"create_workspace": "/v1/workspaces"}, non_v1={})
        text = routes_zig("    create_workspace,")
        clean, _ = gate.collect_violations(text, spec("/v1/workspaces"), carve)
        self.assertEqual(clean, [])
        dirty, _ = gate.collect_violations(text, spec(), carve)
        self.assertEqual(len(dirty), 1)
        self.assertIn("UNDOCUMENTED ROUTE", dirty[0])
        self.assertIn("/v1/workspaces", dirty[0])

    def test_a_differently_indented_variant_is_still_seen(self):
        # greptile P2: a variant at anything other than 4 spaces (a re-indent, a
        # tab) must not silently vanish from the scan — an unseen variant never
        # reaches the unaccounted hard-failure, so a served route could ship
        # uncovered. The match is indentation-agnostic.
        for indent in ("  ", "      ", "\t", "\t  "):
            text = routes_zig(f"{indent}brand_new_route, // no path stated")
            violations, _ = gate.collect_violations(text, spec(), EMPTY)
            self.assertEqual(len(violations), 1, f"indent {indent!r} was dropped")
            self.assertIn("UNACCOUNTED VARIANT", violations[0])

    def test_non_v1_variant_is_out_of_remit(self):
        carve = gate.CarveOuts(internal={}, pathless={}, non_v1={"healthz": "k8s probe"})
        violations, served = gate.collect_violations(routes_zig("    healthz,"), spec(), carve)
        self.assertEqual(violations, [])
        self.assertEqual(served, 0)


class TestStaleCarveOuts(unittest.TestCase):
    def test_internal_allow_entry_no_longer_served_is_flagged(self):
        carve = gate.CarveOuts(internal={"/v1/gone": "retired"}, pathless={}, non_v1={})
        violations, _ = gate.collect_violations(routes_zig("    a, // GET /v1/kept"), spec("/v1/kept"), carve)
        self.assertEqual(violations, ["STALE INTERNAL_ROUTE_ALLOW: /v1/gone is no longer served"])

    def test_pathless_entry_that_gained_a_comment_is_flagged(self):
        carve = gate.CarveOuts(internal={}, pathless={"create_workspace": "/v1/workspaces"}, non_v1={})
        text = routes_zig("    create_workspace, // POST /v1/workspaces")
        violations, _ = gate.collect_violations(text, spec("/v1/workspaces"), carve)
        self.assertTrue(any("now carries a path comment" in v for v in violations), violations)

    def test_pathless_entry_for_a_deleted_variant_is_flagged(self):
        carve = gate.CarveOuts(internal={}, pathless={"deleted_variant": "/v1/gone"}, non_v1={})
        violations, _ = gate.collect_violations(routes_zig("    a, // GET /v1/kept"), spec("/v1/kept"), carve)
        self.assertTrue(any("STALE PATHLESS_VARIANT_PATHS: deleted_variant" in v for v in violations), violations)

    def test_non_v1_entry_for_a_deleted_variant_is_flagged(self):
        carve = gate.CarveOuts(internal={}, pathless={}, non_v1={"gone": "retired probe"})
        violations, _ = gate.collect_violations(routes_zig("    a, // GET /v1/kept"), spec("/v1/kept"), carve)
        self.assertTrue(any("STALE NON_V1_VARIANTS: gone" in v for v in violations), violations)


class TestAgainstTheRealRepo(unittest.TestCase):
    def _real(self):
        routes = gate.read_file(os.path.join(REPO_ROOT, gate.ROUTES_PATH))
        spec_text = gate.read_file(os.path.join(REPO_ROOT, gate.SPEC_PATH))
        self.assertIsNotNone(routes, f"{gate.ROUTES_PATH} missing")
        self.assertIsNotNone(spec_text, f"{gate.SPEC_PATH} missing — run `make openapi`")
        return routes, json.loads(spec_text)

    def test_head_is_clean(self):
        routes, spec_json = self._real()
        violations, served = gate.collect_violations(routes, spec_json)
        self.assertEqual(violations, [], "route coverage dirty at HEAD:\n" + "\n".join(violations))
        self.assertGreater(served, 50, "suspiciously few served routes parsed — did routes.zig move?")

    def test_admin_models_is_documented_at_head(self):
        """The exact drift this gate exists to prevent (§2's regression guard)."""
        _, spec_json = self._real()
        paths = spec_json["paths"]
        verbs = {"get", "post", "patch", "delete"}
        self.assertIn("/v1/admin/models", paths)
        self.assertIn("/v1/admin/models/{uid}", paths)
        self.assertEqual({"get", "post"}, set(paths["/v1/admin/models"]) & verbs)
        self.assertEqual({"patch", "delete"}, set(paths["/v1/admin/models/{uid}"]) & verbs)


if __name__ == "__main__":
    unittest.main()
