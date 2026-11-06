/** Only saved repository policy crosses into the chat; credentials never do. */
export type FleetSources = {
  repositories: readonly string[];
  access: "read" | "write" | null;
  publication: "existing_pr" | "event_draft" | null;
};

export function fleetSources(config: unknown): FleetSources {
  const policy = config && typeof config === "object" && !Array.isArray(config) ? config as Record<string, unknown> : {};
  const repositories = Array.isArray(policy.repositories) ? policy.repositories.filter((value): value is string =>
    typeof value === "string" && /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(value) && !value.split("/").some(part => part === "." || part === "..")) : [];
  return {repositories: [...new Set(repositories)],
    access: policy.repository_access === "read" || policy.repository_access === "write" ? policy.repository_access : null,
    publication: policy.repository_publication === "existing_pr" || policy.repository_publication === "event_draft" ? policy.repository_publication : null};
}
