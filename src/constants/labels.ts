import type { EntryStatus, EntryType } from "../types/generated";
import type { AppView } from "../app/routes";

/** Display labels for entry types and statuses. Kept in a shared file so
 * components can render human-readable labels without duplicating maps. */

export const TYPE_LABELS: Readonly<Record<EntryType, string>> = Object.freeze({
  unclear: "未澄清",
  idea: "想法",
  task: "任务",
  material: "素材",
  question: "问题",
});

export const STATUS_LABELS: Readonly<Record<EntryStatus, string>> =
  Object.freeze({
    pending: "待处理",
    done: "已完成",
    archived: "已归档",
  });

export const VIEW_LABELS: Readonly<Record<AppView, string>> = Object.freeze({
  inbox: "收集箱",
  knowledge: "知识库",
  health: "知识健康",
  search: "搜索",
  tags: "标签",
  trash: "回收站",
  settings: "设置",
});

/** Type options including an empty "all types" sentinel for filters. */
export const TYPE_OPTIONS: Readonly<
  Array<{ value: EntryType | ""; label: string }>
> = Object.freeze([
  { value: "", label: "全部类型" },
  ...(Object.keys(TYPE_LABELS) as EntryType[]).map((value) => ({
    value,
    label: TYPE_LABELS[value],
  })),
]);

/** Status options including an empty "all statuses" sentinel for filters. */
export const STATUS_OPTIONS: Readonly<
  Array<{ value: EntryStatus | ""; label: string }>
> = Object.freeze([
  { value: "", label: "全部状态" },
  ...(Object.keys(STATUS_LABELS) as EntryStatus[]).map((value) => ({
    value,
    label: STATUS_LABELS[value],
  })),
]);

export const KNOWLEDGE_LABEL = "知识";
