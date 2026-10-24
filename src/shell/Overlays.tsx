import { useEffect, useRef, useState } from "react";
import { useWorkspace, type Overlay } from "../state/workspace";
import { Button, Modal } from "../ui/primitives";

function Toast() {
  const toast = useWorkspace((s) => s.toast);
  const [visible, setVisible] = useState<number | null>(null);
  useEffect(() => {
    if (!toast) return;
    setVisible(toast.id);
    const t = setTimeout(() => setVisible(null), 4_000);
    return () => clearTimeout(t);
  }, [toast]);
  if (!toast || visible !== toast.id) return null;
  return (
    <div
      role="status"
      data-testid="toast"
      className="fixed bottom-10 left-1/2 z-50 max-w-[520px] -translate-x-1/2 rounded-lg border border-control bg-raised px-3.5 py-2 text-ui-13 text-fg shadow-[var(--shadow-popover)]"
    >
      {toast.text}
    </div>
  );
}

/** Keyed by `seq`, so a question that follows another remounts and takes focus again. Enter picks the primary choice. */
function AskDialog({ question }: { question: Extract<Overlay, { kind: "ask" }> }) {
  const answer = useWorkspace((s) => s.answer);
  const close = useWorkspace((s) => s.close);
  const primary = useRef<HTMLButtonElement>(null);
  const hasPrimary = question.choices.some((c) => c.variant === "primary");
  return (
    <Modal
      open
      onOpenChange={(open) => !open && close()}
      title={question.title}
      description={question.body}
      width={440}
      testId={`${question.testId}-dialog`}
      initialFocus={hasPrimary ? primary : undefined}
    >
      <div className="flex justify-end gap-2">
        <Button variant="ghost" data-testid={`${question.testId}-cancel`} onClick={close}>
          Cancel
        </Button>
        {question.choices.map((c) => (
          <Button
            key={c.id}
            ref={c.variant === "primary" ? primary : undefined}
            data-testid={`${question.testId}-${c.id}`}
            variant={c.variant}
            onClick={() => answer(c.id)}
          >
            {c.label}
          </Button>
        ))}
      </div>
    </Modal>
  );
}

export function Overlays() {
  const overlay = useWorkspace((s) => s.overlay);
  const close = useWorkspace((s) => s.close);
  const onOpenChange = (open: boolean) => !open && close();

  return (
    <>
      {overlay.kind === "ask" && <AskDialog key={overlay.seq} question={overlay} />}
      <Modal open={overlay.kind === "cell"} onOpenChange={onOpenChange} title={overlay.kind === "cell" ? overlay.title : ""} width={720} testId="cell-dialog">
        {overlay.kind === "cell" && (
          <pre className="selectable max-h-[52vh] overflow-auto rounded-lg border border-control bg-bg p-3 font-mono text-ui-12.5 leading-5 whitespace-pre-wrap text-fg">
            {overlay.value ?? <span className="text-faint">NULL</span>}
          </pre>
        )}
      </Modal>
      <Toast />
    </>
  );
}
