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
  6. sentinel rates announce themselves — a rate small enough to be a placeholder
                              must carry `rate_basis`, or it reads as a real
                              price nobody meant to charge.

Exit 0 if clean, 1 with one line per violation.
"""
import json
import sys
from pathlib import Path

ALLOWLIST = Path("scripts/model-library-allowlist.json")
FIXTURE_DIR = Path("samples/fixtures/model-library")

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
        "operator_hosted",
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

# A rate row that is a placeholder rather than a price must say so under this
# key. No provider carries one today; the check stays because a sentinel that
# does not announce itself is indistinguishable from a real rate.
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
