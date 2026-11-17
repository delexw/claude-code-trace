import type { FlatItem } from "../../../shared/projectTree";
import type { CodexSession } from "./types";

type ProjectSession = Pick<CodexSession, "id" | "path" | "cwd" | "source"> & {
  dirs?: string[];
  forked_from_session_id?: string | null;
  is_ongoing?: boolean;
};

interface Folder {
  key: string;
  scope: string;
  path: string;
  label: string;
}

function cleanPath(path: string): string {
  const root = path.startsWith("//") ? "//" : path.startsWith("/") ? "/" : "";
  const parts: string[] = [];
  for (const part of path.split("/")) {
    if (!part || part === ".") continue;
    if (part === ".." && parts.length && parts[parts.length - 1] !== "..") parts.pop();
    else if (part !== ".." || !root) parts.push(part);
  }
  return root + parts.join("/");
}

/** The cwd is a real project identity, never a fabricated Claude transcript path. */
function folder(session: ProjectSession): Folder {
  const original = (session.dirs?.[0] || session.cwd).replace(/\\/g, "/");
  const unc = original.match(/^\/\/(?:wsl\.localhost|wsl\$)\/([^/]+)(\/.*)?$/i);
  const transcriptDistro = session.path
    .replace(/\\/g, "/")
    .match(/^\/\/(?:wsl\.localhost|wsl\$)\/([^/]+)/i)?.[1];
  const distro = unc?.[1] || transcriptDistro || session.source.match(/^WSL:\s*(.+)$/i)?.[1];
  const windows = !unc && /^(?:[a-z]:|\/\/)/i.test(original);
  const scope = windows ? "windows" : distro ? `wsl:${distro.toLowerCase()}` : "posix";
  const displayPath = cleanPath(unc ? unc[2] || "/" : original);
  const path = windows ? displayPath.toLowerCase() : displayPath;
  return {
    key: JSON.stringify([scope, path]),
    scope,
    path,
    label: displayPath.split("/").at(-1) || displayPath || "Unknown folder",
  };
}

function forkRoot(
  session: ProjectSession,
  byId: ReadonlyMap<string, ProjectSession>,
): ProjectSession {
  let current = session;
  const seen = new Set<string>();
  while (current.forked_from_session_id && !seen.has(current.id)) {
    seen.add(current.id);
    const parent = byId.get(current.forked_from_session_id);
    if (!parent) break;
    current = parent;
  }
  return current;
}

export function codexProjectKey(session: ProjectSession): string {
  return folder(session).key;
}

export function filterCodexProjectSessions<T extends ProjectSession>(
  sessions: T[],
  selectedProject: string | null | undefined,
): T[] {
  if (selectedProject == null) return sessions;
  const byId = new Map(sessions.map((session) => [session.id, session]));
  return sessions.filter((session) => folder(forkRoot(session, byId)).key === selectedProject);
}

interface FolderNode extends Folder {
  count: number;
  ongoing: boolean;
  children: FolderNode[];
}

function isDescendant(child: Folder, parent: Folder): boolean {
  return (
    child.scope === parent.scope &&
    child.path !== parent.path &&
    !!parent.path &&
    child.path.startsWith(parent.path.replace(/\/$/, "") + "/")
  );
}

export function buildCodexProjectItems(
  sessions: ProjectSession[],
  collapsedKeys?: ReadonlySet<string>,
): FlatItem[] {
  const byId = new Map(sessions.map((session) => [session.id, session]));
  const folders = new Map<string, FolderNode>();
  for (const session of sessions) {
    const project = folder(forkRoot(session, byId));
    let node = folders.get(project.key);
    if (!node) {
      node = { ...project, count: 0, ongoing: false, children: [] };
      folders.set(project.key, node);
    }
    node.count++;
    node.ongoing ||= !!session.is_ongoing;
  }
  const roots: FolderNode[] = [];
  const nodes = [...folders.values()];
  for (const node of nodes) {
    let parent: FolderNode | undefined;
    for (const candidate of nodes) {
      if (
        isDescendant(node, candidate) &&
        (!parent || candidate.path.length > parent.path.length)
      ) {
        parent = candidate;
      }
    }
    (parent?.children || roots).push(node);
  }
  const items: FlatItem[] = [
    {
      key: null,
      name: "All Projects",
      count: sessions.length,
      ongoing: false,
      depth: 0,
      isGroup: false,
      hasChildren: false,
      isExpanded: true,
    },
  ];
  function walk(children: FolderNode[], depth: number): void {
    for (const node of children.toSorted(
      (a, b) => a.label.localeCompare(b.label) || a.key.localeCompare(b.key),
    )) {
      const expanded = !collapsedKeys?.has(node.key);
      items.push({
        key: node.key,
        name: node.label,
        path: node.path || undefined,
        count: node.count,
        ongoing: node.ongoing,
        depth,
        isGroup: false,
        hasChildren: !!node.children.length,
        isExpanded: expanded,
      });
      if (expanded) walk(node.children, depth + 1);
    }
  }
  walk(roots, 0);
  return items;
}
