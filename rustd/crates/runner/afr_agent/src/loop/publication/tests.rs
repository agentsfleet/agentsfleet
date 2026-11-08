//! Real Git fixtures exercise the sandbox candidate script's publication boundary.

#[test]
fn candidate_preserves_immutable_modes_and_refuses_unsupported_changes() -> std::io::Result<()> {
    let outcome = std::process::Command::new("python3")
        .args(["-I", "-S", "-c", FIXTURES, super::CANDIDATE_SCRIPT])
        .output()?;
    assert!(
        outcome.status.success(),
        "candidate fixture failures: {}",
        String::from_utf8_lossy(&outcome.stderr)
    );
    Ok(())
}

// Fixtures execute the production capture program against real Git index states.
const FIXTURES: &str = r##"
"""Exercise candidate capture against real Git indexes and hostile workspace paths."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SOURCE = sys.argv.pop()
OID = "1" * 40


class CandidateTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.git("init", "--quiet")
        (self.root / "ordinary.txt").write_text("before\n")
        executable = self.root / "executable.sh"
        executable.write_text("#!/bin/sh\nexit 0\n")
        executable.chmod(0o755)
        self.git("add", "ordinary.txt", "executable.sh")
        self.git("update-index", "--add", "--cacheinfo", f"160000,{OID},module")
        (self.root / "module").mkdir()
        self.base = self.git("write-tree").strip()
        (self.root / "ordinary.txt").write_text("after\n")

    def git(self, *args):
        return subprocess.check_output(
            ["git", "-c", "core.fsmonitor=false", *args], cwd=self.root,
            env=dict(os.environ, GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL="/dev/null"),
            stderr=subprocess.DEVNULL, text=True)

    def capture(self):
        return subprocess.run([sys.executable, "-I", "-S", "-c", SOURCE, self.base],
                              cwd=self.root, capture_output=True, text=True, check=False)

    def refused(self):
        result = self.capture()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertIn("Candidate capture refused", result.stderr)

    def test_regular_edit_preserves_empty_gitlink_and_executable(self):
        result = self.capture()
        self.assertEqual(result.returncode, 0, result.stderr)
        snapshot = json.loads(result.stdout)
        self.assertEqual([change["path"] for change in snapshot["changes"]], ["ordinary.txt"])
        for path in ("module", "executable.sh"):
            self.assertEqual(self.git("ls-tree", snapshot["tested_tree"], "--", path),
                             self.git("ls-tree", self.base, "--", path))

    def test_staged_gitlink_oid_change_is_refused(self):
        self.git("update-index", "--cacheinfo", f"160000,{'2' * 40},module")
        self.refused()

    def test_staged_gitlink_deletion_is_refused(self):
        self.git("update-index", "--force-remove", "module")
        self.refused()

    def test_deleted_gitlink_directory_is_refused(self):
        (self.root / "module").rmdir()
        self.refused()

    def test_symlink_gitlink_is_refused(self):
        (self.root / "module").rmdir()
        (self.root / "module").symlink_to(".", target_is_directory=True)
        self.refused()

    def test_regular_file_gitlink_is_refused(self):
        (self.root / "module").rmdir()
        (self.root / "module").write_text("replacement")
        self.refused()

    def test_populated_gitlink_is_refused_despite_index_flags(self):
        (self.root / "module" / "hidden.txt").write_text("changed")
        self.git("update-index", "--assume-unchanged", "module")
        self.refused()

    def test_staged_gitlink_mode_change_is_refused(self):
        blob = self.git("rev-parse", f"{self.base}:ordinary.txt").strip()
        self.git("update-index", "--cacheinfo", f"100644,{blob},module")
        self.refused()

    def test_staged_gitlink_descendant_is_refused(self):
        blob = self.git("rev-parse", f"{self.base}:ordinary.txt").strip()
        self.git("update-index", "--force-remove", "module")
        self.git("update-index", "--add", "--cacheinfo", f"100644,{blob},module/child")
        self.refused()

    def test_changed_executable_is_refused(self):
        (self.root / "executable.sh").write_text("#!/bin/sh\nexit 1\n")
        self.refused()

    def test_deleted_executable_is_refused(self):
        (self.root / "executable.sh").unlink()
        self.refused()


unittest.main()
"##;
