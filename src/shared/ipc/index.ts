// The only module that talks to the Rust engine (invoke / events / drag and drop / file dialog).
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import type { ConversionError, ConversionRequest, Job, ToolDescriptor } from "./types";

export type * from "./types";

export const listTools = () => invoke<ToolDescriptor[]>("list_tools");

export const listJobs = () => invoke<Job[]>("list_jobs");

export const startJobs = (request: ConversionRequest) =>
  invoke<Job[]>("start_jobs", { request });

export const cancelJob = (id: string) => invoke<void>("cancel_job", { id });

/** Called on every job state change. */
export const onJobUpdated = (callback: (job: Job) => void): Promise<UnlistenFn> =>
  listen<Job>("job-updated", (event) => callback(event.payload));

/** Called with the file paths dropped on the window; `hover` tells when files are over it. */
export const onFileDrop = (
  onDrop: (paths: string[]) => void,
  onHover: (hovering: boolean) => void,
): Promise<UnlistenFn> =>
  getCurrentWebview().onDragDropEvent((event) => {
    switch (event.payload.type) {
      case "enter":
      case "over":
        onHover(true);
        break;
      case "leave":
        onHover(false);
        break;
      case "drop":
        onHover(false);
        onDrop(event.payload.paths);
        break;
    }
  });

/**
 * Opens the system file dialog (several files allowed) filtered by `extensions` (lowercase,
 * without the dot) and returns the chosen paths, or an empty list if the user cancelled.
 * `filterName` is the already translated label of the filter.
 */
export async function pickFiles(extensions: string[], filterName: string): Promise<string[]> {
  const chosen = await open({
    multiple: true,
    filters: [{ name: filterName, extensions }],
  });
  return chosen ?? [];
}

/** Commands reject with the serialized `ConversionError`. */
export function toConversionError(reason: unknown): ConversionError {
  if (typeof reason === "object" && reason !== null && "code" in reason) {
    return reason as ConversionError;
  }
  return { code: "internal", message: String(reason) };
}
