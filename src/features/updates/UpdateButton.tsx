import { IconAlertTriangle, IconDownload, IconRefresh, IconRotateClockwise } from "@tabler/icons-react";
import { useLive } from "../../state/live";
import { cx, IconButton, Spinner } from "../../ui/primitives";
import { useNow } from "../../ui/useNow";
import { runUpdateAction } from "./updateActions";
import { updateButtonView, type UpdateButtonView } from "./updateText";

const RING_RADIUS = 6;
const RING_LENGTH = 2 * Math.PI * RING_RADIUS;

function ProgressRing({ percent }: { percent: number | null }) {
  return (
    <svg viewBox="0 0 16 16" className={cx("size-4 -rotate-90", percent === null && "animate-spin")} aria-hidden>
      <circle cx="8" cy="8" r={RING_RADIUS} fill="none" strokeWidth="2" className="stroke-control" />
      <circle
        cx="8"
        cy="8"
        r={RING_RADIUS}
        fill="none"
        strokeWidth="2"
        strokeLinecap="round"
        className="stroke-run transition-[stroke-dashoffset] duration-300"
        strokeDasharray={RING_LENGTH}
        strokeDashoffset={RING_LENGTH * (1 - (percent ?? 25) / 100)}
      />
    </svg>
  );
}

function Glyph({ view }: { view: UpdateButtonView }) {
  switch (view.glyph) {
    case "check":
      return <IconRefresh size={16} />;
    case "spinner":
      return <Spinner />;
    case "available":
      return <IconDownload size={16} />;
    case "ring":
      return <ProgressRing percent={view.percent} />;
    case "restart":
      return <IconRotateClockwise size={16} />;
    case "error":
      return <IconAlertTriangle size={16} className="text-amber" />;
    default: {
      const unreachable: never = view.glyph;
      return unreachable;
    }
  }
}

/** Not `disabled` while a step runs, because a disabled button shows no tooltip with the progress. */
export function UpdateButton() {
  const updates = useLive((s) => s.updates);
  const now = useNow(updates?.status.kind === "UpToDate", 60_000);
  const view = updates && updateButtonView(updates, now);
  if (!updates || !view) return null;
  const { action } = view;
  return (
    <IconButton
      data-testid="update-button"
      data-status={updates.status.kind}
      aria-label={view.label}
      aria-disabled={action === null}
      title={view.tooltip}
      className="relative ml-auto rounded-full aria-disabled:cursor-default aria-disabled:hover:bg-transparent"
      onClick={() => action && runUpdateAction(action)}
    >
      <Glyph view={view} />
      {view.attention && <span className="absolute top-0.5 right-0.5 size-1.5 rounded-full bg-run" />}
    </IconButton>
  );
}
