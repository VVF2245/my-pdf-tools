import { es } from "./es";

// Spanish first; add another dictionary and pick it here to support more languages.
const dictionary = es;

/** Translates `key`, replacing `{name}` placeholders. Unknown keys are returned as they are. */
export function t(key: string, params: Record<string, string | number> = {}): string {
  const template = dictionary[key] ?? key;
  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in params ? String(params[name]) : match,
  );
}
