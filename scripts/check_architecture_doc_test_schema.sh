#!/usr/bin/env bash
# Self-tests for the test_arch_schema_layout_matches_doc row of
# check_architecture_doc.sh. Sourced by scripts/check_architecture_doc_test.sh,
# never run alone: it reads that script's ok/bad counters, WORK_DIR, GATE,
# REPO_ROOT and build_spec_root.
#
# Each case builds a fixture design page and a fixture schema directory that
# match, then plants exactly one difference, so a failing gate can only be this
# row. A failing case must also name the table or column it planted.

readonly LAYOUT_CHECK="test_arch_schema_layout_matches_doc"

# A design page listing `identity.tenants` and `vault.secrets`, with one
# column carrying a parenthesised note the check must drop.
write_layout_doc() {
  local dir="$1" tenant_columns="$2"
  mkdir -p "$dir"
  printf '%s\n' '# Fixture' '' '## identity' '' \
    '| Table | Purpose | Columns |' '|---|---|---|' \
    "| \`tenants\` | Root | \`$tenant_columns\` |" '' '## vault and memory' '' \
    '| Table | Purpose | Columns |' '|---|---|---|' \
    '| `vault.secrets` | One secret | `id, key_name` |' >"$dir/postgres_schemas.md"
}

# Two slots matching the page above: a table constraint, a commented-out
# CREATE TABLE and a constraint-only ALTER TABLE, none of them a column.
write_layout_schema() {
  local dir="$1" extra_column="$2"
  mkdir -p "$dir"
  printf '%s\n' '-- A comment naming CREATE TABLE identity.ghost (x) is not a table.' \
    'CREATE TABLE identity.tenants (' '    id UUID PRIMARY KEY,' \
    "    CONSTRAINT ck_id CHECK (substring(id::text from 15 for 1) = '7'),$extra_column" \
    '    name TEXT NOT NULL,' '    created_at TIMESTAMPTZ(3) NOT NULL' ');' \
    >"$dir/1100_identity_tenants.sql"
  printf '%s\n' 'CREATE TABLE vault.secrets (' '    id UUID PRIMARY KEY,' \
    '    key_name TEXT NOT NULL,' '    UNIQUE (id, key_name)' ');' \
    'ALTER TABLE vault.secrets ADD CONSTRAINT fk_secrets_tenant' \
    '    FOREIGN KEY (id) REFERENCES identity.tenants(id);' >"$dir/1200_vault_secrets.sql"
}

# The gate's whole output against one fixture pair; its status is the gate's.
run_layout_gate() {
  local arch="$1" schema="$2"
  (cd "$REPO_ROOT" && ARCH_DIR="$arch" SPEC_ROOT="$WORK_DIR/specs" SCHEMA_DIR="$schema" \
    DOC_SET_EXTRA="" bash "$GATE" 2>&1)
}

# Fails `name` unless the gate rejects the fixture and names `expected`.
expect_layout_refusal() {
  local name="$1" arch="$2" schema="$3" expected="$4" output
  if output="$(run_layout_gate "$arch" "$schema")"; then
    bad "$name" "the gate passed a layout that differs: $expected"
    return 1
  fi
  if [[ "$output" != *"$LAYOUT_CHECK: $expected"* ]]; then
    bad "$name" "the failure did not name '$expected': $output"
    return 1
  fi
}

test_arch_schema_layout_matches_doc() {
  local name="test_arch_schema_layout_matches_doc" root="$WORK_DIR/layout" output
  build_spec_root "$WORK_DIR/specs"
  write_layout_doc "$root/arch" 'id (PK), name, created_at'
  write_layout_schema "$root/schema" ''
  if ! output="$(run_layout_gate "$root/arch" "$root/schema")"; then
    bad "$name" "a matching layout was rejected: $output"
    return
  fi
  if [[ "$output" != *"$LAYOUT_CHECK: 2 tables and 5 columns match"* ]]; then
    bad "$name" "the pass did not count both tables and their columns: $output"
    return
  fi

  write_layout_schema "$root/extra_column" $'\n    nickname TEXT,'
  expect_layout_refusal "$name" "$root/arch" "$root/extra_column" \
    "identity.tenants.nickname is in $root/extra_column/1100_identity_tenants.sql" || return

  write_layout_doc "$root/doc_column" 'id (PK), name, email, created_at'
  expect_layout_refusal "$name" "$root/doc_column" "$root/schema" \
    "identity.tenants.email is in $root/doc_column/postgres_schemas.md" || return

  mkdir -p "$root/one_table"
  cp "$root/schema/1100_identity_tenants.sql" "$root/one_table/"
  expect_layout_refusal "$name" "$root/arch" "$root/one_table" \
    "vault.secrets is listed in $root/arch/postgres_schemas.md" || return

  write_layout_schema "$root/reshaped" ''
  printf '%s\n' 'ALTER TABLE vault.secrets ADD COLUMN label TEXT;' >>"$root/reshaped/1200_vault_secrets.sql"
  expect_layout_refusal "$name" "$root/arch" "$root/reshaped" \
    "$root/reshaped/1200_vault_secrets.sql: ALTER TABLE vault.secrets reshapes columns" || return

  mkdir -p "$root/no_rows"
  printf '# Fixture\n\nThe layout moved elsewhere.\n' >"$root/no_rows/postgres_schemas.md"
  expect_layout_refusal "$name" "$root/no_rows" "$root/schema" \
    "$root/no_rows/postgres_schemas.md lists no table" || return
  ok "$name"
}
