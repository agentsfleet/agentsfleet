import { useRef, type ReactNode } from "react";
import { Button, cn } from "@agentsfleet/design-system";
import { ScoutInvestigation } from "./ScoutInvestigation";
import { Mention, SlackMessage, SlackThread } from "./SlackThread";
import { DEMO_STAGES, useDemoPlayback } from "./useDemoPlayback";

function Scene({
  active,
  staticMode,
  animate,
  children,
}: {
  active: boolean;
  staticMode: boolean;
  animate: boolean;
  children: ReactNode;
}) {
  return (
    <div
      data-active={active}
      inert={!staticMode && !active}
      className={cn(
        "min-w-0",
        staticMode
          ? "snap-start"
          : "col-start-1 row-start-1 data-[active=false]:invisible data-[active=false]:opacity-0",
        animate && "transition-[opacity,visibility] duration-500",
      )}
    >
      {children}
    </div>
  );
}

export function ProductDemo() {
  const stage = useRef<HTMLDivElement>(null);
  const demo = useDemoPlayback(stage);
  const scenes = [
    <SlackThread key="ticket" label="Slack ticket in customer-support">
      <SlackMessage time="09:02">
        <Mention /> mira: Chat froze again at 8:55. I hit Send twice. Any idea
        why?
      </SlackMessage>
      <SlackMessage bot time="09:02">
        I’ve asked Scout to check the connection.
      </SlackMessage>
    </SlackThread>,
    <ScoutInvestigation key="investigation" onInspect={demo.pause} />,
    <SlackThread key="draft" label="Draft fix in the Slack thread">
      <SlackMessage time="09:08">
        <Mention /> incident-repairer: Can you fix the connection and add a
        test?
      </SlackMessage>
      <SlackMessage bot time="09:19">
        <strong className="block">Draft fix ready for review</strong>
        The runner sent packets that were too big. The fix sends smaller ones.
        The test passes.
      </SlackMessage>
    </SlackThread>,
    <SlackThread key="result" label="Verified result in the Slack thread">
      <SlackMessage time="11:01">
        I reviewed the fix and shipped it at 9:55. <Mention /> mira: Are replies
        faster now?
      </SlackMessage>
      <SlackMessage bot time="11:02">
        <strong className="block">
          95% of replies started within 2.8 seconds, down from 15.1 seconds.
        </strong>
        Packet loss fell from 8% to 0.2%.
        <small className="mt-md block font-mono text-xs/relaxed text-zinc-300">
          Grafana · Similar traffic
          <br />
          08:00–09:00 and 10:00–11:00 IST
        </small>
      </SlackMessage>
    </SlackThread>,
  ];

  return (
    <figure
      className="min-w-0"
      id="product-demo"
      aria-labelledby="film-heading"
      data-animated={!demo.staticMode}
    >
      <figcaption className="mb-md flex flex-wrap justify-between gap-md">
        <h2 id="film-heading" className="font-bold text-text">
          Why did chat freeze?
        </h2>
        <span className="font-mono text-xs text-text-muted">
          Example thread · IST
        </span>
      </figcaption>
      <div className="flex flex-col gap-md">
        {demo.staticMode ? (
          <p className="flex min-h-11 items-center text-label text-text-muted md:order-last motion-reduce:hidden">
            Scroll across to read the full story.
          </p>
        ) : (
          <div className="flex min-h-11 flex-wrap items-center justify-between gap-x-lg gap-y-sm md:order-last">
            <div className="min-w-32 flex-1">
              <div className="mb-sm flex gap-1" aria-hidden="true">
                {DEMO_STAGES.map(({ name }, index) => (
                  <span
                    key={name}
                    className={cn(
                      "h-0.5 flex-1 rounded-full",
                      index <= demo.stage ? "bg-pulse" : "bg-border-strong",
                    )}
                  />
                ))}
              </div>
              <p className="text-label text-text-muted">
                {DEMO_STAGES[demo.stage].name}
              </p>
            </div>
            <Button
              size="sm"
              variant="outline"
              className="min-h-11"
              onClick={demo.toggle}
            >
              {demo.finished
                ? "Replay demo"
                : demo.paused
                  ? "Resume demo"
                  : "Pause demo"}
            </Button>
          </div>
        )}
        <div
          ref={stage}
          className={cn(
            "grid md:motion-safe:min-h-116",
            demo.staticMode &&
              "gap-lg motion-safe:snap-x motion-safe:snap-mandatory motion-safe:auto-cols-[100%] motion-safe:grid-flow-col motion-safe:overflow-x-auto",
          )}
          role={demo.staticMode ? "region" : undefined}
          aria-label={demo.staticMode ? "Support investigation example" : undefined}
          tabIndex={demo.staticMode ? 0 : undefined}
          data-playing={demo.playing}
          onFocus={demo.pause}
        >
          {scenes.map((scene, index) => (
            <Scene
              key={DEMO_STAGES[index].name}
              active={index === demo.stage}
              staticMode={demo.staticMode}
              animate={!demo.staticMode && demo.stage > 0}
            >
              {scene}
            </Scene>
          ))}
        </div>
      </div>
    </figure>
  );
}
