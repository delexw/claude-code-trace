/** Shell-single-quote a string (POSIX): wrap in '…', escape embedded ' as '\''. */
function shq(s: string): string {
  return `'${s.replace(/'/g, "'\\''")}'`;
}

/** The command to paste in a terminal to resume (or fork) a session in its cwd. */
export function resumeCommand(cwd: string, sessionId: string, opts?: { fork?: boolean }): string {
  const base = `cd ${shq(cwd)} && claude --resume ${sessionId}`;
  return opts?.fork ? `${base} --fork-session` : base;
}
