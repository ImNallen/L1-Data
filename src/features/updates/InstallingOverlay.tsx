import { useEffect, useRef } from "react";
import { useLive } from "../../state/live";
import { Spinner } from "../../ui/primitives";

function Blocker({ version }: { version: string }) {
  const self = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const hold = () => self.current?.focus();
    const swallow = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopImmediatePropagation();
    };
    hold();
    window.addEventListener("focusin", hold, { capture: true });
    window.addEventListener("keydown", swallow, { capture: true });
    return () => {
      window.removeEventListener("focusin", hold, { capture: true });
      window.removeEventListener("keydown", swallow, { capture: true });
    };
  }, []);
  return (
    <div
      ref={self}
      tabIndex={-1}
      role="alertdialog"
      aria-modal="true"
      aria-label={`Installing L1 Data ${version}`}
      data-testid="update-installing"
      className="fixed inset-0 z-[60] grid place-items-center bg-backdrop/50 backdrop-blur-[6px] outline-none"
    >
      <div className="flex items-center gap-2.5 rounded-xl border border-control bg-raised px-5 py-4 text-ui-13.5 text-fg shadow-[var(--shadow-dialog)]">
        <Spinner />
        Installing L1 Data {version}… L1 Data will restart.
      </div>
    </div>
  );
}

export function InstallingOverlay() {
  const status = useLive((s) => s.updates?.status);
  return status?.kind === "Installing" ? <Blocker version={status.version} /> : null;
}
