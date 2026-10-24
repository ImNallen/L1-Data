export type DialogAnswer = string | string[] | null;
export type DialogCall = { cmd: "open" | "save"; options: Record<string, unknown> };
export type DialogShim = { answer(...answers: DialogAnswer[]): void; readonly calls: DialogCall[] };

declare global {
  interface Window {
    __l1Dialog?: DialogShim;
  }
}

const COMMANDS: Record<string, DialogCall["cmd"] | undefined> = { "plugin:dialog|open": "open", "plugin:dialog|save": "save" };

export function createDialogShim() {
  const queue: DialogAnswer[] = [];
  const calls: DialogCall[] = [];
  return {
    shim: { answer: (...answers: DialogAnswer[]) => void queue.push(...answers), calls } satisfies DialogShim,
    handles: (cmd: string) => COMMANDS[cmd] !== undefined,
    reply(cmd: string, args: unknown): DialogAnswer {
      const kind = COMMANDS[cmd]!;
      const options = ((args as { options?: Record<string, unknown> } | undefined)?.options ?? {}) as Record<string, unknown>;
      calls.push({ cmd: kind, options });
      if (queue.length === 0) {
        console.warn(`__l1Dialog: no answer queued for ${kind}, so it answers Cancel`);
        return null;
      }
      const answer = queue.shift() ?? null;
      if (answer === null) return null;
      const paths = Array.isArray(answer) ? answer : [answer];
      return kind === "open" && options.multiple ? paths : (paths[0] ?? null);
    },
  };
}

export function installDialogShim(): ReturnType<typeof createDialogShim> {
  const dialogs = createDialogShim();
  window.__l1Dialog = dialogs.shim;
  return dialogs;
}
