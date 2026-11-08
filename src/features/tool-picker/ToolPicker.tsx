import type { ToolDescriptor } from "../../shared/ipc";
import { t } from "../../shared/i18n";

interface Props {
  tools: ToolDescriptor[];
  selectedId: string;
  onSelect: (id: string) => void;
}

/** Lists the available tools. It is drawn from the descriptors the engine returns. */
export function ToolPicker({ tools, selectedId, onSelect }: Props) {
  if (tools.length < 2) return null; // nothing to choose yet
  return (
    <nav className="tool-picker" aria-label="tools">
      {tools.map((tool) => (
        <button
          key={tool.id}
          type="button"
          className={tool.id === selectedId ? "tab active" : "tab"}
          onClick={() => onSelect(tool.id)}
        >
          {t(tool.name)}
        </button>
      ))}
    </nav>
  );
}
