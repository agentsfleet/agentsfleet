import { Terminal } from "@agentsfleet/design-system";
import { DOCS_URL, SELF_HOSTED_URL } from "../config";
import { trackInteractionClicked, trackNavigationClicked } from "../analytics/posthog";

const INSTALL_COMMAND = "curl -fsSL https://agentsfleet.dev | bash";
const SURFACE = "cli_quickstart";

export function CliQuickstart() {
  return <div className="cli-quickstart">
    <p><span>Prefer your coding agent?</span> Claude Code · Codex · OpenCode</p>
    <Terminal label="Install agentsfleet" copyable copyText={INSTALL_COMMAND}
      onCopyClick={() => trackInteractionClicked({ source: "cli_install_copy", surface: SURFACE, target: "installation" })}>{INSTALL_COMMAND}</Terminal>
    <p className="cli-footnote"><a href={`${DOCS_URL}/cli/install`} target="_blank" rel="noopener noreferrer"
      onClick={() => trackNavigationClicked({ source: "cli_install_guide", surface: SURFACE, target: "installation" })}>Installation guide ↗</a><span> · </span><a href={SELF_HOSTED_URL} target="_blank" rel="noopener noreferrer"
      onClick={() => trackNavigationClicked({ source: "cli_self_hosted", surface: SURFACE, target: "self_hosted" })}>Self hosting in your infrastructure ↗</a></p>
  </div>;
}
