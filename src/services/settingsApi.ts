import { invokeCommand } from "./invoke";
import type { AppSettings, SettingsPatch } from "../types/generated";

export function settingsGet(): Promise<AppSettings> {
  return invokeCommand("settings_get");
}

export function settingsUpdate(patch: SettingsPatch): Promise<AppSettings> {
  return invokeCommand("settings_update", { patch });
}
