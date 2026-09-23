// Structural diff for an Edit tool's old_string -> new_string.
//
// Produces a line-level unified diff that PRESERVES unchanged context lines
// (so a one-token edit in a 5-line block shows 4 context lines + 1 changed
// line, not 5 removed + 5 added), and for each pair of changed lines computes
// intra-line WORD-level change ranges. Inspired by umputun/revdiff's worddiff:
// LCS pairing + a similarity gate so dissimilar lines aren't falsely aligned.
//
// Mirrored in tui-py/diff_utils.py — keep the two implementations in sync.

export type DiffLineKind = "context" | "removed" | "added";

export interface DiffSegment {
  text: string;
  /** True when this span differs from the paired line (word-level highlight). */
  changed: boolean;
}

export interface DiffLine {
  kind: DiffLineKind;
  /** Concatenating `segments[].text` reproduces the full line. */
  segments: DiffSegment[];
}

// Beyond this many DP cells the O(n*m) line LCS is skipped in favour of a plain
// "all removed then all added" rendering. Edit payloads are small in practice.
const MAX_LCS_CELLS = 40000;

// Minimum fraction of shared non-whitespace tokens for two lines to be treated
// as an edit of each other (and thus word-diffed rather than shown as wholly
// removed + added). Matches revdiff's 30% gate.
const WORD_SIMILARITY_THRESHOLD = 0.3;

// Words (letters/digits/underscore), whitespace runs, and punctuation runs.
const TOKEN_RE = /[\p{L}\p{N}_]+|\s+|[^\p{L}\p{N}_\s]+/gu;

export function tokenize(s: string): string[] {
  return s.match(TOKEN_RE) ?? [];
}

function isWhitespace(tok: string): boolean {
  return tok.trim() === "";
}

// Marks which elements of a / b participate in their longest common
// subsequence (by equality). O(n*m) time and space.
function lcsMatched(a: string[], b: string[]): { aMatched: boolean[]; bMatched: boolean[] } {
  const n = a.length;
  const m = b.length;
  const aMatched: boolean[] = Array.from({ length: n }, () => false);
  const bMatched: boolean[] = Array.from({ length: m }, () => false);
  if (n === 0 || m === 0) return { aMatched, bMatched };

  const dp: number[][] = Array.from({ length: n + 1 }, () =>
    Array.from({ length: m + 1 }, () => 0),
  );
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }

  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      aMatched[i] = true;
      bMatched[j] = true;
      i++;
      j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      i++;
    } else {
      j++;
    }
  }
  return { aMatched, bMatched };
}

// Merge adjacent tokens with the same changed flag. Whitespace is never flagged
// changed, so leading/trailing spaces aren't highlighted on their own.
function buildSegments(tokens: string[], matched: boolean[]): DiffSegment[] {
  const segs: DiffSegment[] = [];
  for (let i = 0; i < tokens.length; i++) {
    const changed = !matched[i] && !isWhitespace(tokens[i]);
    const last = segs[segs.length - 1];
    if (last && last.changed === changed) last.text += tokens[i];
    else segs.push({ text: tokens[i], changed });
  }
  return segs;
}

// Word-level diff of two lines. Returns null when the lines are too dissimilar
// to be considered a single edit (caller then shows them wholly removed/added).
export function wordDiff(
  oldLine: string,
  newLine: string,
): { oldSegments: DiffSegment[]; newSegments: DiffSegment[] } | null {
  const a = tokenize(oldLine);
  const b = tokenize(newLine);
  const { aMatched, bMatched } = lcsMatched(a, b);

  const aNonWs = a.filter((t) => !isWhitespace(t)).length;
  const bNonWs = b.filter((t) => !isWhitespace(t)).length;
  const denom = Math.max(aNonWs, bNonWs);
  if (denom === 0) return null;

  let commonNonWs = 0;
  for (let i = 0; i < a.length; i++) {
    if (aMatched[i] && !isWhitespace(a[i])) commonNonWs++;
  }
  if (commonNonWs / denom < WORD_SIMILARITY_THRESHOLD) return null;

  return { oldSegments: buildSegments(a, aMatched), newSegments: buildSegments(b, bMatched) };
}

interface LineOp {
  kind: DiffLineKind;
  text: string;
}

function lineDiffOps(oldLines: string[], newLines: string[]): LineOp[] {
  const n = oldLines.length;
  const m = newLines.length;
  if (n * m > MAX_LCS_CELLS) {
    return [
      ...oldLines.map((text): LineOp => ({ kind: "removed", text })),
      ...newLines.map((text): LineOp => ({ kind: "added", text })),
    ];
  }

  const dp: number[][] = Array.from({ length: n + 1 }, () =>
    Array.from({ length: m + 1 }, () => 0),
  );
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i][j] =
        oldLines[i] === newLines[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }

  const ops: LineOp[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (oldLines[i] === newLines[j]) {
      ops.push({ kind: "context", text: oldLines[i] });
      i++;
      j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      ops.push({ kind: "removed", text: oldLines[i] });
      i++;
    } else {
      ops.push({ kind: "added", text: newLines[j] });
      j++;
    }
  }
  while (i < n) ops.push({ kind: "removed", text: oldLines[i++] });
  while (j < m) ops.push({ kind: "added", text: newLines[j++] });
  return ops;
}

// Within each maximal run of changes, emit all removed lines before all added
// lines so removed[i] can be paired with added[i] for word-diffing.
function groupRuns(ops: LineOp[]): LineOp[] {
  const out: LineOp[] = [];
  let k = 0;
  while (k < ops.length) {
    if (ops[k].kind === "context") {
      out.push(ops[k]);
      k++;
      continue;
    }
    const removed: LineOp[] = [];
    const added: LineOp[] = [];
    while (k < ops.length && ops[k].kind !== "context") {
      if (ops[k].kind === "removed") removed.push(ops[k]);
      else added.push(ops[k]);
      k++;
    }
    out.push(...removed, ...added);
  }
  return out;
}

export function computeEditDiff(oldLines: string[], newLines: string[]): DiffLine[] {
  const ops = groupRuns(lineDiffOps(oldLines, newLines));
  const result: DiffLine[] = [];
  let k = 0;
  while (k < ops.length) {
    if (ops[k].kind === "context") {
      result.push({ kind: "context", segments: [{ text: ops[k].text, changed: false }] });
      k++;
      continue;
    }
    const removed: string[] = [];
    const added: string[] = [];
    while (k < ops.length && ops[k].kind === "removed") removed.push(ops[k++].text);
    while (k < ops.length && ops[k].kind === "added") added.push(ops[k++].text);

    const pairs = Math.min(removed.length, added.length);
    const wds = [];
    for (let i = 0; i < pairs; i++) wds.push(wordDiff(removed[i], added[i]));

    for (let i = 0; i < removed.length; i++) {
      const wd = i < pairs ? wds[i] : null;
      result.push({
        kind: "removed",
        segments: wd ? wd.oldSegments : [{ text: removed[i], changed: false }],
      });
    }
    for (let i = 0; i < added.length; i++) {
      const wd = i < pairs ? wds[i] : null;
      result.push({
        kind: "added",
        segments: wd ? wd.newSegments : [{ text: added[i], changed: false }],
      });
    }
  }
  return result;
}

// Detects embedded unified-diff hunks (e.g. Claude Code's `bashEditDiffEnabled`
// diff of files a Bash command changed) within otherwise plain tool output text.
//
// The parser's generic content-block flattening (`stringify_content` in
// classify.rs) joins separate content blocks with "\n" into one string with no
// separator, so a diff blob appended after stdout/stderr is otherwise
// indistinguishable from plain command output. This lets the UI render the
// diff portion distinctly instead of as plain stdout.
//
// Mirrored in tui-py/diff_utils.py — keep the two implementations in sync.
export type DiffTextSegmentKind = "text" | "diff";

export interface DiffTextSegment {
  kind: DiffTextSegmentKind;
  content: string;
}

const HUNK_HEADER_RE = /^@@ -\d+(?:,\d+)? \+\d+(?:,\d+)? @@/;
const FILE_HEADER_RE = /^(?:---|\+\+\+) /;

export function splitDiffSections(text: string): DiffTextSegment[] {
  const lines = text.split("\n");
  const segments: DiffTextSegment[] = [];
  let plainStart = 0;
  let i = 0;

  while (i < lines.length) {
    if (!HUNK_HEADER_RE.test(lines[i])) {
      i++;
      continue;
    }

    // Found a hunk header — walk backwards over adjacent `---`/`+++` file
    // headers so they're included in the same diff segment.
    let blockStart = i;
    while (blockStart > plainStart && FILE_HEADER_RE.test(lines[blockStart - 1])) {
      blockStart--;
    }
    if (blockStart > plainStart) {
      segments.push({ kind: "text", content: lines.slice(plainStart, blockStart).join("\n") });
    }

    // Walk forward consuming hunk lines (context/added/removed) and any
    // further hunk headers for the same file. A `---`/`+++` pair immediately
    // followed by a hunk header starts the NEXT file's diff, not a removed/
    // added content line of this one, so stop there instead of swallowing it.
    let j = i + 1;
    while (j < lines.length) {
      const l = lines[j];
      if (HUNK_HEADER_RE.test(l)) {
        j++;
        continue;
      }
      if (
        l.startsWith("--- ") &&
        FILE_HEADER_RE.test(lines[j + 1] ?? "") &&
        HUNK_HEADER_RE.test(lines[j + 2] ?? "")
      ) {
        break;
      }
      if (l.startsWith(" ") || l.startsWith("+") || l.startsWith("-")) {
        j++;
        continue;
      }
      break;
    }
    segments.push({ kind: "diff", content: lines.slice(blockStart, j).join("\n") });
    plainStart = j;
    i = j;
  }

  if (plainStart < lines.length) {
    segments.push({ kind: "text", content: lines.slice(plainStart).join("\n") });
  }

  return segments.length ? segments : [{ kind: "text", content: text }];
}
