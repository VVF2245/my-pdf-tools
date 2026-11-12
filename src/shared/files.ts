// File-name helpers shared by every way of adding files to the convert panel (drop, file
// dialog). Pure functions, covered by files.test.ts.

/** The last part of a path, for either kind of separator. */
export function baseName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/** Lowercase extension without the dot, or "" when the file name has none. */
export function extensionOf(path: string): string {
  const name = baseName(path);
  const dot = name.lastIndexOf(".");
  return dot === -1 ? "" : name.slice(dot + 1).toLowerCase();
}

/** Splits `paths` into those whose extension is in `accepted` (lowercase) and the rest. */
export function splitAccepted(
  paths: string[],
  accepted: string[],
): { ok: string[]; rejected: string[] } {
  const ok = paths.filter((p) => accepted.includes(extensionOf(p)));
  const rejected = paths.filter((p) => !ok.includes(p));
  return { ok, rejected };
}

/**
 * Appends the files in `added` that are not in `current` yet, keeping the order. Paths are
 * compared after Unicode normalization (macOS may give the same accented name decomposed
 * (NFD) in a drop and composed (NFC) in the file dialog) and ignoring case (Windows and macOS
 * volumes are case-insensitive by default, so `C:\a.md` and `c:\A.md` are one file; a
 * case-sensitive volume holding two names that differ only in case is not supported).
 * The path itself is kept as received.
 */
export function mergeFiles(current: string[], added: string[]): string[] {
  const keyOf = (path: string) => path.normalize("NFC").toLowerCase();
  const known = new Set(current.map(keyOf));
  const merged = [...current];
  for (const path of added) {
    const key = keyOf(path);
    if (!known.has(key)) {
      known.add(key);
      merged.push(path);
    }
  }
  return merged;
}
