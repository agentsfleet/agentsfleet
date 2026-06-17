import { Link } from "react-router-dom";
import { List, ListItem } from "@agentsfleet/design-system";
import { DISCORD_URL, DOCS_URL, GITHUB_URL } from "../config";
import { LOOP_ANCHOR_ID, PRODUCT_NAME } from "../lib/marketing-copy";

const COL_LABEL =
  "font-mono text-label uppercase tracking-label text-text-muted m-0 mb-3";
const COL_LINK =
  "font-mono text-mono text-text-muted hover:text-text transition-colors";
const FOOTER_TAGLINE =
  "Resident engineer that compounds operational knowledge from recurring problem classes.";

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
      <div className="wrap grid gap-12 lg:grid-cols-[2fr_1fr_1fr_1fr_1fr]">
        <div className="flex flex-col gap-3">
          <span className="font-mono text-body font-medium text-text">
            {PRODUCT_NAME}
          </span>
          <p className="font-sans text-body-sm leading-body text-text-muted m-0 max-w-tagline">
            {FOOTER_TAGLINE}
          </p>
        </div>

        <div>
          <h4 className={COL_LABEL}>product</h4>
          <List variant="plain" className="m-0 flex flex-col gap-2 space-y-0">
            <ListItem><a href={`/#${LOOP_ANCHOR_ID}`} className={COL_LINK}>loop</a></ListItem>
            <ListItem><a href="/#pricing" className={COL_LINK}>pricing</a></ListItem>
            <ListItem><Link to="/agents" className={COL_LINK}>agents</Link></ListItem>
          </List>
        </div>

        <div>
          <h4 className={COL_LABEL}>resources</h4>
          <List variant="plain" className="m-0 flex flex-col gap-2 space-y-0">
            <ListItem><a href={DOCS_URL} target="_blank" rel="noopener noreferrer" className={COL_LINK}>docs</a></ListItem>
            <ListItem><a href="/llms.txt" className={COL_LINK}>llms.txt</a></ListItem>
            <ListItem><a href="/llms-full.txt" className={COL_LINK}>llms-full.txt</a></ListItem>
            <ListItem><a href="/openapi.json" className={COL_LINK}>OpenAPI</a></ListItem>
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
          © {new Date().getFullYear()} {PRODUCT_NAME}. all rights reserved.
        </span>
      </div>
    </footer>
  );
}
