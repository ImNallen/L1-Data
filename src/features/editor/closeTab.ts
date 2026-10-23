import { commands, type TabId } from "../../ipc/bindings";
import { disconnectErrorText } from "../../ipc/errors";
import { useLive } from "../../state/live";
import { pendingCount, useTableEdits } from "../../state/tableEdits";
import { confirm, useWorkspace } from "../../state/workspace";
import { forgetTab, plural } from "./runActions";

const ws = () => useWorkspace.getState();
const exists = (id: TabId) => ws().tabs[id] !== undefined;

/**
 * Asks about each kind of work closing the tab would lose, and closes it only if every answer lets it. A question
 * whose answer loses nothing comes before one that destroys something: staged edits are dropped only at `forgetTab`,
 * and the rollback happens only in the last `consoleClose`, so a Cancel at any step loses nothing.
 */
export async function closeTab(tabId: TabId): Promise<void> {
  if (!(await dropEdits(tabId)) || !exists(tabId)) return;
  if (!(await closeConsole(tabId)) || !exists(tabId)) return;
  forgetTab(tabId);
}

async function dropEdits(id: TabId): Promise<boolean> {
  const edits = useTableEdits.getState().byTab[id];
  if (!edits || pendingCount(edits) === 0) return true;
  return confirm({
    title: "Close tab with unsaved changes?",
    body: `Closing discards ${plural(pendingCount(edits), "change")} that haven't been submitted.`,
    confirmLabel: "Discard and close",
  });
}

async function closeConsole(id: TabId): Promise<boolean> {
  if (!useLive.getState().consoles[id]) return true;
  const res = await commands.consoleClose(id, false);
  if (res.status === "ok") return true;
  if (res.error.kind !== "OpenTransactions") {
    ws().notify(disconnectErrorText(res.error));
    return false;
  }
  const rollBack = await confirm({ title: "Close tab with an open transaction?", body: "Closing rolls back the uncommitted work in this tab.", confirmLabel: "Roll back and close" });
  if (!rollBack || !exists(id)) return false;
  await commands.consoleClose(id, true);
  return true;
}
