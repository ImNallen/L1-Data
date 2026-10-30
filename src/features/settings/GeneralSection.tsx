import type { AssistantAccess } from "../../ipc/bindings";
import { useWorkspace } from "../../state/workspace";
import { saveSettings } from "../editor/runActions";
import { Select } from "./AppearanceControls";
import { Row } from "./SettingLayout";

const ACCESS_OPTIONS = [
  { value: "SchemaOnly", label: "Schema only" },
  { value: "Full", label: "Full access" },
] satisfies { value: AssistantAccess; label: string }[];

export function AssistantAccessSetting() {
  const access = useWorkspace((s) => s.settings.assistantAccess);
  return (
    <Row
      setting="assistantAccess"
      hint="Applies across all chats. Switching stops active turns and starts fresh AI context. Local history stays."
      control={
        <Select
          testId="setting-assistantAccess"
          label="Assistant access"
          value={access}
          options={ACCESS_OPTIONS}
          onChange={(value) => {
            if (value === "SchemaOnly" || value === "Full") void saveSettings({ assistantAccess: value });
          }}
        />
      }
    >
      <div className="mt-3 space-y-2 text-ui-12 leading-relaxed text-dim">
        <p>
          Schema only sends database and schema structure, including table names, columns, types, and keys. It excludes editor SQL, errors, and result rows. The
          assistant cannot query data.
        </p>
        <p>
          Full access also sends editor SQL and errors, allows result attachments, and enables approved read-only queries on PostgreSQL. SQL Server queries
          remain unavailable.
        </p>
        <p>Everything you type or paste into chat is sent to your AI provider in either mode.</p>
      </div>
    </Row>
  );
}
