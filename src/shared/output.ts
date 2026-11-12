// Where converted files go. Pure functions (the storage is passed in) so `npm test` covers them.

const KEY = "mpt.outputDir";

/** The part of `Storage` used here. */
export interface KeyValueStore {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

/** The folder results are written to: the user's choice, else Downloads, else `null`
 * (next to each original, which the window says out loud). */
export function effectiveOutputDir(chosen: string | null, defaultDir: string | null): string | null {
  return chosen ?? defaultDir;
}

/** The remembered folder, or `null`. Storage can be missing or throw (private windows). */
export function loadChosenDir(store: KeyValueStore | undefined): string | null {
  try {
    const value = store?.getItem(KEY);
    return value ? value : null;
  } catch {
    return null;
  }
}

/** Remembers `dir`, or forgets the choice when it is `null`. Never throws. */
export function saveChosenDir(store: KeyValueStore | undefined, dir: string | null): void {
  try {
    if (dir === null) store?.removeItem(KEY);
    else store?.setItem(KEY, dir);
  } catch {
    // not remembered: the window still works
  }
}
