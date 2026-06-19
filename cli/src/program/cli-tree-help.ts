// Top-level help tail for the agentsfleet program — a one-line pointer to the
// configuration reference. The full environment-variable matrix lives in the
// docs (docs.agentsfleet.net/cli/configuration) so the CLI help stays terse and
// the env list keeps a single source of truth. `addHelpText("after", …)`
// appends this verbatim; commander still owns the layout above it.

const CONFIG_DOCS_URL = "https://docs.agentsfleet.net/cli/configuration";

export function helpTail(): string {
  return [
    "",
    `Environment variables: ${CONFIG_DOCS_URL}`,
  ].join("\n");
}
