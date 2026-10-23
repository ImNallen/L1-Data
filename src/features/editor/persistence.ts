import { commands } from "../../ipc/bindings";
import { useWorkspace, workspaceSnapshot } from "../../state/workspace";
import { isDirty, onBuffersChanged, textOf } from "./editorStates";

const SAVE_DEBOUNCE_MS = 500;

let timer: ReturnType<typeof setTimeout> | undefined;

const save = () => commands.workspaceSave(workspaceSnapshot(useWorkspace.getState(), { textOf, isDirty }));

/** A background tab's reload from disk and a save's new baseline change no store field, so buffer changes schedule too. */
export function startPersistence(): void {
  const schedule = () => {
    clearTimeout(timer);
    timer = setTimeout(() => void save(), SAVE_DEBOUNCE_MS);
  };
  useWorkspace.subscribe((s, prev) => {
    if (s.tabs !== prev.tabs || s.order !== prev.order || s.active !== prev.active || s.layout !== prev.layout) schedule();
  });
  onBuffersChanged(schedule);
}

/** Writes the pending save now. The snapshot is taken before this returns its promise. */
export async function flushWorkspaceSave(): Promise<void> {
  clearTimeout(timer);
  await save();
}
