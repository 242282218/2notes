/** Display labels for entry types and statuses. Kept in a shared file so
 * components can render human-readable labels without duplicating maps. */

export const TYPE_LABELS: Readonly<Record<string, string>> = Object.freeze({
  unclear: "未澄清",
  idea: "想法",
  task: "任务",
  material: "素材",
  question: "问题",
});

export const STATUS_LABELS: Readonly<Record<string, string>> = Object.freeze({
  pending: "待处理",
  done: "已完成",
  archived: "已归档",
});
