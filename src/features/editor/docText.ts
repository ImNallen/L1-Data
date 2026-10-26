import type { Text } from "@codemirror/state";

const texts = new WeakMap<Text, string>();

/** A document's text, joined once per version, so the run mark and completion share one copy of a large file. */
export function docText(doc: Text): string {
  let text = texts.get(doc);
  if (text === undefined) {
    text = doc.toString();
    texts.set(doc, text);
  }
  return text;
}
