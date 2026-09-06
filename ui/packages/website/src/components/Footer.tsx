import { Link } from "react-router-dom";
import { List, ListItem } from "@agentsfleet/design-system";
import { WAITLIST_URL, DISCORD_URL, DOCS_URL, GITHUB_URL } from "../config";
import { LOOP_ANCHOR_ID, PRODUCT_NAME } from "../lib/marketing-copy";
import { SUPPORT_EMAIL } from "../lib/contact";
import { trackNavigationClicked } from "../analytics/posthog";

const COL_LABEL =
  "font-sans text-label uppercase tracking-label text-text-muted m-0 mb-3";
const COL_LINK =
  "inline-flex min-h-11 min-w-11 items-center font-sans text-body-sm text-text-muted hover:text-text transition-colors";
const COL_VARIANT = "plain";
const COL_LIST = "m-0 flex flex-col gap-1 space-y-0";
const EXTERNAL_TARGET = "_blank";
const EXTERNAL_REL = "noopener noreferrer";
const FOOTER_SURFACE = "footer";
const FOOTER_TAGLINE =
  "AI teammates that investigate incidents and help prepare fixes. You control access and decide what ships.";

export default function Footer() {
  return (
    <footer
      className="border-t border-border mt-24 pt-16 pb-12"
      data-testid={FOOTER_SURFACE}
    >
      <div className="wrap grid grid-cols-2 gap-x-6 gap-y-8 lg:gap-12 lg:grid-cols-[2fr_1fr_1fr_1fr_1fr]">
        <FooterBrand />
        <FooterColumns />
      </div>
      <FooterMeta />
    </footer>
  );
}

function FooterBrand() {
  return (
    <div className="col-span-2 flex flex-col gap-3 lg:col-span-1">
      <span className="font-sans text-body font-medium text-text">
        {PRODUCT_NAME}
      </span>
      <p className="font-sans text-body-sm leading-body text-text-muted m-0 max-w-tagline">
        {FOOTER_TAGLINE}
      </p>
    </div>
  );
}

function FooterColumns() {
  return (
    <>
      <div>
        <h2 className={COL_LABEL}>product</h2>
        <List variant={COL_VARIANT} className={COL_LIST}>
          <ListItem><a href={`/#${LOOP_ANCHOR_ID}`} className={COL_LINK}>use cases</a></ListItem>
          <ListItem><a href={WAITLIST_URL} className={COL_LINK}>dashboard</a></ListItem>
        </List>
      </div>

      <div>
        <h2 className={COL_LABEL}>resources</h2>
        <List variant={COL_VARIANT} className={COL_LIST}>
          <ListItem><a href={DOCS_URL} target={EXTERNAL_TARGET} rel={EXTERNAL_REL} className={COL_LINK}>docs</a></ListItem>
          <ListItem><Link to="/agents" className={COL_LINK}>agents</Link></ListItem>
        </List>
      </div>

      <div>
        <h2 className={COL_LABEL}>community</h2>
        <List variant={COL_VARIANT} className={COL_LIST}>
          <ListItem><a href={GITHUB_URL} target={EXTERNAL_TARGET} rel={EXTERNAL_REL} className={COL_LINK}>github</a></ListItem>
          <ListItem><a href={DISCORD_URL} target={EXTERNAL_TARGET} rel={EXTERNAL_REL} className={COL_LINK}>discord</a></ListItem>
        </List>
      </div>

      <div>
        <h2 className={COL_LABEL}>legal</h2>
        <List variant={COL_VARIANT} className={COL_LIST}>
          <ListItem><Link to="/privacy" className={COL_LINK}>privacy</Link></ListItem>
          <ListItem><Link to="/terms" className={COL_LINK}>terms</Link></ListItem>
        </List>
      </div>
    </>
  );
}

function FooterMeta() {
  return (
    <div className="wrap mt-12 pt-6 border-t border-border flex flex-wrap justify-between items-center gap-3">
      <span className="font-sans text-label text-text-subtle">
        © {new Date().getFullYear()} {PRODUCT_NAME}. all rights reserved.
      </span>
      <div className="flex flex-wrap gap-6">
        <a href={`mailto:${SUPPORT_EMAIL}`} className={COL_LINK}
          onClick={() => trackNavigationClicked({ source: "footer_contact", surface: FOOTER_SURFACE, target: "contact" })}
        >Contact</a>
      </div>
    </div>
  );
}
