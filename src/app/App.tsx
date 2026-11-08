import { useEffect, useState } from "react";
import { ConvertPanel } from "../features/convert/ConvertPanel";
import { JobList } from "../features/jobs/JobList";
import { useJobs } from "../features/jobs/useJobs";
import { ToolPicker } from "../features/tool-picker/ToolPicker";
import { t } from "../shared/i18n";
import { listTools, toConversionError, type ToolDescriptor } from "../shared/ipc";
import { errorMessage } from "../features/jobs/JobList";
import "./app.css";

export default function App() {
  const [tools, setTools] = useState<ToolDescriptor[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const { jobs, upsert } = useJobs();

  useEffect(() => {
    listTools()
      .then((list) => {
        setTools(list);
        setSelectedId((current) => current ?? list[0]?.id ?? null);
      })
      .catch((reason) => setLoadError(errorMessage(toConversionError(reason))));
  }, []);

  const selected = tools.find((tool) => tool.id === selectedId);

  return (
    <main className="app">
      <header>
        <h1>{t("app.title")}</h1>
        <p className="muted">{t("app.tagline")}</p>
      </header>

      {loadError && <p className="notice">{loadError}</p>}
      {selected && (
        <>
          <ToolPicker tools={tools} selectedId={selected.id} onSelect={setSelectedId} />
          <ConvertPanel tool={selected} onJobs={(created) => created.forEach(upsert)} />
        </>
      )}
      <JobList jobs={jobs} />
    </main>
  );
}
