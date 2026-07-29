import { invokeCommand } from "./invoke";
import type {
  HealthIssueKind,
  HealthIssuePage,
  KnowledgeHealthSummary,
  PageRequest,
} from "../types/generated";

export function knowledgeHealthSummaryGet(): Promise<KnowledgeHealthSummary> {
  return invokeCommand("knowledge_health_summary_get");
}

export function knowledgeHealthIssuesGet(
  kind: HealthIssueKind,
  page: PageRequest,
): Promise<HealthIssuePage> {
  return invokeCommand("knowledge_health_issues_get", { kind, page });
}
