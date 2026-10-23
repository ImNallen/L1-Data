import type { Text } from "@codemirror/state";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { commands, type FileBinding, type FileOpened, type FileSaved, type SaveTarget, type TabId } from "../../ipc/bindings";
import { fileReadErrorText, fileWriteErrorText } from "../../ipc/errors";
import { ask, tabWithFile, useWorkspace, type Question } from "../../state/workspace";
import { docOf, isDirty, markSynced, markSyncedText, replaceDoc } from "./editorStates";
import { flushWorkspaceSave } from "./persistence";
import { openSqlTab } from "./runActions";

const ws = () => useWorkspace.getState();
const notify = (text: string) => ws().notify(text);

const SQL_FILTER = { name: "SQL", extensions: ["sql"] };

/** Does nothing while a dialog is up, since leaving settings would close it and so cancel a question. */
export async function openFiles(): Promise<void> {
  if (ws().overlay.kind !== "none") return;
  ws().closeSettings();
  for (const path of (await pickFiles()) ?? []) await openFile(path);
}

async function openFile(path: string): Promise<void> {
  const open = tabWithFile(ws().tabs, path);
  if (open) return ws().setActive(open.id);
  const res = await commands.fileOpen(path);
  if (res.status === "error") return notify(fileReadErrorText(res.error, path));
  // The engine names a file by its canonical path, which a tab opened through another spelling of it already has.
  const same = tabWithFile(ws().tabs, res.data.binding.path);
  if (same) return ws().setActive(same.id);
  const id = openSqlTab({ text: res.data.text, title: res.data.name });
  adopt(id, res.data, docOf(id));
}

/** The only writer of a binding and its baseline together. `doc` is what the file now holds. */
function adopt(id: TabId, file: FileSaved, doc: Text | undefined): void {
  if (!ws().tabs[id] || !doc) return;
  markSynced(id, doc);
  ws().bindFile(id, file);
}

const sameBinding = (id: TabId, binding: FileBinding | undefined) => {
  const tab = ws().tabs[id];
  return tab !== undefined && tab.file?.binding === binding;
};

export type SaveMode = "save" | "saveAs";

const saving = new Map<TabId, Promise<boolean>>();

/**
 * Resolves `true` once the text is on disk and the tab is bound to it. Saves of one tab run in order, so a second ⌘S
 * compares the disk against the stamp the first one wrote and never mistakes our own write for an outside change.
 */
export function saveTab(id: TabId, mode: SaveMode = "save"): Promise<boolean> {
  const once = () => saveOnce(id, mode).catch((e: unknown) => (notify(String(e)), false));
  const run = (saving.get(id) ?? Promise.resolve(true)).then(once);
  saving.set(id, run);
  void run.then(() => {
    if (saving.get(id) === run) saving.delete(id);
  });
  return run;
}

export function savesInFlight(): Promise<void> {
  return Promise.all(saving.values()).then(() => undefined);
}

async function saveOnce(id: TabId, mode: SaveMode): Promise<boolean> {
  const tab = ws().tabs[id];
  if (!tab) return false;
  const binding = tab.file?.binding;
  if (mode === "save" && binding) return write(id, binding, { kind: "Bound", binding, overwrite: null });
  const path = await pickSavePath(binding?.path ?? defaultFileName(tab.title));
  if (!path || !sameBinding(id, binding)) return false;
  const taken = Object.values(ws().tabs).flatMap((t) => (t.id !== id && t.file ? [t.file.binding.path] : []));
  return write(id, binding, { kind: "New", path, format: binding?.format ?? null, taken });
}

/**
 * Sends the doc as it is when the write starts, and the same doc again after an overwrite question, so what lands
 * on disk is always a text the tab held. Stops before any write once the tab is gone or bound elsewhere.
 */
async function write(id: TabId, binding: FileBinding | undefined, first: SaveTarget): Promise<boolean> {
  const doc = docOf(id);
  if (!doc) return false;
  // The stored buffer is then never older than the file, so a crash after the write restores a tab that matches it.
  await flushWorkspaceSave();
  let target = first;
  for (;;) {
    if (!sameBinding(id, binding)) return false;
    const res = await commands.fileSave(target, doc.toString());
    if (res.status === "ok") {
      adopt(id, res.data, doc);
      return true;
    }
    if (res.error.kind !== "Changed" || target.kind !== "Bound") {
      notify(fileWriteErrorText(res.error, target));
      return false;
    }
    if (!sameBinding(id, binding)) return false;
    // The disk holds text this tab never read, so unless the overwrite goes through the tab is not clean.
    markSynced(id, null);
    ws().setFileHealth(id, "changed");
    if ((await ask(overwriteQuestion(ws().tabs[id]?.title ?? ""))) !== "overwrite") return false;
    target = { ...target, overwrite: res.error.disk };
  }
}

function overwriteQuestion(name: string): Question<"overwrite"> {
  return {
    title: `${name} changed on disk`,
    body: "Another program changed the file after this tab last opened or saved it. Overwrite it with this tab's text?",
    choices: [{ id: "overwrite", label: "Overwrite", variant: "danger" }],
  };
}

export function defaultFileName(title: string): string {
  const name = title.replace(/[/\\:]/g, "-");
  return /\.\w{1,10}$/.test(name) ? name : `${name}.sql`;
}

export type FileRead = Awaited<ReturnType<typeof commands.fileOpen>>;

export type FileVerdict =
  | { kind: "adopt"; file: FileOpened }
  | { kind: "same"; text: string }
  | { kind: "reload"; file: FileOpened }
  | { kind: "diverged" }
  | { kind: "missing" }
  | { kind: "unreadable" };

export function judgeFile(binding: FileBinding, buffer: string, dirty: boolean, read: FileRead): FileVerdict {
  if (read.status === "error") return read.error.kind === "NotFound" ? { kind: "missing" } : { kind: "unreadable" };
  const file = read.data;
  if (file.text === buffer) return { kind: "adopt", file };
  if (file.binding.stamp === binding.stamp) return { kind: "same", text: file.text };
  return dirty ? { kind: "diverged" } : { kind: "reload", file };
}

export async function reconcileFiles(): Promise<void> {
  const reads = Object.values(ws().tabs).flatMap((t) => (t.file ? [reconcileTab(t.id, t.file.binding)] : []));
  const reloaded = (await Promise.all(reads)).filter((name) => name !== null);
  if (reloaded.length > 0) notify(reloaded.length === 1 ? `Reloaded ${reloaded[0]} from disk.` : `Reloaded ${reloaded.length} files from disk.`);
}

async function reconcileTab(id: TabId, binding: FileBinding): Promise<string | null> {
  const doc = docOf(id);
  if (!doc) return null;
  const read = await commands.fileOpen(binding.path);
  if (docOf(id) !== doc || !sameBinding(id, binding)) return null;
  const verdict = judgeFile(binding, doc.toString(), isDirty(id), read);
  switch (verdict.kind) {
    case "adopt":
      adopt(id, verdict.file, doc);
      return null;
    case "same":
      markSyncedText(id, verdict.text);
      ws().setFileHealth(id, "ok");
      return null;
    case "reload":
      replaceDoc(id, verdict.file.text);
      adopt(id, verdict.file, docOf(id));
      return verdict.file.name;
    case "diverged":
      ws().setFileHealth(id, "changed");
      return null;
    case "missing":
    case "unreadable":
      markSynced(id, null);
      ws().setFileHealth(id, verdict.kind);
      return null;
    default: {
      const unreachable: never = verdict;
      return unreachable;
    }
  }
}

async function pickFiles(): Promise<string[] | null> {
  try {
    return await openDialog({ multiple: true, directory: false, filters: [SQL_FILTER, { name: "All files", extensions: ["*"] }] });
  } catch {
    notify("Opening files needs the desktop app's file dialog.");
    return null;
  }
}

/** The OS dialog asks before replacing a file, so the path it returns is consent to replace it. */
async function pickSavePath(defaultPath: string): Promise<string | null> {
  try {
    return await saveDialog({ defaultPath, filters: [SQL_FILTER] });
  } catch {
    notify("Saving needs the desktop app's save dialog.");
    return null;
  }
}
