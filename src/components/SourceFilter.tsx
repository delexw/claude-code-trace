import { ALL_SOURCES } from "../lib/sourceFilter";

interface SourceFilterProps {
  sources: { id: string; label: string }[];
  value: string;
  onChange: (filter: string) => void;
}

/** All / one-source selector for the picker. Hidden unless there is a choice to make. */
export function SourceFilter({ sources, value, onChange }: SourceFilterProps) {
  if (sources.length < 2) return null;
  const options = [{ id: ALL_SOURCES, label: "All" }, ...sources];
  return (
    <div className="picker__sources" role="group" aria-label="Show sessions from">
      {options.map((option) => (
        <button
          key={option.id}
          type="button"
          className={`picker__sources-btn${option.id === value ? " picker__sources-btn--active" : ""}`}
          aria-pressed={option.id === value}
          onClick={() => onChange(option.id)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}
