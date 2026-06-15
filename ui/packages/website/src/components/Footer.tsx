import { Link } from "react-router-dom";
import { List, ListItem } from "@agentsfleet/design-system";
import { DISCORD_URL, DOCS_URL, GITHUB_URL } from "../config";

const BRAND_NAME = "agentsfleet";

const COL_LABEL =
  "font-mono text-label uppercase tracking-label text-text-muted m-0 mb-3";
const COL_LINK =
  "font-mono text-mono text-text-muted hover:text-text transition-colors";

/*
 * Footer — mono labels, restrained two-row layout. No decorative
 * separator gradient; one hairline border at the top of the lower row.
 */
export default function Footer() {
  return (
    <footer
      className="border-t border-border mt-24 pt-16 pb-12"
      data-testid="footer"
    >
      <div className="wrap grid gap-12 lg:grid-cols-footer">
        <div className="flex flex-col gap-3">
          <span className="font-mono text-body font-medium text-text">
            {BRAND_NAME}
          </span>
          <p className="font-sans text-body-sm leading-body text-text-muted m-0 max-w-tagline">
            Durable, markdown-defined agents that wake on your events and own
            the outcome.
          </p>
        </div>

        <div>
          <h4 className={COL_LABEL}>product</h4>
          <List variant="plain" className="m-0 flex flex-col gap-2 space-y-0">
            <ListItem><Link to="/" className={COL_LINK}>features</Link></ListItem>
            <ListItem><a href="/#pricing" className={COL_LINK}>pricing</a></ListItem>
            <ListItem><a href={DOCS_URL} target="_blank" rel="noopener noreferrer" className={COL_LINK}>docs</a></ListItem>
            <ListItem><Link to="/agents" className={COL_LINK}>agents</Link></ListItem>
          </List>
        </div>

        <div>
          <h4 className={COL_LABEL}>community</h4>
          <List variant="plain" className="m-0 flex flex-col gap-2 space-y-0">
            <ListItem><a href={GITHUB_URL} target="_blank" rel="noopener noreferrer" className={COL_LINK}>github</a></ListItem>
            <ListItem><a href={DISCORD_URL} target="_blank" rel="noopener noreferrer" className={COL_LINK}>discord</a></ListItem>
          </List>
        </div>

        <div>
          <h4 className={COL_LABEL}>legal</h4>
          <List variant="plain" className="m-0 flex flex-col gap-2 space-y-0">
            <ListItem><Link to="/privacy" className={COL_LINK}>privacy</Link></ListItem>
            <ListItem><Link to="/terms" className={COL_LINK}>terms</Link></ListItem>
          </List>
        </div>
      </div>

      <div className="wrap mt-12 pt-6 border-t border-border flex flex-wrap justify-between items-center gap-3">
        <span className="font-mono text-label text-text-subtle">
          © {new Date().getFullYear()} {BRAND_NAME}. all rights reserved.
        </span>
      </div>
    </footer>
  );
}
