import { describe, expect, it } from "vitest";

import { getSaveStateLabel } from "./useSaveState";

describe("getSaveStateLabel", () => {
  it("returns Chinese labels for each save state", () => {
    expect(getSaveStateLabel("dirty")).toBe("未保存");
    expect(getSaveStateLabel("saving")).toBe("保存中");
    expect(getSaveStateLabel("saved")).toBe("已保存");
    expect(getSaveStateLabel("failed")).toBe("保存失败");
    expect(getSaveStateLabel("idle")).toBe("空闲");
  });
});
