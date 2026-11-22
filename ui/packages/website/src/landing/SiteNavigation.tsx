import { BrandMark, Button, Nav } from "@agentsfleet/design-system";
import { DOCS_URL, GITHUB_URL, DISCORD_URL } from "../lib/site-links";
import { SUPPORT_EMAIL } from "../lib/contact";
import { linkTracking } from "./link-tracking";

function Wordmark({ surface }: { surface: string }) {
  return (
    <a
      className="wordmark"
      href="#top"
      aria-label="agentsfleet home"
      {...linkTracking(`${surface}_home`, surface, "home")}
    >
      <BrandMark className="site-mark" /> agentsfleet
    </a>
  );
}
export function SiteHeader({ waitlistUrl }: { waitlistUrl: string }) {
  return (
    <header className="site-header">
      <div className="wrap header-content">
        <Wordmark surface="header" />
        <Nav className="site-nav" aria-label="Primary">
          <a
            href="#how-it-works"
            {...linkTracking("header_example", "header", "example")}
          >
            Product
          </a>
          <a
            href="#pricing"
            {...linkTracking("header_pricing", "header", "pricing")}
          >
            Pricing
          </a>
          <a
            href={DOCS_URL + "/"}
            {...linkTracking("header_docs", "header", "docs")}
          >
            Docs
          </a>
          <a
            target="_blank"
            rel="noopener noreferrer"
            href={GITHUB_URL}
            {...linkTracking("header_github", "header", "github")}
          >
            GitHub
          </a>
        </Nav>
        <Button asChild>
          <a
            data-testid="header-install-cta"
            className="button header-button"
            href={waitlistUrl}
            {...linkTracking("header_signup", "header", "signup")}
          >
            Join waitlist
          </a>
        </Button>
      </div>
    </header>
  );
}
const footerLinks = [
  { label: "Docs", href: DOCS_URL + "/", target: "docs" },
  { label: "Examples", href: "/examples", target: "examples" },
  { label: "Agents", href: "/agents", target: "agents" },
  { label: "OpenAPI", href: "/openapi.json", target: "openapi_json" },
  { label: "GitHub", href: GITHUB_URL, target: "github" },
  { label: "Discord", href: DISCORD_URL, target: "discord" },
  { label: "Privacy", href: "/privacy", target: "privacy" },
  { label: "Terms", href: "/terms", target: "terms" },
  { label: "Contact", href: `mailto:${SUPPORT_EMAIL}`, target: "mailto" },
];

export function SiteFooter() {
  return (
    <footer className="site-footer">
      <div className="wrap footer-content">
        <Wordmark surface="footer" />
        <Nav aria-label="Footer navigation" className="footer-links">
          {footerLinks.map(({ label, href, target }) => (
            <a
              key={target}
              href={href}
              {...(target === "github" && {
                target: "_blank",
                rel: "noopener noreferrer",
              })}
              {...linkTracking(`footer_${target}`, "footer", target)}
            >
              {label}
            </a>
          ))}
        </Nav>
      </div>
    </footer>
  );
}
