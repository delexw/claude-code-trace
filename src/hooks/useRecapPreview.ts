import { useCallback, useState } from "react";
import { loadRecapPreview, saveRecapPreview } from "../lib/recapPreview";

export function useRecapPreview(): [boolean, (on: boolean) => void] {
  const [on, setOn] = useState(loadRecapPreview);
  const set = useCallback((next: boolean) => {
    setOn(next);
    saveRecapPreview(next);
  }, []);
  return [on, set];
}
