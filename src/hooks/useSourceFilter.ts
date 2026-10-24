import { useCallback, useState } from "react";
import { loadSourceFilter, saveSourceFilter } from "../lib/sourceFilter";

export function useSourceFilter(): [string, (filter: string) => void] {
  const [filter, setFilter] = useState(loadSourceFilter);
  const set = useCallback((next: string) => {
    setFilter(next);
    saveSourceFilter(next);
  }, []);
  return [filter, set];
}
