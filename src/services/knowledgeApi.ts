import { invokeCommand } from "./invoke";
import type {
  EntryDetail,
  EntryBreadcrumb,
  EntryTreeNode,
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

export function knowledgeTreeGet(): Promise<EntryTreeNode[]> {
  return invokeCommand("knowledge_tree_get");
}

export function knowledgeBreadcrumbsGet(
  id: string,
): Promise<EntryBreadcrumb[]> {
  return invokeCommand("knowledge_breadcrumbs_get", { id });
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

export function knowledgeMove(
  id: string,
  parentEntryId: string | null,
  siblingOrder: number,
  expectedRevision: number,
): Promise<EntryDetail> {
  return invokeCommand("knowledge_move", {
    id,
    parentEntryId,
    siblingOrder,
    expectedRevision,
  });
}

export function knowledgeDemote(
  id: string,
  expectedRevision: number,
): Promise<EntryDetail> {
  return invokeCommand("knowledge_demote", { id, expectedRevision });
}
