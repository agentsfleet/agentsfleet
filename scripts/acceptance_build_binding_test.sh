#!/usr/bin/env bash
# acceptance_build_binding_test.sh — mismatched builds stop acceptance before
# any fixture is touched.
#
# The vitest suite proves the preflight's ordering against mocked reads. This
# runs the preflight itself over real artifacts: the daemon and runner
# binaries cargo built, the CLI's dist tree and stamp, the app's `.next`
# output, the schema and provider sources. Every outbound request goes through
# a tripwire proxy that answers nothing, so a Clerk, bootstrap or cleanup
# write — or any network read — is recorded rather than sent.
#
# Part one copies the preflight and inspector into a candidate root whose four
# components match one manifest, then breaks one component at a time. Part two
# runs the two real lane entry points — the app's Playwright global setup and
# the CLI's live preflight — in this worktree, over this worktree's builds.
#
# Needs bun, git, python3, the root, app and CLI `node_modules`, a built
# `cli/dist`, `ui/packages/app/.next`, and both Rust binaries (override with
# AGENTSFLEET_DAEMON_BIN / AGENTSFLEET_RUNNER_BIN). Writes only under a
# temporary directory.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly ROOT
readonly FIXTURES="ui/packages/app/tests/e2e/acceptance/fixtures"
readonly APP_OUTPUT="ui/packages/app/.next"
readonly EVIDENCE=".tmp/acceptance-preflight.md"
readonly OTHER_DIGEST="bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
readonly OTHER_REVISION="dddddddddddddddddddddddddddddddddddddddd"
DAEMON_BIN="${AGENTSFLEET_DAEMON_BIN:-$ROOT/rustd/target/debug/agentsfleetd}"
RUNNER_BIN="${AGENTSFLEET_RUNNER_BIN:-$ROOT/rustd/target/debug/agentsfleet-runner}"
readonly DAEMON_BIN RUNNER_BIN
FAILURES=0
WORK=""
TRIP_PID=""

ok()  { printf 'ok   %s\n' "$1"; }
bad() { printf 'FAIL %s\n     %s\n' "$1" "$2"; FAILURES=$((FAILURES + 1)); }

cleanup() {
  if [ -n "$TRIP_PID" ]; then kill "$TRIP_PID" 2>/dev/null; wait "$TRIP_PID" 2>/dev/null; fi
  if [ -n "$WORK" ]; then rm -rf "$WORK"; fi
}
trap cleanup EXIT

require() {
  local missing=0 path
  for path in "$@"; do
    if [ ! -e "$path" ]; then printf 'missing prerequisite: %s\n' "$path" >&2; missing=1; fi
  done
  [ "$missing" -eq 0 ] || exit 2
}

# APFS and reflink filesystems clone instead of copying; both fall back to a copy.
clone() { cp -cR "$1" "$2" 2>/dev/null || cp -R "$1" "$2"; }

start_tripwire() {
  mkdir -p "$WORK/bin"
  cat > "$WORK/bin/tripwire.py" <<'PY'
import socket, sys
server = socket.socket()
server.bind(("127.0.0.1", 0))
server.listen(16)
with open(sys.argv[2], "w") as port:
    port.write(str(server.getsockname()[1]))
with open(sys.argv[1], "a") as log:
    while True:
        client, _ = server.accept()
        client.settimeout(2)
        try:
            line = client.recv(4096).split(b"\r\n", 1)[0].decode("latin-1")
        except OSError:
            line = "<connection without a request line>"
        log.write(line + "\n")
        log.flush()
        try:
            client.sendall(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        except OSError:
            pass
        client.close()
PY
  : > "$WORK/requests.log"
  : > "$WORK/empty.env"
  python3 -I "$WORK/bin/tripwire.py" "$WORK/requests.log" "$WORK/port" &
  TRIP_PID=$!
  local tries=0
  while [ ! -s "$WORK/port" ] && [ "$tries" -lt 100 ]; do
    tries=$((tries + 1)); python3 -I -c 'import time; time.sleep(0.05)'
  done
  [ -s "$WORK/port" ] || { echo "tripwire did not start" >&2; exit 2; }
  TRIP="http://127.0.0.1:$(cat "$WORK/port")"
  readonly TRIP
}

# A clean environment: nothing from the developer's shell, no worktree `.env`,
# an empty machine-level UI file, and every request routed through the tripwire.
hermetic() {
  env -i PATH="$PATH" HOME="$WORK/home" TMPDIR="$WORK/tmp" \
    HTTP_PROXY="$TRIP" HTTPS_PROXY="$TRIP" http_proxy="$TRIP" https_proxy="$TRIP" \
    AGENTSFLEET_UI_ENV_FILE="$WORK/empty.env" "$@"
}

requests() { wc -l < "$WORK/requests.log" | tr -d ' '; }

test_tripwire_catches_clerk_and_api_traffic() {
  local name="test_tripwire_catches_clerk_and_api_traffic" before
  before="$(requests)"
  hermetic bun -e "await fetch('https://api.clerk.com/v1/users').catch(() => {});
    await fetch('$TRIP/v1/auth/identity-events/clerk', {method: 'POST'}).catch(() => {});" >/dev/null 2>&1
  if [ "$(requests)" -eq $((before + 2)) ] && grep -q '^CONNECT api.clerk.com:443' "$WORK/requests.log"; then
    ok "$name"
  else bad "$name" "a Clerk or API write would not have been recorded"; fi
  : > "$WORK/requests.log"
}

# --- Part one: one broken component at a time, over a matched candidate ------

build_candidate() {
  CAND="$WORK/candidate"
  readonly CAND
  local path
  git clone --shared --no-checkout --quiet "$ROOT" "$CAND"
  mkdir -p "$CAND/scripts" "$CAND/$FIXTURES" "$CAND/cli" "$CAND/ui/packages/app" \
    "$CAND/rustd/crates" "$CAND/out" "$WORK/home" "$WORK/tmp"
  for path in build-manifest.ts build-source.ts inspect-acceptance-build.ts; do
    cp "$ROOT/scripts/$path" "$CAND/scripts/$path"
  done
  cp "$ROOT/$FIXTURES/build-binding.ts" "$ROOT/$FIXTURES/build-binding-io.ts" "$CAND/$FIXTURES/"
  cp "$ROOT/VERSION" "$CAND/VERSION"
  cp "$ROOT/rustd/Cargo.lock" "$CAND/rustd/Cargo.lock"
  for path in afd_fleet_runtime afd_db afd_credential afd_authoring afr_providers; do
    clone "$ROOT/rustd/crates/$path" "$CAND/rustd/crates/$path"
  done
  clone "$ROOT/schema" "$CAND/schema"
  clone "$ROOT/cli/dist" "$CAND/cli/dist"
  cp "$ROOT/cli/package.json" "$CAND/cli/package.json"
  clone "$ROOT/$APP_OUTPUT" "$CAND/$APP_OUTPUT"
  clone "$DAEMON_BIN" "$CAND/out/agentsfleetd"
  clone "$RUNNER_BIN" "$CAND/out/agentsfleet-runner"
  ln -s "$ROOT/node_modules" "$CAND/node_modules"
  ln -s "$ROOT/cli/node_modules" "$CAND/cli/node_modules"
  printf '%s\n' node_modules out/ cli/dist/ "$APP_OUTPUT/" .tmp/ > "$CAND/.gitignore"
  record_candidate_manifest
}

# What `scripts/build-acceptance.ts` records after its builds, read from the
# candidate's existing outputs instead of rebuilding them.
record_candidate_manifest() {
  cat > "$WORK/bin/manifest.ts" <<'TS'
const [root, revision, out] = Bun.argv.slice(2);
const source = await import(`${root}/scripts/build-source.ts`);
const { buildManifestSchema } = await import(`${root}/scripts/build-manifest.ts`);
const sourceId = await source.sourceIdentity();
await Bun.write(`${root}/cli/dist/build-info.json`, `${JSON.stringify({ build_id: sourceId })}\n`);
const daemonPath = `${root}/out/agentsfleetd`;
const runnerPath = `${root}/out/agentsfleet-runner`;
await Bun.write(out, JSON.stringify(buildManifestSchema.parse({
  source_id: sourceId, revision, built_at: new Date().toISOString(), environment: "mismatch-rehearsal",
  revisions: {daemon: revision, runner: revision, cli: revision, app: revision},
  daemon_path: daemonPath, runner_path: runnerPath, metadata: await source.buildMetadata(),
  daemon_hash: await source.fileHash(Bun.file(daemonPath)),
  runner_hash: await source.fileHash(Bun.file(runnerPath)),
  cli_hash: await source.outputHash(`${root}/cli/dist`),
  app_hash: await source.outputHash(`${root}/ui/packages/app/.next`),
}), null, 2));
TS
  hermetic bun "$WORK/bin/manifest.ts" "$CAND" "$(git -C "$CAND" rev-parse HEAD)" "$WORK/matched.json" \
    || { echo "could not record the candidate manifest" >&2; exit 2; }
}

# Writes a copy of the matched manifest with one field replaced.
manifest_with() {
  python3 -I - "$WORK/matched.json" "$WORK/case.json" "$1" "$2" <<'PY'
import json, sys
source, target, path, value = sys.argv[1:]
manifest = json.load(open(source))
node = manifest
keys = path.split(".")
for key in keys[:-1]:
    node = node[key]
node[keys[-1]] = json.loads(value)
json.dump(manifest, open(target, "w"))
PY
}

# Runs the candidate's preflight entry and checks it refused for `expected`,
# with no request sent and no evidence written.
expect_refusal() {
  local name="$1" manifest="$2" expected="$3" output status
  : > "$WORK/requests.log"
  output="$(hermetic ACCEPTANCE_BUILD_MANIFEST="$manifest" \
    bun "$CAND/$FIXTURES/build-binding.ts" "$TRIP/api" "$TRIP/app" 2>&1)"
  status=$?
  if [ "$status" -eq 0 ]; then bad "$name" "the preflight accepted the build"; return; fi
  if ! printf '%s' "$output" | grep -qF -- "$expected"; then
    bad "$name" "refused for another reason; wanted \"$expected\": $(printf '%s' "$output" | grep -m1 -E 'error|Error' )"
    return
  fi
  if [ "$(requests)" -ne 0 ]; then bad "$name" "requests left before the refusal: $(cat "$WORK/requests.log")"; return; fi
  if [ -e "$CAND/$EVIDENCE" ]; then bad "$name" "evidence was written for a refused build"; return; fi
  ok "$name"
}

test_matched_candidate_passes_every_artifact_check() {
  expect_refusal "test_matched_candidate_passes_every_artifact_check" "$WORK/matched.json" \
    "ACCEPTANCE_RUNNER_PROCESS_EVIDENCE must name a fresh trusted host process record"
}

test_manifest_field_mismatches_refuse() {
  local field
  for field in daemon_hash runner_hash cli_hash app_hash; do
    manifest_with "$field" "\"$OTHER_DIGEST\""
    expect_refusal "test_manifest_${field}_mismatch_refuses" "$WORK/case.json" \
      "Acceptance $field differs from its build manifest"
  done
  manifest_with source_id "\"$OTHER_DIGEST\""
  expect_refusal "test_manifest_from_another_source_refuses" "$WORK/case.json" \
    "Source or CLI build differs from the acceptance manifest"
  manifest_with revision "\"$OTHER_REVISION\""
  expect_refusal "test_manifest_from_another_revision_refuses" "$WORK/case.json" \
    "Acceptance checkout revision differs from its build manifest"
  for field in daemon runner cli app; do
    manifest_with "revisions.$field" "\"$OTHER_REVISION\""
    expect_refusal "test_manifest_${field}_revision_mismatch_refuses" "$WORK/case.json" \
      "Acceptance $field revision differs from the candidate"
  done
  manifest_with metadata.schema_latest 1
  expect_refusal "test_manifest_with_other_schema_refuses" "$WORK/case.json" \
    "schema, policy, tokenizer or provider metadata differs"
}

# Rebuilds one component after the manifest was written, then restores it.
test_rebuilt_components_refuse() {
  local stamp="$CAND/cli/dist/build-info.json" pristine="$WORK/pristine"
  mkdir -p "$pristine"
  for binary in agentsfleetd agentsfleet-runner; do
    clone "$CAND/out/$binary" "$pristine/$binary"
    printf 'rebuilt' >> "$CAND/out/$binary"
  done
  expect_refusal "test_rebuilt_daemon_refuses" "$WORK/matched.json" \
    "Acceptance daemon_hash differs from its build manifest"
  clone "$pristine/agentsfleetd" "$CAND/out/agentsfleetd.restored"
  mv -f "$CAND/out/agentsfleetd.restored" "$CAND/out/agentsfleetd"
  expect_refusal "test_rebuilt_runner_refuses" "$WORK/matched.json" \
    "Acceptance runner_hash differs from its build manifest"
  mv -f "$pristine/agentsfleet-runner" "$CAND/out/agentsfleet-runner"

  cp "$stamp" "$pristine/build-info.json"
  printf '{"build_id":"%s"}\n' "$OTHER_DIGEST" > "$stamp"
  expect_refusal "test_cli_built_from_another_source_refuses" "$WORK/matched.json" \
    "Source or CLI build differs from the acceptance manifest"
  printf '{"build_id":null}\n' > "$stamp"
  expect_refusal "test_unbound_cli_refuses" "$WORK/matched.json" "build_id"
  mv -f "$pristine/build-info.json" "$stamp"

  printf 'rebuilt\n' > "$CAND/$APP_OUTPUT/agentsfleet-rebuilt"
  expect_refusal "test_rebuilt_app_refuses" "$WORK/matched.json" \
    "Acceptance app_hash differs from its build manifest"
  rm -f "$CAND/$APP_OUTPUT/agentsfleet-rebuilt"
  test_matched_candidate_passes_every_artifact_check
}

# --- Part two: the real lane entry points, over this worktree's builds -------

# A manifest naming this worktree's real binaries but a build nothing here is.
write_foreign_manifest() {
  manifest_with daemon_path "\"$DAEMON_BIN\""
  mv -f "$WORK/case.json" "$WORK/foreign.json"
  python3 -I - "$WORK/foreign.json" "$RUNNER_BIN" "$OTHER_DIGEST" <<'PY'
import json, sys
path, runner, other = sys.argv[1:]
manifest = json.load(open(path))
manifest.update(runner_path=runner, source_id=other, daemon_hash=other)
json.dump(manifest, open(path, "w"))
PY
}

evidence_stamp() { if [ -e "$ROOT/$EVIDENCE" ]; then ls -l "$ROOT/$EVIDENCE"; else echo absent; fi; }

# The binding's own refusal of this worktree's builds: the CLI here is either
# unbound (a null stamp fails the inspection schema) or bound to a source the
# foreign manifest does not name. Anything else is an unrelated crash.
readonly BINDING_REFUSAL='Source or CLI build differs from the acceptance manifest|"build_id"'

# Runs a real entry point from an empty directory and checks it was the build
# binding that refused, with no request sent and no file written.
expect_lane_refusal() {
  local name="$1" expected="$2" output status before cwd
  shift 2
  cwd="$WORK/lane/$name/a/b/c"
  mkdir -p "$cwd"
  before="$(evidence_stamp)"
  : > "$WORK/requests.log"
  output="$(cd "$cwd" && hermetic ACCEPTANCE_BUILD_MANIFEST="$WORK/foreign.json" "$@" 2>&1)"
  status=$?
  if [ "$status" -eq 0 ]; then bad "$name" "the lane accepted a foreign build"; return; fi
  if ! printf '%s' "$output" | grep -qE -- "$BINDING_REFUSAL" \
    || ! printf '%s' "$output" | grep -qF -- "$expected"; then
    bad "$name" "refused for another reason: $(printf '%s' "$output" | grep -m3 -iE 'error|failed')"
    return
  fi
  if [ "$(requests)" -ne 0 ]; then bad "$name" "fixture traffic before the refusal: $(cat "$WORK/requests.log")"; return; fi
  if [ -n "$(find "$WORK/lane/$name" -type f)" ]; then
    bad "$name" "fixture files were written: $(find "$WORK/lane/$name" -type f)"; return
  fi
  if [ "$(evidence_stamp)" != "$before" ]; then bad "$name" "build evidence changed"; return; fi
  ok "$name"
}

test_app_global_setup_refuses_before_clerk_bootstrap_or_cleanup() {
  expect_lane_refusal "test_app_global_setup_refuses_before_clerk_bootstrap_or_cleanup" "build" \
    NEXT_PUBLIC_API_URL="$TRIP/api" BASE_URL="$TRIP/app" \
    CLERK_SECRET_KEY=sk_test_rehearsal CLERK_PUBLISHABLE_KEY=pk_test_rehearsal \
    CLERK_WEBHOOK_SECRET=whsec_rehearsal \
    bun -e "const setup = await import('$ROOT/ui/packages/app/tests/e2e/acceptance/global-setup.ts');
      await setup.default();"
}

test_cli_live_preflight_refuses_before_fixture_creation() {
  expect_lane_refusal "test_cli_live_preflight_refuses_before_fixture_creation" \
    "CLI acceptance build binding failed before fixture creation" \
    AGENTSFLEET_ACCEPTANCE_TARGET=https://api.rehearsal.invalid \
    AGENTSFLEET_ACCEPTANCE_DASHBOARD_URL=https://app.rehearsal.invalid \
    AGENTSFLEET_ACCEPTANCE_LOGIN_HANDSHAKE=1 \
    CLERK_SECRET_KEY=sk_test_rehearsal CLERK_PUBLISHABLE_KEY=pk_test_rehearsal \
    CLERK_WEBHOOK_SECRET=whsec_rehearsal \
    AUTH_E2E_REGULAR_EMAIL=regular@rehearsal.invalid AUTH_E2E_ADMIN_EMAIL=admin@rehearsal.invalid \
    bun "$ROOT/cli/test/acceptance/global-setup.ts"
}

main() {
  require "$DAEMON_BIN" "$RUNNER_BIN" "$ROOT/cli/dist/bin/agentsfleet.js" "$ROOT/$APP_OUTPUT" \
    "$ROOT/node_modules" "$ROOT/cli/node_modules" "$ROOT/ui/packages/app/node_modules"
  WORK="$(mktemp -d "${TMPDIR:-/tmp}/acceptance-binding.XXXXXX")"
  start_tripwire
  build_candidate
  test_tripwire_catches_clerk_and_api_traffic
  test_matched_candidate_passes_every_artifact_check
  test_manifest_field_mismatches_refuse
  test_rebuilt_components_refuse
  write_foreign_manifest
  test_app_global_setup_refuses_before_clerk_bootstrap_or_cleanup
  test_cli_live_preflight_refuses_before_fixture_creation
  if [ "$FAILURES" -ne 0 ]; then printf '%d acceptance build-binding check(s) failed\n' "$FAILURES"; exit 1; fi
  echo "acceptance build binding refuses every mismatch before fixture mutation"
}

main "$@"
