//! The toolbox in a real microVM: the tools the manifest pins run under the
//! production policy, and the release manifest the build writes beside the
//! image names that image and everything in it.

use std::fs;

use libtest_mimic::Failed;
use serde_json::Value;

use crate::lane::{Lane, toolbox_source};
use crate::release::{JSON, RUNNER_VERSION};
use crate::run::expect;
use crate::tools::{GIT_WORKFLOWS, answers, shell, shell_calls};

/// Two of the pinned tools, each named again where its step runs.
const GIT: &str = "git";
const RIPGREP: &str = "ripgrep";
/// The key a release manifest record holds its SHA-256 under.
const SHA256: &str = "sha256";
/// The Debian packages the run's tools come from, each pinned with the hash
/// apt checked it by.
const PINNED: [&str; 15] = [
    "ca-certificates",
    GIT,
    "python3",
    RIPGREP,
    "jq",
    "curl",
    "procps",
    "unzip",
    "xz-utils",
    "bzip2",
    "zstd",
    "file",
    "patch",
    "make",
    "tree",
];
/// The binaries Debian does not ship, each vendored by URL and SHA-256.
const UV_RELEASE: &str = "astral-sh/uv/releases/download/";
const BUN_RELEASE: &str = "oven-sh/bun/releases/download/";
/// The source manifest the lane's image was built from, in the toolbox source.
const SOURCE_MANIFEST: &str = "manifest.txt";
/// The source manifest's keys for the snapshot and each archive at it.
const SNAPSHOT: &str = "snapshot";
const ARCHIVE: &str = "archive";
/// The builder's tools the release records, each by its Debian package.
const BUILDER: [&str; 5] = ["mmdebstrap", "apt", "dpkg", "erofs-utils", "liblz4-1"];

/// How long uv may take to lock and install from the loopback index.
const UV_TIMEOUT_MS: u64 = 120_000;
/// How long a JavaScript step or the git workflows may take.
const NODE_TIMEOUT_MS: u64 = 30_000;
/// The exit status of a command that succeeded.
const SUCCESS: i32 = 0;
/// Builds a one-module wheel and a package index for it, serves the index on
/// loopback inside the sandbox, then has uv lock and install a project whose
/// one dependency it is, and imports the result.
const UV_FROM_LOOPBACK: &str = r#"
export HOME=/workspace UV_CACHE_DIR=/workspace/.cache/uv UV_NO_PROGRESS=1 UV_PYTHON_DOWNLOADS=never
mkdir -p /workspace/index/simple/hello-spike /workspace/demo || exit 1
python3 - <<'PY' || exit 1
import base64, hashlib, zipfile
root = "/workspace/index/simple/hello-spike/"
name = "hello_spike-1.0-py3-none-any.whl"
files = {
    "hello_spike/__init__.py": 'print("hi from a loopback index")\n',
    "hello_spike-1.0.dist-info/METADATA": "Metadata-Version: 2.1\nName: hello-spike\nVersion: 1.0\n",
    "hello_spike-1.0.dist-info/WHEEL": "Wheel-Version: 1.0\nGenerator: kernel-lane\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
}
record = []
with zipfile.ZipFile(root + name, "w", zipfile.ZIP_DEFLATED) as wheel:
    for path, text in files.items():
        data = text.encode()
        wheel.writestr(path, data)
        digest = base64.urlsafe_b64encode(hashlib.sha256(data).digest()).rstrip(b"=").decode()
        record.append(f"{path},sha256={digest},{len(data)}")
    record.append("hello_spike-1.0.dist-info/RECORD,,")
    wheel.writestr("hello_spike-1.0.dist-info/RECORD", "".join(line + "\n" for line in record))
with open(root + "index.html", "w") as page:
    page.write(f'<!DOCTYPE html><html><body><a href="{name}">{name}</a></body></html>\n')
PY
python3 -m http.server 8765 --bind 127.0.0.1 --directory /workspace/index >/dev/null 2>&1 &
server=$!
for _ in 1 2 3 4 5 6 7 8 9 10; do
  python3 -c 'import socket; socket.create_connection(("127.0.0.1", 8765), 1)' 2>/dev/null && break
  sleep 0.2
done
cd /workspace/demo || exit 1
cat >pyproject.toml <<'TOML'
[project]
name = "demo"
version = "0.1.0"
requires-python = ">=3.13"
dependencies = ["hello-spike==1.0"]

[[tool.uv.index]]
url = "http://127.0.0.1:8765/simple"
default = true
TOML
uv lock 2>&1 && uv sync 2>&1 && .venv/bin/python -c 'import hello_spike'
status=$?
kill "$server" 2>/dev/null
exit "$status"
"#;
/// What the installed module prints on import.
const IMPORTED: &str = "hi from a loopback index";
/// uv's line for the package it installed.
const INSTALLED: &str = "hello-spike==1.0";
/// Asks the name `node` its version, then runs one script through that name
/// and through its `#!/usr/bin/env node` line. The name is Bun's: no Node.js
/// is installed, and under it Bun answers `-p process.version`, never
/// `--version`.
const NODE_NAME: &str = r"
mkdir -p /workspace/node && cd /workspace/node || exit 1
cat >sum.js <<'JS'
#!/usr/bin/env node
const assert = require('node:assert');
assert.strictEqual(1 + 1, 2);
console.log('ran under bun', process.versions.bun);
JS
chmod +x sum.js
node -p process.version && node sum.js && ./sum.js 2>&1
";
/// What the name answers its version with, and each run of the script says.
const NODE_VERSION: &str = "v";
const NODE_RAN: &str = "ran under bun 1.";
/// Has Bun, under both its names, answer its version, then writes one
/// TypeScript test and runs Bun's own runner over it: nothing is fetched.
const BUN_TEST: &str = r#"
export HOME=/workspace
mkdir -p /workspace/bun && cd /workspace/bun || exit 1
cat >sum.test.ts <<'TS'
import { expect, test } from "bun:test";
test("adds", () => { expect(1 + 1).toBe(2); });
TS
bun --version && bunx --version && bun test 2>&1
"#;
/// Bun's summary lines when its one test passes.
const BUN_PASSED: &str = " 1 pass";
const BUN_NONE_FAILED: &str = " 0 fail";
/// The other pinned tools answer their versions.
const VERSIONS: &str = "rg --version | head -n1 && jq --version && curl --version | head -n1";
/// A word from each tool's version line.
const VERSION_WORDS: [&str; 3] = [RIPGREP, "jq-", "curl "];
/// Go, the Go-built GitHub client, and build-only tools stay outside the image.
const EXCLUDED_TOOLS: &str = r#"
for tool in go gh syft grype cosign perl apt apt-get dpkg dpkg-query passwd useradd chpasswd; do
  if command -v "$tool" >/dev/null 2>&1; then
    echo "$tool must not be in the toolbox" >&2
    exit 1
  fi
done
"#;

/// Under the production policy, uv installs a locked project from a loopback
/// index, a script runs under the name `node` and by its interpreter line,
/// `bun test` passes, git commits, and the other pinned tools run; the
/// release manifest beside the image accounts for all of them.
pub(crate) fn toolbox_carries_the_tools(lane: &Lane) -> Result<(), Failed> {
    release_manifest_names_the_image(lane)?;
    let calls = [
        shell(UV_FROM_LOOPBACK, Some(UV_TIMEOUT_MS)),
        shell(NODE_NAME, Some(NODE_TIMEOUT_MS)),
        shell(BUN_TEST, Some(NODE_TIMEOUT_MS)),
        shell(GIT_WORKFLOWS, Some(NODE_TIMEOUT_MS)),
        shell(VERSIONS, None),
        shell(EXCLUDED_TOOLS, None),
    ];
    let outputs = shell_calls(lane, "toolbox-tools", &calls)?;
    let [uv, node, bun, git, versions, excluded] = answers(&outputs)?;
    for (step, output) in [
        ("uv", uv),
        ("node", node),
        ("bun", bun),
        (GIT, git),
        ("versions", versions),
        ("excluded tools", excluded),
    ] {
        expect(
            output.exit_code == Some(SUCCESS) && output.error_code.is_none(),
            format!("{step} succeeds, got {output:?}"),
        )?;
    }
    expect(
        uv.text.contains(INSTALLED) && uv.text.contains(IMPORTED),
        format!("uv installed and the module imported, got {:?}", uv.text),
    )?;
    javascript_ran(&node.text, &bun.text)?;
    expect(
        git.text.contains("\"perl\": null") && git.text.contains("\"interactive staging\""),
        format!("core Git workflows passed without Perl, got {:?}", git.text),
    )?;
    expect(
        VERSION_WORDS
            .iter()
            .all(|word| versions.text.contains(word)),
        format!("each tool names its version, got {:?}", versions.text),
    )
}

/// The script ran under the name `node` and by its interpreter line, both
/// times as Bun, and Bun's own runner passed its one test.
fn javascript_ran(node: &str, bun: &str) -> Result<(), Failed> {
    expect(
        node.starts_with(NODE_VERSION) && node.matches(NODE_RAN).count() == 2,
        format!("the script ran twice under Bun's `node`, got {node:?}"),
    )?;
    expect(
        [BUN_PASSED, BUN_NONE_FAILED]
            .iter()
            .all(|line| bun.contains(line)),
        format!("bun's one test passed, got {bun:?}"),
    )
}

/// `record`'s `key` as text, if it holds text.
fn text<'a>(record: &'a Value, key: &str) -> Option<&'a str> {
    record.get(key).and_then(Value::as_str)
}

/// `record`'s `key` as a list, empty when it holds none.
fn list<'a>(record: &'a Value, key: &str) -> &'a [Value] {
    record
        .get(key)
        .and_then(Value::as_array)
        .map_or_default(Vec::as_slice)
}

/// The manifest beside the lane's image names that image's digest and
/// length, every pinned package with the hash apt checked, the vendored uv
/// and Bun for this architecture, the EROFS features it was made with, and
/// this runner's version.
fn release_manifest_names_the_image(lane: &Lane) -> Result<(), Failed> {
    let image = lane.image.path();
    let manifest: Value = serde_json::from_slice(&fs::read(image.with_extension(JSON))?)?;
    let length = fs::metadata(image)?.len();
    expect(
        text(&manifest, SHA256) == Some(lane.image.digest())
            && manifest.get("length").and_then(Value::as_u64) == Some(length),
        format!("the manifest names {}, got {manifest}", lane.image.digest()),
    )?;
    records_its_inputs(&manifest)?;
    let packages = list(&manifest, "packages");
    for name in PINNED {
        expect(
            packages.iter().any(|package| {
                text(package, "package") == Some(name)
                    && text(package, SHA256).is_some_and(|hash| !hash.is_empty())
            }),
            format!("{name} is pinned with its hash, got {packages:?}"),
        )?;
    }
    let arch = std::env::consts::ARCH;
    let vendored = list(&manifest, "vendored");
    // Bun names x86-64 `x64`, so its release alone is looked for: the build
    // records only the lines of the architecture it built for.
    for (release, named) in [(UV_RELEASE, arch), (BUN_RELEASE, "")] {
        expect(
            vendored.iter().any(|binary| {
                text(binary, "url").is_some_and(|url| url.contains(release) && url.contains(named))
            }),
            format!("{release} for {arch} is vendored, got {vendored:?}"),
        )?;
    }
    expect(
        !list(&manifest, "erofs_features").is_empty(),
        format!("the EROFS features are recorded, got {manifest}"),
    )?;
    expect(
        list(&manifest, "runner_versions")
            .iter()
            .any(|version| version.as_str() == Some(RUNNER_VERSION)),
        format!("the image serves runner {RUNNER_VERSION}, got {manifest}"),
    )
}

/// Every value the source manifest gives `key`, in order.
fn source_values(key: &str) -> Result<Vec<String>, Failed> {
    let directory = toolbox_source().ok_or("the lane names no toolbox source")?;
    let source = fs::read_to_string(directory.join(SOURCE_MANIFEST))?;
    Ok(source
        .lines()
        .filter_map(|line| line.strip_prefix(key)?.strip_prefix(' '))
        .map(str::to_owned)
        .collect())
}

/// The release records the snapshot and archives the source manifest names,
/// and the version of each builder tool that made the image.
fn records_its_inputs(manifest: &Value) -> Result<(), Failed> {
    let snapshot = source_values(SNAPSHOT)?;
    expect(
        text(manifest, SNAPSHOT).is_some_and(|recorded| snapshot == [recorded]),
        format!("the snapshot {snapshot:?} is recorded, got {manifest}"),
    )?;
    let archives: Vec<&str> = list(manifest, "archives")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let sources = source_values(ARCHIVE)?;
    expect(
        archives == sources,
        format!("the archives {sources:?} are recorded, got {archives:?}"),
    )?;
    let builder = manifest.get("builder").unwrap_or(&Value::Null);
    for tool in BUILDER {
        expect(
            text(builder, tool).is_some_and(|version| !version.is_empty()),
            format!("the builder's {tool} is recorded, got {builder}"),
        )?;
    }
    Ok(())
}
