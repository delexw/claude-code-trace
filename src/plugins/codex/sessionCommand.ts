import type { CodexSession } from "./types";

type SessionLocation = Pick<CodexSession, "id" | "cwd" | "path">;

export interface CodexSessionCommand {
  command: string;
  shell: "PowerShell" | "POSIX shell";
}

function quotePosix(value: string): string {
  return `'${value.replace(/'/g, "'\\''")}'`;
}

function quotePowerShell(value: string): string {
  return `'${value.replace(/'/g, "''")}'`;
}

function windowsPath(path: string): string | null {
  const mounted = path.match(/^\/mnt\/([a-z])\/(.*)$/i);
  if (mounted) return `${mounted[1].toUpperCase()}:\\${mounted[2].replaceAll("/", "\\")}`;
  return /^[a-z]:[\\/]|^(\\\\|\/\/)/i.test(path) ? path.replaceAll("/", "\\") : null;
}

function codexHome(path: string): string | null {
  const normalized = path.replaceAll("\\", "/");
  const marker = Math.max(
    normalized.lastIndexOf("/sessions/"),
    normalized.lastIndexOf("/archived_sessions/"),
  );
  return marker > 0 ? normalized.slice(0, marker) : null;
}

/** Copyable commands target the chat's environment, rather than the browser's OS.
 * Preserve CODEX_HOME so sessions from custom homes can actually be found. */
export function codexSessionCommand(
  session: SessionLocation,
  { fork = false }: { fork?: boolean } = {},
): CodexSessionCommand | null {
  if (!session.id || !session.cwd || /[\0\r\n]/.test(session.id + session.cwd + session.path))
    return null;
  const home = codexHome(session.path);
  if (!home) return null;
  const action = fork ? "fork" : "resume";
  const cwd = windowsPath(session.cwd);
  if (cwd) {
    const nativeHome = windowsPath(home);
    if (!nativeHome) return null;
    const quote = quotePowerShell;
    const command = `codex -C ${quote(cwd)} ${action} -- ${quote(session.id)}`;
    return {
      shell: "PowerShell",
      command: `& { $codexTracePreviousHome = $env:CODEX_HOME; try { $env:CODEX_HOME = ${quote(nativeHome)}; ${command} } finally { $env:CODEX_HOME = $codexTracePreviousHome } }`,
    };
  }
  // Windows auto-discovery exposes WSL homes through UNC paths. Commands are
  // pasted into that distro's POSIX shell, where the home has its native path.
  const nativeHome = home.replace(/^\/\/wsl(?:\.localhost|\$)\/[^/]+(?=\/)/i, "");
  if (!session.cwd.startsWith("/") || !nativeHome.startsWith("/")) return null;
  return {
    shell: "POSIX shell",
    command: `CODEX_HOME=${quotePosix(nativeHome)} codex -C ${quotePosix(session.cwd)} ${action} -- ${quotePosix(session.id)}`,
  };
}
