#!/usr/bin/env python3
"""Regression tests for public documentation checks."""

from pathlib import Path
import sys
import unittest

sys.dont_write_bytecode = True

import check_documentation_rules as checker


VALID_OPENAPI = """{
  "paths": {
    "/v1/fleets": {
      "get": {
        "summary": "List fleets",
        "description": "Returns fleets visible to the caller."
      }
    }
  }
}
"""


class OpenApiTextTests(unittest.TestCase):
    def lint(self, text: str) -> list[str]:
        return checker.lint_openapi_document(Path("public/openapi.json"), text)

    def test_accepts_short_public_text_and_required_versions(self) -> None:
        self.assertEqual([], self.lint(VALID_OPENAPI))

    def test_rejects_banned_word(self) -> None:
        text = VALID_OPENAPI.replace("Returns fleets", "Returns persisted fleets")
        self.assertTrue(any("DOC-05" in issue for issue in self.lint(text)))

    def test_rejects_sentence_over_twenty_five_words(self) -> None:
        long_text = " ".join(f"word{index}" for index in range(26)) + "."
        text = VALID_OPENAPI.replace("Returns fleets visible to the caller.", long_text)
        self.assertTrue(any("DOC-02" in issue for issue in self.lint(text)))

    def test_rejects_price_in_public_prose(self) -> None:
        text = VALID_OPENAPI.replace("Returns fleets visible to the caller.", "This action costs $5.")
        self.assertTrue(any("DOC-23" in issue for issue in self.lint(text)))

    def test_rejects_internal_storage_detail(self) -> None:
        text = VALID_OPENAPI.replace("Returns fleets visible to the caller.", "Reads rows from core.fleets.")
        self.assertTrue(any("DOC-22" in issue for issue in self.lint(text)))


class CommandTextTests(unittest.TestCase):
    def test_rejects_removed_command(self) -> None:
        issues = checker.lint_removed_commands(
            Path("cli/src/example.ts"),
            'const hint = "agentsfleet install --from ./fleet";\n',
        )
        self.assertTrue(any("DOC-09" in issue for issue in issues))

    def test_accepts_current_command(self) -> None:
        issues = checker.lint_removed_commands(
            Path("cli/src/example.ts"),
            'const hint = "agentsfleet install --library library_id";\n',
        )
        self.assertEqual([], issues)

    def test_checks_command_line_help_descriptions(self) -> None:
        text = 'description: "A powerful command for fleets",\n'
        issues = checker.lint_cli_source(Path("cli/src/program/example.ts"), text)
        self.assertTrue(any("DOC-07" in issue for issue in issues))


if __name__ == "__main__":
    unittest.main()
