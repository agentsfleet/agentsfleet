#!/usr/bin/env python3
"""Every dialable provider is either priced, or says why it never will be.

scripts/model-library-allowlist.json feeds core.model_library, which is what
UZ-PROVIDER-004 checks before a tenant may activate a model, and what
computeStageCharge prices a platform-posture slice from. Both consequences are
silent when the file is wrong: an uncurated provider looks dialable and refuses
activation, and a zero rate bills nobody while every downstream guard passes.

So the file's invariants are checked here rather than trusted to review:

  1. priced XOR reasoned    — a provider has models, or one reason code, never
                              both and never neither. "Neither" is the state
                              that let 87 providers sit uncurated unnoticed.
  2. no zero rates          — a zero in any rate column must never reach the
                              cost path (schema/400_model_library.sql).
  3. closed reason vocabulary — an unrecognised code is a typo, and a typo'd
                              reason reads as a decision that was never made.
  4. region agreement       — a rate read from an international price page may
                              not hang off a mainland-China endpoint. This is
                              the wrong-continent failure the file's own header
                              warns about, and it shipped for months.
  5. api providers have fixtures — otherwise the integration lane silently
                              depends on the network being up.
  6. local-runtime parity   — four lists must be equal: the providers carrying
                              an activation floor here, the ones the server's
                              credential layer exempts (from the catalogue check
                              and from the non-empty-api_key check), and the CLI
                              and dashboard mirrors of the same set. Adding to
                              one alone leaves a local runtime that still
                              refuses activation, one that bypasses the gates
                              with nothing behind it, or a client surface that
                              rejects a credential the API would have accepted.

Exit 0 if clean, 1 with one line per violation.
"""
import json
import re
import sys
from pathlib import Path

ALLOWLIST = Path("scripts/model-library-allowlist.json")
FIXTURE_DIR = Path("samples/fixtures/model-library")
PROVIDER_METADATA = Path("src/agentsfleetd/secrets/metadata.zig")
CLI_PROVIDER_CONSTANTS = Path("cli/src/constants/custom-endpoint.ts")
UI_PROVIDER_CONSTANTS = Path("ui/packages/app/lib/types.ts")

RATE_FIELDS = ("input", "cached_input", "output")

# Closed vocabulary. A sixth value means the vocabulary is wrong — extend it
# deliberately here and in the file's `unpriced_reasons` legend, together.
VALID_REASONS = frozenset(
    {
        "cn_endpoint",
        "subscription_plan",
        "gateway_passthrough",
        "deployment_scoped",
        "credentialed_feed",
        "no_public_rates",
        "awaiting_curation",
    }
)

# Hosts that serve mainland China. Not TLD-sniffable: Xiaomi MiMo is CN-only on
# a .com, while api.z.ai is the international arm of a Chinese vendor.
CN_HOSTS = (
    "api.moonshot.cn",
    "open.bigmodel.cn",
    "api.minimaxi.com",
    "ark.cn-beijing.volces.com",
    "aip.baidubce.com",
    "api.hunyuan.cloud.tencent.com",
    "api.baichuan-ai.com",
    "api.siliconflow.cn",
    "router.shengsuanyun.com",
    "api.xiaomimimo.com",
    "dashscope.aliyuncs.com",
)

# Providers whose rate row is an activation floor, not a price (local runtimes).
RATE_BASIS_FLOOR = "activation_floor"


def _is_cn(base_url: str) -> bool:
    return any(host in base_url for host in CN_HOSTS)


def check_priced_xor_reasoned(name: str, cfg: dict) -> list[str]:
    priced = bool(cfg.get("models"))
    reason = cfg.get("unpriced_reason")
    if priced and reason:
        return [f"{name}: priced XOR reasoned — has {len(cfg['models'])} models AND unpriced_reason={reason!r}"]
    if not priced and not reason:
        return [f"{name}: priced XOR reasoned — no models and no unpriced_reason (uncurated gap)"]
    return []


def check_reason_vocabulary(name: str, cfg: dict) -> list[str]:
    reason = cfg.get("unpriced_reason")
    if reason is None or reason in VALID_REASONS:
        return []
    return [f"{name}: unknown unpriced_reason {reason!r} (expected one of {sorted(VALID_REASONS)})"]


def check_no_zero_rates(name: str, cfg: dict) -> list[str]:
    problems = []
    for model in cfg.get("models") or []:
        if not isinstance(model, dict):
            continue  # api-source providers list bare ids; rates arrive at run time
        for field in RATE_FIELDS:
            if model.get(field) == 0:
                problems.append(f"{name}/{model.get('model_id')}: {field} is 0 — a zero rate must never enter the cost path")
    return problems


def check_region_agreement(name: str, cfg: dict) -> list[str]:
    """A CN endpoint may not carry rates, and a priced provider may not be CN."""
    base_url = cfg.get("base_url") or ""
    if not base_url or not _is_cn(base_url):
        return []
    if cfg.get("models"):
        return [f"{name}: priced against a mainland-China endpoint ({base_url}) — the international rule says price the intl arm"]
    if cfg.get("unpriced_reason") != "cn_endpoint":
        return [f"{name}: CN endpoint ({base_url}) but unpriced_reason={cfg.get('unpriced_reason')!r}, expected 'cn_endpoint'"]
    return []


def check_api_has_fixture(name: str, cfg: dict) -> list[str]:
    if cfg.get("source") != "api":
        return []
    if (FIXTURE_DIR / f"{name}.json").is_file():
        return []
    return [f"{name}: source=api with no fixture at {FIXTURE_DIR / f'{name}.json'} — the integration lane would need the network"]


def check_floor_is_marked(name: str, cfg: dict) -> list[str]:
    """A sentinel rate must announce itself, or it reads as a real price."""
    models = [m for m in (cfg.get("models") or []) if isinstance(m, dict)]
    if not models:
        return []
    tiny = all(m.get("input", 1) < 0.0001 for m in models)
    if tiny and cfg.get("rate_basis") != RATE_BASIS_FLOOR:
        return [f"{name}: rates are sentinel-sized but rate_basis is not {RATE_BASIS_FLOOR!r}"]
    if cfg.get("rate_basis") == RATE_BASIS_FLOOR and not tiny:
        return [f"{name}: rate_basis={RATE_BASIS_FLOOR!r} but carries real-looking rates"]
    return []


CHECKS = (
    check_priced_xor_reasoned,
    check_reason_vocabulary,
    check_no_zero_rates,
    check_region_agreement,
    check_api_has_fixture,
    check_floor_is_marked,
)


def zig_local_runtime_providers() -> set[str]:
    """The provider names LOCAL_RUNTIME_PROVIDERS lists in the credential layer.

    Parsed rather than duplicated: the Zig array is the authority for which
    providers skip the catalogue-membership check AND the non-empty-api_key
    check, and this file is the authority for which carry an activation floor.
    They describe the same set, so a provider added to one and not the other is
    a real defect — either a local runtime that still refuses activation, or one
    that bypasses both gates with no floor behind it.
    """
    body = PROVIDER_METADATA.read_text(encoding="utf-8")
    # Comments first: this is a scrape, not a parse, so a `//` line mentioning the
    # declaration or carrying a quoted example inside the array's byte range would
    # otherwise be read as a member. Dropping comment text makes the scan see only
    # code, which is the only thing `isLocalRuntime` compiles against. The scan
    # starts at the array's opening brace so a doc comment between the name and
    # the members cannot contribute one.
    code = _strip_line_comments(body)
    start = code.find("const LOCAL_RUNTIME_PROVIDERS")
    if start == -1:
        raise ValueError(f"LOCAL_RUNTIME_PROVIDERS not found in {PROVIDER_METADATA}")
    eq_at = code.find("=", start)
    if eq_at == -1:
        raise ValueError(f"LOCAL_RUNTIME_PROVIDERS in {PROVIDER_METADATA} is never assigned")
    open_at = code.find("{", eq_at)
    if open_at == -1:
        raise ValueError(f"LOCAL_RUNTIME_PROVIDERS in {PROVIDER_METADATA} has no opening brace")
    end = code.find("};", open_at)
    if end == -1:
        raise ValueError(f"LOCAL_RUNTIME_PROVIDERS in {PROVIDER_METADATA} has no closing brace")
    return set(re.findall(r'"([^"]+)"', code[open_at:end]))


def _strip_line_comments(body: str) -> str:
    """Drop `//` comments WITHOUT cutting inside a string literal.

    The naive `line.split("//")[0]` truncates any line carrying a URL — and a
    provider list is exactly where a `"http://localhost:11434/v1"` would sit. A
    cut mid-literal leaves an unbalanced quote, and the member regex then re-pairs
    quotes across the rest of the window, yielding a WRONG-but-non-empty set that
    a vacuity check on length would not notice.
    """
    out = []
    for line in body.splitlines():
        in_str = False
        cut = len(line)
        i = 0
        while i < len(line):
            ch = line[i]
            if in_str:
                if ch == "\\":
                    i += 2
                    continue
                if ch == '"':
                    in_str = False
            elif ch == '"':
                in_str = True
            elif ch == "/" and line.startswith("//", i):
                cut = i
                break
            i += 1
        out.append(line[:cut])
    return "\n".join(out)


def _ts_local_runtime_providers(path: Path) -> set[str]:
    """The provider names a TypeScript surface mirrors, for the same set.

    Both surfaces enforce their own copy of the rules before any request is
    made — the CLI refuses `--provider ollama` with no `--api-key` and checks
    `--model` against the catalogue; the dashboard disables Save and builds the
    model picker from catalogue rows. A mirror that lags the Zig list therefore
    rejects credentials the API would accept, from the surfaces operators
    actually use. `cli/` and `ui/` share no module graph with the server or with
    each other, so the literal is restated once in each and pinned here.
    """
    body = path.read_text(encoding="utf-8")
    code = _strip_line_comments(body)
    start = code.find("const LOCAL_RUNTIME_PROVIDERS")
    if start == -1:
        raise ValueError(f"LOCAL_RUNTIME_PROVIDERS not found in {path}")
    # Anchor on the ASSIGNMENT, then the array's opening bracket. A type
    # annotation (`: readonly string[] = [...]`) puts an empty `[]` between the
    # name and the members, so scanning from the declaration finds that bracket
    # pair instead and yields an empty set — which compares equal to nothing and
    # would report clean forever.
    eq_at = code.find("=", start)
    if eq_at == -1:
        raise ValueError(f"LOCAL_RUNTIME_PROVIDERS in {path} is never assigned")
    open_at = code.find("[", eq_at)
    if open_at == -1:
        raise ValueError(f"LOCAL_RUNTIME_PROVIDERS in {path} has no opening bracket")
    end = code.find("]", open_at + 1)
    if end == -1:
        raise ValueError(f"LOCAL_RUNTIME_PROVIDERS in {path} has no closing bracket")
    return set(re.findall(r'"([^"]+)"', code[open_at:end]))


def ts_local_runtime_providers() -> set[str]:
    """The CLI's mirror."""
    return _ts_local_runtime_providers(CLI_PROVIDER_CONSTANTS)


def ui_local_runtime_providers() -> set[str]:
    """The dashboard's mirror."""
    return _ts_local_runtime_providers(UI_PROVIDER_CONSTANTS)


def check_local_runtime_parity(doc: dict) -> list[str]:
    floor = {
        name
        for name, cfg in doc.get("providers", {}).items()
        if cfg.get("rate_basis") == RATE_BASIS_FLOOR
    }
    try:
        zig = zig_local_runtime_providers()
    except (OSError, ValueError) as err:
        return [f"could not read the credential layer's local-runtime list: {err}"]
    try:
        ts = ts_local_runtime_providers()
    except (OSError, ValueError) as err:
        return [f"could not read the CLI's local-runtime list: {err}"]
    try:
        ui = ui_local_runtime_providers()
    except (OSError, ValueError) as err:
        return [f"could not read the dashboard's local-runtime list: {err}"]
    problems = []
    if floor - zig:
        problems.append(
            f"carry an activation floor but the credential layer still enforces catalogue membership: {sorted(floor - zig)}"
        )
    if zig - floor:
        problems.append(
            f"bypass the credential gates but carry no activation floor in the allowlist: {sorted(zig - floor)}"
        )
    if zig != ts:
        problems.append(
            f"the CLI's local-runtime mirror disagrees with the server's: "
            f"server-only={sorted(zig - ts)}, cli-only={sorted(ts - zig)}"
        )
    if zig != ui:
        problems.append(
            f"the dashboard's local-runtime mirror disagrees with the server's: "
            f"server-only={sorted(zig - ui)}, dashboard-only={sorted(ui - zig)}"
        )
    return problems


def check_legend_covers_vocabulary(doc: dict) -> list[str]:
    """The in-file legend and this module's vocabulary must not drift apart."""
    legend = {k for k in doc.get("unpriced_reasons", {}) if not k.startswith("_")}
    missing = VALID_REASONS - legend
    extra = legend - VALID_REASONS
    problems = []
    if missing:
        problems.append(f"unpriced_reasons legend is missing {sorted(missing)}")
    if extra:
        problems.append(f"unpriced_reasons legend documents unknown codes {sorted(extra)}")
    return problems


def main() -> int:
    try:
        with ALLOWLIST.open(encoding="utf-8") as handle:
            doc = json.load(handle)
    except FileNotFoundError:
        print(f"✗ {ALLOWLIST} not found — run from the repository root", file=sys.stderr)
        return 1
    except json.JSONDecodeError as err:
        print(f"✗ {ALLOWLIST} is not valid JSON: {err}", file=sys.stderr)
        return 1

    problems = check_legend_covers_vocabulary(doc)
    problems.extend(check_local_runtime_parity(doc))
    providers = doc.get("providers", {})
    for name, cfg in providers.items():
        for check in CHECKS:
            problems.extend(check(name, cfg))

    if problems:
        for line in problems:
            print(f"✗ {line}", file=sys.stderr)
        print(f"\n✗ [models] {len(problems)} allowlist violation(s)", file=sys.stderr)
        return 1

    priced = sum(1 for cfg in providers.values() if cfg.get("models"))
    print(f"✓ [models] {len(providers)} providers — {priced} priced, {len(providers) - priced} reasoned")
    return 0


if __name__ == "__main__":
    sys.exit(main())
