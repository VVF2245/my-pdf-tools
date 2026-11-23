import { useEffect, useRef, useState } from "react";
import {
  onFileDrop,
  pickFiles,
  startJobs,
  toConversionError,
  type Job,
  type OptionValue,
  type ToolDescriptor,
} from "../../shared/ipc";
import { baseName, mergeFiles, splitAccepted } from "../../shared/files";
import { t } from "../../shared/i18n";
import { errorMessage } from "../jobs/JobList";
import { OptionsForm, defaultValues, type OptionValues } from "./OptionsForm";
import { useOutputFolder } from "./OutputFolder";

interface Props {
  tool: ToolDescriptor;
  onJobs: (jobs: Job[]) => void;
}

/** Drop files, tweak the options, convert. Works for any tool through its descriptor. */
export function ConvertPanel({ tool, onJobs }: Props) {
  const [files, setFiles] = useState<string[]>([]);
  const [values, setValues] = useState<OptionValues>(() => defaultValues(tool.options));
  const [hovering, setHovering] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [browsing, setBrowsing] = useState(false);
  const output = useOutputFolder();

  // The drop handler must always see the current tool without re-subscribing.
  const accepted = useRef(tool.inputs);
  accepted.current = tool.inputs;

  /** Adds files from a drop or from the file dialog: same filter, same list, same notice. */
  function addFiles(paths: string[]) {
    const { ok, rejected } = splitAccepted(paths, accepted.current);
    setFiles((current) => mergeFiles(current, ok));
    setNotice(
      rejected.length > 0
        ? t("convert.rejected", {
            extensions: accepted.current.map((e) => `.${e}`).join(", "),
            names: rejected.map(baseName).join(", "),
          })
        : null,
    );
  }
  const addFilesRef = useRef(addFiles);
  addFilesRef.current = addFiles;

  async function browse() {
    // one dialog at a time: a second click before the window shows would open another
    setBrowsing(true);
    try {
      const paths = await pickFiles(tool.inputs, t("convert.fileTypes"));
      if (paths.length > 0) addFiles(paths); // an empty list means the user cancelled
    } catch (reason) {
      setNotice(`${t("convert.browseFailed")}: ${errorMessage(toConversionError(reason))}`);
    } finally {
      setBrowsing(false);
    }
  }

  useEffect(() => {
    setFiles([]);
    setValues(defaultValues(tool.options));
    setNotice(null);
  }, [tool]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    onFileDrop((paths) => addFilesRef.current(paths), setHovering).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  async function convert() {
    try {
      const jobs = await startJobs({
        toolId: tool.id,
        inputs: files,
        outputDir: output.dir,
        options: values as Record<string, OptionValue>,
      });
      onJobs(jobs);
      setFiles([]);
      setNotice(null);
    } catch (reason) {
      setNotice(`${t("convert.startFailed")}: ${errorMessage(toConversionError(reason))}`);
    }
  }

  const extensions = tool.inputs.map((e) => `.${e}`).join(", ");

  return (
    <section>
      <h2>{t(tool.name)}</h2>
      <p className="muted">{t(tool.description)}</p>

      <div className={hovering ? "dropzone hover" : "dropzone"}>
        {hovering ? t("convert.dropActive") : t("convert.dropHint", { extensions })}
        <div>
          <button type="button" onClick={browse} disabled={browsing}>
            {t("convert.browse")}
          </button>
        </div>
      </div>

      <h3>{t("convert.files")}</h3>
      {files.length === 0 ? (
        <p className="muted">{t("convert.noFiles")}</p>
      ) : (
        <ul className="files">
          {files.map((path) => (
            <li key={path}>
              <span title={path}>{baseName(path)}</span>
              <button
                type="button"
                className="link"
                onClick={() => setFiles((current) => current.filter((p) => p !== path))}
              >
                {t("convert.remove")}
              </button>
            </li>
          ))}
        </ul>
      )}
      {notice && <p className="notice">{notice}</p>}

      <OptionsForm
        specs={tool.options}
        values={values}
        onChange={(key, value) => setValues((current) => ({ ...current, [key]: value }))}
      />

      {output.control}

      <button
        type="button"
        className="primary"
        disabled={files.length === 0 || !output.ready}
        onClick={convert}
      >
        {t("convert.start")}
      </button>
    </section>
  );
}
