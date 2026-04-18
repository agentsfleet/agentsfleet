import { FormEvent, useEffect, useMemo, useRef, useState } from "react";
import { Button } from "@usezombie/design-system";
import { MARKETING_LEAD_CAPTURE_URL, TEAM_EMAIL } from "../config";
import {
  trackLeadCaptureFailed,
  trackLeadCaptureSubmitted,
} from "../analytics/posthog";

type LeadIntent = {
  ctaId: string;
  planInterest: string;
  title: string;
  description: string;
  actionLabel: string;
};

type Props = {
  intent: LeadIntent | null;
};

function readUtms(search: string) {
  const params = new URLSearchParams(search);
  return {
    utm_source: params.get("utm_source") ?? "",
    utm_medium: params.get("utm_medium") ?? "",
    utm_campaign: params.get("utm_campaign") ?? "",
  };
}

export default function PricingLeadCapture({ intent }: Props) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [email, setEmail] = useState("");
  const [status, setStatus] = useState<"idle" | "submitting" | "success" | "error">("idle");
  const [errorMessage, setErrorMessage] = useState("");

  const [prevIntent, setPrevIntent] = useState(intent);
  if (prevIntent !== intent) {
    setPrevIntent(intent);
    setStatus("idle");
    setErrorMessage("");
    setEmail("");
  }

  useEffect(() => {
    if (intent) inputRef.current?.focus();
  }, [intent]);

  const utms = useMemo(() => readUtms(window.location.search), []);

  if (!intent) {
    return (
      <section className="pricing-lead-shell" aria-label="Plan interest">
        <div className="pricing-lead-card pricing-lead-card--placeholder">
          <p className="pricing-lead-eyebrow">Pricing interest</p>
          <h2>Choose a paid plan to open the on-site notify flow.</h2>
          <p>
            Keep the homepage clean, then capture launch demand only when someone has
            pricing intent.
          </p>
        </div>
      </section>
    );
  }

  const activeIntent = intent;

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    if (!MARKETING_LEAD_CAPTURE_URL) {
      setStatus("error");
      setErrorMessage(`Notify me is not configured yet. Add VITE_MARKETING_LEAD_CAPTURE_URL or contact ${TEAM_EMAIL}.`);
      trackLeadCaptureFailed({
        page: "pricing",
        surface: "pricing_lead_capture",
        cta_id: activeIntent.ctaId,
        plan_interest: activeIntent.planInterest,
        status: "missing_endpoint",
      });
      return;
    }

    setStatus("submitting");
    setErrorMessage("");

    const payload = {
      email,
      page: "pricing",
      cta_id: activeIntent.ctaId,
      plan_interest: activeIntent.planInterest,
      timestamp: new Date().toISOString(),
      ...utms,
    };

    try {
      const response = await fetch(MARKETING_LEAD_CAPTURE_URL, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify(payload),
      });

      if (!response.ok) {
        throw new Error(`lead_capture_failed:${response.status}`);
      }

      setStatus("success");
      trackLeadCaptureSubmitted({
        page: "pricing",
        surface: "pricing_lead_capture",
        cta_id: activeIntent.ctaId,
        plan_interest: activeIntent.planInterest,
        status: "success",
        ...utms,
      });
    } catch {
      setStatus("error");
      setErrorMessage("Something went wrong. Please try again in a moment.");
      trackLeadCaptureFailed({
        page: "pricing",
        surface: "pricing_lead_capture",
        cta_id: activeIntent.ctaId,
        plan_interest: activeIntent.planInterest,
        status: "submit_failed",
        ...utms,
      });
    }
  }

  return (
    <section className="pricing-lead-shell" aria-label="Plan interest">
      <div className="pricing-lead-card">
        <div className="pricing-lead-copy">
          <p className="pricing-lead-eyebrow">{activeIntent.planInterest}</p>
          <h2>{activeIntent.title}</h2>
          <p>{activeIntent.description}</p>
        </div>

        {status === "success" ? (
          <div className="pricing-lead-success" role="status" aria-live="polite">
            <p className="pricing-lead-success-tag">You’re in</p>
            <h3>We’ve saved your interest for {activeIntent.planInterest}.</h3>
            <p>
              We’ll reach out when this plan is ready for early access, rollout details,
              or pricing updates.
            </p>
          </div>
        ) : (
          <form className="pricing-lead-form" onSubmit={handleSubmit}>
            <label className="pricing-lead-label" htmlFor="pricing-lead-email">
              Work email
            </label>
            <div className="pricing-lead-row">
              <input
                ref={inputRef}
                id="pricing-lead-email"
                className="pricing-lead-input"
                type="email"
                autoComplete="email"
                value={email}
                onChange={(event) => setEmail(event.target.value)}
                placeholder="you@company.com"
                required
              />
              <Button type="submit" disabled={status === "submitting"}>
                {status === "submitting" ? "Submitting..." : activeIntent.actionLabel}
              </Button>
            </div>
            <p className="pricing-lead-note">
              This stays on-site. We only capture pricing intent from this page, with
              UTM attribution when present.
            </p>
            {status === "error" ? (
              <p className="pricing-lead-error" role="alert">
                {errorMessage}
              </p>
            ) : null}
          </form>
        )}
      </div>
    </section>
  );
}
