#!/bin/sh
# capture.sh — M84_004 nftables netlink oracle (run INSIDE a privileged Linux
# container; see fixtures/README.md). Replays the exact per-lease ruleset
# `nfnetlink_rule.zig` must serialize and captures, per statement:
#   *.netlink.txt — `nft --debug=netlink` mnemonic expression dump
#   *.mnl.txt     — `nft --debug=mnl` raw netlink buffer hex dump
# Worker index 0 constants mirror Plan.zig: veth uzveth0, subnet 10.69.0.0/30,
# table uz_egress, chains egress_fwd/egress_nat, set allow0 ("fwd" is an nft
# CLI reserved word — undebuggable by operators — so chains avoid it).
# Element IPs mirror Plan.zig tests.
set -eu

OUT="${1:?usage: capture.sh <output-dir>}"
mkdir -p "$OUT"

nft --version > "$OUT/nft.version"
uname -srm >> "$OUT/nft.version"

# Each statement runs in a FRESH ruleset built up cumulatively, so every
# capture contains only its own statement's messages.
ESC="$(printf '\033')" # libmnl colors unconditionally; busybox sed has no \x1b
strip_ansi() { sed "s/${ESC}\[[0-9;]*m//g"; }
run() { # run <name> <nft statement...>
  name="$1"; shift
  nft --debug=netlink "$@" 2>&1 | strip_ansi > "$OUT/$name.netlink.txt"
  nft flush ruleset
  replay_prefix
  nft --debug=mnl "$@" 2>&1 | strip_ansi > "$OUT/$name.mnl.txt"
}

# Rebuild everything captured so far (idempotent prefix for the next capture).
PREFIX=""
replay_prefix() {
  [ -z "$PREFIX" ] || printf '%s\n' "$PREFIX" | nft -f -
}
commit() { # commit <nft statement...>
  PREFIX="$(printf '%s\n%s' "$PREFIX" "$*")"
}

nft flush ruleset

# 1. Table.
run 01_table add table inet uz_egress
commit "add table inet uz_egress"

# 2. Filter chain: forward hook, priority 0, policy drop.
run 02_chain_egress_fwd add chain inet uz_egress egress_fwd '{ type filter hook forward priority 0 ; policy drop ; }'
commit 'add chain inet uz_egress egress_fwd { type filter hook forward priority 0 ; policy drop ; }'

# 3. NAT chain: postrouting hook, srcnat priority (100).
run 03_chain_egress_nat add chain inet uz_egress egress_nat '{ type nat hook postrouting priority 100 ; }'
commit "add chain inet uz_egress egress_nat { type nat hook postrouting priority 100 ; }"

# 4. IPv4 allowlist set.
run 04_set_allow0 add set inet uz_egress allow0 '{ type ipv4_addr ; }'
commit "add set inet uz_egress allow0 { type ipv4_addr ; }"

# 5. Set elements (Plan.zig test addresses).
run 05_elem_allow0 add element inet uz_egress allow0 '{ 1.2.3.4, 10.20.30.40 }'
commit "add element inet uz_egress allow0 { 1.2.3.4, 10.20.30.40 }"

# 6+7. DNS-tunnel closure (§3.3): ALL port-53 egress from the child dropped,
# BEFORE the allowlist accept — an allowed IP must not be reachable on :53.
run 06_rule_drop_dns_udp add rule inet uz_egress egress_fwd iifname '"uzveth0"' udp dport 53 drop
commit 'add rule inet uz_egress egress_fwd iifname "uzveth0" udp dport 53 drop'
run 07_rule_drop_dns_tcp add rule inet uz_egress egress_fwd iifname '"uzveth0"' tcp dport 53 drop
commit 'add rule inet uz_egress egress_fwd iifname "uzveth0" tcp dport 53 drop'

# 8. The allowlist accept: child egress (in via host-side veth) to a resolved IP.
run 08_rule_allow_set add rule inet uz_egress egress_fwd iifname '"uzveth0"' ip daddr @allow0 accept
commit 'add rule inet uz_egress egress_fwd iifname "uzveth0" ip daddr @allow0 accept'

# 9. Return path: established/related back toward the child survives policy drop.
run 09_rule_ct_return add rule inet uz_egress egress_fwd oifname '"uzveth0"' ct state established,related accept
commit 'add rule inet uz_egress egress_fwd oifname "uzveth0" ct state established,related accept'

# 10. Masquerade the /30 source out the uplink.
run 10_rule_masquerade add rule inet uz_egress egress_nat ip saddr 10.69.0.0/30 oifname != '"uzveth0"' masquerade
commit 'add rule inet uz_egress egress_nat ip saddr 10.69.0.0/30 oifname != "uzveth0" masquerade'

# 11. Teardown: delete the whole table (drops chains/sets/rules in one shot).
# (The veth delete is rtnetlink — no nft dump exists for it.)
nft flush ruleset
replay_prefix
nft --debug=netlink delete table inet uz_egress > "$OUT/11_del_table.netlink.txt" 2>&1
nft flush ruleset
replay_prefix
nft --debug=mnl delete table inet uz_egress 2>&1 | strip_ansi > "$OUT/11_del_table.mnl.txt"

# Final state: human-readable ruleset with handles, for cross-reference.
nft flush ruleset
replay_prefix
nft -a list ruleset > "$OUT/99_final_ruleset.txt"

echo "CAPTURE_OK: $(ls "$OUT" | wc -l) files in $OUT"
