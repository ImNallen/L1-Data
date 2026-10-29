import { create } from "zustand";
import type { Param, ParamValue, TabId } from "../../ipc/bindings";

let sequence = 0;

export type ParamPrompt = {
  sequence: number;
  tabId: TabId;
  params: Param[];
  values: ParamValue[];
  description: string;
  valid: () => boolean;
  resolve: (values: ParamValue[] | null) => void;
};

export const useParams = create<{ prompt: ParamPrompt | null }>(() => ({ prompt: null }));
const remembered = new Map<TabId, { target: string; values: ParamValue[] }>();

export function finishParams(values: ParamValue[] | null): void {
  const { prompt } = useParams.getState();
  if (!prompt) return;
  useParams.setState({ prompt: null });
  prompt.resolve(prompt.valid() ? values : null);
}

export function cancelStaleParams(): void {
  const { prompt } = useParams.getState();
  if (prompt && !prompt.valid()) finishParams(null);
}

export function forgetParams(tabId: TabId): void {
  remembered.delete(tabId);
  if (useParams.getState().prompt?.tabId === tabId) finishParams(null);
}

export async function promptParams(tabId: TabId, target: string, params: Param[], description: string, valid: () => boolean): Promise<ParamValue[] | null> {
  finishParams(null);
  const saved = remembered.get(tabId);
  if (saved?.target !== target) remembered.delete(tabId);
  const values: ParamValue[] = params.map(({ name }) => {
    const previous = saved?.target === target ? saved.values.find((p) => p.name === name) : undefined;
    return { name, value: previous ? previous.value : "" };
  });
  const accepted = await new Promise<ParamValue[] | null>((resolve) =>
    useParams.setState({ prompt: { sequence: ++sequence, tabId, params, values, description, valid, resolve } }),
  );
  if (accepted && valid()) remembered.set(tabId, { target, values: accepted });
  return accepted;
}
