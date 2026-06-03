# Spec: JSONL Parser Pipeline

**Location**: `src-tauri/src/parser/`

The parser transforms raw Claude Code JSONL session files into structured, display-ready message
trees. It is a pure pipeline with no side effects: the same input always produces the same output.

---

## Pipeline Overview

```mermaid
flowchart TD
    FILE["JSONL file on disk"]

    subgraph entry["entry.rs — Deserialisation"]
        E1["Read lines\n(LineReader)"]
        E2["serde_json::from_str\n→ Entry struct"]
        E3["parse_entry()\nvalidate uuid + entry_type"]
    end

    subgraph classify["classify.rs — Semantic classification"]
        C1["ClassifiedMsg enum\nUser / AI / System / Teammate\nHook / Compact"]
        C2["Normalise tool inputs\n(pre-v2.1.92 string bug)"]
        C3["Rescue hook events\nfrom progress entries"]
        C4["Extract command output tags\nlocal-command / bash / task-notification"]
    end

    subgraph chunk["chunk.rs — Assembly"]
        CH1["build_chunks()"]
        CH2["Merge consecutive AI msgs\n(merge_ai_buffer)"]
        CH3["Pair tool_use ↔ tool_result"]
        CH4["Mark orphan / deferred tools"]
        CH5["suppress_inflated_durations"]
        CH6["→ Vec&lt;Chunk&gt;"]
    end

    subgraph recon["Reconstruction"]
        SA["subagent.rs\ndiscover + link subagents"]
        TM["team.rs\nreconstruct_teams()"]
        ONG["ongoing.rs\nOngoingChecker"]
    end

    subgraph conv["convert.rs — Frontend serialisation"]
        CV1["chunks_to_messages()"]
        CV2["Color assignment\n(team colors / pool)"]
        CV3["Cycle detection\n(visited set)"]
        CV4["→ Vec&lt;DisplayMessage&gt;"]
    end

    FILE --> E1 --> E2 --> E3 --> C1
    C1 --> C2 --> C3 --> C4 --> CH1
    CH1 --> CH2 --> CH3 --> CH4 --> CH5 --> CH6
    CH6 --> SA
    CH6 --> TM
    CH6 --> ONG
    SA --> CV1
    TM --> CV1
    CV1 --> CV2 --> CV3 --> CV4
```

---

## Stage 1: Entry Deserialisation (`entry.rs`)

Each JSONL line is decoded into an `Entry` struct that mirrors the raw Claude Code format.

### Key Fields

| Field              | Description                                                              |
| ------------------ | ------------------------------------------------------------------------ |
| `uuid`             | Unique message identifier                                                |
| `entry_type`       | Discriminant: `user`, `assistant`, `system`, `hook_event`, etc.          |
| `role`             | Same as `entry_type` for most messages                                   |
| `content`          | Message body (string or content-block array)                             |
| `model`            | Model string (assistant messages only)                                   |
| `subtype`          | Hook subtype: `PreToolUse`, `PostToolUse`, `Stop`, …                     |
| `hookEvent`        | Hook event name                                                          |
| `isCompactSummary` | Compaction boundary marker                                               |
| `away_summary`     | Session-recap text                                                       |
| `forkedFrom`       | Pre-v2.1.118 fork reference                                              |
| `tool_use_result`  | JSON object for tool results                                             |
| `background_tasks` | v2.1.145+: running background task descriptors (Stop/SubagentStop hooks) |
| `session_crons`    | v2.1.145+: registered session cron jobs (Stop/SubagentStop hooks)        |
| `workflowId`       | v2.1.154+: workflow identifier on lifecycle entries                      |
| `workflowName`     | v2.1.154+: workflow name on lifecycle entries                            |
| `workflowRunUrl`   | v2.1.154+: workflow run URL on lifecycle entries                         |
| `workflowStatus`   | v2.1.154+: workflow run status on lifecycle entries                      |

```mermaid
classDiagram
    class Entry {
        +String uuid
        +String entry_type
        +String role
        +ContentBlock[] content
        +String model
        +String subtype
        +String hookEvent
        +bool isCompactSummary
        +String away_summary
        +TokenUsage usage
        +JsonValue tool_use_result
    }
```

---

## Stage 2: Classification (`classify.rs`)

Classification converts each `Entry` into a `ClassifiedMsg` variant by inspecting `entry_type`,
`role`, `subtype`, and content. This stage normalises differences between Claude Code versions.

```mermaid
flowchart TD
    E["Entry"]
    E --> R{entry_type?}
    R -->|user| U["ClassifiedMsg::User\n(permission_mode)"]
    R -->|assistant| AI["ClassifiedMsg::AI\n(tool_calls, thinking)"]
    R -->|hook_event| HK{subtype?}
    HK -->|Progress / Attachment| RESCUE["Rescue hook metadata\nfrom content text"]
    HK -->|other| SYS["ClassifiedMsg::System"]
    R -->|compact_boundary| CB["ClassifiedMsg::Compact"]
    R -->|away_summary| AS["ClassifiedMsg::Compact\n(recap)"]
    AI --> NORM["Normalise tool inputs\nif string → JSON parse\n(pre-v2.1.92 bug)"]
    RESCUE --> HK2["ClassifiedMsg::Hook"]
    NORM --> AI2["ClassifiedMsg::AI (normalised)"]
```

### Version-Compatibility Normalisations

| Issue                                                                                     | Version      | Fix                                                                       |
| ----------------------------------------------------------------------------------------- | ------------ | ------------------------------------------------------------------------- |
| Tool inputs JSON-encoded as strings                                                       | pre-v2.1.92  | Deserialise inner string → object                                         |
| Fork reference in `forkedFrom` field                                                      | pre-v2.1.118 | Map to synthetic `fork-context-ref`                                       |
| Hook payload in content text                                                              | all          | Regex extraction of teammate ID, color, protocol                          |
| Large outputs written to disk                                                             | v2.1.89+     | `RE_PERSISTED_OUTPUT_PATH` → file read                                    |
| Dynamic Workflow lifecycle types                                                          | v2.1.154+    | Add to `NOISE_ENTRY_TYPES`; capture workflow fields on `Entry`            |
| `cache_creation_input_tokens` always 0 when API uses nested `cache_creation.input_tokens` | v2.1.152+    | `cache_creation_from_value()` reads both flat and nested forms; takes max |

---

## Stage 3: Chunk Assembly (`chunk.rs`)

Chunks are the displayable conversation turns. Assembly merges sequences of classified messages
and pairs tool calls with their results.

```mermaid
stateDiagram-v2
    [*] --> Idle
    Idle --> AI_Buffer : AI message arrives
    AI_Buffer --> AI_Buffer : More AI messages (merge)
    AI_Buffer --> Idle : Non-AI message flushes buffer → Chunk
    Idle --> User_Chunk : User message → emit Chunk
    Idle --> System_Chunk : System message → emit Chunk
    Idle --> Compact_Chunk : Compact marker → emit Chunk

    note right of AI_Buffer
        Tool calls accumulated.
        Results matched by tool_id.
        Unmatched → orphan or deferred.
    end note
```

### Tool Pairing Logic

```mermaid
flowchart LR
    TC["tool_use block\n(tool_id)"]
    TR["tool_result block\n(tool_id)"]
    TC -->|"id match"| PAIR["Paired ToolCall DisplayItem"]
    TC -->|"no match found"| CHECK{"result in\nsubagent?"}
    CHECK -->|"yes (deferred)"| DEF["Marked is_deferred=true"]
    CHECK -->|"no"| ORPHAN["Marked is_orphan=true"]
    TR -->|"already consumed"| SKIP["Skip (no duplicate)"]
```

### Chunk Types

| Type      | Source              | Key Fields                                         |
| --------- | ------------------- | -------------------------------------------------- |
| `AI`      | assistant entries   | text, model, usage, tool_calls, items, duration_ms |
| `User`    | user entries        | user_text, permission_mode                         |
| `System`  | hook/system entries | output, is_error                                   |
| `Compact` | compact_boundary    | (separator marker)                                 |
| `Recap`   | away_summary        | output text                                        |

### DisplayItem Types

```mermaid
classDiagram
    class DisplayItem {
        +String id
        +DisplayItemType item_type
        +String text
        +String tool_name
        +String tool_summary
        +ToolCategory tool_category
        +JsonValue tool_input
        +String tool_result
        +bool tool_error
        +u64 duration_ms
        +u64 token_count
        +bool is_orphan
        +bool is_deferred
    }
    class DisplayItemType {
        <<enumeration>>
        Thinking
        Output
        ToolCall
        Subagent
        TeammateMessage
        HookEvent
    }
    class ToolCategory {
        <<enumeration>>
        Read
        Edit
        Write
        Bash
        Grep
        Glob
        Task
        Tool
        Web
        Cron
        MCP
        Other
    }
    DisplayItem --> DisplayItemType
    DisplayItem --> ToolCategory
```

---

## Stage 4: Subagent Reconstruction (`subagent.rs`)

Subagents are child Claude processes, each writing to their own `agent-*.jsonl` file.
This stage discovers them, parses their files, and links each to the parent tool call.

```mermaid
sequenceDiagram
    participant Parent as Parent Session
    participant Disk as Disk (agent-*.jsonl)
    participant Graph as ProcGraph

    Parent ->> Disk: scan session's subagent directory
    Disk -->> Parent: list of agent files
    loop per agent file
        Parent ->> Disk: read + parse (recursive)
        Disk -->> Parent: SubagentProcess
        Parent ->> Graph: insert(tool_id → SubagentProcess)
    end

    Note over Parent,Graph: Four-phase linking

    Parent ->> Graph: Phase 1: match toolUseResult.agentId (authoritative)
    Parent ->> Graph: Phase 2: match team-member description
    Parent ->> Graph: Phase 3: positional (temporal proximity)
    Parent ->> Graph: Phase 4: nested enrichment
    Graph -->> Parent: linked ProcGraph

    alt unlinked agents remain
        Parent ->> Parent: inject_orphan_subagents()\nsynthesize DisplayItem
    end
```

### Token Deduplication

Agents can appear both in the parent's tool_result AND as a separate JSONL file.
`TokenSnapshot` and `insert_best_snapshot()` keep only the more-complete token record,
preventing double-counting.

---

## Stage 5: Team Reconstruction (`team.rs`)

Teams are reconstructed from sparse signals (TaskCreate, TaskUpdate, SendMessage) in the message
stream.

```mermaid
flowchart TD
    subgraph Signals["Signals extracted from chunks"]
        TC["TaskCreate items\n(subject, owner, agentId)"]
        TU["TaskUpdate items\n(status changes)"]
        SM["SendMessage items\n(teammate metadata)"]
    end

    TC --> SNAP["TeamSnapshot\n(name, members, tasks, colors)"]
    TU --> SNAP
    SM --> SNAP

    SNAP --> ENRICH["Recursive enrichment\nfrom subagent completion status"]
    ENRICH --> FINAL["Final TeamSnapshot[]"]
```

---

## Stage 6: Completion Detection (`ongoing.rs`)

`OngoingChecker` determines if a session is still running.

```mermaid
flowchart LR
    INPUT["Session file\n+ subagent statuses"]

    INPUT --> S1{"Modified in\nlast 60 s?"}
    S1 -->|"yes"| ONGOING["ONGOING"]
    S1 -->|"no"| S2{"Has shutdown\nmessage?"}
    S2 -->|"yes"| DONE["COMPLETE"]
    S2 -->|"no"| S3{"Has background\ntasks?"}
    S3 -->|"still running"| ONGOING
    S3 -->|"all done"| S4{"Subagents\nongoing?"}
    S4 -->|"yes"| ONGOING
    S4 -->|"no"| DONE
```

---

## Stage 7: Frontend Conversion (`convert.rs`)

Translates internal `Chunk` trees into JSON-serialisable `DisplayMessage` structs for the frontend.

```mermaid
flowchart TD
    CK["Vec&lt;Chunk&gt;"]
    PG["ProcGraph\n(subagent lookup)"]

    CK --> F["chunks_to_messages()"]
    PG --> F

    F --> AI_MSG["AI Chunk → DisplayMessage\n- count thinking/tool/output\n- extract last_output\n- expand subagent_messages (recursive)\n- assign colors (team or pool)\n- cycle guard (visited set)"]
    F --> USER_MSG["User Chunk → DisplayMessage"]
    F --> SYS_MSG["System Chunk → DisplayMessage"]
    F --> CMP_MSG["Compact/Recap → DisplayMessage"]

    AI_MSG --> OUT["Vec&lt;DisplayMessage&gt;"]
    USER_MSG --> OUT
    SYS_MSG --> OUT
    CMP_MSG --> OUT
```

### Color Assignment

```mermaid
flowchart LR
    AG["SubagentProcess"]
    AG --> Q{"Has team\ncolor?"}
    Q -->|"yes"| TC["Use team color"]
    Q -->|"no"| Q2{"Already in\npool map?"}
    Q2 -->|"yes"| PC["Use existing pool color"]
    Q2 -->|"no"| NEXT["Next from 8-color pool"]
    NEXT --> PC
```

---

## Helper Modules

| Module          | Purpose                                                                |
| --------------- | ---------------------------------------------------------------------- |
| `linereader.rs` | Buffered line reader; tolerates lines exceeding default buffer         |
| `sanitize.rs`   | Strip XML tags, extract command output, resolve persisted output paths |
| `taxonomy.rs`   | `categorize_tool_name()` — maps tool name → ToolCategory               |
| `summary.rs`    | `tool_summary()` — generates human-readable one-liner per tool call    |
| `patterns.rs`   | Compiled regex patterns (command tags, teammate metadata, etc.)        |
| `dategroup.rs`  | Groups session list by Today / Yesterday / This Week / Older           |
| `debuglog.rs`   | Incremental debug log reader with deduplication                        |
| `project.rs`    | `project_name()` — derives "repo // branch" from cwd                   |
| `cache.rs`      | Per-file parse memoisation keyed by (path, mtime, size)                |

---

## Related Specs

- [02-file-watcher.md](02-file-watcher.md) — triggers re-runs of this pipeline
- [07-data-types.md](07-data-types.md) — full type definitions
- [08-session-lifecycle.md](08-session-lifecycle.md) — end-to-end flow including this pipeline
