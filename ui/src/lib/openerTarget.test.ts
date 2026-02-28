import { describe, expect, it } from "vitest";
import {
  getOpenerTargetExtensions,
  getOpenerTargetLabel,
  normalizeOpenerTarget,
} from "./openerTarget";

describe("opener target normalization", () => {
  it("normalizes ext targets with dotted lowercase extensions", () => {
    expect(normalizeOpenerTarget("ext: png, .JPG, gif, png")).toBe(
      "ext:.gif,.jpg,.png",
    );
  });

  it("normalizes folder target casing", () => {
    expect(normalizeOpenerTarget(" Folder ")).toBe("folder");
  });

  it("returns normalized extension list for editing", () => {
    expect(getOpenerTargetExtensions("ext:png,.JPG")).toBe(".jpg,.png");
  });

  it("returns folder label for folder targets", () => {
    expect(getOpenerTargetLabel("folder")).toBe("フォルダ");
  });
});
