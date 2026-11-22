import type { CSSProperties } from "react";
import { ArrowUpIcon } from "@phosphor-icons/react/dist/ssr/ArrowUp";
import { AtIcon } from "@phosphor-icons/react/dist/ssr/At";
import { CaretDownIcon } from "@phosphor-icons/react/dist/ssr/CaretDown";
import { CheckCircleIcon } from "@phosphor-icons/react/dist/ssr/CheckCircle";
import { CpuIcon } from "@phosphor-icons/react/dist/ssr/Cpu";
import { GitBranchIcon } from "@phosphor-icons/react/dist/ssr/GitBranch";
import { KeyIcon } from "@phosphor-icons/react/dist/ssr/Key";
import { ListBulletsIcon } from "@phosphor-icons/react/dist/ssr/ListBullets";
import { NotePencilIcon } from "@phosphor-icons/react/dist/ssr/NotePencil";
import { PaperclipIcon } from "@phosphor-icons/react/dist/ssr/Paperclip";
import { PlugsConnectedIcon } from "@phosphor-icons/react/dist/ssr/PlugsConnected";
import { SidebarSimpleIcon } from "@phosphor-icons/react/dist/ssr/SidebarSimple";
import { StackIcon } from "@phosphor-icons/react/dist/ssr/Stack";

function FleetSummary() {
  return (
    <aside className="summary setup" aria-label="Scout fleet setup summary">
      <details open>
        <summary className="panel-heading">
          Summary{" "}
          <span className="product-icon" aria-hidden="true">
            <ListBulletsIcon />
          </span>
        </summary>
        <dl className="setup-meta">
          <div>
            <dt>Model</dt>
            <dd>Default model</dd>
          </div>
          <div>
            <dt>Spending limit</dt>
            <dd>$3.00 / day</dd>
          </div>
          <div>
            <dt>Next wake-up</dt>
            <dd>09:37 AM IST</dd>
          </div>
        </dl>
        <section className="setup-block">
          <h3>
            Repositories <span>Manage</span>
          </h3>
          <p className="resource">
            <span className="product-icon" aria-hidden="true">
              <GitBranchIcon />
            </span>{" "}
            agentsfleet/agentsfleet
          </p>
          <p className="tools-line">
            Tools <span>http request · +3 more</span>
          </p>
        </section>
        <section className="setup-block">
          <h3>Public internet access</h3>
          <p className="network">
            Allow all <input className="switch" type="checkbox" disabled aria-label="Allow all destinations" />
          </p>
          <p className="network">
            Enable package manager domains{" "}
            <input className="switch" type="checkbox" disabled aria-label="Enable package manager domains" />
          </p>
        </section>
        <section className="setup-block">
          <h3>
            Secrets <span>Manage</span>
          </h3>
          <p className="resource credential">
            <span className="product-icon" aria-hidden="true">
              <KeyIcon />
            </span>{" "}
            grafana
          </p>
        </section>
      </details>
    </aside>
  );
}

function GrafanaEvidence() {
  return (
    <details className="work-group">
      <summary>
        <i data-icon="check-circle" aria-hidden="true">
          <CheckCircleIcon />
        </i>
        <span>Worked for 4s · 2 tools</span>
        <i data-icon="caret-down" aria-hidden="true">
          <CaretDownIcon />
        </i>
      </summary>
      <div className="work-content">
        <details className="trace">
          <summary>
            <i data-icon="check-circle" aria-hidden="true">
              <CheckCircleIcon />
            </i>
            <span>Read Grafana metrics</span>
            <i data-icon="caret-down" aria-hidden="true">
              <CaretDownIcon />
            </i>
          </summary>
          <div className="tool-output">
            <header>
              <code>http_request</code>
            </header>
            <pre>
              {
                "Prometheus · 08:00–09:00 IST\nFirst words p95: 15.1 s. Packet loss: 8%. Runner reconnects: 5. Model first token remained near baseline."
              }
            </pre>
          </div>
        </details>
        <details className="trace">
          <summary>
            <i data-icon="check-circle" aria-hidden="true">
              <CheckCircleIcon />
            </i>
            <span>Read Grafana logs</span>
            <i data-icon="caret-down" aria-hidden="true">
              <CaretDownIcon />
            </i>
          </summary>
          <div className="tool-output">
            <header>
              <code>http_request</code>
            </header>
            <pre>
              {
                "Loki · runner connection events\nThe runner lost its connection during the freezes."
              }
            </pre>
          </div>
        </details>
      </div>
    </details>
  );
}

function ComposerPreview() {
  return (
    <div className="composer" aria-hidden="true">
      <span>Message your fleet…</span>
      <div className="composer-controls">
        <span className="product-icon" aria-hidden="true">
          <PaperclipIcon />
        </span>
        <span className="product-icon" aria-hidden="true">
          <AtIcon />
        </span>
        <span className="composer-model">
          <span className="product-icon" aria-hidden="true">
            <CpuIcon />
          </span>{" "}
          Default model{" "}
          <span className="product-icon" aria-hidden="true">
            <CaretDownIcon />
          </span>
        </span>
        <span className="composer-send">
          <span className="product-icon" aria-hidden="true">
            <ArrowUpIcon />
          </span>
        </span>
      </div>
    </div>
  );
}

export function ScoutInvestigation() {
  return (
    <article
      className="beat app investigation"
      aria-label="Scout investigates the recurring chat freeze"
    >
      <div className="app-rail" aria-hidden="true">
        <img src="/favicon.svg" width="37" height="37" alt="" />
        <span className="rail-item current">
          <span className="product-icon" aria-hidden="true">
            <NotePencilIcon />
          </span>
        </span>
        <span className="rail-item">
          <span className="product-icon" aria-hidden="true">
            <KeyIcon />
          </span>
        </span>
        <span className="rail-item">
          <span className="product-icon" aria-hidden="true">
            <PlugsConnectedIcon />
          </span>
        </span>
        <span className="rail-item">
          <span className="product-icon" aria-hidden="true">
            <StackIcon />
          </span>
        </span>
        <span className="rail-item bottom">
          <span className="product-icon" aria-hidden="true">
            <SidebarSimpleIcon />
          </span>
        </span>
        <span className="rail-avatar">A</span>
      </div>
      <div className="app-surface">
        <div className="app-top">
          <span className="product-icon" aria-hidden="true">
            <StackIcon />
          </span>
          <strong>Scout</strong>
          <span className="fleet-job">Performance check</span>
          <span className="product-icon" aria-hidden="true">
            <NotePencilIcon />
          </span>
        </div>
        <div className="app-body">
          <section className="conversation" aria-label="Scout’s conversation">
            <article className="user-turn">
              <div className="user-bubble">
                <p>What slowed down replies around 8:55?</p>
              </div>
              <div className="message-meta">@mira · 09:02</div>
            </article>
            <article className="assistant-turn">
              <div className="message-meta">
                <img src="/favicon.svg" alt="" />
                <strong>Scout</strong>
                <span>· 09:03</span>
              </div>
              <GrafanaEvidence />
              <p
                className="evidence"
                data-arrive=""
                style={{ "--at": "7.6s" } as CSSProperties}
              >
                Grafana · 08:00–09:00 IST
                <br />
                <strong>
                  About 1 in 20 replies took 15 seconds or more to start.
                </strong>{" "}
                The model answered as fast as usual.
              </p>
              <p
                className="evidence link-result"
                data-arrive=""
                style={{ "--at": "9.15s" } as CSSProperties}
              >
                The runner lost <strong>8% of packets</strong> and reconnected{" "}
                <strong>5 times</strong> that hour. I’d check the connection
                first.
              </p>
            </article>
            <ComposerPreview />
          </section>
          <FleetSummary />
        </div>
      </div>
    </article>
  );
}
