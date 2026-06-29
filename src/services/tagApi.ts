import { invokeCommand } from "./invoke";
import type { Tag } from "../types/generated";

export function tagsSuggest(query: string): Promise<Tag[]> {
  return invokeCommand("tags_suggest", { query });
}

export function tagsList(): Promise<Tag[]> {
  return invokeCommand("tags_list");
}
