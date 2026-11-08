import { parseRegistryAllowlist, type NetworkPolicy } from "@/lib/api/runners-types";

// Mirrors af_wire::policy::PACKAGE_MANAGER_DOMAINS, the runner's exact-host preset.
export const PACKAGE_MANAGER_DOMAINS = ["registry.npmjs.org", "pypi.org", "files.pythonhosted.org", "static.crates.io", "crates.io", "index.crates.io", "proxy.golang.org", "sum.golang.org"] as const;
const preset: ReadonlySet<string> = new Set(PACKAGE_MANAGER_DOMAINS);

export function runnerNetworkForm(policy: NetworkPolicy, registry: string) {
  const parsed = parseRegistryAllowlist(registry);
  const packages = policy !== "deny_all_egress" && (parsed.hosts.length === 0 || PACKAGE_MANAGER_DOMAINS.every(host => parsed.hosts.includes(host)));
  return {publicInternet: policy === "allow_all", packages,
    domains: packages ? parsed.hosts.filter(host => !preset.has(host)) : parsed.hosts};
}

/** Empty registry lists enable the native preset, so off + no domains uses deny-all. */
export function runnerNetworkAssignment(publicInternet: boolean, packages: boolean, domains: readonly string[]) {
  const hosts = [...new Set(packages && domains.length ? [...PACKAGE_MANAGER_DOMAINS, ...domains] : domains)];
  const network_policy: NetworkPolicy = publicInternet ? "allow_all" : packages || hosts.length ? "allow_list_egress" : "deny_all_egress";
  return {network_policy, registry_allowlist: hosts.join(", ")};
}
