import { closeTab } from "../features/editor/closeTab";
import { openFiles, saveTab, type SaveMode } from "../features/editor/fileActions";
import { openSqlTab, submitEdits } from "../features/editor/runActions";
import { saveActiveQuery } from "../features/saved/savedActions";
import { pendingCount, useTableEdits } from "../state/tableEdits";
import { useWorkspace, type Overlay } from "../state/workspace";
import { isMac } from "./hostedWindow";

export type Command = { id: string; label: string; key: string; shift?: boolean; run: () => void };

const MAC = typeof navigator !== "undefined" && isMac(navigator.userAgent);

const ws = () => useWorkspace.getState();

const inWorkspace = (run: () => void) => () => {
  ws().closeSettings();
  run();
};

const open = (overlay: Overlay) => inWorkspace(() => ws().open(overlay));

function toggleSaved() {
  const wasOpen = ws().overlay.kind === "saved";
  ws().closeSettings();
  if (!wasOpen) ws().open({ kind: "saved", entry: { kind: "browse" } });
}

export const START_COMMANDS: Command[] = [
  { id: "new-query", label: "New query tab", key: "t", run: inWorkspace(() => openSqlTab()) },
  { id: "open-file", label: "Open file…", key: "o", run: () => void openFiles() },
  { id: "new-connection", label: "New connection", key: "n", run: open({ kind: "connection", editing: null }) },
  { id: "search", label: "Search", key: "p", run: () => ws().focusSearch() },
  { id: "saved-queries", label: "Saved queries", key: "e", run: toggleSaved },
  { id: "settings", label: "Settings", key: ",", run: () => (ws().screen.kind === "settings" ? ws().closeSettings() : ws().openSettings()) },
];

const CLOSE_TAB: Command = {
  id: "close-tab",
  label: "Close tab",
  key: "w",
  run: () => {
    const { screen, active, closeSettings } = ws();
    if (screen.kind === "settings") closeSettings();
    else if (active) void closeTab(active);
  },
};

/**
 * A save acts on the workspace screen with no dialog up, so a save dialog never stacks on another one. With staged
 * table edits in the active tab, ⌘S submits them wherever focus is.
 */
const saveActive = (mode: SaveMode) => () => {
  const { screen, overlay, active } = ws();
  if (screen.kind !== "workspace" || overlay.kind !== "none" || !active) return;
  const edits = useTableEdits.getState().byTab[active];
  if (mode === "save" && edits && pendingCount(edits) > 0) void submitEdits(active);
  else void saveTab(active, mode);
};

const SAVE: Command = { id: "save", label: "Save", key: "s", run: saveActive("save") };
const SAVE_AS: Command = { id: "save-as", label: "Save as…", key: "s", shift: true, run: saveActive("saveAs") };

const SAVE_QUERY: Command = { id: "save-query", label: "Save query", key: "e", shift: true, run: () => void saveActiveQuery() };

const KEYMAP = [...START_COMMANDS, CLOSE_TAB, SAVE, SAVE_AS, SAVE_QUERY];

/** On macOS a command takes ⌘ alone, because the editor owns the Ctrl chords there (Ctrl-T transposes, Ctrl-P moves up). */
export function commandForKey(e: Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "shiftKey" | "altKey">, mac = MAC): Command | undefined {
  const mod = mac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
  if (!mod || e.altKey) return undefined;
  const key = e.key.toLowerCase();
  return KEYMAP.find((c) => c.key === key && !!c.shift === e.shiftKey);
}

/**
 * The window's keydown listener. A control that handles a chord itself calls `preventDefault`, and the global
 * command then stands down, so the chord never fires twice.
 */
export function handleGlobalKey(e: KeyboardEvent): void {
  if (e.defaultPrevented) return;
  const command = commandForKey(e);
  if (!command) return;
  e.preventDefault();
  command.run();
}

export function shortcutLabel(c: Command): string {
  return `⌘${c.shift ? "⇧" : ""}${c.key.toUpperCase()}`;
}
