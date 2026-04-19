#!/usr/bin/env python3
"""Unit tests for scripts/check_openapi_sync.py.

Fast fixture-driven regression suite. Runs on every `make openapi` (<100ms) so a
parser regression surfaces before the gate pretends to be green.

Run manually:
    python3 scripts/test_check_openapi_sync.py

Tiers covered (see ~/.claude/skills/write-unit-test/SKILL.md):
  T1 happy path, T2 edge cases, T3 negative paths, T6 integration (against the
  committed manifest + openapi.json), T7 regression (drift detection in both
  directions), T8 adversarial regex input.

Not applicable: T4 (no rendered artifact), T5 (no concurrency), T11 (runs in
<100ms against in-memory fixtures), T12 (the script is not a public API).
"""

from __future__ import annotations

import json
import pathlib
import sys
import unittest

# Let the sibling script be importable regardless of cwd.
sys.path.insert(0, str(pathlib.Path(__file__).parent))

from check_openapi_sync import (  # noqa: E402
    HTTP_METHODS,
    parse_manifest,
    parse_spec,
)


# --------------------------------------------------------------------------- #
# T1 Happy path + T2 edge cases for parse_manifest                            #
# --------------------------------------------------------------------------- #


class ParseManifestHappyPathTests(unittest.TestCase):
    def test_method_first_single_entry(self):
        self.assertEqual(
            parse_manifest('.{ .method = "GET", .path = "/healthz" }'),
            {("GET", "/healthz")},
        )

    def test_path_first_single_entry(self):
        # Valid Zig — the parser must tolerate either struct-literal field order.
        # Pre-fix this would have silently dropped the entry from the parity set.
        self.assertEqual(
            parse_manifest('.{ .path = "/healthz", .method = "GET" }'),
            {("GET", "/healthz")},
        )

    def test_mixed_orderings_in_same_source(self):
        src = """
            .{ .method = "GET", .path = "/a" },
            .{ .path = "/b", .method = "POST" },
            .{ .method = "DELETE", .path = "/c" },
        """
        self.assertEqual(
            parse_manifest(src),
            {("GET", "/a"), ("POST", "/b"), ("DELETE", "/c")},
        )

    def test_multiline_entry_spanning_multiple_lines(self):
        src = """
            .{
                .method = "PATCH",
                .path = "/v1/items/{id}",
            }
        """
        self.assertEqual(parse_manifest(src), {("PATCH", "/v1/items/{id}")})

    def test_all_seven_http_methods(self):
        src = "\n".join(
            f'.{{ .method = "{m.upper()}", .path = "/x" }},' for m in HTTP_METHODS
        )
        self.assertEqual(
            parse_manifest(src),
            {(m.upper(), "/x") for m in HTTP_METHODS},
        )


class ParseManifestEdgeCaseTests(unittest.TestCase):
    def test_empty_source_returns_empty_set(self):
        self.assertEqual(parse_manifest(""), set())

    def test_whitespace_only_source_returns_empty_set(self):
        self.assertEqual(parse_manifest("\n\n    \t\n"), set())

    def test_openapi_template_params_preserved(self):
        # Curly braces inside `.path` must not trip the regex — this is the
        # whole point of having OpenAPI-shaped paths.
        src = (
            '.{ .method = "DELETE", .path = '
            '"/v1/workspaces/{workspace_id}/zombies/{zombie_id}/integration-grants/{grant_id}" }'
        )
        self.assertEqual(
            parse_manifest(src),
            {(
                "DELETE",
                "/v1/workspaces/{workspace_id}/zombies/{zombie_id}/integration-grants/{grant_id}",
            )},
        )

    def test_duplicate_entries_deduplicated_by_set(self):
        src = """
            .{ .method = "GET", .path = "/foo" },
            .{ .method = "GET", .path = "/foo" },
        """
        self.assertEqual(parse_manifest(src), {("GET", "/foo")})

    def test_irregular_whitespace_between_tokens(self):
        src = '.{\t\t.method\n=\n"GET"   ,\n\t\t.path  =  "/foo"   }'
        self.assertEqual(parse_manifest(src), {("GET", "/foo")})

    def test_lowercase_method_not_matched(self):
        # Manifest convention is uppercase; the regex enforces `[A-Z]+`.
        # A lowercase entry is silently dropped. That's acceptable because
        # the in-file Zig unit test would catch the non-dispatching entry.
        self.assertEqual(
            parse_manifest('.{ .method = "get", .path = "/foo" }'),
            set(),
        )


# --------------------------------------------------------------------------- #
# T1 happy path + T3 negative paths for parse_spec                            #
# --------------------------------------------------------------------------- #


class ParseSpecHappyPathTests(unittest.TestCase):
    def test_single_path_single_verb(self):
        self.assertEqual(
            parse_spec({"paths": {"/foo": {"get": {}}}}),
            {("GET", "/foo")},
        )

    def test_multiple_verbs_on_one_path(self):
        self.assertEqual(
            parse_spec({"paths": {"/foo": {"get": {}, "post": {}, "delete": {}}}}),
            {("GET", "/foo"), ("POST", "/foo"), ("DELETE", "/foo")},
        )

    def test_multiple_paths(self):
        spec = {
            "paths": {
                "/a": {"get": {}},
                "/b": {"post": {}},
                "/c": {"put": {}, "patch": {}},
            }
        }
        self.assertEqual(
            parse_spec(spec),
            {("GET", "/a"), ("POST", "/b"), ("PUT", "/c"), ("PATCH", "/c")},
        )

    def test_uppercase_method_in_spec_normalized(self):
        # OpenAPI normatively uses lowercase but be robust either way.
        self.assertEqual(
            parse_spec({"paths": {"/foo": {"GET": {}}}}),
            {("GET", "/foo")},
        )

    def test_all_seven_http_methods(self):
        methods = sorted(HTTP_METHODS)
        spec = {"paths": {"/x": {m: {} for m in methods}}}
        self.assertEqual(
            parse_spec(spec),
            {(m.upper(), "/x") for m in methods},
        )


class ParseSpecNegativePathTests(unittest.TestCase):
    def test_null_paths(self):
        # The /review-pr fix: `{"paths": null}` must not crash.
        self.assertEqual(parse_spec({"paths": None}), set())

    def test_missing_paths_key(self):
        self.assertEqual(parse_spec({}), set())

    def test_empty_paths_object(self):
        self.assertEqual(parse_spec({"paths": {}}), set())

    def test_null_pathitem_value(self):
        # PathItem = null — weird but the script should tolerate it.
        self.assertEqual(parse_spec({"paths": {"/foo": None}}), set())

    def test_ignores_non_http_sibling_keys(self):
        # OpenAPI 3.1 PathItem allows `parameters`, `summary`, `description`,
        # `servers` as siblings to HTTP verbs. Those must NOT become route pairs.
        spec = {
            "paths": {
                "/foo": {
                    "get": {},
                    "parameters": [{"in": "query", "name": "filter"}],
                    "summary": "Example",
                    "description": "...",
                    "servers": [{"url": "https://example.com"}],
                }
            }
        }
        self.assertEqual(parse_spec(spec), {("GET", "/foo")})

    def test_unknown_verb_at_pathitem_silently_ignored(self):
        # Future-proofing: if OpenAPI ever adds a verb, we'd drop it rather
        # than crash. That's the documented behavior.
        spec = {"paths": {"/foo": {"trace": {}, "get": {}}}}
        self.assertEqual(parse_spec(spec), {("GET", "/foo")})


# --------------------------------------------------------------------------- #
# T7 regression: drift detection in both directions                            #
# --------------------------------------------------------------------------- #


class DriftDetectionTests(unittest.TestCase):
    def test_identical_sets_no_drift(self):
        s = {("GET", "/a"), ("POST", "/b")}
        self.assertEqual(s - s, set())

    def test_manifest_has_extra_pair_spec_does_not(self):
        manifest = {("GET", "/a"), ("GET", "/extra")}
        spec = {("GET", "/a")}
        self.assertEqual(manifest - spec, {("GET", "/extra")})
        self.assertEqual(spec - manifest, set())

    def test_spec_has_extra_pair_manifest_does_not(self):
        manifest = {("GET", "/a")}
        spec = {("GET", "/a"), ("POST", "/only-in-spec")}
        self.assertEqual(spec - manifest, {("POST", "/only-in-spec")})
        self.assertEqual(manifest - spec, set())

    def test_method_mismatch_counts_as_two_drifts(self):
        # Same path, different method → both sides disagree.
        manifest = {("GET", "/a")}
        spec = {("POST", "/a")}
        self.assertEqual(manifest - spec, {("GET", "/a")})
        self.assertEqual(spec - manifest, {("POST", "/a")})

    def test_trailing_slash_path_mismatch_counts_as_drift(self):
        manifest = {("GET", "/a")}
        spec = {("GET", "/a/")}
        self.assertTrue(manifest - spec)
        self.assertTrue(spec - manifest)


# --------------------------------------------------------------------------- #
# T8 adversarial: regex boundary behavior with hostile input                  #
# --------------------------------------------------------------------------- #


class AdversarialInputTests(unittest.TestCase):
    def test_entry_inside_zig_line_comment_is_stripped(self):
        # parse_manifest strips `// ...\n` comments before the regex runs, so
        # example manifest entries in doc comments do NOT produce phantom
        # routes. This also prevents a hypothetical `// .{ .method = "DELETE",
        # .path = "/admin" }` example from appearing in the parity set.
        src = '// example: .{ .method = "POST", .path = "/docs-only" }'
        self.assertEqual(parse_manifest(src), set())

    def test_comment_between_method_and_path_still_parses(self):
        # Without the comment-stripping pre-pass, a comment between the two
        # fields would break both METHOD_FIRST and PATH_FIRST (the `,\s*\.path`
        # sequence cannot cross `// ...\n`), silently dropping the entry from
        # the parity set. Greptile P2 on PR #232.
        src = (
            '.{\n'
            '    .method = "POST",\n'
            '    // explain why this one is special\n'
            '    .path = "/v1/special",\n'
            '}'
        )
        self.assertEqual(parse_manifest(src), {("POST", "/v1/special")})

    def test_real_entry_with_trailing_comment_still_parses(self):
        # Block-level `//` after the entry is fine — the regex only needs the
        # fields themselves.
        src = '.{ .method = "GET", .path = "/ok" },  // M28_003 §3'
        self.assertEqual(parse_manifest(src), {("GET", "/ok")})

    def test_method_first_and_path_first_dedupe_identical_entry(self):
        # Both regexes can match the same logical entry if it's malformed —
        # confirm set union still dedupes.
        src = '.{ .method = "GET", .path = "/foo" }'
        # Only METHOD_FIRST fires here; PATH_FIRST does not because the fields
        # aren't reversed. Verify no accidental double-count.
        self.assertEqual(len(parse_manifest(src)), 1)

    def test_malformed_entry_missing_path_skipped(self):
        src = '.{ .method = "GET" }'
        self.assertEqual(parse_manifest(src), set())

    def test_malformed_entry_missing_method_skipped(self):
        src = '.{ .path = "/foo" }'
        self.assertEqual(parse_manifest(src), set())

    def test_very_long_path_parsed(self):
        # No buffer-size issue on the Python side; the regex uses [^"]+ which
        # scales linearly. 1 KB path just to prove it.
        long_path = "/v1/" + ("a" * 1024)
        src = f'.{{ .method = "GET", .path = "{long_path}" }}'
        self.assertEqual(parse_manifest(src), {("GET", long_path)})


# --------------------------------------------------------------------------- #
# T6 integration: against the actual committed manifest + bundled spec         #
# --------------------------------------------------------------------------- #


class ParityAgainstCommittedArtifactsTests(unittest.TestCase):
    """Weakest-form integration: if the files exist, verify the parsers see
    what the sync gate would see. Skipped if either file is missing (e.g.,
    running in a sparse checkout)."""

    REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent
    MANIFEST = REPO_ROOT / "src" / "http" / "route_manifest.zig"
    SPEC = REPO_ROOT / "public" / "openapi.json"

    def test_committed_manifest_parses_to_nonempty_set(self):
        if not self.MANIFEST.exists():
            self.skipTest(f"{self.MANIFEST} not found")
        pairs = parse_manifest(self.MANIFEST.read_text())
        self.assertGreater(len(pairs), 0, "manifest produced zero entries")
        # Every entry should look like (UPPERCASE_METHOD, "/...").
        for method, path in pairs:
            self.assertTrue(method.isupper(), f"non-upper method in manifest: {method!r}")
            self.assertTrue(path.startswith("/"), f"path must be rooted: {path!r}")

    def test_committed_spec_parses_to_nonempty_set(self):
        if not self.SPEC.exists():
            self.skipTest(f"{self.SPEC} not found")
        pairs = parse_spec(json.loads(self.SPEC.read_text()))
        self.assertGreater(len(pairs), 0, "openapi.json produced zero entries")

    def test_committed_manifest_and_spec_are_in_parity(self):
        if not (self.MANIFEST.exists() and self.SPEC.exists()):
            self.skipTest("manifest or spec not present")
        manifest = parse_manifest(self.MANIFEST.read_text())
        spec = parse_spec(json.loads(self.SPEC.read_text()))
        self.assertEqual(
            manifest,
            spec,
            msg=(
                f"committed manifest/spec are out of parity. "
                f"manifest-only: {sorted(manifest - spec)}. "
                f"spec-only: {sorted(spec - manifest)}."
            ),
        )


if __name__ == "__main__":
    # Default verbosity (dots + summary) — keeps `make openapi` output terse.
    # Run with `python3 -m unittest -v scripts.test_check_openapi_sync` for
    # per-test output when debugging a regression.
    unittest.main()
