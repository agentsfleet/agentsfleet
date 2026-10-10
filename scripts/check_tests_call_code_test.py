#!/usr/bin/env python3
"""The check finds a test reading a repository file, and only that.

    python3 scripts/check_tests_call_code_test.py
"""

from __future__ import annotations

import contextlib
import io
import sys
import unittest

sys.dont_write_bytecode = True

import check_tests_call_code as checker

# Repository files a seeded test reads; they exist and are tracked.
SUBJECTS = {
    "cli/src/commands/login.ts": "export const login = 1;\n",
    "docs/AUTH.md": "# Auth\n",
    "deploy/baremetal/deploy.sh": "#!/usr/bin/env bash\n",
    "make/quality.mk": "lint-all:\n",
    "cli/test/fixtures/reply.json": "{}\n",
    "tests/fixtures/fleetbundle/reviewer/SKILL.md": "---\nname: reviewer\n---\n",
}

SEEDED = {
    "ts": ("cli/test/login.unit.test.ts", 'import { readFileSync } from "node:fs";\nimport { join } from "node:path";\nconst SRC = join(import.meta.dir, "..", "src", "commands");\nconst text = readFileSync(join(SRC, "login.ts"), "utf8");\n', 4),
    "rust": ("rustd/crates/afd_auth/tests/doc.rs", 'const PAGE: &str = include_str!("../../../../docs/AUTH.md");\n', 1),
    "shell": ("deploy/baremetal/deploy_test.sh", 'SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"\nif grep -q \\\n  "HEALTH" "$SCRIPT_DIR/deploy.sh"; then echo ok; fi\n', 2),
    "python": ("scripts/wiring_test.py", 'from pathlib import Path\nROOT = Path(__file__).resolve().parents[1]\nmakefile = (ROOT / "make/quality.mk").read_text(encoding="utf-8")\n', 3),
}


def lint(files: dict[str, str]) -> tuple[int, str]:
    with checker.lib.fixture_repository({**SUBJECTS, **files}) as root:
        out = io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(io.StringIO()):
            code = checker.main([checker.lib.ROOT_FLAG, str(root)])
        return code, out.getvalue()


class SeededReads(unittest.TestCase):
    def test_a_seeded_source_read_fails_in_each_language(self) -> None:
        for language, (path, text, line) in SEEDED.items():
            with self.subTest(language=language):
                code, output = lint({path: text})
                self.assertEqual(checker.lib.EXIT_FINDINGS, code, output)
                self.assertIn(f"{path}:{line}: test reads ", output)
                self.assertIn(f"; {checker.MESSAGE}", output)

    def test_a_fixture_read_passes_in_each_language(self) -> None:
        fixtures = {
            "cli/test/reply.unit.test.ts": 'const reply = readFileSync(join(import.meta.dir, "fixtures", "reply.json"), "utf8");\n',
            "rustd/crates/afd_fleet_runtime/tests/corpus.rs": (
                'const CORPUS: &str = "../../../tests/fixtures/fleetbundle";\n'
                'let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CORPUS);\n'
                'let raw = std::fs::read_to_string(dir.join("reviewer/SKILL.md"));\n'
            ),
            "cli/test/reply_test.sh": 'SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"\ngrep -q x "$SCRIPT_DIR/fixtures/reply.json"\n',
            "scripts/reply_test.py": 'ROOT = Path(__file__).parent\nreply = (ROOT / "fixtures" / "reply.json").read_text()\n',
        }
        code, output = lint(fixtures)
        self.assertEqual(checker.lib.EXIT_CLEAN, code, output)


class NotReads(unittest.TestCase):
    """Each shape the matcher once flagged and must not."""

    def assert_clean(self, files: dict[str, str]) -> None:
        code, output = lint(files)
        self.assertEqual(checker.lib.EXIT_CLEAN, code, output)

    def test_a_file_the_test_made_in_a_temporary_directory(self) -> None:
        self.assert_clean({
            "cli/test/out.unit.test.ts": 'const dir = mkdtempSync(join(tmpdir(), "x-"));\nconst out = readFileSync(join(dir, "out.txt"), "utf8");\n',
            "cli/test/out_test.sh": 'work_dir="$(mktemp -d)"\ncalls="$work_dir/calls"\ngrep -q api "$calls"\n',
            "scripts/out_test.py": 'with tempfile.TemporaryDirectory() as d:\n    text = (Path(d) / "out.txt").read_text()\n',
        })

    def test_a_cache_the_harness_writes_names_nothing_git_tracks(self) -> None:
        self.assert_clean({"ui/packages/app/tests/e2e/smoke.spec.ts": 'const CACHE = path.join(process.cwd(), ".fixture-jwts.json");\nconst jwts = readFileSync(CACHE, "utf8");\n'})

    def test_a_read_named_only_in_a_comment(self) -> None:
        self.assert_clean({"cli/test/note.unit.test.ts": '// readFileSync(join(__dirname, "../src/commands/login.ts"))\nexport {};\n'})

    def test_a_production_module_reading_its_own_inputs(self) -> None:
        self.assert_clean({
            "cli/src/load.ts": 'const text = readFileSync(join(__dirname, "../package.json"), "utf8");\n',
            "rustd/crates/afd_obs/src/registry.rs": '#[cfg(test)]\nmod tests;\n\npub const CENSUS: &str = include_str!("../../../../docs/AUTH.md");\n',
        })

    def test_handing_the_subject_to_something_that_runs_it(self) -> None:
        self.assert_clean({
            "deploy/baremetal/run_test.sh": (
                'SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"\n'
                'source "$SCRIPT_DIR/deploy.sh"\n'
                'bash "$SCRIPT_DIR/deploy.sh" runner\n'
                'head -n 3 "$SCRIPT_DIR/deploy.sh" | PATH="$FAKE" bash\n'
                'sed "s|^A=.*|A=1|" "$SCRIPT_DIR/deploy.sh" >"$copy"\n'
                'output="$(bash "$SCRIPT_DIR/deploy.sh")"\n'
                'rg --quiet ok <<<"$output"\n'
            ),
            "cli/test/lane.unit.test.ts": 'await Bun.write(join(dir, "bunfig.toml"), Bun.file(join(CLI_ROOT, "bunfig.toml")));\n',
        })

    def test_a_read_spelled_inside_a_python_string_literal(self) -> None:
        self.assert_clean({
            "scripts/seeds_test.py": (
                'ROOT = Path(__file__).resolve().parents[1]\n'
                'SAMPLE = \'text = (ROOT / "make/quality.mk").read_text()\'\n'
                'LONG = """\nwith open(ROOT / "make/quality.mk") as f:\n    pass\n"""\n'
                'LABEL = f"{ROOT} / make/quality.mk .read_text()"\n'
            ),
        })

    def test_code_between_f_string_braces_is_still_a_read(self) -> None:
        text = 'ROOT = Path(__file__).resolve().parents[1]\nprint(f"{(ROOT / \'make/quality.mk\').read_text()}")\n'
        code, output = lint({"scripts/braces_test.py": text})
        self.assertIn("scripts/braces_test.py:2: test reads ", output)


class HelperAndRegionReads(unittest.TestCase):
    def test_a_shell_helper_handed_an_anchored_path_is_a_read(self) -> None:
        text = (
            'SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"\n'
            'DEPLOY="$SCRIPT_DIR/deploy.sh"\n'
            'require_literal() {\n  local file="$1"\n  grep -Fq "$2" "$file"\n}\n'
            'require_literal "$DEPLOY" "HEALTH"\n'
        )
        code, output = lint({"deploy/baremetal/literal_test.sh": text})
        self.assertIn("deploy/baremetal/literal_test.sh:7: test reads ", output)

    def test_a_read_inside_a_cfg_test_module_is_a_read(self) -> None:
        text = 'pub fn f() {}\n\n#[cfg(test)]\nmod tests {\n    const PAGE: &str = include_str!("../../../../docs/AUTH.md");\n}\n'
        code, output = lint({"rustd/crates/afd_tenant/src/access.rs": text})
        self.assertIn("rustd/crates/afd_tenant/src/access.rs:5: test reads ", output)

    def test_an_unknown_argument_is_a_usage_error(self) -> None:
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(checker.lib.EXIT_USAGE, checker.main(["--bogus"]))


if __name__ == "__main__":
    unittest.main()
