---
name: write-unit-test
description: >
  Generates robust, multi-tier test coverage for any code change across 12 stacks:
  Python, Python SDK, OpenAPI, JS/TS CLI, React/TS, Zig, Rust, Go, Go CLI,
  Terraform, Shell, Perl/Ruby. Covers happy path, edge cases, error paths,
  fidelity, concurrency, integration, regression, security, DRY, constants,
  performance, and API contract compliance. Use when writing tests, reviewing
  coverage, or hardening a codebase folder-by-folder.
---

# Write Unit Test

Generate production-grade test coverage that catches bugs before they ship.

## Do This First

1. Identify the changed files or the folder to improve coverage for.
2. Read every changed file top-to-bottom before writing a single test.
3. Detect the stack from project files:

| File | Stack |
|------|-------|
| `pyproject.toml` / `setup.py` | Python / Python SDK |
| `openapi.yaml` / `swagger.json` | OpenAPI spec |
| `package.json` + `bin` field | JS/TS CLI |
| `package.json` + `react` dep | React/TypeScript |
| `build.zig` / `build.zig.zon` | Zig |
| `Cargo.toml` | Rust |
| `go.mod` | Go / Go CLI / Terraform provider |
| `*.tf` / `*.tfvars` | Terraform HCL |
| `*.sh` / `*.bash` | Shell script |
| `*.pl` / `*.pm` / `Gemfile` / `*.rb` | Perl / Ruby |

4. Follow existing test conventions (naming, file layout, fixture patterns, assertion style).
5. Run the existing test suite first to establish the green baseline.

## Incremental Coverage Improvement Mode

When asked to improve existing coverage (not just cover a changeset):

1. **Start with a single folder.** Do not boil the ocean.
2. Run coverage on that folder: identify uncovered lines, branches, functions.
3. Sort uncovered code by risk: public API > business logic > helpers > glue code.
4. Write tests tier-by-tier for the highest-risk uncovered code first.
5. After each batch, re-run coverage and report the delta.
6. Move to the next folder only when the current one meets the target (≥90% line, ≥80% branch).
7. Produce a folder-level coverage report card before moving on:

```
## Coverage Report: {{folder}}
| Metric        | Before | After | Target |
|---------------|--------|-------|--------|
| Line coverage | 62%    | 94%   | ≥90%   |
| Branch cov.   | 41%    | 85%   | ≥80%   |
| Functions     | 55%    | 100%  | ≥90%   |
| Uncovered     | 38 lines | 6 lines | <10  |
```

### Coverage tools by stack

| Stack | Coverage tool | Command |
|-------|--------------|---------|
| Python | `coverage.py` / `pytest-cov` | `pytest --cov=src --cov-branch` |
| Rust | `llvm-cov` / `cargo-tarpaulin` | `cargo tarpaulin --out Html` |
| Zig | `kcov` / built-in | `zig test --coverage` |
| React/TS | `c8` / `istanbul` / `vitest` | `vitest --coverage` |
| Go | built-in | `go test -coverprofile=c.out ./...` |
| Shell | `bashcov` / `kcov` | `kcov --include-path=./src coverage/ ./test.sh` |
| Ruby | `simplecov` | auto via `spec_helper.rb` |
| Perl | `Devel::Cover` | `cover -test` |
| Terraform | N/A (plan-based) | `terraform plan -detailed-exitcode` |

---

## Test Tiers

For every file touched, satisfy ALL tiers. If a tier does not apply, state why.

### TIER 1 — HAPPY PATH

For each public function, method, struct impl, component, endpoint, CLI command, or resource:

- [ ] Call with valid, realistic production-like input
- [ ] Assert return value, status code, content type, and shape
- [ ] Assert side effects (DB writes, cache sets, signals, events, state changes)
- [ ] Assert downstream calls with correct arguments

**Stack-specific happy path:**

| Stack | Pattern |
|-------|---------|
| Python | `assert response.status_code == 200`; `mock.assert_called_once_with(...)` |
| Python SDK | `client = MySDK(api_key="test"); result = client.resources.list(); assert result.data` |
| OpenAPI | Validate response against schema: `openapi_core.validate_response(spec, request, response)` |
| JS/TS CLI | `const { stdout, exitCode } = await execa('./cli', ['cmd', '--flag']); expect(exitCode).toBe(0)` |
| React | `render(<Component />); expect(screen.getByRole(...)).toBeInTheDocument()` |
| Zig | `try std.testing.expectEqual(expected, actual);` no error union |
| Rust gRPC | `let resp = client.get_resource(request).await?; assert_eq!(resp.into_inner().id, expected_id);` |
| Rust HTTP | `let resp = app.oneshot(Request::get("/api/v1/items")).await; assert_eq!(resp.status(), 200);` |
| Go Terraform | `resource.Test(t, resource.TestCase{Steps: []resource.TestStep{{Config: cfg, Check: ...}}})` |
| Go CLI | `cmd := rootCmd; cmd.SetArgs([]string{"create", "--name", "test"}); assert.NoError(t, cmd.Execute())` |
| Terraform | `terraform plan` exits 0; `terraform apply` is idempotent on second run |
| Shell | `run my_script.sh --flag; [ "$status" -eq 0 ]; [[ "$output" == *"expected"* ]]` (bats) |
| Perl | `ok(process("input") eq "expected", "happy path");` (Test::More) |
| Ruby | `expect(subject.call(valid_input)).to eq(expected)` (RSpec) |

### TIER 2 — EDGE CASES

For each input parameter:

- [ ] Empty / blank / null / None / undefined / `Option::None` / `null` (Zig) / `nil` (Go/Ruby)
- [ ] Zero, negative, float overflow, NaN, Infinity, `i64::MAX`, `i64::MIN`, `Number.MAX_SAFE_INTEGER`
- [ ] Maximum-length and minimum-length strings
- [ ] Unicode: multibyte (CJK 中文), emoji (👨‍👩‍👧‍👦), RTL (Arabic/Hebrew), combining chars (é vs é), zero-width joiners
- [ ] HTML entities (`&amp;`), angle brackets, backticks in user content
- [ ] Whitespace-only, leading/trailing whitespace, embedded newlines, CRLF vs LF
- [ ] Single-element vs many-element collections; empty collections
- [ ] Duplicate entries in lists/sets/vecs
- [ ] Missing optional keys in dicts/structs/objects
- [ ] Pagination boundaries: page=0, page=999999, per_page=0, per_page=-1
- [ ] Date/time: epoch (1970-01-01), far-future (9999-12-31), leap second, DST transitions, timezone boundaries

**Stack-specific edge cases:**

| Stack | Additional edge cases |
|-------|----------------------|
| Python SDK | Expired/rotated API keys; paginated responses with `next_page=null`; rate-limit retry with `Retry-After` header |
| OpenAPI | Request missing required field; extra unknown field (additionalProperties); wrong content-type header |
| JS/TS CLI | No TTY (piped input); `--help` with/without subcommand; env var overrides flag; `SIGINT` during operation |
| React | Missing/undefined props; empty `children`; key collisions in lists; controlled vs uncontrolled input switching |
| Zig | Sentinel-terminated slices; comptime vs runtime paths; `@alignCast` with misaligned data |
| Rust | `&str` vs `String` ownership; lifetime edge cases; `Vec<T>` capacity 0; `From`/`Into` trait boundary |
| Go Terraform | Attribute unknown at plan time (`(known after apply)`); resource import with partial state; `ForceNew` trigger |
| Go CLI | Flag/env/config file precedence conflict; `--output=json` vs `--output=table`; empty stdin with `--` separator |
| Terraform | `count = 0` vs `count = 1`; `for_each` with empty map; conditional resource with `? :` ternary; module source change |
| Shell | Unquoted variables with spaces/globs; `set -euo pipefail` behavior; heredoc with variable expansion; empty `$@` |
| Perl | `undef` vs `""` vs `0`; array vs scalar context; regex with unicode modifiers; tainted input |
| Ruby | `nil` vs `false` vs `0`; `freeze`/`frozen?` on mutated strings; `Hash` with default proc; symbol vs string keys |

### TIER 3 — NEGATIVE / ERROR PATHS

- [ ] Invalid input types (string where int expected, wrong enum variant)
- [ ] Authentication/authorization failures (no token, expired, wrong role, revoked)
- [ ] Downstream service errors (HTTP 500, timeout, connection refused, DNS failure)
- [ ] Database constraint violations (duplicate key, FK violation, deadlock, lock timeout)
- [ ] File not found / permission denied / disk full
- [ ] Malformed/corrupt input (truncated JSON, invalid PDF, broken HTML, partial UTF-8)
- [ ] Assert correct error codes, messages; no sensitive data in error responses

**Stack-specific error paths:**

| Stack | Error path specifics |
|-------|---------------------|
| Python SDK | `raise ApiError(status=429, retry_after=30)` on rate limit; `ConnectionError` on network failure; stale pagination cursor |
| OpenAPI | 4xx responses match error schema; `406 Not Acceptable` for wrong `Accept` header |
| JS/TS CLI | Exit code 1 for user error, 2 for usage error; stderr for errors, stdout for output; `--no-color` disables ANSI |
| React | Error boundary renders fallback UI; `onError` callbacks fire with error object; suspended component shows fallback |
| Zig | Assert error set members; test `catch` and `orelse` branches; `OutOfMemory` from allocator |
| Rust gRPC | `tonic::Status::not_found()`; streaming error mid-response; deadline exceeded; invalid protobuf payload |
| Rust HTTP | `axum::extract::rejection`; missing `Content-Type`; payload too large (413) |
| Go Terraform | `diag.HasError()` for invalid config; `ImportStateVerify` fails on drift; provider returns unexpected error |
| Go CLI | Invalid flag combination returns non-zero; `--config` points to missing file; JSON output on error includes `"error"` key |
| Terraform | `terraform validate` catches bad HCL; `terraform plan` fails on missing provider; cyclic dependency detected |
| Shell | Non-zero exit from subcommand propagates; `trap` handler fires on SIGTERM; missing required env var prints usage |
| Perl | `die` / `croak` caught by `eval`; file handle errors with `or die $!`; `use strict` catches undeclared vars |
| Ruby | `raise CustomError` caught by `rescue`; `ActiveRecord::RecordNotFound`; `Timeout::Error` from HTTP calls |

### TIER 4 — REAL RENDERING / OUTPUT FIDELITY

For any generated artifacts (PDF, HTML, CSV, images, binary formats, CLI output, Terraform plans):

- [ ] Use the REAL renderer (not mocked) — assert artifact is structurally valid
- [ ] For PDFs: `%PDF` header, non-empty bytes, `application/pdf` content type, text extraction
- [ ] For HTML: valid DOM structure, correct semantic elements, no broken tags
- [ ] Assert layout/ordering (e.g., "Header before Footer" via positional extraction)
- [ ] Test with REAL production templates, not synthetic stubs
- [ ] Verify deterministic data markers survive the render pipeline (names, dates, amounts, currency)
- [ ] Empty data → graceful "no data" message renders
- [ ] Large data (100+ rows) → no truncation, crash, or memory blowup
- [ ] Special characters in data flowing into templates

**Stack-specific fidelity:**

| Stack | Fidelity checks |
|-------|----------------|
| Python SDK | Response model `to_dict()` → `from_dict()` round-trip equality; pagination `__iter__` yields all pages |
| OpenAPI | Generated client code compiles; request/response examples in spec actually validate against schema |
| JS/TS CLI | `--json` output is valid JSON and parseable; `--plain` strips ANSI; `--help` text matches documented usage |
| React | Snapshot tests for complex renders; `axe` accessibility audit passes; responsive breakpoints render correctly |
| Zig | Binary output byte-exact with golden reference; serialized structs round-trip through `@bitCast` |
| Rust gRPC | Protobuf serialization round-trip; streaming response collects all chunks; `tonic::codec` golden test |
| Go Terraform | `terraform plan` output contains expected resource changes; state file is valid JSON after apply |
| Terraform | `terraform show -json` plan output matches expected diff; no unexpected destroy/recreate |
| Shell | stdout matches golden file via `diff`; stderr is empty on success; output is pipe-safe (no ANSI when `! -t 1`) |

### TIER 5 — CONCURRENCY & RACE CONDITIONS

- [ ] Concurrent identical requests (same user, same endpoint, same payload)
- [ ] Concurrent conflicting writes (two actors updating the same resource)
- [ ] Assert no data corruption: final state is consistent
- [ ] Assert idempotency where expected (retry same request → same result)
- [ ] Assert proper locking (optimistic/pessimistic) if applicable
- [ ] Database transaction isolation: no dirty reads, no phantom reads under load

**Stack-specific concurrency (≥5 concurrent calls):**

| Stack | How to test concurrency |
|-------|------------------------|
| Python | `concurrent.futures.ThreadPoolExecutor` or `asyncio.gather` |
| Python SDK | Concurrent `client.resources.create()` calls; assert no duplicate IDs returned |
| JS/TS CLI | `Promise.all(Array(10).fill().map(() => execa('./cli', ['create'])))` |
| React | Rapid `userEvent.click` or state updates; `useEffect` cleanup race; `AbortController` cancellation |
| Zig | `std.Thread.spawn`; test with `--release-safe` for safety checks under optimization |
| Rust | `tokio::spawn` or `std::thread::scope` with `Arc<Mutex<_>>`; run under MIRI if safe |
| Go | `sync.WaitGroup` + goroutines; `-race` flag; `t.Parallel()` |
| Terraform | N/A (single-threaded apply); test state locking with DynamoDB backend |
| Shell | Background jobs `&` + `wait`; file locking with `flock`; signal delivery during operation |
| Perl | `threads` module + `threads::shared`; or `fork()` with IPC |
| Ruby | `Thread.new` with shared state; `Mutex` assertions; connection pool under load |

### TIER 6 — INTEGRATION VERIFICATION

- [ ] End-to-end through the real stack (HTTP request → response, not just unit)
- [ ] Full middleware chain executes (auth, rate-limit, logging, CORS)
- [ ] Serialization/deserialization round-trip (request → DB → response matches)
- [ ] Cross-module integration: A calls B with real B (not mocked)
- [ ] Database migrations + schema compatible with test data
- [ ] External service contract tests: request shape matches API docs

**Stack-specific integration:**

| Stack | Integration pattern |
|-------|---------------------|
| Python | Django `TestCase` with real DB; `APIClient().post()` end-to-end |
| Python SDK | Mock HTTP server (`responses` / `httpretty` / `respx`) with real SDK client; assert request headers/body |
| OpenAPI | `schemathesis` / `dredd` against running server; assert all endpoints match spec |
| JS/TS CLI | Execute binary as subprocess; parse stdout; verify file system side effects |
| React | Full page render with `BrowserRouter`, context providers, MSW for API mocks |
| Zig | Test across compilation units; test C-ABI boundaries; `@cImport` interop |
| Rust gRPC | `tonic` test server + client in same process; tower `ServiceExt::ready().call()` |
| Rust HTTP | `axum::test::TestServer` with real router, middleware, extractors |
| Go Terraform | Acceptance tests with `TF_ACC=1`; `resource.Test()` with real or mocked provider backend |
| Go CLI | `cmd.SetArgs()` + `cmd.Execute()` with captured stdout/stderr; test config file loading |
| Terraform | `terraform init && terraform plan && terraform apply -auto-approve && terraform destroy` cycle |
| Shell | Source functions and call them; test with `bats` using `setup`/`teardown`; `tmpdir` isolation |
| Perl | `Test::WWW::Mechanize` for web apps; DBI with SQLite for DB integration |
| Ruby | `RSpec` request specs with `rack-test`; `FactoryBot` for data setup; `VCR` for HTTP recording |

### TIER 7 — REGRESSION SAFETY NETS

- [ ] Before/after comparison: same input → same (or explicitly expected different) output
- [ ] Golden output artifacts: pin a reference PDF/JSON/HTML and assert semantic equivalence
- [ ] Canary imports: assert every `from X import Y` / `use crate::X` / `@import` still resolves
- [ ] Version-sniff test: assert installed dependency versions match expected range
- [ ] For dependency upgrades: run the OLD test suite against NEW code — all must pass

**Stack-specific regression:**

| Stack | Regression specifics |
|-------|---------------------|
| Python SDK | Public API surface unchanged: assert all `__all__` exports resolve; `dir(client)` matches expected methods |
| OpenAPI | Breaking change detector: `oasdiff breaking old.yaml new.yaml` exits 0 |
| JS/TS CLI | Golden stdout/stderr files per command; `--version` matches `package.json` version |
| React | Prop-type contract tests; component API shape unchanged; CSS module class names stable |
| Zig | `@sizeOf(MyStruct)` matches expected; ABI stability for exported functions |
| Rust | `#[cfg(test)] const _: () = assert!(std::mem::size_of::<MyStruct>() == N);` for ABI stability |
| Go Terraform | Schema version test: provider schema diff after upgrade; state migration functions tested per version |
| Terraform | `terraform plan` on existing state shows no changes (drift-free); module version pin in `required_providers` |
| Shell | Golden file tests: `diff <(./script.sh --help) expected_help.txt` |
| Perl | `use_ok('My::Module')` for all modules; method resolution via `can()` |
| Ruby | `respond_to?` checks; `gemspec` dependency version constraints; `bundle exec` resolves cleanly |

### TIER 8 — SECURITY & ROBUSTNESS

- [ ] SQL injection attempts in string inputs
- [ ] XSS payloads in user-supplied content that renders to HTML/PDF
- [ ] Path traversal in file-based inputs (`../../etc/passwd`)
- [ ] Rate limiting works (if applicable)
- [ ] CSRF/CORS protections intact
- [ ] Secret detection is handled by `gitleaks` / pre-commit hooks (not duplicated in unit tests)

**Stack-specific security:**

| Stack | Security specifics |
|-------|-------------------|
| Python SDK | API key not logged in debug output; HTTPS enforced; certificate verification enabled by default |
| OpenAPI | Authentication schemes enforced per-endpoint; response does not leak internal errors to client |
| JS/TS CLI | `--token` not visible in process list (`/proc/PID/cmdline`); secrets read from env not flags |
| React | `dangerouslySetInnerHTML` never with unsanitized input; CSP-compatible; no `eval()` |
| Zig | No `@ptrCast` / `@intFromPtr` without bounds validation; `@memcpy` length checked |
| Rust | No `unsafe` without safety comment + test; `#[deny(unsafe_code)]` on library crates |
| Go Terraform | Provider does not log sensitive attributes; `Sensitive: true` on secret schema fields |
| Terraform | No secrets in `terraform.tfvars` committed; remote state encryption enabled |
| Shell | All variables quoted (`"$var"` not `$var`); no `eval` with user input; `mktemp` for temp files |
| Perl | Taint mode (`-T`) for CGI; `use strict; use warnings;`; parameterized DBI queries |
| Ruby | `ActiveRecord` parameterized queries; `Rails.application.credentials` not hardcoded; `Brakeman` passes |

**OWASP Agent Security — data sent to or propagated through agents:**

When any code path sends data directly to an agent (LLM, tool-calling agent, autonomous workflow) or indirectly propagates data to an agent through intermediate systems (queues, databases, APIs, event buses), the following checks apply:

- [ ] **Prompt injection resistance:** User-controlled input is never concatenated raw into agent prompts; use structured message formats, system/user role separation, or parameterized templates
- [ ] **Untrusted input handling:** All external input reaching an agent is validated, type-checked, and length-bounded before inclusion; reject or sanitize payloads that contain instruction-like content (`ignore previous`, `you are now`, XML/JSON injection markers)
- [ ] **Least privilege for tools/actions:** Agents have access only to the minimum set of tools required for the task; tool permissions are scoped per-invocation, not globally granted
- [ ] **Data minimization and secret redaction:** Only the data necessary for the agent's task is included in the prompt/context; secrets, credentials, PII, and internal identifiers are stripped or replaced with opaque references before reaching the agent
- [ ] **Output validation before execution:** Agent-generated outputs (code, commands, API calls, file writes) are validated against an allowlist or schema before any side effect executes; never `eval()` or `exec()` raw agent output
- [ ] **Authorization at every hop:** Each system in the data flow (caller → intermediary → agent → tool) independently verifies authorization; an agent inherits the caller's permissions, not elevated service-level permissions
- [ ] **Audit logging and traceability:** Every agent invocation logs: who triggered it, what input was provided, what tools were called, what output was produced, and what side effects occurred; logs are tamper-resistant and queryable
- [ ] **Fail-closed on trust/safety failure:** If any trust check (input validation, output validation, authorization, rate limit) fails, the agent call is rejected — never fall back to a permissive path or silently proceed with partial validation

**Direct vs. indirect data flow checks:**

| Data flow | What to test |
|-----------|-------------|
| Direct (caller → agent) | Input sanitization at call site; prompt template immutability; tool scope matches caller intent |
| Indirect (caller → queue/DB/API → agent) | Data integrity preserved through intermediary; no injection via stored fields; agent re-validates input regardless of upstream checks |

### TIER 9 — CODE DUPLICATION & DRY COMPLIANCE

- [ ] No copy-pasted logic across test files — extract shared fixtures/helpers/builders
- [ ] No duplicated business logic between modules — if found, flag and refactor to single source
- [ ] Test helpers themselves are tested if they contain logic (not just wiring)
- [ ] No magic strings/URLs/ports repeated across tests — use constants or fixtures
- [ ] Assert no function exceeds 3 near-identical call patterns without parameterization
- [ ] If test setup is >10 lines, extract to a fixture/builder/factory

**Parameterized test patterns by stack:**

| Stack | How to parameterize |
|-------|---------------------|
| Python | `@pytest.mark.parametrize("input,expected", [...])` |
| Rust | `macro_rules!` generated tests or `#[test_case]` crate |
| Zig | inline `for` over test case tuples with `std.testing` |
| React/TS | `it.each([...])` / `describe.each([...])` |
| Go | table-driven `[]struct{ name, input, want }` in `t.Run` loop |
| Shell (bats) | Loop over cases in `@test` with array |
| Perl | `Test::More` with `foreach` over `@cases` |
| Ruby | `RSpec` `shared_examples` + `it_behaves_like`; `subject { described_class.new }` |

### TIER 10 — CONSTANTS & MAGIC VALUES POLICY

- [ ] No magic numbers in production code — every numeric literal has a named constant (0, 1, -1 exempt)
- [ ] No magic strings (URLs, keys, status names, error messages) — use constants or enums
- [ ] Constants defined once, in one canonical location
- [ ] Enum/union variants preferred over string comparisons for state/status/type fields
- [ ] Configuration values (timeouts, retries, limits) are injectable, not hardcoded
- [ ] Test data uses realistic but obviously fake values (`user@example.com`, `555-0100`, `Acme Corp`)
- [ ] No test depends on the specific VALUE of a constant — test behavior, pin constant separately

**Canonical constant locations by stack:**

| Stack | Where constants live |
|-------|---------------------|
| Python | `constants.py` or module-level `UPPER_SNAKE_CASE` |
| Python SDK | `sdk/constants.py`; version in `__version__`; API base URL in config, not code |
| JS/TS | `src/constants.ts` as named `export const`; no string enums without explicit values |
| React | co-located `constants.ts` per feature or shared `@/constants` |
| Zig | `pub const` in dedicated namespace or file; `comptime` for compile-time known values |
| Rust | `const` or `static` in `constants.rs` or crate root; `enum` for variants, never strings |
| Go | `const` block in `constants.go` per package; `iota` for enumerations |
| Terraform | `variables.tf` for inputs; `locals {}` for derived values; no inline strings in resources |
| Shell | `readonly MY_CONST="value"` at top of script; sourced from `lib/constants.sh` |
| Perl | `use constant` or `Readonly` module; `our @EXPORT_OK` in module |
| Ruby | `UPPER_CASE` constants in module namespace; `freeze` string constants |

### TIER 11 — PERFORMANCE & RESOURCE SAFETY

- [ ] No unbounded allocations: test with large input and assert memory stays within bounds
- [ ] No O(n²) or worse in hot paths: benchmark or assert timing for large-N inputs
- [ ] File handles / connections are closed (test with resource tracking or mocks)
- [ ] Timeout guards on all async/network tests (no hanging CI)

**Stack-specific performance:**

| Stack | Performance specifics |
|-------|----------------------|
| Python | `assertNumQueries` for Django; no N+1 queries; `@pytest.mark.timeout(10)` |
| Python SDK | Connection pooling reuses sessions; retry backoff is bounded; pagination doesn't load all pages in memory |
| JS/TS CLI | Startup time under 500ms; no synchronous `fs.readFileSync` in hot paths |
| React | No re-render storms; `React.memo` / `useMemo` where measured; bundle size assertion |
| Zig | `std.testing.allocator` detects leaks automatically; `@memset`/`@memcpy` bounds checked; no `GeneralPurposeAllocator` in prod without `deinit` |
| Rust | No `.clone()` in hot loops; `#[bench]` or criterion benchmarks; `cargo clippy` pedantic |
| Rust gRPC | Streaming doesn't buffer full response in memory; connection pool size bounded |
| Go | `-benchmem` for allocation tracking; `pprof` for profiling; query count via `DB.Stats()` |
| Go Terraform | Provider `Read` doesn't make unnecessary API calls; `PlanResourceChange` is fast |
| Terraform | `terraform plan` completes under 60s for 100 resources; parallelism setting tested |
| Shell | No subshell forks in loops (use builtins); `read -r` instead of `cat | while` |
| Perl | `Benchmark` module for timing; `Devel::NYTProf` for profiling; avoid regex catastrophic backtracking |
| Ruby | `bullet` gem for N+1 detection; `benchmark-ips` for hot paths; `ObjectSpace.count_objects` delta |

### TIER 12 — API CONTRACT & SCHEMA COMPLIANCE

> Note: Secret/credential leak detection is handled by `gitleaks` and pre-commit hooks, not duplicated here.

- [ ] Every public API endpoint has a matching schema definition (OpenAPI/protobuf/GraphQL)
- [ ] Request and response shapes match the schema — test with a validator, not just status codes
- [ ] Breaking changes are detected before merge (field removed, type changed, required added)
- [ ] Versioning is correct: API version header/path matches handler version
- [ ] Deprecation warnings present for deprecated fields/endpoints
- [ ] SDK/client code generated from spec actually compiles and passes type checking

**Stack-specific contract compliance:**

| Stack | Contract tools and patterns |
|-------|----------------------------|
| Python | `pydantic` model validation; `jsonschema.validate()` against OpenAPI components |
| Python SDK | `mypy --strict` on public API surface; `__all__` exports match docs; method signatures match spec |
| OpenAPI | `schemathesis run spec.yaml --base-url=...` for fuzz testing; `oasdiff breaking old.yaml new.yaml` for breaking changes; `spectral lint spec.yaml` for style |
| JS/TS CLI | `zod` / `yargs` type inference matches expected; `--help` output tested against documented usage |
| React | Prop types / TypeScript interfaces match API response types; `tsc --noEmit` passes |
| Zig | Exported function signatures match header file; `@typeInfo` compile-time checks on public structs |
| Rust gRPC | Protobuf schema backward-compatible (`buf breaking`); generated code compiles with `--strict` |
| Rust HTTP | `utoipa` / `aide` generated OpenAPI matches handwritten spec; request extractor types match schema |
| Go Terraform | Provider schema matches docs; `schema.Schema` field types match API response types; `ValidateFunc` covers all constraints |
| Go CLI | `cobra` command tree matches `--help` output; `man` page generation matches actual flags |
| Terraform | Module `variables.tf` types match usage; `outputs.tf` values are non-sensitive where documented; module version constraints in `required_providers` |
| Shell | `--help` text matches README usage section; exit codes documented and tested |
| Perl | POD documentation matches method signatures; `Test::Pod` and `Test::Pod::Coverage` pass |
| Ruby | `YARD` doc coverage; `rspec-openapi` generates spec from tests; `rubocop` passes |

---

## Output Format

For each test:
1. Name following stack convention:
   - Python/Go: `test_<module>_<function>_<scenario>`
   - Rust: `fn <scenario>()` in `mod tests`
   - Zig: `test "<scenario>"`
   - React/TS: `it("should <scenario>")`
   - Shell/bats: `@test "<scenario>"`
   - Perl: `subtest "<scenario>"`
   - Ruby: `it "<scenario>"`
2. Tier(s) it satisfies
3. The actual test code
4. What regression it catches if it fails

## Completeness Matrix

After writing all tests, produce:

```
## Coverage Matrix: {{module/folder}}
| Function/Endpoint | T1 | T2 | T3 | T4 | T5 | T6 | T7 | T8 | T9 | T10 | T11 | T12 |
|-|-|-|-|-|-|-|-|-|-|-|-|-|
| function_a        | ✅ | ✅ | ✅ | N/A| ✅ | ✅ | ✅ | ✅ | ✅ | ✅  | ✅  | ✅  |
| function_b        | ✅ | ✅ | ❌ | ✅ | N/A| ✅ | ✅ | ✅ | ✅ | ✅  | ❌  | N/A |

❌ = skipped — must include written justification
N/A = not applicable — must state why
```

## Validation Gate

All tests must pass in a single CI gate invocation.
Report: exact command, output summary (X passed, Y failed, Z skipped), and coverage delta.

---

## Stack Quick-Reference Examples

### 1. Python (pytest)
```python
@pytest.mark.parametrize("input_val,expected", [
    ("", "default"), ("café ☕", "café ☕"), (None, "default"), ("a" * 10000, "truncated"),
])
def test_process_edge_cases(input_val, expected):  # T2 + T9
    assert process(input_val) == expected

def test_concurrent_writes_no_corruption():  # T5
    with ThreadPoolExecutor(max_workers=10) as pool:
        futures = [pool.submit(create_order, user_id=1) for _ in range(10)]
        results = [f.result() for f in futures]
    assert len(set(r.id for r in results)) == 10
```

### 2. Python SDK
```python
@responses.activate
def test_sdk_list_resources_paginates():  # T1 + T6
    responses.get("https://api.example.com/v1/items?page=1", json={"data": [{"id": 1}], "next": "?page=2"})
    responses.get("https://api.example.com/v1/items?page=2", json={"data": [{"id": 2}], "next": None})
    client = MySDK(api_key="test-key")
    items = list(client.items.list())
    assert len(items) == 2

def test_sdk_rate_limit_retry():  # T3
    responses.get("https://api.example.com/v1/items", status=429, headers={"Retry-After": "1"})
    responses.get("https://api.example.com/v1/items", json={"data": []})
    client = MySDK(api_key="test-key")
    result = client.items.list()
    assert result.data == []
```

### 3. OpenAPI
```python
# schemathesis for fuzzing (T2 + T8 + T12)
# CLI: schemathesis run http://localhost:8080/openapi.json --checks all
# Programmatic:
@given(case=schemathesis.from_uri("http://localhost:8080/openapi.json").as_strategy())
def test_api_conforms_to_spec(case):
    response = case.call()
    case.validate_response(response)

# Breaking change detection (T7 + T12)
# CI: oasdiff breaking main.yaml feature-branch.yaml --fail-on ERR
```

### 4. JS/TS CLI (vitest + execa)
```typescript
test('create command outputs JSON with --json flag', async () => {  // T1 + T4
  const { stdout, exitCode } = await execa('./bin/cli', ['create', '--name', 'test', '--json']);
  expect(exitCode).toBe(0);
  const parsed = JSON.parse(stdout);
  expect(parsed).toHaveProperty('id');
});

test('exits 2 on unknown flag', async () => {  // T3
  const result = await execa('./bin/cli', ['--badflg'], { reject: false });
  expect(result.exitCode).toBe(2);
  expect(result.stderr).toContain('unknown flag');
});

test('respects NO_COLOR env', async () => {  // T2
  const { stdout } = await execa('./bin/cli', ['status'], { env: { NO_COLOR: '1' } });
  expect(stdout).not.toMatch(/\x1b\[/);
});
```

### 5. React / TypeScript (vitest + testing-library)
```tsx
it.each([
  ['empty name', { ...mockUser, name: '' }],
  ['unicode name', { ...mockUser, name: '田中太郎 👨‍💻' }],
  ['missing email', { ...mockUser, email: undefined }],
])('handles %s gracefully', (_label, user) => {  // T2
  expect(() => render(<UserProfile user={user} />)).not.toThrow();
});

it('passes axe accessibility audit', async () => {  // T8
  const { container } = render(<UserProfile user={mockUser} />);
  const results = await axe(container);
  expect(results).toHaveNoViolations();
});

it('suspense shows fallback while loading', async () => {  // T3
  render(<Suspense fallback={<Spinner />}><LazyProfile /></Suspense>);
  expect(screen.getByRole('progressbar')).toBeInTheDocument();
  await waitFor(() => expect(screen.getByRole('heading')).toBeInTheDocument());
});
```

### 6. Zig (zig test) with zap/httpz
```zig
test "GET /api/items returns 200 with JSON body" {  // T1
    const allocator = std.testing.allocator;
    var app = try App.init(allocator);
    defer app.deinit();
    const resp = try app.request(.GET, "/api/items", null);
    defer allocator.free(resp.body);
    try std.testing.expectEqual(@as(u16, 200), resp.status);
    const parsed = try std.json.parseFromSlice(ItemList, allocator, resp.body, .{});
    defer parsed.deinit();
    try std.testing.expect(parsed.value.items.len > 0);
}

test "POST with corrupt JSON returns 400" {  // T3
    const allocator = std.testing.allocator;
    var app = try App.init(allocator);
    defer app.deinit();
    const resp = try app.request(.POST, "/api/items", "{invalid");
    defer allocator.free(resp.body);
    try std.testing.expectEqual(@as(u16, 400), resp.status);
}

test "no memory leaks in request handler" {  // T11
    const allocator = std.testing.allocator;  // auto-detects leaks
    var app = try App.init(allocator);
    defer app.deinit();
    for (0..100) |_| {
        const resp = try app.request(.GET, "/api/items", null);
        allocator.free(resp.body);
    }
}
```

### 7. Rust gRPC (tonic) + HTTP (axum)
```rust
#[tokio::test]
async fn grpc_get_item_returns_valid_response() {  // T1
    let (client, _server) = spawn_test_server().await;
    let resp = client.get_item(GetItemRequest { id: "abc".into() }).await.unwrap();
    assert_eq!(resp.into_inner().name, "expected-item");
}

#[tokio::test]
async fn grpc_invalid_id_returns_not_found() {  // T3
    let (client, _server) = spawn_test_server().await;
    let status = client.get_item(GetItemRequest { id: "".into() }).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn http_concurrent_creates_no_duplicates() {  // T5
    let app = test_app().await;
    let handles: Vec<_> = (0..10).map(|_| {
        let app = app.clone();
        tokio::spawn(async move { app.post("/items").json(&new_item()).send().await })
    }).collect();
    let results: Vec<_> = futures::future::join_all(handles).await;
    let ids: HashSet<_> = results.iter().map(|r| r.as_ref().unwrap().json::<Item>().id).collect();
    assert_eq!(ids.len(), 10);
}

#[test]
fn protobuf_round_trip() {  // T12
    let original = MyMessage { field: "test".into() };
    let bytes = original.encode_to_vec();
    let decoded = MyMessage::decode(bytes.as_slice()).unwrap();
    assert_eq!(original, decoded);
}
```

### 8. Go — Terraform Provider SDK
```go
func TestAccResourceItem_basic(t *testing.T) {  // T1 + T6
    resource.Test(t, resource.TestCase{
        ProtoV6ProviderFactories: testAccProviders,
        Steps: []resource.TestStep{
            {Config: testAccItemConfig("test-item"), Check: resource.ComposeTestCheckFunc(
                resource.TestCheckResourceAttr("mycloud_item.test", "name", "test-item"),
                resource.TestCheckResourceAttrSet("mycloud_item.test", "id"),
            )},
            {ResourceName: "mycloud_item.test", ImportState: true, ImportStateVerify: true},  // T7
        },
    })
}

func TestAccResourceItem_disappears(t *testing.T) {  // T3
    resource.Test(t, resource.TestCase{
        ProtoV6ProviderFactories: testAccProviders,
        Steps: []resource.TestStep{
            {Config: testAccItemConfig("ephemeral"), Check: resource.ComposeTestCheckFunc(
                testAccCheckItemExists("mycloud_item.test"),
                testAccCheckItemDelete("mycloud_item.test"),  // delete externally
            ), ExpectNonEmptyPlan: true},
        },
    })
}
```

### 9. Go CLI (cobra)
```go
func TestCreateCmd_Success(t *testing.T) {  // T1
    buf := new(bytes.Buffer)
    cmd := NewRootCmd()
    cmd.SetOut(buf)
    cmd.SetArgs([]string{"create", "--name", "test-resource"})
    assert.NoError(t, cmd.Execute())
    assert.Contains(t, buf.String(), "created successfully")
}

func TestCreateCmd_JSONOutput(t *testing.T) {  // T4
    buf := new(bytes.Buffer)
    cmd := NewRootCmd()
    cmd.SetOut(buf)
    cmd.SetArgs([]string{"create", "--name", "test", "--output", "json"})
    assert.NoError(t, cmd.Execute())
    var result map[string]interface{}
    assert.NoError(t, json.Unmarshal(buf.Bytes(), &result))
    assert.Contains(t, result, "id")
}

func TestCreateCmd_EnvOverridesFlag(t *testing.T) {  // T2
    t.Setenv("MYAPP_API_URL", "https://override.example.com")
    cmd := NewRootCmd()
    cmd.SetArgs([]string{"create", "--name", "test"})
    assert.NoError(t, cmd.Execute())
    // assert the override URL was used
}
```

### 10. Terraform HCL (terratest)
```go
func TestTerraformBasicExample(t *testing.T) {  // T1 + T6
    opts := &terraform.Options{TerraformDir: "../examples/basic"}
    defer terraform.Destroy(t, opts)
    terraform.InitAndApply(t, opts)
    output := terraform.Output(t, opts, "instance_id")
    assert.NotEmpty(t, output)
}

func TestTerraformIdempotent(t *testing.T) {  // T5 + T7
    opts := &terraform.Options{TerraformDir: "../examples/basic"}
    defer terraform.Destroy(t, opts)
    terraform.InitAndApply(t, opts)
    exitCode := terraform.PlanExitCode(t, opts)
    assert.Equal(t, 0, exitCode, "second plan should show no changes")
}

func TestTerraformValidate(t *testing.T) {  // T12
    opts := &terraform.Options{TerraformDir: "../modules/network"}
    terraform.Init(t, opts)
    terraform.Validate(t, opts)
}
```

### 11. Shell (bats)
```bash
@test "script exits 0 with valid input" {  # T1
  run ./deploy.sh --env staging --version 1.2.3
  [ "$status" -eq 0 ]
  [[ "$output" == *"deployed 1.2.3 to staging"* ]]
}

@test "script exits 1 with missing required flag" {  # T3
  run ./deploy.sh --env staging
  [ "$status" -eq 1 ]
  [[ "$output" == *"--version is required"* ]]
}

@test "script handles spaces in arguments" {  # T2
  run ./deploy.sh --env "staging env" --version "1.2.3"
  [ "$status" -eq 0 ]
}

@test "help text matches golden file" {  # T7
  run ./deploy.sh --help
  diff <(echo "$output") tests/fixtures/help.golden.txt
}

@test "no output to stdout when --quiet" {  # T4
  run ./deploy.sh --env staging --version 1.2.3 --quiet
  [ "$status" -eq 0 ]
  [ -z "$output" ]
}
```

### 12. Perl (Test::More) / Ruby (RSpec)
```perl
# Perl — Test::More
use Test::More tests => 4;

subtest 'happy path' => sub {  # T1
    my $result = process("valid input");
    is($result->{status}, 'ok', 'returns ok status');
    is($result->{code}, 200, 'returns 200');
};

subtest 'edge cases' => sub {  # T2
    is(process("")->{status}, 'default', 'empty string');
    is(process(undef)->{status}, 'default', 'undef input');
    is(process("café ☕")->{status}, 'ok', 'unicode input');
};

subtest 'error path' => sub {  # T3
    eval { process({invalid => 1}) };
    like($@, qr/InvalidInput/, 'dies on wrong type');
};
```

```ruby
# Ruby — RSpec
RSpec.describe OrderService do
  describe '#create' do
    subject { described_class.new.create(params) }

    context 'with valid params' do  # T1
      let(:params) { { item: 'widget', qty: 1 } }
      it { is_expected.to be_success }
      it { expect(subject.order.id).to be_present }
    end

    context 'edge cases' do  # T2
      it 'handles unicode' do
        expect(described_class.new.create(item: '日本語 ☕', qty: 1)).to be_success
      end
      it 'handles nil quantity' do
        expect { described_class.new.create(item: 'x', qty: nil) }.to raise_error(ArgumentError)
      end
    end

    context 'concurrent creates' do  # T5
      it 'does not produce duplicate IDs' do
        results = Array.new(10) { Thread.new { described_class.new.create(item: 'x', qty: 1) } }.map(&:value)
        expect(results.map(&:order).map(&:id).uniq.size).to eq(10)
      end
    end
  end
end
```

---

## When to Use Each Tier

| Scenario | Minimum Tiers |
|---|---|
| Bug fix (single function) | T1, T2, T3, T7, T9, T10 |
| New feature | T1, T2, T3, T6, T9, T10, T11 |
| Dependency upgrade | T1, T4, T7 (canary + golden), T11 |
| PDF / report generation | T1, T2, T4 (all), T5, T9 |
| Auth / security changes | T1, T3, T6, T8 (all) |
| Database migration | T1, T3, T5, T6, T11 |
| UI component (React) | T1, T2, T3, T7 (snapshots), T8 (a11y), T9, T10 |
| Systems code (Rust/Zig) | T1, T2, T3, T5, T11 (memory + perf) |
| SDK / client library | T1, T2, T3, T6, T7 (API surface), T12 (contract) |
| OpenAPI spec change | T2, T7 (breaking change), T12 (all) |
| CLI tool | T1, T2, T3, T4 (output format), T7 (golden files) |
| Terraform module | T1, T3, T5 (idempotent), T6 (apply/destroy), T12 (validate) |
| Shell script | T1, T2, T3, T4 (golden), T7, T8 (quoting) |
| Perl/Ruby service | T1, T2, T3, T5, T6, T9 |
| Full release QA | ALL TIERS |
| Incremental coverage uplift | Start T1+T2+T3 per folder, then expand |
