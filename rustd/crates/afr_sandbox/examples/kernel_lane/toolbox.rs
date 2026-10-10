//! The toolbox in a real sandbox: the tools the manifest pins run under the
//! production policy, and the release manifest the build writes beside the
//! image names that image and everything in it.

use std::fs;
use std::path::Path;

use libtest_mimic::Failed;
use serde_json::Value;

use crate::lane::Lane;
use crate::run::expect;
use crate::tools::{answers, shell, shell_calls};

/// The release manifest's extension, beside the image's `.erofs`.
const JSON: &str = "json";
/// Two of the pinned tools, each named again where its step runs.
const GIT: &str = "git";
const RIPGREP: &str = "ripgrep";
/// The key a release manifest record holds its SHA-256 under.
const SHA256: &str = "sha256";
/// The Debian packages the lease's tools come from, each pinned with the hash
/// apt checked it by.
const PINNED: [&str; 7] = [
    "ca-certificates",
    GIT,
    "python3",
    "nodejs",
    RIPGREP,
    "jq",
    "curl",
];
/// The binary Debian does not ship, vendored by URL and SHA-256.
const UV_RELEASE: &str = "astral-sh/uv/releases/download/";
/// The runner version the lane's image was built to serve.
const RUNNER_VERSION: &str = env!("CARGO_PKG_VERSION");
/// The source manifest the lane's image was built from, from this crate.
const SOURCE_MANIFEST: &str = "../../../scripts/toolbox/manifest.txt";
/// The source manifest's keys for the snapshot and each archive at it.
const SNAPSHOT: &str = "snapshot";
const ARCHIVE: &str = "archive";
/// The builder's tools the release records, each by its Debian package.
const BUILDER: [&str; 5] = ["mmdebstrap", "apt", "dpkg", "erofs-utils", "liblz4-1"];

/// How long uv may take to lock and install from the loopback index.
const UV_TIMEOUT_MS: u64 = 120_000;
/// How long `node --test` may take.
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
/// Writes one Node test and runs the runner over it.
const NODE_TEST: &str = r"
mkdir -p /workspace/node && cd /workspace/node || exit 1
cat >sum.test.js <<'JS'
const test = require('node:test');
const assert = require('node:assert');
test('adds', () => { assert.strictEqual(1 + 1, 2); });
JS
node --test 2>&1
";
/// The runner's summary lines when its one test passes.
const NODE_PASSED: &str = "# pass 1";
const NODE_NONE_FAILED: &str = "# fail 0";
/// Makes a repository, commits once, and prints the commit's log line.
const GIT_COMMIT: &str = r"
export HOME=/workspace
mkdir -p /workspace/repo && cd /workspace/repo || exit 1
git init -q && echo one >README && git add README \
  && git -c user.name=agentsfleet -c user.email=noreply@agentsfleet.net commit -q -m one \
  && git log --oneline -1
";
/// How the log line ends.
const COMMITTED: &str = " one";
/// The other pinned tools answer their versions.
const VERSIONS: &str = "rg --version | head -n1 && jq --version && curl --version | head -n1";
/// A word from each tool's version line.
const VERSION_WORDS: [&str; 3] = [RIPGREP, "jq-", "curl "];
/// Go, the Go-built GitHub client, and build-only tools stay outside the image.
const EXCLUDED_TOOLS: &str = r#"
for tool in go gh syft grype cosign; do
  if command -v "$tool" >/dev/null 2>&1; then
    echo "$tool must not be in the toolbox" >&2
    exit 1
  fi
done
"#;

/// Under the production policy, uv installs a locked project from a loopback
/// index, `node --test` passes, git commits, and the other pinned tools run;
/// the release manifest beside the image accounts for all of them.
pub(crate) fn toolbox_carries_the_tools(lane: &Lane) -> Result<(), Failed> {
    release_manifest_names_the_image(lane)?;
    let calls = [
        shell(UV_FROM_LOOPBACK, Some(UV_TIMEOUT_MS)),
        shell(NODE_TEST, Some(NODE_TIMEOUT_MS)),
        shell(GIT_COMMIT, None),
        shell(VERSIONS, None),
        shell(EXCLUDED_TOOLS, None),
    ];
    let outputs = shell_calls(lane, "toolbox-tools", &calls)?;
    let [uv, node, git, versions, excluded] = answers(&outputs)?;
    for (step, output) in [
        ("uv", uv),
        ("node", node),
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
    expect(
        node.text.contains(NODE_PASSED) && node.text.contains(NODE_NONE_FAILED),
        format!("node's one test passed, got {:?}", node.text),
    )?;
    expect(
        git.text
            .lines()
            .last()
            .is_some_and(|line| line.ends_with(COMMITTED)),
        format!("git committed, got {:?}", git.text),
    )?;
    expect(
        VERSION_WORDS
            .iter()
            .all(|word| versions.text.contains(word)),
        format!("each tool names its version, got {:?}", versions.text),
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
        .map(Vec::as_slice)
        .unwrap_or_default()
}

/// The manifest beside the lane's image names that image's digest and
/// length, every pinned package with the hash apt checked, the vendored uv
/// for this architecture, the EROFS features it was made with, and this
/// runner's version.
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
    expect(
        vendored.iter().any(|binary| {
            text(binary, "url").is_some_and(|url| url.contains(UV_RELEASE) && url.contains(arch))
        }),
        format!("uv for {arch} is vendored, got {vendored:?}"),
    )?;
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
    let source = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(SOURCE_MANIFEST))?;
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
