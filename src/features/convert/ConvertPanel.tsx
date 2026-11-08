import { useEffect, useRef, useState } from "react";
import {
  onFileDrop,
  startJobs,
  toConversionError,
  type Job,
  type OptionValue,
  type ToolDescriptor,
} from "../../shared/ipc";
import { t } from "../../shared/i18n";
import { baseName, errorMessage } from "../jobs/JobList";
import { OptionsForm, defaultValues, type OptionValues } from "./OptionsForm";

interface Props {
  tool: ToolDescriptor;
  onJobs: (jobs: Job[]) => void;
}

function extensionOf(path: string): string {
  const name = baseName(path);
  const dot = name.lastIndexOf(".");
  return dot === -1 ? "" : name.slice(dot + 1).toLowerCase();
}

/** Drop files, tweak the options, convert. Works for any tool through its descriptor. */
export function ConvertPanel({ tool, onJobs }: Props) {
  const [files, setFiles] = useState<string[]>([]);
  const [values, setValues] = useState<OptionValues>(() => defaultValues(tool.options));
  const [hovering, setHovering] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  // The drop handler must always see the current tool without re-subscribing.
  const accepted = useRef(tool.inputs);
  accepted.current = tool.inputs;

  useEffect(() => {
    setFiles([]);
    setValues(defaultValues(tool.options));
    setNotice(null);
  }, [tool]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    onFileDrop(
      (paths) => {
        const ok = paths.filter((p) => accepted.current.includes(extensionOf(p)));
        const rejected = paths.filter((p) => !ok.includes(p));
        setFiles((current) => [...current, ...ok.filter((p) => !current.includes(p))]);
        setNotice(
          rejected.length > 0
            ? t("convert.rejected", {
                extensions: accepted.current.map((e) => `.${e}`).join(", "),
                names: rejected.map(baseName).join(", "),
              })
            : null,
        );
      },
      setHovering,
    ).then((fn) => {
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
        outputDir: null,
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

      <button type="button" className="primary" disabled={files.length === 0} onClick={convert}>
        {t("convert.start")}
      </button>
    </section>
  );
}
