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
  local dir="$1" tenant_columns="$2" secret_columns="${3:-id, key_name}"
  mkdir -p "$dir"
  printf '%s\n' '# Fixture' '' '## identity' '' \
    '| Table | Purpose | Columns |' '|---|---|---|' \
    "| \`tenants\` | Root | \`$tenant_columns\` |" '' '## vault and memory' '' \
    '| Table | Purpose | Columns |' '|---|---|---|' \
    "| \`vault.secrets\` | One secret | \`$secret_columns\` |" >"$dir/postgres_schemas.md"
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
    "vault.secrets.label is in $root/reshaped/1200_vault_secrets.sql" || return

  mkdir -p "$root/no_rows"
  printf '# Fixture\n\nThe layout moved elsewhere.\n' >"$root/no_rows/postgres_schemas.md"
  expect_layout_refusal "$name" "$root/no_rows" "$root/schema" \
    "$root/no_rows/postgres_schemas.md lists no table" || return
  test_layout_forward_columns "$root" || return
  ok "$name"
}

# Forward migrations must match their final inventory; malformed or unsupported
# changes still fail, including a matching-looking inventory after a bad edit.
layout_alter_refusal() {
  local root="$1" tag="$2" sql="$3" expected="$4"
  write_layout_schema "$root/$tag" ''
  printf '%s\n' "$sql" >"$root/$tag/1300_forward.sql"
  expect_layout_refusal "$LAYOUT_CHECK" "$root/arch" "$root/$tag" "$expected"
}

test_layout_forward_columns() {
  local root="$1" output
  write_layout_schema "$root/forward" ''
  printf '%s\n' \
    "ALTER TABLE identity.tenants ADD COLUMN nickname TEXT DEFAULT 'a,b', ADD COLUMN score NUMERIC(12,3);" \
    'ALTER TABLE identity.tenants DROP COLUMN nickname, DROP COLUMN score;' \
    'ALTER TABLE vault.secrets ADD COLUMN value TEXT;' >"$root/forward/1300_add.sql"
  printf '%s\n' 'ALTER TABLE vault.secrets DROP COLUMN value;' >"$root/forward/1400_drop.sql"
  write_layout_doc "$root/forward_doc" 'id, name, created_at'
  if ! output="$(run_layout_gate "$root/forward_doc" "$root/forward")"; then
    bad "$LAYOUT_CHECK" "ordered forward migrations were rejected: $output"
    return 1
  fi
  layout_alter_refusal "$root" dropped 'ALTER TABLE identity.tenants DROP COLUMN name;' \
    "identity.tenants.name is in $root/arch/postgres_schemas.md" || return
  layout_alter_refusal "$root" renamed 'ALTER TABLE identity.tenants RENAME COLUMN name TO title;' \
    "$root/renamed/1300_forward.sql: ALTER TABLE identity.tenants: unsupported column change" || return
  layout_alter_refusal "$root" shorthand 'ALTER TABLE identity.tenants DROP name;' \
    "$root/shorthand/1300_forward.sql: ALTER TABLE identity.tenants: unsupported column change" || return
  layout_alter_refusal "$root" unknown 'ALTER TABLE identity.ghost ADD COLUMN id UUID;' \
    "$root/unknown/1300_forward.sql: ALTER TABLE identity.ghost: table has not been created" || return
  layout_alter_refusal "$root" duplicate 'ALTER TABLE identity.tenants ADD COLUMN name TEXT;' \
    "$root/duplicate/1300_forward.sql: ALTER TABLE identity.tenants: column name already exists" || return
  layout_alter_refusal "$root" missing 'ALTER TABLE identity.tenants DROP COLUMN nickname;' \
    "$root/missing/1300_forward.sql: ALTER TABLE identity.tenants: column nickname does not exist" || return
  layout_alter_refusal "$root" malformed 'ALTER TABLE identity.tenants ADD COLUMN nickname;' \
    "$root/malformed/1300_forward.sql: ALTER TABLE identity.tenants: unsupported column change" || return
  layout_alter_refusal "$root" optional_malformed 'ALTER TABLE identity.tenants ADD COLUMN IF NOT EXISTS name;' \
    "$root/optional_malformed/1300_forward.sql: ALTER TABLE identity.tenants: unsupported column change" || return
  layout_alter_refusal "$root" trailing 'ALTER TABLE identity.tenants ADD COLUMN score NUMERIC(8,2)) ignored;' \
    "$root/trailing/1300_forward.sql: ALTER TABLE identity.tenants: unexpected text after column list" || return
  layout_alter_refusal "$root" removed 'DROP TABLE vault.secrets;' \
    "$root/removed/1300_forward.sql: DROP TABLE removes a table after its CREATE" || return
  layout_alter_refusal "$root" premature \
    'ALTER TABLE identity.later ADD COLUMN id UUID; CREATE TABLE identity.later (id UUID);' \
    "$root/premature/1300_forward.sql: ALTER TABLE identity.later: table has not been created" || return
}

test_arch_schema_forward_columns() {
  local name="test_arch_schema_forward_columns" root="$WORK_DIR/forward_layout" output
  write_layout_doc "$root/arch" 'id, name, created_at' 'id, key_name, label, score'
  write_layout_schema "$root/schema" ''
  printf '%s\n' 'ALTER TABLE vault.secrets' \
    '  ADD label TEXT, ADD COLUMN score NUMERIC(8, 2);' \
    'ALTER TABLE vault.secrets ADD COLUMN IF NOT EXISTS label TEXT;' \
    'ALTER TABLE vault.secrets ALTER COLUMN label SET NOT NULL;' \
    >"$root/schema/1300_secret_columns.sql"
  if ! output="$(run_layout_gate "$root/arch" "$root/schema")"; then
    bad "$name" "documented forward additions were rejected: $output"
    return
  fi
  if [[ "$output" != *"$LAYOUT_CHECK: 2 tables and 7 columns match"* ]]; then
    bad "$name" "added columns were not counted: $output"
    return
  fi
  write_layout_doc "$root/missing_column" 'id, name, created_at' 'id, key_name, score'
  expect_layout_refusal "$name" "$root/missing_column" "$root/schema" \
    "vault.secrets.label is in $root/schema/1300_secret_columns.sql" || return
  write_layout_doc "$root/extra_column" 'id, name, created_at' 'id, key_name, label, score, absent'
  expect_layout_refusal "$name" "$root/extra_column" "$root/schema" \
    "vault.secrets.absent is in $root/extra_column/postgres_schemas.md" || return
  ok "$name"
}

test_arch_schema_unsupported_changes() {
  local name="test_arch_schema_unsupported_changes" root="$WORK_DIR/unsupported_layout"
  local action index=0 schema
  write_layout_doc "$root/arch" 'id, name, created_at'
  for action in 'RENAME COLUMN key_name TO other_name' \
    'RENAME TO other_table'; do
    schema="$root/schema_$index"
    write_layout_schema "$schema" ''
    printf 'ALTER TABLE vault.secrets %s;\n' "$action" >"$schema/1300_change.sql"
    expect_layout_refusal "$name" "$root/arch" "$schema" \
      "$schema/1300_change.sql: ALTER TABLE vault.secrets: unsupported column change" || return
    index=$((index + 1))
  done
  write_layout_schema "$root/duplicate" ''
  printf '%s\n' 'ALTER TABLE vault.secrets ADD COLUMN key_name TEXT;' \
    >"$root/duplicate/1300_change.sql"
  expect_layout_refusal "$name" "$root/arch" "$root/duplicate" \
    "$root/duplicate/1300_change.sql: ALTER TABLE vault.secrets: column key_name already exists" || return
  write_layout_schema "$root/unknown" ''
  printf '%s\n' 'ALTER TABLE vault.missing ADD COLUMN label TEXT;' \
    >"$root/unknown/1300_change.sql"
  expect_layout_refusal "$name" "$root/arch" "$root/unknown" \
    "$root/unknown/1300_change.sql: ALTER TABLE vault.missing: table has not been created" || return
  write_layout_schema "$root/malformed" ''
  printf '%s\n' 'ALTER TABLE vault.secrets ADD COLUMN score NUMERIC(8, 2;' \
    >"$root/malformed/1300_change.sql"
  expect_layout_refusal "$name" "$root/arch" "$root/malformed" \
    "$root/malformed/1300_change.sql: ALTER TABLE vault.secrets: the column list never closes" || return
  ok "$name"
}
