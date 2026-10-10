"""What `check_tests_call_code.py` reads as a test, a fixture, an anchor and a read.

One table per language: the tokens that anchor a path in the repository, how a
variable is assigned (so an anchor carries through it), and the calls that read
a file or walk a directory.
"""

from __future__ import annotations

import re

TEST_DIRS = frozenset({"test", "tests", "__tests__"})
FIXTURE_DIRS = ("fixtures", "testdata", "__fixtures__", "__snapshots__", "golden")
FIXTURE = re.compile(r"(?:^|[/\"'`\s(,])(?:%s)(?:[/\"'`\s),]|$)" % "|".join(FIXTURE_DIRS))
TS_TEST_NAME = re.compile(r"\.(?:test|spec)\.(?:ts|tsx|js|jsx|mjs|mts)$")
JS_SUFFIXES = (".ts", ".tsx", ".js", ".jsx", ".mjs", ".mts")
SKIPPED = frozenset({"node_modules", ".next", "dist", "coverage", "target", ".git"})

# A path relative to the test, or one rooted at a top-level directory.
RELATIVE_LITERAL = r"""["'`](?:\.\.?/|/?(?:src|cli|ui|docs|rustd|schema|scripts|playbooks|deploy|make|public|\.github|bench|test|tests|app|components|lib)/)"""

LANGUAGES = {
    "js": {
        "anchors": r"__dirname|__filename|import\.meta\.(?:url|dir|dirname|filename)|process\.cwd\(\)|" + RELATIVE_LITERAL,
        "assign": re.compile(r"\b(?:const|let|var)\s+(?P<name>\w+)\s*(?::[^=]+)?=\s*(?P<value>[^;\n]+)"),
        "reads": re.compile(
            r"\b(?:readFileSync|readFile|readdirSync|readdir|existsSync|statSync|createReadStream|opendirSync)\s*\("
            r"|\bBun\.file\s*\(|\.scan\s*\(|\bimport\.meta\.glob\s*(?:<[^>]*>)?\s*\("
            r"|\b(?:execSync|spawnSync|execFileSync)\s*\(\s*[\"'`](?:grep|rg|cat|find)\b"
            r"|\bfrom\s+[\"'][^\"']+\?raw[\"']"
        ),
        "comment": "//",
    },
    "rust": {
        "anchors": r'env!\(\s*"CARGO_MANIFEST_DIR"\s*\)|' + r'"(?:\.\./)+',
        "assign": re.compile(r"\b(?:let|const|static)\s+(?:mut\s+)?(?P<name>\w+)\s*(?::[^=]+)?=\s*(?P<value>[^;]+)"),
        "reads": re.compile(
            r"\binclude_(?:str|bytes)!\s*\(|\bfs::(?:read_to_string|read|read_dir)\s*\(|\bFile::open\s*\("
            r"|\bWalkDir::new\s*\("
        ),
        "comment": "//",
    },
    "python": {
        "anchors": r"__file__|\bROOT\b|\bREPO_ROOT\b|rev-parse|" + RELATIVE_LITERAL,
        "assign": re.compile(r"^\s*(?P<name>[A-Za-z_]\w*)\s*(?::[^=]+)?=\s*(?P<value>.+)$", re.MULTILINE),
        "reads": re.compile(
            r"\bopen\s*\(|\.read_text\s*\(|\.read_bytes\s*\(|\.open\s*\(|\.r?glob\s*\(|\.iterdir\s*\(|\bos\.(?:walk|listdir|scandir)\s*\("
        ),
        "comment": "#",
    },
    "shell": {
        "anchors": r"BASH_SOURCE|rev-parse --show-toplevel|dirname\s+\"?\$0|\$\{?(?:repo_root|REPO_ROOT|ROOT)\b|\$\(pwd\)|(?<![\w/$.])\.\./",
        "assign": re.compile(
            r"^\s*(?:readonly\s+|local\s+|export\s+)?(?P<name>[A-Za-z_]\w*)=(?P<value>.+)$"
            r"|\bfor\s+(?P<loop>[A-Za-z_]\w*)\s+in\s+(?P<items>[^;]+?)\s*;\s*do\b",
            re.MULTILINE,
        ),
        "reads": re.compile(
            r"(?:^|[\s;|&(`])(?:cat|grep|egrep|fgrep|rg|sed|awk|head|tail|jq|yq|cut|sort|diff|cmp)\b(?![-=.])",
            re.MULTILINE,
        ),
        "comment": "#",
    },
}
