export type PayloadMode = "minimized" | "full-transcript";

export type RecommendationProvider =
  | { type: "codex-subscription"; model?: string | null }
  | { type: "claude-code-subscription"; model?: string | null }
  | { type: "openai-compatible"; baseUrl: string; model: string; apiKeyConfigured: boolean };

export interface AnalyticsSettings {
  jev: { configured: boolean; source: "environment" | "secure-storage" | null; status: string };
  defaultPayloadMode: PayloadMode;
  recommendationProvider: RecommendationProvider;
}

export interface EfficiencyAction {
  index: number;
  tool: string;
  category: string;
  summary: string;
  durationMs: number;
  error: boolean;
  repeatedSimilarCallCount: number;
}

export interface EfficiencyInput {
  task: { firstUserMessage: string; turns: number; durationMs: number; totalTokens: number };
  actions: EfficiencyAction[];
  signals: {
    toolCalls: number;
    failedToolCalls: number;
    repeatedToolCalls: number;
    subagentCount: number;
    contextGrowth: number;
  };
  selectedExcerpts: string[];
}

export interface PreparedEfficiencyPayload {
  sessionId: string;
  sessionPath: string;
  transcriptFingerprint: string;
  payloadMode: PayloadMode;
  destination: {
    provider: string;
    endpoint: string;
    model: string;
  };
  input: EfficiencyInput;
}

export type EfficiencyFindingType =
  | "repeated-work"
  | "thrashing"
  | "excessive-exploration"
  | "failed-retries"
  | "recovery"
  | "useful-subagent";

export interface EfficiencyFinding {
  type: EfficiencyFindingType;
  probability: number;
  startMessageIndex: number;
  endMessageIndex: number;
  activitySummary?: string;
}

export interface JevEfficiencyDecisions {
  progressingEfficiently: number;
  toolCallsUseful: number;
  redundantWorkPresent: number;
  excessiveExploration: number;
  likelyThrashing: number;
  effectiveRecovery: number;
  tokenUsageEfficient: number;
  subagentsUseful: number;
  likelyTaskCompleted: number;
}

export interface EfficiencyMetricEvaluation {
  key: string;
  label: string;
  question: string;
  higherProbabilityIsBetter: boolean;
  probability: number;
  score: number;
}

export interface SessionEfficiencyAnalysis {
  sessionId: string;
  sessionPath: string;
  score: number;
  dimensions: {
    progress: number;
    toolUse: number;
    focus: number;
    exploration: number;
    recovery: number;
    tokenUse: number;
  };
  metricEvaluations: EfficiencyMetricEvaluation[];
  findings: EfficiencyFinding[];
  decisions: JevEfficiencyDecisions;
  analyzedAt: string;
  analyzedTurns: number;
  transcriptFingerprint: string;
  analysisVersion: number;
  decisionSetVersion: number;
  scoreFormulaVersion: number;
  stale: boolean;
}

export type EfficiencyJobStatus =
  | "queued"
  | "preparing"
  | "redacting"
  | "sending"
  | "analysing"
  | "processing_result"
  | "completed"
  | "failed"
  | "cancelled";

export interface EfficiencyAnalysisJob {
  analysisId: string;
  sessionId: string;
  sessionPath: string;
  sessionName: string;
  status: EfficiencyJobStatus;
  progress?: number | null;
  message: string;
  updatedAt: string;
  error?: string | null;
  score?: number | null;
}

export interface EfficiencySummary {
  sessionPath: string;
  score: number;
  analyzedAt: string;
  stale: boolean;
}
