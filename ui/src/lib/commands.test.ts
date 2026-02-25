import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "./invoke";
import { findCommand, initCommands } from "./commands";

vi.mock("./invoke", () => ({
  openAbout: vi.fn(async () => {}),
  openSettings: vi.fn(async () => {}),
  rebuildIndex: vi.fn(async () => true),
  quitApp: vi.fn(async () => {}),
}));

describe("slash command actions", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("/a calls hide before openAbout", async () => {
    const order: string[] = [];
    initCommands(async () => {
      order.push("hide");
    });
    vi.mocked(api.openAbout).mockImplementation(async () => {
      order.push("openAbout");
    });

    const cmd = findCommand("/a");
    expect(cmd).toBeDefined();
    await cmd!.action();

    expect(order).toEqual(["hide", "openAbout"]);
    expect(api.openAbout).toHaveBeenCalledTimes(1);
  });

  it("/o calls hide before openSettings", async () => {
    const order: string[] = [];
    initCommands(async () => {
      order.push("hide");
    });
    vi.mocked(api.openSettings).mockImplementation(async () => {
      order.push("openSettings");
    });

    const cmd = findCommand("/o");
    expect(cmd).toBeDefined();
    await cmd!.action();

    expect(order).toEqual(["hide", "openSettings"]);
    expect(api.openSettings).toHaveBeenCalledTimes(1);
  });

  it("/s calls hide before rebuildIndex", async () => {
    const order: string[] = [];
    initCommands(async () => {
      order.push("hide");
    });
    vi.mocked(api.rebuildIndex).mockImplementation(async () => {
      order.push("rebuildIndex");
      return true;
    });

    const cmd = findCommand("/s");
    expect(cmd).toBeDefined();
    await cmd!.action();

    expect(order).toEqual(["hide", "rebuildIndex"]);
    expect(api.rebuildIndex).toHaveBeenCalledTimes(1);
  });

  it("/q does not call hide and calls quitApp", async () => {
    const hide = vi.fn(async () => {});
    initCommands(hide);

    const cmd = findCommand("/q");
    expect(cmd).toBeDefined();
    await cmd!.action();

    expect(hide).not.toHaveBeenCalled();
    expect(api.quitApp).toHaveBeenCalledTimes(1);
  });
});
