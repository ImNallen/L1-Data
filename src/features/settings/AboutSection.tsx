import type { UpdateTrack } from "../../ipc/bindings";
import { useLive } from "../../state/live";
import { Button, cx } from "../../ui/primitives";
import { useNow } from "../../ui/useNow";
import { chooseTrack, runUpdateAction } from "../updates/updateActions";
import { aboutStatusText, actionLabel, updateAction } from "../updates/updateText";
import { Select } from "./AppearanceControls";
import { Row } from "./SettingLayout";

const TRACKS = [
  { value: "Stable", label: "Stable" },
  { value: "Nightly", label: "Nightly" },
] satisfies { value: UpdateTrack; label: string }[];

export function AppVersionSetting() {
  const updates = useLive((s) => s.updates);
  const now = useNow(updates?.status.kind === "UpToDate", 30_000);
  if (!updates) return null;
  const action = updateAction(updates.status);
  return (
    <Row
      setting="appVersion"
      hint={`L1 Data ${updates.currentVersion}`}
      control={
        <Button data-testid="update-action" disabled={action === null} onClick={() => action && runUpdateAction(action)}>
          {actionLabel(updates.status)}
        </Button>
      }
    >
      <p data-testid="update-status" className={cx("mt-1.5 text-ui-12", updates.status.kind === "Failed" ? "text-amber" : "text-dim")}>
        {aboutStatusText(updates, now)}
      </p>
    </Row>
  );
}

export function UpdateTrackSetting() {
  const updates = useLive((s) => s.updates);
  if (!updates) return null;
  const transferring = updates.status.kind === "Downloading" || updates.status.kind === "Installing";
  return (
    <Row
      setting="updateTrack"
      hint="Stable gets tested releases, and Nightly gets each day's build. Switch back anytime."
      control={
        <Select
          testId="update-track"
          label="Update track"
          value={updates.track}
          options={TRACKS}
          disabled={transferring}
          onChange={(track) => void chooseTrack(track as UpdateTrack)}
          className="w-32"
        />
      }
    />
  );
}
