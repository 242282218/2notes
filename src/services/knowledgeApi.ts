import { invokeCommand } from "./invoke";
import type {
  EntryDetail,
  KnowledgeIndexReport,
  KnowledgeRelations,
  KnowledgeSuggestion,
} from "../types/generated";

export function knowledgeSuggest(
  query: string,
  limit = 10,
): Promise<KnowledgeSuggestion[]> {
  return invokeCommand("knowledge_suggest", { query, limit });
}

export function knowledgeRelationsGet(id: string): Promise<KnowledgeRelations> {
  return invokeCommand("knowledge_relations_get", { id });
}

export function knowledgeRebuildIndex(): Promise<KnowledgeIndexReport> {
  return invokeCommand("knowledge_rebuild_index");
}

export function knowledgePromote(
  id: string,
  expectedRevision: number,
): Promise<EntryDetail> {
  return invokeCommand("knowledge_promote", { id, expectedRevision });
}

export function knowledgeDemote(
  id: string,
  expectedRevision: number,
): Promise<EntryDetail> {
  return invokeCommand("knowledge_demote", { id, expectedRevision });
}
