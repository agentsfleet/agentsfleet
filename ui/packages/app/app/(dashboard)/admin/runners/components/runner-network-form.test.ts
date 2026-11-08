import { expect, it } from "vitest";
import { PACKAGE_MANAGER_DOMAINS, runnerNetworkAssignment, runnerNetworkForm } from "./runner-network-form";

it("encodes all-off as deny-all so empty lists cannot silently enable the native package preset", () => {
  expect(runnerNetworkAssignment(false, false, [])).toEqual({network_policy: "deny_all_egress", registry_allowlist: ""});
  expect(runnerNetworkForm("deny_all_egress", "")).toEqual({publicInternet: false, packages: false, domains: []});
});
it("keeps public access distinct from package-only and custom-only assignments", () => {
  expect(runnerNetworkAssignment(true, false, [])).toEqual({network_policy: "allow_all", registry_allowlist: ""});
  expect(runnerNetworkAssignment(false, true, [])).toEqual({network_policy: "allow_list_egress", registry_allowlist: ""});
  expect(runnerNetworkAssignment(false, false, ["mirror.example", "mirror.example"])).toEqual({network_policy: "allow_list_egress", registry_allowlist: "mirror.example"});
  expect(runnerNetworkForm("allow_list_egress", "mirror.example")).toEqual({publicInternet: false, packages: false, domains: ["mirror.example"]});
});
it("combines the exact package preset with additional hosts without replacing either", () => {
  const stored = runnerNetworkAssignment(false, true, ["mirror.example"]);
  expect(stored).toEqual({network_policy: "allow_list_egress", registry_allowlist: [...PACKAGE_MANAGER_DOMAINS, "mirror.example"].join(", ")});
  expect(runnerNetworkForm(stored.network_policy, stored.registry_allowlist)).toEqual({publicInternet: false, packages: true, domains: ["mirror.example"]});
  expect(runnerNetworkForm("allow_list_egress", "registry.npmjs.org")).toEqual({publicInternet: false, packages: false, domains: ["registry.npmjs.org"]});
});
