import { commands, type ChatId, type RestartImpact, type TabId, type UpdateError, type UpdateTrack } from "../../ipc/bindings";
import { updateErrorText } from "../../ipc/errors";
import { useLive } from "../../state/live";
import { useWorkspace } from "../../state/workspace";
import { flushWorkspaceSave } from "../editor/persistence";
import { restartBody, restartTitle, type UpdateAction } from "./updateText";

const ws = () => useWorkspace.getState();

function notifyUnlessStale(e: UpdateError): void {
  const text = updateErrorText(e);
  if (text !== null) ws().notify(text);
}

export function runUpdateAction(action: UpdateAction): void {
  switch (action.kind) {
    case "check":
      return void commands.updateCheck();
    case "download":
      return void downloadUpdate(action.version);
    case "install":
      return void installUpdate(action.version, null);
    default: {
      const unreachable: never = action;
      return unreachable;
    }
  }
}

async function downloadUpdate(version: string): Promise<void> {
  const res = await commands.updateDownload(version);
  if (res.status === "error") notifyUnlessStale(res.error);
}

const tabTitle = (id: TabId) => ws().tabs[id]?.title ?? "a closed tab";
const chatTitle = (id: ChatId) => useLive.getState().chats[id]?.title || "an untitled chat";

export async function installUpdate(version: string, accepted: RestartImpact | null): Promise<void> {
  if (accepted) await flushWorkspaceSave();
  const res = await commands.updateInstall(version, accepted);
  if (res.status === "error") return notifyUnlessStale(res.error);
  if (res.data.kind === "Restarting") return;
  const { impact } = res.data;
  ws().open({
    kind: "confirm",
    title: restartTitle(version, useLive.getState().updates),
    body: restartBody(impact, tabTitle, chatTitle),
    confirmLabel: "Restart and install",
    onConfirm: () => void installUpdate(version, impact),
  });
}

/** The select shows the engine's track, so a refused switch snaps back on its own. */
export async function chooseTrack(track: UpdateTrack): Promise<void> {
  const res = await commands.updateSetTrack(track);
  if (res.status === "error") notifyUnlessStale(res.error);
}

/** Failures never toast: a scheduled check on a laptop that is offline would toast every hour. */
export function startUpdateSync(): () => void {
  return useLive.subscribe((s, prev) => {
    const was = prev.updates?.status;
    const now = s.updates?.status;
    if (was?.kind === "Downloading" && now?.kind === "Downloaded") {
      ws().notify(`L1 Data ${now.version} is ready. Click the update button in the sidebar to restart into it.`);
    }
  });
}
