import { commands } from "../../ipc/bindings";
import { useWorkspace, workspaceSnapshot } from "../../state/workspace";
import { textOf } from "./editorStates";
import { onEditorDocChanged } from "./runActions";

const SAVE_DEBOUNCE_MS = 500;

let timer: ReturnType<typeof setTimeout> | undefined;

const save = () => commands.workspaceSave(workspaceSnapshot(useWorkspace.getState(), textOf));

export function startPersistence(): void {
  const schedule = () => {
    clearTimeout(timer);
    timer = setTimeout(() => void save(), SAVE_DEBOUNCE_MS);
  };
  useWorkspace.subscribe((s, prev) => {
    if (s.tabs !== prev.tabs || s.order !== prev.order || s.active !== prev.active || s.layout !== prev.layout) schedule();
  });
  onEditorDocChanged(schedule);
}

/** An update's restart ends the process before the debounce would fire. */
export async function flushWorkspaceSave(): Promise<void> {
  clearTimeout(timer);
  await save();
}
