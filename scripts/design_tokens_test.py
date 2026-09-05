"""Exercise the design gate against isolated tracked consumer files."""
import subprocess
import tempfile
import unittest
from pathlib import Path

GATE = Path(__file__).resolve().parents[1] / "audits/design-tokens.sh"
APP_SOURCE = "ui/packages/app/components/Fixture.tsx"
SITE_CSS = "ui/packages/website/src/styles.css"


class DesignTokensTest(unittest.TestCase):
    def check_gate(self, files, mode="--all"):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(["git", "init", "-q", directory], check=True)
            for name, content in files.items():
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(content, encoding="utf-8")
            subprocess.run(["git", "add", "."], cwd=root, check=True)
            return subprocess.run(
                ["bash", str(GATE), mode], cwd=root, text=True, capture_output=True,
                check=False,
            )

    def test_consumers_can_select_roles_without_redefining_them(self):
        result = self.check_gate({
            APP_SOURCE: '<code className="font-mono text-body-sm">id</code>',
            "ui/packages/app/lib/appearance.ts": 'const FONT_SANS = "var(--ff-sans)"; const widget = { fontFamily: FONT_SANS };',
            SITE_CSS: 'body { font-family: var(--ff-sans); }',
            "ui/packages/design-system/src/tokens.css": '--ff-sans: "Example";',
        })
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_consumer_font_bypasses_are_rejected_in_both_modes(self):
        for mode in ("--all", "--staged"):
            for source in (
                '<div className="font-display" />',
                '<DisplayXL>Title</DisplayXL>',
                '<div className="font-[Arial]" />',
                '<div style={{fontFamily: "Arial"}} />',
            ):
                with self.subTest(mode=mode, source=source):
                    result = self.check_gate({APP_SOURCE: source}, mode)
                    self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                    self.assertIn(APP_SOURCE, result.stdout)

    def test_consumer_css_cannot_redefine_fonts_or_hide_a_second_declaration(self):
        for source in (
            'body { font-family: "Arial"; }',
            ':root { --ff-sans: Arial; }',
            ':root { --font-mono: monospace; }',
            'a {font-family: var(--ff-sans);} b {font-family: serif;}',
        ):
            with self.subTest(source=source):
                result = self.check_gate({SITE_CSS: source})
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)

    def test_gradient_and_palette_checks_remain_enforced(self):
        for source in (
            '<div className="bg-red-500" />',
            '<div style={{background: "linear-gradient(red, blue)"}} />',
        ):
            with self.subTest(source=source):
                result = self.check_gate({APP_SOURCE: source})
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
