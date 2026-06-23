# Spec: Subagent Discovery and Linking

**Location**: `src-tauri/src/parser/subagent.rs`

Subagent linking is the most complex part of the pipeline. It reconstructs the parent→child
relationship between a session's tool calls and the spawned agent sessions, each stored in a
separate `agent-*.jsonl` file.

---

## Overview

```mermaid
flowchart TD
    PARENT["Parent session JSONL"]
    FS["Disk: agent-*.jsonl\n(subagent directory)"]
    TEAM_FS["Disk: team worker sessions\n(project directory)"]

    PARENT --> DISCOVER["discover_subagents()\n+ discover_team_sessions()"]
    FS --> DISCOVER
    TEAM_FS --> DISCOVER
    DISCOVER --> PROC["Vec&lt;SubagentProcess&gt;\n(parsed, unlinked)"]
    PROC --> LINK["link_subagents()\nfour-phase algorithm"]
    LINK --> GRAPH["ProcGraph\ntool_id → SubagentProcess"]
    GRAPH --> ORPHANS["inject_orphan_subagents()\nsynthesize DisplayItems"]
    ORPHANS --> FINAL["Fully linked session graph"]
```

---

## SubagentProcess Structure

```mermaid
classDiagram
    class SubagentProcess {
        +String id
        +PathBuf file_path
        +Vec~Chunk~ chunks
        +Option~DateTime~ start_time
        +Option~DateTime~ end_time
        +u64 duration_ms
        +TokenUsage usage
        +String description
        +String agent_type
        +Option~String~ parent_task_id
        +Option~JsonValue~ team_metadata
        +bool end_marker
        +String prompt
    }
    class ProcGraph {
        -HashMap~String, SubagentProcess~ by_tool_id
        +get(tool_id) Option~SubagentProcess~
        +insert(tool_id, proc)
        +all_unlinked() Vec~SubagentProcess~
    }
    ProcGraph --> SubagentProcess
```

---

## Discovery Phase

```mermaid
sequenceDiagram
    participant FN as discover_subagents()
    participant FS as File System

    FN ->> FS: list session_dir/<uuid>/\n(subagent subdirectory)
    FS -->> FN: [agent-abc.jsonl, agent-def.jsonl, ...]

    loop per agent file
        FN ->> FS: read file
        FS -->> FN: raw lines
        FN ->> FN: parse_entry() per line
        FN ->> FN: classify_entry()
        FN ->> FN: build_chunks() (recursive)
        FN ->> FN: discover nested subagents\n(recursion)
        FN ->> FN: extract description\nfrom first user prompt
        FN ->> FN: build SubagentProcess
    end
    FN -->> FN: Vec<SubagentProcess>
```

Recursion depth is bounded by the actual agent nesting depth in the file system. Deep nesting
(e.g., agent spawns agent spawns agent) is handled by recursive calls, but stack overflows are
mitigated by iterative traversal for deeply nested structures.

### Team Session Discovery

`discover_team_sessions()` scans the project directory for session files that match
`(teamName, agentName)` pairs extracted from parent chunks. Each candidate file is
identified by `read_team_session_meta()`, which scans lines until finding one with
a non-empty `agentName`. `teamName` is optional: pre-v2.1.178 sessions carry both,
while v2.1.178+ implicit-team sessions carry only `agentName`.

`is_team_task()` identifies named-agent tool calls by checking for the `name` key in
tool input (not `team_name`), so both explicit team spawns (pre-v2.1.178) and implicit
team spawns (v2.1.178+) are correctly classified.

Before Claude Code v2.1.174, Workflow tool `agent()` subagents omitted attribution
headers from their JSONL entries. A session file where no line carries any attribution
returns `("", "")` and is gracefully skipped. A file where attribution appears only
on later entries (mixed pre/post-fix session) is still correctly identified.

Claude Code v2.1.178 removed `TeamCreate`/`TeamDelete` and made `teamName` optional
in multi-agent sessions. Sessions recorded after this version may have teammates with
`agentName` set but `teamName` absent — this is the authoritative signal and such
sessions are fully discoverable.

---

## Four-Phase Linking Algorithm

The algorithm tries progressively weaker signals to link each `SubagentProcess` to its parent
tool call. Stronger signals take precedence and cannot be overridden.

### Phase 1: Result-Based (Authoritative)

Reads `tool_use_result` JSON in the parent session. Claude Code v2.1.118+ includes the agent's
UUID in the result payload.

```mermaid
flowchart LR
    TR["tool_result entry\ntool_use_result JSON"]
    TR --> EX{"has\nagent_context.id?"}
    EX -->|"yes"| MATCH["find SubagentProcess\nwith id == agent_context.id"]
    MATCH --> LINK["set parent_task_id\n= tool_id"]
    EX -->|"no"| SKIP["skip → phase 2"]
```

### Phase 2: Team Member Description Match

For team-based sessions, each team member has a known description string. This phase matches the
agent's extracted prompt against team member descriptions.

```mermaid
flowchart LR
    PROC["SubagentProcess\n(unlinked)"]
    TM["TeamSnapshot.members\n(id → description)"]
    PROC --> CMP{"prompt matches\nmember description?"}
    TM --> CMP
    CMP -->|"yes"| LINK["set parent_task_id\n+ team_metadata"]
    CMP -->|"no"| SKIP["skip → phase 3"]
```

### Phase 3: Positional (Temporal Proximity)

Assigns the agent to the tool call that is temporally closest **before** the agent's start time,
among tool calls that are still unlinked.

```mermaid
flowchart LR
    PROC["SubagentProcess\nstart_time = T"]
    TOOLS["Unlinked tool calls\nwith spawn semantics"]
    PROC --> FIND["find latest tool call\nwith timestamp ≤ T"]
    TOOLS --> FIND
    FIND --> LINK["set parent_task_id\n= that tool_id"]
```

### Phase 4: Nested Enrichment

After all agents are linked, recursively update team metadata from linked subagent completion
states.

```mermaid
flowchart LR
    LINKED["Linked ProcGraph"]
    LINKED --> WALK["walk all SubagentProcesses"]
    WALK --> ENRICH["if has team_metadata:\nupdate member_ongoing\nfrom subagent.end_marker"]
    ENRICH --> OUT["Enriched ProcGraph"]
```

---

## Orphan Injection

Subagents that survive all four phases without being linked (no tool call found) get a synthetic
`DisplayItem` injected at the position of their start time.

```mermaid
sequenceDiagram
    participant FN as inject_orphan_subagents()
    participant CHUNKS as parent Chunk[]

    FN ->> FN: collect all unlinked SubagentProcesses
    loop per orphan
        FN ->> FN: orphan_description_from_prompt()\nextract first meaningful line
        FN ->> FN: find insertion point in chunks\n(by start_time)
        FN ->> CHUNKS: insert synthetic ToolCall DisplayItem\n(is_orphan=true)
    end
```

---

## Token Deduplication

The same agent can appear in two places:

1. In the parent session's `tool_result` (from `SendMessage`'s response)
2. As its own `agent-*.jsonl` file

```mermaid
flowchart TD
    TR["Parent tool_result\nagentId=X, tokens=1000"]
    JF["Agent X JSONL file\nself-reported tokens=1200"]

    TR --> TS1["TokenSnapshot { id=X, tokens=1000, source=Result }"]
    JF --> TS2["TokenSnapshot { id=X, tokens=1200, source=File }"]
    TS1 --> BEST["insert_best_snapshot()\nprefer File > Result\n(more complete)"]
    TS2 --> BEST
    BEST --> TOTAL["Session total:\nuses 1200 (not 1000+1200)"]
```

---

## Cycle Detection

Mutually-referencing agents (e.g., agent A spawns agent B which references agent A) are caught
by a `visited: HashSet<AgentId>` passed through the recursive expansion in `convert.rs`.

```mermaid
flowchart LR
    EXPAND["expand_subagent_messages(agent_id=A)"]
    EXPAND --> VIS{"A in visited?"}
    VIS -->|"yes"| STOP["return [] (prevent infinite recursion)"]
    VIS -->|"no"| ADD["visited.insert(A)"]
    ADD --> RECURSE["expand children of A"]
    RECURSE --> DONE["return expanded messages"]
```

---

## Description Extraction (`orphan_description_from_prompt`)

For orphan agents where no tool call context exists, the display description is derived from the
agent's prompt text:

1. Skip blank lines and lines matching a list of common metadata prefixes
2. Take the first substantive line (max 120 chars)
3. Truncate with `…` if needed
4. Fallback to `"Agent"` if no line found

---

## Related Specs

- [01-parser-pipeline.md](01-parser-pipeline.md) — context for subagent discovery within the pipeline
- [07-data-types.md](07-data-types.md) — `DisplayItem.subagent_messages` recursive type
- [08-session-lifecycle.md](08-session-lifecycle.md) — where linking fits in the full lifecycle
