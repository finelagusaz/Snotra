import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createOwnedTimer } from "./ownedTimer";

describe("createOwnedTimer", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.clearAllTimers();
    vi.useRealTimers();
  });

  it("arm すると ms 後に fn が 1 回呼ばれる（trailing）", () => {
    const t = createOwnedTimer(50);
    const fn = vi.fn();
    t.arm(fn);
    vi.advanceTimersByTime(49);
    expect(fn).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it("arm は fn を同期発火しない（再入安全の構造的証跡）", () => {
    const t = createOwnedTimer(50);
    let ran = false;
    // advance を挟まずに確認: fn は setTimeout の中でしか呼ばれない（同期発火なし）。
    // これが「arm() 実行中への同期再入が起こらない（＝再入禁止契約が不要）」ことの担保。
    t.arm(() => {
      ran = true;
    });
    expect(ran).toBe(false);
  });

  it("fn 内で再 arm しても新タイマーが正しく生きる（再入は安全）", () => {
    const t = createOwnedTimer(50);
    const g = vi.fn();
    // fn は別マクロタスクで走り、発火時に timer は先に undefined 化されている。
    // ゆえに fn 内の arm(g) は orphan にならず、新タイマーとして 1 回だけ発火する。
    t.arm(() => {
      t.arm(g);
    });
    vi.advanceTimersByTime(50); // 外側 fn 発火 → 内側で g を arm
    expect(g).not.toHaveBeenCalled(); // g はまだ（arm 直後）
    expect(t.isPending()).toBe(true); // g のタイマーが生きている
    vi.advanceTimersByTime(50); // g の arm から 50ms
    expect(g).toHaveBeenCalledTimes(1);
    expect(t.isPending()).toBe(false);
  });

  it("burst（連続 arm）では最後の 1 回だけ発火する", () => {
    const t = createOwnedTimer(50);
    const a = vi.fn();
    const b = vi.fn();
    t.arm(a);
    vi.advanceTimersByTime(30); // 50ms 未満で再 arm
    t.arm(b); // 前回タイマーを破棄して張り直す
    vi.advanceTimersByTime(49);
    expect(a).not.toHaveBeenCalled();
    expect(b).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1); // b の arm から 50ms
    expect(a).not.toHaveBeenCalled(); // 破棄されたので不発
    expect(b).toHaveBeenCalledTimes(1);
  });

  it("cancel すると ms 経過しても発火しない", () => {
    const t = createOwnedTimer(50);
    const fn = vi.fn();
    t.arm(fn);
    t.cancel();
    vi.advanceTimersByTime(100);
    expect(fn).not.toHaveBeenCalled();
  });

  it("二重 cancel / 未 arm での cancel は安全（冪等）", () => {
    const t = createOwnedTimer(50);
    const fn = vi.fn();
    expect(() => t.cancel()).not.toThrow(); // 未 arm でも安全
    t.arm(fn);
    t.cancel();
    expect(() => t.cancel()).not.toThrow(); // 二重 cancel も安全
    vi.advanceTimersByTime(100);
    expect(fn).not.toHaveBeenCalled();
  });

  it("isPending は false → arm 後 true → 発火後 false → cancel 後 false", () => {
    const t = createOwnedTimer(50);
    expect(t.isPending()).toBe(false);
    t.arm(() => {});
    expect(t.isPending()).toBe(true);
    vi.advanceTimersByTime(50);
    expect(t.isPending()).toBe(false); // trailing 発火後
    t.arm(() => {});
    expect(t.isPending()).toBe(true);
    t.cancel();
    expect(t.isPending()).toBe(false); // cancel 後
  });

  it("fn 実行中に isPending() は false（timer を fn の前に undefined 化）", () => {
    const t = createOwnedTimer(50);
    let pendingDuringFire: boolean | undefined;
    // flush 経路（保留なら cancel して即 run）の二重発火防止の根拠:
    // trailing 発火中は「もう保留していない」＝ isPending() false であること。
    t.arm(() => {
      pendingDuringFire = t.isPending();
    });
    vi.advanceTimersByTime(50);
    expect(pendingDuringFire).toBe(false);
  });

  it("msOverride を渡すと生成時の ms ではなく override 側の ms で発火する", () => {
    const t = createOwnedTimer(50);
    const fn = vi.fn();
    t.arm(fn, 200);
    vi.advanceTimersByTime(50); // 生成時 ms（50）が経過しても発火しない
    expect(fn).not.toHaveBeenCalled();
    vi.advanceTimersByTime(150); // override（200）分の経過で発火
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it("msOverride 省略時は生成時の ms が使われる", () => {
    const t = createOwnedTimer(50);
    const fn = vi.fn();
    t.arm(fn);
    vi.advanceTimersByTime(49);
    expect(fn).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it("異なる msOverride で連続 arm すると前回の保留を破棄し新しい override の ms で発火する", () => {
    const t = createOwnedTimer(50);
    const a = vi.fn();
    const b = vi.fn();
    t.arm(a, 100);
    vi.advanceTimersByTime(30); // a の override（100ms）未満で再 arm
    t.arm(b, 10); // 前回タイマーを破棄して張り直す
    vi.advanceTimersByTime(9);
    expect(b).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1); // b の arm から 10ms
    expect(b).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(100);
    expect(a).not.toHaveBeenCalled(); // 破棄されたので不発
  });

  it("msOverride に 0 を渡すと 0ms で発火する（?? は 0 をフォールバックしない）", () => {
    const t = createOwnedTimer(50);
    const fn = vi.fn();
    t.arm(fn, 0);
    vi.advanceTimersByTime(0);
    expect(fn).toHaveBeenCalledTimes(1);
  });
});
