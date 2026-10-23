import { closeTab } from "../features/editor/closeTab";
import { openSqlTab } from "../features/editor/runActions";
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

export const START_COMMANDS: Command[] = [
  { id: "new-query", label: "New query tab", key: "t", run: inWorkspace(() => openSqlTab()) },
  { id: "new-connection", label: "New connection", key: "n", run: open({ kind: "connection", editing: null }) },
  { id: "search", label: "Search", key: "p", run: () => ws().focusSearch() },
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

const KEYMAP = [...START_COMMANDS, CLOSE_TAB];

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
