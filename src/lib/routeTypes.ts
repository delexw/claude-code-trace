export interface Route {
  method?: "POST" | "DELETE";
  path: string | ((args: Record<string, unknown>) => string);
  body?: (args: Record<string, unknown>) => unknown;
}

/** Declarative mapping from Tauri command names to HTTP endpoints. */
export type RouteTable = Record<string, Route>;
