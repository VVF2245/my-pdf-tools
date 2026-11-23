import { useEffect, useState } from "react";
import { defaultOutputDir, folderExists, pickFolder, toConversionError } from "../../shared/ipc";
import { t } from "../../shared/i18n";
import { effectiveOutputDir, loadChosenDir, saveChosenDir } from "../../shared/output";
import { errorMessage } from "../jobs/JobList";

/** The system should answer at once; if not, the window goes on as if it did not know. */
const ASK_TIMEOUT_MS = 3000;

function withTimeout<T>(promise: Promise<T>, fallback: T): Promise<T> {
  return new Promise((resolve) => {
    const timer = setTimeout(() => resolve(fallback), ASK_TIMEOUT_MS);
    promise.then(resolve, () => resolve(fallback)).finally(() => clearTimeout(timer));
  });
}

function store(): Storage | undefined {
  try {
    return window.localStorage;
  } catch {
    return undefined;
  }
}

/**
 * The folder results are written to. Downloads unless the user picked another one, which is
 * remembered. Returns the folder to send (`null` = next to each original) and the control.
 */
export function useOutputFolder() {
  const [chosen, setChosen] = useState<string | null>(() => loadChosenDir(store()));
  // `undefined` while asking the system; `null` when it does not know the folder
  const [downloads, setDownloads] = useState<string | null | undefined>(undefined);
  const [notice, setNotice] = useState<string | null>(null);
  const [picking, setPicking] = useState(false);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      const found = await withTimeout(defaultOutputDir(), null);
      // a remembered folder may be gone (an unplugged drive): say so and go back to Downloads
      const remembered = loadChosenDir(store());
      const gone = remembered !== null && !(await withTimeout(folderExists(remembered), true));
      if (cancelled) return;
      if (gone) {
        setChosen(null);
        saveChosenDir(store(), null);
        setNotice(t("output.gone", { path: remembered }));
      }
      setDownloads(found);
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  async function change() {
    setPicking(true);
    try {
      const dir = await pickFolder(t("output.pickTitle"));
      if (dir !== null) {
        setChosen(dir);
        saveChosenDir(store(), dir);
      }
      setNotice(null);
    } catch (reason) {
      setNotice(`${t("output.pickFailed")}: ${errorMessage(toConversionError(reason))}`);
    } finally {
      setPicking(false);
    }
  }

  function resetToDownloads() {
    setChosen(null);
    saveChosenDir(store(), null);
  }

  const dir = effectiveOutputDir(chosen, downloads ?? null);
  // until the system has answered, the folder is not known yet
  const ready = downloads !== undefined;

  const control = (
    <div className="output-folder">
      <span>{t("output.saveIn")}:</span>{" "}
      {!ready ? (
        <span className="muted">…</span>
      ) : dir === null ? (
        <span className="notice">{t("output.nextToOriginal")}</span>
      ) : (
        <code title={dir}>{dir}</code>
      )}{" "}
      <button type="button" className="link" onClick={change} disabled={picking}>
        {t("output.change")}
      </button>
      {chosen !== null && (
        <button type="button" className="link" onClick={resetToDownloads}>
          {t("output.useDownloads")}
        </button>
      )}
      {notice && <p className="notice">{notice}</p>}
    </div>
  );

  return { dir, control, ready };
}
