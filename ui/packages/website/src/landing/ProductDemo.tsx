import { useRef, type CSSProperties, type ReactNode } from "react";
import { ScoutInvestigation } from "./ScoutInvestigation";
import { useDemoPlayback } from "./useDemoPlayback";

function SlackMessage({
  bot,
  time,
  at,
  children,
}: {
  bot?: boolean;
  time: string;
  at?: string;
  children: ReactNode;
}) {
  return (
    <div
      className="sl-message"
      data-arrive={at && ""}
      style={{ "--at": at } as CSSProperties}
    >
      <span className="sl-avatar" aria-hidden="true">
        {bot ? <img src="/favicon.svg" alt="" /> : "L"}
      </span>
      <div>
        <p className="sl-sender">
          <strong>{bot ? "agentfleet" : "Leah"}</strong>
          {bot && <small className="bot-label">APP</small>}
          <time>{time}</time>
        </p>
        <p>{children}</p>
      </div>
    </div>
  );
}

function Mention() {
  return <b className="sl-mention">@agentfleet</b>;
}

function SlackThread({
  scene,
  label,
  children,
}: {
  scene: string;
  label: string;
  children: ReactNode;
}) {
  return (
    <section className={`beat slack ${scene}`} aria-label={label}>
      <div className="sl-title">
        <span className="sl-hash" aria-hidden="true">
          #
        </span>
        customer-support <span>Slack thread</span>
      </div>
      <div className="sl-body">
        <SlackMessage time="09:02">
          <Mention /> mira: Chat froze again at 8:55. I hit Send twice. Any idea
          why?
        </SlackMessage>
        <div className="sl-replies">{children}</div>
      </div>
    </section>
  );
}

export function ProductDemo() {
  const film = useRef<HTMLElement>(null);
  useDemoPlayback(film);
  return (
    <figure
      ref={film}
      className="film"
      id="product-demo"
      aria-labelledby="film-heading"
    >
      <figcaption className="film-head">
        <h2 id="film-heading">Why did chat freeze?</h2>
        <small>Example thread · IST</small>
      </figcaption>
      <div className="stage">
        <SlackThread scene="opening" label="Slack ticket in customer-support">
          <SlackMessage bot time="09:02" at="1.25s">
            I’ve asked Scout to check the connection.
          </SlackMessage>
        </SlackThread>
        <ScoutInvestigation />
        <SlackThread
          scene="returning"
          label="Updates to Leah’s original Slack ticket"
        >
          <div className="updates repair">
            <SlackMessage time="09:08">
              <Mention /> incident-repairer: Can you fix the connection and add
              a test?
            </SlackMessage>
            <SlackMessage bot time="09:19" at="17.8s">
              <strong>Draft fix ready for review</strong>
              <br />
              The runner sent packets that were too big. The fix sends smaller
              ones. The test passes.
            </SlackMessage>
          </div>
          <div className="updates verify">
            <SlackMessage time="11:01">
              I reviewed the fix and shipped it at 9:55. <Mention /> mira: Are
              replies faster now?
            </SlackMessage>
            <SlackMessage bot time="11:02" at="25.6s">
              <strong>
                95% of replies started within 2.8 seconds, down from 15.1
                seconds.
              </strong>{" "}
              Packet loss fell from 8% to 0.2%.
              <small className="window">
                Grafana · Similar traffic
                <br />
                08:00–09:00 and 10:00–11:00 IST
              </small>
            </SlackMessage>
          </div>
        </SlackThread>
      </div>
      <div className="controls" hidden>
        <div className="meter" aria-hidden="true">
          <span />
        </div>
        <button type="button">Pause demo</button>
      </div>
    </figure>
  );
}
