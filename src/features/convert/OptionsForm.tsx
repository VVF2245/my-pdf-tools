import type { OptionSpec, OptionValue } from "../../shared/ipc";
import { t } from "../../shared/i18n";

export type OptionValues = Record<string, OptionValue>;

/** Initial values taken from the descriptor's defaults. */
export function defaultValues(specs: OptionSpec[]): OptionValues {
  const values: OptionValues = {};
  for (const spec of specs) values[spec.key] = spec.kind.default;
  return values;
}

interface Props {
  specs: OptionSpec[];
  values: OptionValues;
  onChange: (key: string, value: OptionValue) => void;
}

/** Renders one control per option spec; knows nothing about any specific tool. */
export function OptionsForm({ specs, values, onChange }: Props) {
  if (specs.length === 0) return null;
  return (
    <fieldset className="options">
      <legend>{t("convert.options")}</legend>
      {specs.map((spec) => {
        const id = `opt-${spec.key}`;
        const { kind } = spec;
        return (
          <div key={spec.key} className="option-block">
            <label htmlFor={id} className="option">
              <span>{t(spec.label)}</span>
              {kind.type === "choice" && (
                <select
                  id={id}
                  aria-describedby={spec.help ? `${id}-help` : undefined}
                  value={String(values[spec.key])}
                  onChange={(e) => onChange(spec.key, e.target.value)}
                >
                  {kind.choices.map((choice) => (
                    <option key={choice.value} value={choice.value}>
                      {t(choice.label)}
                    </option>
                  ))}
                </select>
              )}
              {kind.type === "number" && (
                <input
                  id={id}
                  aria-describedby={spec.help ? `${id}-help` : undefined}
                  type="number"
                  min={kind.min}
                  max={kind.max}
                  step={kind.step}
                  value={Number(values[spec.key])}
                  onChange={(e) => {
                    const n = e.target.valueAsNumber;
                    if (!Number.isNaN(n)) onChange(spec.key, n);
                  }}
                />
              )}
              {kind.type === "bool" && (
                <input
                  id={id}
                  aria-describedby={spec.help ? `${id}-help` : undefined}
                  type="checkbox"
                  checked={Boolean(values[spec.key])}
                  onChange={(e) => onChange(spec.key, e.target.checked)}
                />
              )}
            </label>
            {spec.help && (
              <p id={`${id}-help`} className="option-help">
                {t(spec.help)}
              </p>
            )}
          </div>
        );
      })}
    </fieldset>
  );
}
