import * as Dialog from "@radix-ui/react-dialog";
import { useEffect, useLayoutEffect, useRef } from "react";
import { useLive } from "../../state/live";
import { Spinner } from "../../ui/primitives";

function Blocker({ version }: { version: string }) {
  const opener = useRef<Element | null>(null);
  useLayoutEffect(() => {
    opener.current = document.activeElement;
  }, []);
  useEffect(() => {
    const swallow = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopImmediatePropagation();
    };
    window.addEventListener("keydown", swallow, { capture: true });
    return () => {
      window.removeEventListener("keydown", swallow, { capture: true });
    };
  }, []);
  return (
    <Dialog.Root open>
      <Dialog.Portal>
        <Dialog.Content
          role="alertdialog"
          aria-modal="true"
          aria-describedby={undefined}
          data-testid="update-installing"
          onCloseAutoFocus={(event) => {
            event.preventDefault();
            if (opener.current instanceof HTMLElement && opener.current.isConnected) opener.current.focus();
          }}
          className="fixed inset-0 z-[60] grid place-items-center bg-backdrop/50 backdrop-blur-[6px] outline-none"
        >
          <div className="flex items-center gap-2.5 rounded-xl border border-control bg-raised px-5 py-4 text-ui-13.5 text-fg shadow-[var(--shadow-dialog)]">
            <Spinner />
            <Dialog.Title>Installing L1 Data {version}… L1 Data will restart.</Dialog.Title>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

export function InstallingOverlay() {
  const status = useLive((s) => s.updates?.status);
  return status?.kind === "Installing" ? <Blocker version={status.version} /> : null;
}
