import { BrandMark, Button, Nav, cn } from "@agentsfleet/design-system";
import { linkTracking } from "../analytics/link-tracking";
import { SUPPORT_EMAIL } from "../lib/contact";
import { DISCORD_URL, DOCS_URL, GITHUB_URL } from "../lib/site-links";
import { SITE_CONTAINER } from "./site-classes";

const NAV_LINK =
  "inline-flex min-h-11 items-center rounded-xs font-semibold text-body-sm text-text-muted hover:text-text focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring";

const EXTERNAL_LINK = { target: "_blank", rel: "noopener noreferrer" };

function Wordmark({ surface }: { surface: "header" | "footer" }) {
  return (
    <a
      className={cn(NAV_LINK, "shrink-0 gap-2 text-body font-bold text-text")}
      href="/"
      aria-label="agentsfleet home"
      data-testid={surface === "header" ? "brand-link" : undefined}
      {...linkTracking(`${surface}_home`, surface, "home")}
    >
      <BrandMark
        className="size-6"
        data-testid={surface === "header" ? "brand-mark" : undefined}
      />
      agentsfleet
    </a>
  );
}

export function SiteHeader({
  waitlistUrl,
  landing,
}: {
  waitlistUrl: string;
  landing: boolean;
}) {
  const home = landing ? "" : "/";
  return (
    <header className="border-b border-border bg-surface-deep">
      <div
        className={cn(
          SITE_CONTAINER,
          "flex flex-wrap items-center justify-between gap-x-lg py-2 md:grid md:grid-cols-[1fr_auto_1fr] md:py-4",
        )}
      >
        <Wordmark surface="header" />
        <Nav
          className="order-last flex basis-full flex-wrap justify-between gap-x-lg md:order-none md:gap-2xl"
          aria-label="Primary"
        >
          <a
            className={NAV_LINK}
            href={`${home}#how-it-works`}
            {...linkTracking("header_nav_product", "header", "example")}
          >
            Product
          </a>
          <a
            className={NAV_LINK}
            href={`${home}#pricing`}
            {...linkTracking("header_pricing", "header", "pricing")}
          >
            Pricing
          </a>
          <a
            className={NAV_LINK}
            href={DOCS_URL}
            {...linkTracking("header_nav_docs", "header", "docs")}
          >
            Docs
          </a>
          <a
            className={NAV_LINK}
            href={GITHUB_URL}
            {...EXTERNAL_LINK}
            {...linkTracking("header_nav_source", "header", "github")}
          >
            GitHub
          </a>
        </Nav>
        <Button asChild className="ml-auto min-h-11 justify-self-end">
          <a
            href={waitlistUrl}
            {...EXTERNAL_LINK}
            data-testid="header-install-cta"
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
  { label: "Examples", href: "/examples", target: "examples" },
  { label: "Docs", href: DOCS_URL, target: "docs" },
  { label: "Agents", href: "/agents", target: "agents" },
  { label: "OpenAPI", href: "/openapi.json", target: "openapi_json" },
  { label: "GitHub", href: GITHUB_URL, target: "github", newTab: true },
  { label: "Discord", href: DISCORD_URL, target: "discord", newTab: true },
  { label: "Privacy", href: "/privacy", target: "privacy" },
  { label: "Terms", href: "/terms", target: "terms" },
  { label: "Contact", href: `mailto:${SUPPORT_EMAIL}`, target: "contact" },
];

export function SiteFooter({ waitlistUrl }: { waitlistUrl: string }) {
  return (
    <footer
      className="border-t border-border bg-surface-deep py-2xl"
      data-testid="footer"
    >
      <div
        className={cn(
          SITE_CONTAINER,
          "flex flex-wrap items-center justify-between gap-x-3xl gap-y-md",
        )}
      >
        <Wordmark surface="footer" />
        <Nav aria-label="Footer navigation" className="flex flex-wrap gap-x-xl">
          {footerLinks.map(({ label, href, target, newTab }) => (
            <a
              key={target}
              className={NAV_LINK}
              href={href}
              {...(newTab && EXTERNAL_LINK)}
              {...linkTracking(`footer_${target}`, "footer", target)}
            >
              {label}
            </a>
          ))}
          <a
            className={NAV_LINK}
            href={waitlistUrl}
            {...EXTERNAL_LINK}
            {...linkTracking("footer_signup", "footer", "signup")}
          >
            Join waitlist
          </a>
        </Nav>
      </div>
    </footer>
  );
}
