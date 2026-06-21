# M84_004 nftables netlink oracle fixtures

`nfnetlink_rule.zig` (and the rest of `src/runner/network/`) serializes
netlink by hand. Self-invented golden bytes prove only self-consistency, so
the rule/expression encoding is validated against what the real `nft`
userspace sends the kernel — captured here, per statement, for the exact
per-lease ruleset the spec mandates (`docs/v2/active/M84_004_*.md` §1–§3):

| Fixture | Statement |
|---|---|
| `01_table` | `add table inet uz_egress` |
| `02_chain_egress_fwd` | filter chain, forward hook, priority 0, policy drop |
| `03_chain_egress_nat` | nat chain, postrouting hook, priority 100 (srcnat) |
| `04_set_allow0` | IPv4-key allowlist set |
| `05_elem_allow0` | set elements `{ 1.2.3.4, 10.20.30.40 }` |
| `06/07_rule_drop_dns_*` | drop ALL child egress to port 53, UDP+TCP (§3.3) |
| `08_rule_allow_set` | `iifname "uzveth0" ip daddr @allow0 accept` |
| `09_rule_ct_return` | `oifname "uzveth0" ct state established,related accept` |
| `10_rule_masquerade` | `ip saddr 10.69.0.0/30 oifname != "uzveth0" masquerade` |
| `99_final_ruleset` | `nft -a list ruleset` of the assembled state |

Per statement: `*.netlink.txt` is the `--debug=netlink` mnemonic expression
dump (the expression-VM oracle: registers, cmp operands, expr order);
`*.mnl.txt` is the `--debug=mnl` raw netlink buffer hex (the attribute-level
oracle: lengths, types, nesting, byte order). Capture environment is pinned
in `nft.version` (nftables 1.1.6, Linux 6.12, aarch64 — netlink bytes are
identical across little-endian arches).

Findings the oracle already produced:

- **`NFTA_SET_KEY_TYPE` (attr 4) = 7 (`TYPE_IPADDR`) is mandatory** in
  `NEWSET` — `nfnetlink.zig`'s `newSet` omitted it. `nft` also sends set
  userdata (attr 13, typeof metadata) which only feeds `nft list` rendering.
- **`fwd` is an nft CLI reserved word** (the netdev `fwd` statement): a chain
  named `fwd` is valid at the netlink layer but cannot be addressed by
  operators via the `nft` CLI at all. Chains are therefore `egress_fwd` /
  `egress_nat`.

## Regenerating

Any Linux with nftables works; on a Mac, Docker Desktop's VM is enough:

```sh
docker run --rm --privileged -v "$PWD":/fixtures alpine:latest \
  sh -c 'apk add -q nftables && /fixtures/capture.sh /fixtures/captured'
```

Re-capture only with intent (a ruleset change in the spec, or an nft/kernel
bump worth re-pinning) — the golden-byte tests cite these files.
