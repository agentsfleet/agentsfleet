---
Milestone: M11
Workstream: M11_001
Name: API_ERROR_STANDARDIZATION
Status: DONE
Priority: P1 — establishes RFC 7807 error contract; foundational for all handler work
Categories: API
Branch: feat/m11-api-error-standardization
Created: Apr 10, 2026
---

# M11_001 — API Error Standardization

## Goal

Every error response from `zombied` uses a single structured body with
`docs_uri`, `title`, `detail`, and `error_code` fields, served with
`Content-Type: application/problem+json` (RFC 7807). Error codes own their
HTTP status — callers no longer pass `.not_found` / `.bad_request` inline.

**Inspired by:** exonum `components/api/src/backends/actix.rs` — fluent error
builder, `ErrorBody` struct, `application/problem+json` content type, error
code → status mapping table.

**Demo:** `curl /v1/zombies/bad-id` returns:
```json
{
  "docs_uri": "https://docs.usezombie.com/error-codes#UZ-ZMB-001",
  "title": "Zombie not found",
  "detail": "No zombie with id 'bad-id' in this workspace.",
  "error_code": "UZ-ZMB-001",
  "request_id": "019abc..."
}
```
with `Content-Type: application/problem+json` and HTTP 404.

---

## Surface Area Checklist

- [x] **OpenAPI spec update** — yes: response schema changes for all error
  responses across all endpoints. `application/problem+json` content type added.
- [ ] **`zombiectl` CLI changes** — no: CLI parses `error.code` today. After
  this change it reads `error_code` at the top level. CLI must update field
  name. Flag for CLI owner approval.
- [x] **User-facing doc changes** — yes: error reference page at
  `docs.usezombie.com/error-codes` needs to exist (all codes + docs_uri links).
- [x] **Release notes** — patch bump: `0.8.0` → `0.8.1` (error body is
  additive; `error_code` top-level is new, old `error.code` path removed —
  breaking for CLI, non-breaking for typical REST clients).
- [ ] **Schema changes** — no.

---

## Background: Current vs Target

### Current `errorResponse` (common.zig:80)
```zig
writeJson(res, status, .{
    .@"error" = .{
        .code = code,       // "UZ-ZMB-001"
        .message = message, // "Zombie not found"
    },
    .request_id = request_id,
});
```
Problems:
1. Caller must pass `status` — error code does not own its HTTP status.
2. No `docs_uri` — client cannot link to documentation.
3. No `title` / `detail` distinction — one `message` field does both.
4. `Content-Type: application/json` — not `application/problem+json`.
5. Error code is nested under `error.code` — non-standard path.

### Target body (RFC 7807 / exonum-style)
```json
{
  "docs_uri": "https://docs.usezombie.com/error-codes#UZ-ZMB-001",
  "title": "Zombie not found",
  "detail": "No zombie with id '...' in this workspace.",
  "error_code": "UZ-ZMB-001",
  "request_id": "019abc..."
}
```
- `docs_uri` — stable link to docs page for this code.
- `title` — short, human-readable label (same for every occurrence of this code).
- `detail` — instance-specific context (may vary per call).
- `error_code` — top-level, not nested.
- `request_id` — retained for correlation.

---

## Section 1: Error Code Registry

### 1.1 — `error_table.zig`: code → (HTTP status, title, docs_uri)

New file `src/errors/error_table.zig`. Each entry:
```zig
pub const ErrorEntry = struct {
    code: []const u8,
    http_status: std.http.Status,
    title: []const u8,
    docs_uri: []const u8,
};

pub const ERROR_DOCS_BASE = "https://docs.usezombie.com/error-codes#";

pub const TABLE = [_]ErrorEntry{
    .{ .code = "UZ-ZMB-001",      .http_status = .not_found,            .title = "Zombie not found",           .docs_uri = ERROR_DOCS_BASE ++ "UZ-ZMB-001" },
    .{ .code = "UZ-ZMB-INACTIVE", .http_status = .conflict,             .title = "Zombie not active",          .docs_uri = ERROR_DOCS_BASE ++ "UZ-ZMB-INACTIVE" },
    .{ .code = "UZ-ZMB-MSG-TOO-LONG", .http_status = .bad_request,      .title = "Message too long",           .docs_uri = ERROR_DOCS_BASE ++ "UZ-ZMB-MSG-TOO-LONG" },
    .{ .code = "UZ-RUNS-410",     .http_status = .gone,                  .title = "Pipeline v1 removed",        .docs_uri = ERROR_DOCS_BASE ++ "UZ-RUNS-410" },
    .{ .code = "UZ-UNAUTHORIZED", .http_status = .unauthorized,          .title = "Unauthorized",               .docs_uri = ERROR_DOCS_BASE ++ "UZ-UNAUTHORIZED" },
    .{ .code = "UZ-FORBIDDEN",    .http_status = .forbidden,             .title = "Forbidden",                  .docs_uri = ERROR_DOCS_BASE ++ "UZ-FORBIDDEN" },
    .{ .code = "UZ-INVALID-REQ",  .http_status = .bad_request,          .title = "Invalid request",            .docs_uri = ERROR_DOCS_BASE ++ "UZ-INVALID-REQ" },
    // ... all codes from src/errors/codes.zig
};

pub fn lookup(code: []const u8) ?ErrorEntry { ... }
```

### 1.2 — Unit test: every code in `codes.zig` has a TABLE entry
`comptime` loop over all exported constants in `codes.zig`, assert each
appears as a `code` in `TABLE`. New code without a TABLE entry is a compile
error. This prevents silent gaps.

### 1.3 — Unit test: `lookup` returns correct status and title
Spot-check 5 entries: correct HTTP status, non-empty title, docs_uri starts
with `ERROR_DOCS_BASE`.

### 1.4 — File ≤400 lines
If table exceeds 400 lines, split into `error_table_zombie.zig`,
`error_table_pipeline.zig`, etc., imported by a thin `error_table.zig` facade.

---

## Section 2: New `errorResponse` in `common.zig`

### 2.1 — Replace signature
Old: `errorResponse(res, status, code, message, request_id)`
New: `errorResponse(res, code, detail, request_id)`

`status` and `title` are looked up from `error_table.lookup(code)`. Callers
no longer pass status. `detail` replaces `message` — it is the instance-specific
context string.

```zig
pub fn errorResponse(
    res: *httpz.Response,
    code: []const u8,
    detail: []const u8,
    request_id: []const u8,
) void {
    const entry = error_table.lookup(code) orelse error_table.UNKNOWN_ENTRY;
    res.status = @intFromEnum(entry.http_status);
    res.header("Content-Type", "application/problem+json");
    writeJson(res, .{
        .docs_uri = entry.docs_uri,
        .title = entry.title,
        .detail = detail,
        .error_code = code,
        .request_id = request_id,
    });
}
```

### 2.2 — Update all callers in `src/http/handlers/`
Every `errorResponse(res, .not_found, "UZ-ZMB-001", "...", req_id)` call
becomes `errorResponse(res, "UZ-ZMB-001", "...", req_id)`. Mechanical
find-replace across all handlers. No handler logic changes.

### 2.3 — Retain helper shorthands
`internalDbUnavailable`, `internalDbError`, `internalOperationError` keep
their signatures but delegate to new `errorResponse`. No callers change.

### 2.4 — Unit test: errorResponse sets correct status and Content-Type
Mock `httpz.Response`, call `errorResponse(res, "UZ-ZMB-001", "detail", "req")`.
Assert: status == 404, `Content-Type == "application/problem+json"`,
body contains `"docs_uri"`, `"title"`, `"error_code"`, `"request_id"`.

---

## Section 3: OpenAPI + docs

### 3.1 — Error response schema in `openapi.json`
All `4xx`/`5xx` responses use a single `$ref: "#/components/schemas/ErrorBody"`:
```json
"ErrorBody": {
  "type": "object",
  "required": ["docs_uri", "title", "detail", "error_code", "request_id"],
  "properties": {
    "docs_uri": {"type": "string", "format": "uri"},
    "title": {"type": "string"},
    "detail": {"type": "string"},
    "error_code": {"type": "string"},
    "request_id": {"type": "string"}
  }
}
```

### 3.2 — Error code reference page
`docs/error-codes.md` — one row per code: code, HTTP status, title, short
description, common causes. Consumed by the docs site to generate
`docs.usezombie.com/error-codes`. Each row matches a TABLE entry exactly.

### 3.3 — `make lint` passes with no unused imports
Removing the `std.http.Status` argument from callers may leave unused status
imports in some handlers. Lint gate catches them.

### 3.4 — Cross-compile passes
`zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`

---

## Acceptance Criteria

1. `curl /v1/zombies/bad-id` → 404, `Content-Type: application/problem+json`,
   body has `docs_uri`, `title`, `detail`, `error_code`, `request_id` at top level.
2. `curl /v1/zombies/bad-id` → `docs_uri` is `https://docs.usezombie.com/error-codes#UZ-ZMB-001`.
3. No handler in `src/http/handlers/` calls `errorResponse` with a `std.http.Status` argument.
4. `make test` passes — comptime table coverage check passes.
5. `make lint` passes.
6. Cross-compile passes.
7. `openapi.json` uses `$ref: ErrorBody` for all error responses.

---

## Error Contracts

| Condition | Code | HTTP | Title |
|---|---|---|---|
| Unknown error code (not in TABLE) | `UZ-INTERNAL-001` | 500 | Internal error |

---

## Interfaces

### New: `src/errors/error_table.zig`
```zig
pub const ErrorEntry = struct { code, http_status, title, docs_uri }
pub const TABLE: []const ErrorEntry
pub fn lookup(code: []const u8) ?ErrorEntry
pub const UNKNOWN_ENTRY: ErrorEntry  // fallback for unregistered codes
```

### Modified: `src/http/handlers/common.zig`
```zig
// Old:
pub fn errorResponse(res, status: std.http.Status, code, message, request_id) void
// New:
pub fn errorResponse(res, code: []const u8, detail: []const u8, request_id: []const u8) void
```

---

## Spec-Claim Tracing

| Claim | Test | Status |
|---|---|---|
| Every code in codes.zig has TABLE entry | §1.2 comptime | PENDING |
| lookup returns correct status + title | §1.3 unit | PENDING |
| errorResponse sets 404 + problem+json for ZMB-001 | §2.4 unit | PENDING |
| No handler passes status arg | §2.2 acceptance criterion 3 | PENDING |
| openapi.json uses ErrorBody ref | §3.1 acceptance criterion 7 | PENDING |

---

## Verification Plan

```bash
make build
make lint
make test
zig build -Dtarget=x86_64-linux
zig build -Dtarget=aarch64-linux

# Manual check
curl -i http://localhost:7000/v1/zombies/bad-id \
  -H "Authorization: Bearer $TOKEN"
# Expect: HTTP/1.1 404
# Content-Type: application/problem+json
# {"docs_uri":"https://docs.usezombie.com/error-codes#UZ-ZMB-001","title":"Zombie not found",...}
```
