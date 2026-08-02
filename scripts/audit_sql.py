#!/usr/bin/env python3
"""Diff every SQL statement in the Zig tree against what the live database HAS.

Successor to `audit_seeds.py`, which only parsed INSERT column lists. Seven
production bugs in M154 were invisible to grep because the columns were DROPPED
rather than renamed — the statement named nothing stale, it named something the
table no longer had. `audit_seeds.py` caught those only in INSERTs; the eighth
bug (tenant_provider_resolver's `ORDER BY ... id DESC` against a table keyed on
`provider`) was a SELECT, and survived it.

This walks SELECT / UPDATE / DELETE predicates too:

  1. Scan each `.zig` file into tokens, honouring Zig's `\\\\` multiline literals,
     quoted strings, and `//` comments.
  2. Rebuild `++` concatenation chains and resolve same-file `const` references,
     so statements assembled from fragments (cron/sql.zig) parse as one string.
     That concatenation is what `audit_seeds.py` reported as a false positive.
  3. For each statement, resolve the tables and aliases it names, then check
     every column reference against the live catalogue.

Reports two tiers. QUALIFIED (`alias.column`) resolves to exactly one table and
is near-certain. BARE is checked against the union of every table the statement
names, so a join can mask a typo but a dropped column still surfaces.

Deliberately does NOT rewrite. The fix per statement differs — rename, drop, or
add a missing column — and blind substitution is what let five production
statements survive an earlier sweep.
"""
import re
import subprocess
import sys
from bisect import bisect_right
from collections import defaultdict
from pathlib import Path

ROOT = Path("/Users/kishore/Projects/agentsfleet-m154-schema-rebuild")
PG = ["docker", "exec", "agentsfleet-m154-schema-rebuild-postgres-1",
      "psql", "-U", "agentsfleet", "-d", "agentsfleetdb", "-Atc"]

CATALOGUE_SQL = """
select t||'|'||cols from (
  select table_schema||'.'||table_name as t,
         string_agg(column_name, ',' order by ordinal_position) as cols
  from information_schema.columns
  where table_schema not in ('pg_catalog','information_schema')
  group by 1) s
"""


NOT_NULL_SQL = """
select t||'|'||cols from (
  select table_schema||'.'||table_name as t,
         string_agg(column_name, ',') as cols
  from information_schema.columns
  where table_schema not in ('pg_catalog','information_schema')
    and is_nullable = 'NO' and column_default is null
  group by 1) s
"""


def catalogue(sql):
    out = subprocess.run(PG + [sql], capture_output=True, text=True,
                         check=True).stdout
    table = {}
    for line in out.strip().splitlines():
        if "|" not in line:
            continue
        name, cols = line.split("|", 1)
        table[name] = set(cols.split(","))
    return table


COLS = catalogue(CATALOGUE_SQL)
# Absorbed from the retired `audit_seeds.py`: a column the table requires and
# the statement omits fails just as loudly as one that no longer exists.
REQUIRED = catalogue(NOT_NULL_SQL)
SCHEMAS = {t.split(".", 1)[0] for t in COLS}
# Unqualified table names in SQL resolve through the search_path; map them.
ALIAS_TABLE = {}
for _full in COLS:
    ALIAS_TABLE.setdefault(_full.split(".", 1)[1], _full)

# ---------------------------------------------------------------- Zig scanner

TOK_RE = re.compile(r"""
      (?P<multi>(?:^[ \t]*\\\\[^\n]*\n?)+)   # one or more \\ literal lines
    | (?P<str>"(?:[^"\\\n]|\\.)*")           # "..." with escapes
    | (?P<comment>//[^\n]*)
    | (?P<cat>\+\+)
    | (?P<ident>[A-Za-z_@][A-Za-z0-9_]*)
    | (?P<op>[=;,()])
""", re.X | re.M)

# Anchored at the start: a test NAME like "concurrent PATCH + INSERT into
# fleet_events — both succeed" is a string literal containing a verb and a table,
# and an unanchored match audits its prose as a predicate. Fragments that begin
# mid-statement are skipped here and audited as part of the assembled whole.
SQL_VERB = re.compile(r"^\s*(WITH|SELECT|INSERT|UPDATE|DELETE)\b", re.I)


def unescape(raw):
    return (raw[1:-1].replace('\\"', '"').replace("\\n", "\n")
            .replace("\\t", "\t").replace("\\\\", "\\"))


def strip_multi(raw):
    return "\n".join(ln.strip()[2:] for ln in raw.splitlines() if ln.strip())


def scan(text):
    """Yield (kind, value, offset) tokens; comments dropped."""
    for m in TOK_RE.finditer(text):
        kind = m.lastgroup
        if kind == "comment":
            continue
        raw = m.group()
        if kind == "multi":
            yield ("LIT", strip_multi(raw), m.start())
        elif kind == "str":
            yield ("LIT", unescape(raw), m.start())
        elif kind == "cat":
            yield ("CAT", "++", m.start())
        elif kind == "ident":
            yield ("IDENT", raw, m.start())
        else:
            yield ("OP", raw, m.start())


def chains(tokens):
    """Group tokens into `++` concatenation chains.

    Returns (name_or_None, parts, offset) where parts is a list of
    ('lit', text) / ('ref', ident). A chain is named when the tokens
    immediately before it are `const NAME =`.
    """
    out, i, n = [], 0, len(tokens)
    while i < n:
        kind, val, off = tokens[i]
        if kind not in ("LIT", "IDENT"):
            i += 1
            continue
        if kind == "IDENT" and not (i + 1 < n and tokens[i + 1][0] == "CAT"):
            i += 1  # a bare identifier only starts a chain if `++` follows
            continue
        parts, start = [], off
        while i < n:
            k, v, _ = tokens[i]
            if k == "LIT":
                parts.append(("lit", v))
            elif k == "IDENT":
                parts.append(("ref", v))
            else:
                break
            if i + 1 < n and tokens[i + 1][0] == "CAT":
                i += 2
                continue
            i += 1
            break
        name = None
        # walk back over the chain's own tokens to find `const NAME =`
        j = next(idx for idx, t in enumerate(tokens) if t[2] == start)
        if j >= 3 and tokens[j - 1][1] == "=" and tokens[j - 2][0] == "IDENT" \
                and tokens[j - 3][1] == "const":
            name = tokens[j - 2][1]
        out.append((name, parts, start))
    return out


def resolve(parts, consts, depth=0):
    """Expand `++`-joined parts, following same-file const references."""
    if depth > 6:
        return None
    text = []
    for kind, val in parts:
        if kind == "lit":
            text.append(val)
            continue
        if val not in consts:
            return None  # cross-file or computed — cannot audit safely
        inner = resolve(consts[val], consts, depth + 1)
        if inner is None:
            return None
        text.append(inner)
    return "".join(text)


def statements(path):
    """Yield (offset, sql) for every resolvable SQL statement in a Zig file."""
    text = path.read_text()
    toks = list(scan(text))
    all_chains = chains(toks)
    consts = {name: parts for name, parts, _ in all_chains if name}
    for name, parts, off in all_chains:
        sql = resolve(parts, consts, 0)
        if not sql or not SQL_VERB.search(sql):
            continue
        yield off, sql


# ------------------------------------------------------------- SQL inspection

SQL_COMMENT = re.compile(r"--[^\n]*")
# Classic quote-aware form: a run of non-quotes, then any number of doubled-quote
# escapes. The naive `'(?:[^']|'')*'` is greedy across the whole statement and
# swallows real SQL between two separate literals.
SQL_STRING = re.compile(r"'[^']*(?:''[^']*)*'")
QUOTED_IDENT = re.compile(r'"[^"]*"')       # COLLATE "C"
ZIG_FMT = re.compile(r"\{[a-z0-9_:.\[\]]*\}")  # {s} / {d} in allocPrint templates
CAST = re.compile(r"::\s*[A-Za-z_][\w\[\]]*")
DOLLAR = re.compile(r"\$\d+")
# `jsonb_array_elements(...) trig`, `) g`, `AS r(k, v)` — names a statement binds
# itself. They are columns of a derived relation, not of any catalogue table.
DERIVED_ALIAS = re.compile(r"\)\s*(?:AS\s+)?([A-Za-z_]\w*)")
COLUMN_ALIAS_LIST = re.compile(r"\bAS\s+[A-Za-z_]\w*\s*\(([^)]*)\)", re.I)

TABLE_REF = re.compile(
    r"\b(?:FROM|JOIN|UPDATE|INTO)\s+(?:ONLY\s+)?([A-Za-z_][\w.]*)"
    r"(?:\s+(?:AS\s+)?([A-Za-z_]\w*))?", re.I)
CTE = re.compile(r"(?:WITH(?:\s+RECURSIVE)?|,)\s+([A-Za-z_]\w*)\s+AS\s*\(", re.I)
OUT_ALIAS = re.compile(r"\bAS\s+([A-Za-z_]\w*)", re.I)
QUALIFIED = re.compile(r"\b([A-Za-z_]\w*)\.([A-Za-z_]\w*)\b")
WORD = re.compile(r"\b([A-Za-z_]\w*)\b")

KEYWORDS = set("""
select insert update delete from where and or not in is null true false into
values set returning join left right inner outer full cross on using as order
by group having limit offset distinct union all except intersect with
recursive case when then else end exists any some between like ilike similar
asc desc nulls first last for share update_ nowait skip locked lateral
conflict do nothing constraint primary key foreign references unique check
default cast coalesce greatest least count sum avg min max now interval
extract date_trunc to_timestamp to_char array unnest jsonb_build_object
json_build_object jsonb_agg json_agg row_number over partition rank filter
current_timestamp current_date localtimestamp only lock table mode exclusive
access share row level begin commit rollback savepoint analyze explain
collate escape offset_ within ordinality tablesample materialized
generated always identity add drop alter column rename to if not_ cascade
restrict text uuid integer bigint smallint boolean numeric decimal real
double precision timestamptz timestamp date time varchar char bytea json jsonb
inet cidr macaddr interval_ serial bigserial oid regclass tsvector int int4
int8 int2 bool float float4 float8 nullif abs round floor ceil length lower
upper trim substring position replace concat concat_ws split_part md5 encode
decode gen_random_uuid random string_agg array_agg exclude of desc_ asc_
""".split())

SKIP_LEFT = SCHEMAS | {"pg_catalog", "information_schema", "excluded", "pg"}


def find_tables(sql):
    """Return (alias -> full table name, list of full table names)."""
    aliases, tables = {}, []
    for m in TABLE_REF.finditer(sql):
        raw, alias = m.group(1), m.group(2)
        full = raw if raw in COLS else ALIAS_TABLE.get(raw)
        if not full:
            continue
        tables.append(full)
        aliases[raw.split(".")[-1]] = full
        if alias and alias.lower() not in KEYWORDS:
            aliases[alias] = full
    return aliases, tables


INSERT_COLS = re.compile(r"INSERT\s+INTO\s+([A-Za-z_][\w.]*)\s*\(([^)]*)\)", re.I)
IDENT_ONLY = re.compile(r"^[a-z_][a-z0-9_]*$")


def balanced_group(text, open_at):
    """Return the substring inside the parens starting at `open_at`."""
    depth, i = 0, open_at
    while i < len(text):
        if text[i] == "(":
            depth += 1
        elif text[i] == ")":
            depth -= 1
            if depth == 0:
                return text[open_at + 1:i]
        i += 1
    return None


def split_top_level(s):
    """Split on commas that are not inside parentheses."""
    parts, depth, cur = [], 0, []
    for ch in s:
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
        if ch == "," and depth == 0:
            parts.append("".join(cur))
            cur = []
        else:
            cur.append(ch)
    if "".join(cur).strip():
        parts.append("".join(cur))
    return [p.strip() for p in parts if p.strip()]


def insert_arity(body):
    """INSERT column count vs VALUES expression count.

    A dropped column leaves the two out of step, which Postgres reports as
    'INSERT has more expressions than target columns' only at runtime — and
    only if the statement is ever executed. Counting is paren-aware so a
    subquery supplying one column is one expression, not several.
    """
    m = INSERT_COLS.search(body)
    if not m:
        return []
    cols = split_top_level(m.group(2))
    # Only the single-row `(cols) VALUES (exprs)` form is comparable. An
    # `INSERT ... SELECT` has no VALUES list, and a multi-row VALUES has
    # several — both are correct and neither is what this check is for.
    tail = body[m.end():]
    vm = re.match(r"\s*VALUES\s*\(", tail, re.I)
    if not vm:
        return []
    open_at = m.end() + vm.end() - 1
    inner = balanced_group(body, open_at)
    if inner is None:
        return []
    # Every row of a multi-row VALUES must match the column list. Checking only
    # the first row misses the case where a column was added to one row and not
    # its siblings, which Postgres reports as "VALUES lists must all be the same
    # length" — again only at runtime, and only if the statement is executed.
    rows, pos = [], open_at
    while True:
        group = balanced_group(body, pos)
        if group is None:
            break
        rows.append(group)
        rest = body[pos + len(group) + 2:]
        nxt = re.match(r"\s*,\s*\(", rest)
        if not nxt:
            break
        pos = pos + len(group) + 2 + nxt.end() - 1

    out = []
    for i, row in enumerate(rows):
        vals = split_top_level(row)
        if cols and vals and len(cols) != len(vals):
            where = f" (row {i + 1} of {len(rows)})" if len(rows) > 1 else ""
            out.append(f"{len(cols)} columns against {len(vals)} values{where} — "
                       f"{m.group(1)} ({', '.join(c for c in cols)})")
    return out


def missing_required(body):
    """Required columns an INSERT omits (the retired audit_seeds.py check)."""
    m = INSERT_COLS.search(body)
    if not m:
        return []
    full = m.group(1) if m.group(1) in COLS else ALIAS_TABLE.get(m.group(1))
    if not full:
        return []
    named = {c.strip() for c in m.group(2).split(",") if IDENT_ONLY.match(c.strip())}
    if not named:
        return []
    return [f"{c} — {full} requires it, statement omits it"
            for c in sorted(REQUIRED.get(full, set()) - named)]


def audit(sql):
    """Return (qualified_findings, bare_findings) for one statement."""
    # Strings BEFORE comments: a seeded markdown literal like '---\nname: x'
    # opens with `--`, and comment-first eats to end of line, taking the closing
    # quote with it and exposing every later literal as a bare identifier.
    body = SQL_COMMENT.sub(" ", SQL_STRING.sub(" ", sql))
    body = QUOTED_IDENT.sub(" ", ZIG_FMT.sub(" ", body))
    body = DOLLAR.sub(" ", CAST.sub(" ", body))
    aliases, tables = find_tables(body)
    if not tables:
        return [], [], []
    # Arity counts value POSITIONS, so it needs text where every position still
    # holds a token. `body` has had params, casts and literals blanked out, which
    # collapses `($1::uuid, 'x', 0)` to a single surviving item. Sanitize string
    # literals to a placeholder instead — commas inside them must not count.
    arity_body = SQL_COMMENT.sub(" ", SQL_STRING.sub("'X'", sql))
    missing = missing_required(body) + insert_arity(arity_body)

    qualified = []
    for m in QUALIFIED.finditer(body):
        left, right = m.group(1), m.group(2)
        if left in SKIP_LEFT or left not in aliases:
            continue
        if right == "*" or right.lower() in KEYWORDS:
            continue
        if right not in COLS[aliases[left]]:
            qualified.append(f"{left}.{right} — {aliases[left]} has no {right}")

    ctes = {m.group(1) for m in CTE.finditer(body)}
    if ctes:
        # derived columns are unresolvable; qualified + INSERT shape only
        return qualified, [], missing

    known = set().union(*(COLS[t] for t in tables))
    derived = {m.group(1) for m in DERIVED_ALIAS.finditer(body)}
    for m in COLUMN_ALIAS_LIST.finditer(body):
        derived.update(c.strip() for c in m.group(1).split(","))
    ignore = (KEYWORDS | ctes | set(aliases) | derived
              | {m.group(1) for m in OUT_ALIAS.finditer(body)}
              | {t.split(".")[-1] for t in tables} | SCHEMAS)
    # names that are part of a qualified reference, or a function call
    consumed = {g for m in QUALIFIED.finditer(body) for g in m.groups()}
    funcs = set(re.findall(r"\b([A-Za-z_]\w*)\s*\(", body))

    bare = []
    for m in WORD.finditer(body):
        w = m.group(1)
        if w.lower() in KEYWORDS or w in ignore or w in consumed or w in funcs:
            continue
        if w not in known:
            bare.append(f"{w} — none of {', '.join(sorted(set(tables)))} has it")
    return qualified, sorted(set(bare)), missing


# ------------------------------------------------------------------- reporting

def line_of(text, off):
    starts = [0] + [m.end() for m in re.finditer(r"\n", text)]
    return bisect_right(starts, off)


def main():
    only_prod = "--all" not in sys.argv
    findings = defaultdict(list)
    scanned = 0
    for path in sorted(ROOT.joinpath("src").rglob("*.zig")):
        if only_prod and path.name.endswith("_test.zig"):
            continue
        text = path.read_text()
        for off, sql in statements(path):
            scanned += 1
            qual, bare, missing = audit(sql)
            if qual or bare or missing:
                rel = str(path.relative_to(ROOT))
                findings[rel].append((line_of(text, off), qual, bare, missing))

    total = 0
    for path in sorted(findings):
        for line, qual, bare, missing in sorted(findings[path]):
            total += 1
            print(f"{path}:{line}")
            for q in qual:
                print(f"    QUALIFIED  {q}")
            for b in bare:
                print(f"    BARE       {b}")
            for m in missing:
                print(f"    MISSING    {m}")
    scope = "production" if only_prod else "all"
    print(f"\n{scanned} {scope} statements scanned, {total} with findings",
          file=sys.stderr)


if __name__ == "__main__":
    main()
