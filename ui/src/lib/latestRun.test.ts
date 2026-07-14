import { describe, expect, it } from "vitest";
import { createLatestRun } from "./latestRun";

/** await 中の task を外部から解放できる gate。supersede の順序を制御するために使う。 */
function makeGate(): { gate: Promise<void>; release: () => void } {
  let release!: () => void;
  const gate = new Promise<void>((r) => (release = r));
  return { gate, release };
}

describe("createLatestRun", () => {
  it("current() は 0 から始まり run/invalidate で +1 単調に進む", () => {
    const lane = createLatestRun();
    expect(lane.current()).toBe(0);
    expect(lane.invalidate()).toBe(1);
    expect(lane.current()).toBe(1);
    void lane.run(async () => {});
    expect(lane.current()).toBe(2);
  });

  it("run 中の requestId は current() と一致する（perf 相関の基礎）", async () => {
    const lane = createLatestRun();
    let capturedId = -1;
    let capturedCurrent = -1;
    await lane.run(async ({ requestId }) => {
      capturedId = requestId;
      capturedCurrent = lane.current();
    });
    expect(capturedId).toBe(1);
    expect(capturedCurrent).toBe(1);
  });

  it("最新の run の isStale() は false のまま", async () => {
    const lane = createLatestRun();
    const stale = await lane.run(async ({ isStale }) => isStale());
    expect(stale).toBe(false);
  });

  it("後続の run が挟まると先行 task の isStale() が true になる（supersede）", async () => {
    const lane = createLatestRun();
    let firstStaleAfter: boolean | undefined;
    const { gate, release } = makeGate();

    const first = lane.run(async ({ isStale }) => {
      await gate; // await 中に後続 run が世代を進める
      firstStaleAfter = isStale();
    });

    void lane.run(async () => {});
    release();
    await first;

    expect(firstStaleAfter).toBe(true);
  });

  it("run 中の invalidate() で isStale() が true 化する（モード遷移・起動相当）", async () => {
    const lane = createLatestRun();
    let staleAfter: boolean | undefined;
    const { gate, release } = makeGate();

    const p = lane.run(async ({ isStale }) => {
      await gate;
      staleAfter = isStale();
    });

    lane.invalidate();
    release();
    await p;

    expect(staleAfter).toBe(true);
  });

  it("async task が throw すると run() の戻り promise が reject する", async () => {
    const lane = createLatestRun();
    await expect(
      lane.run(async () => {
        throw new Error("boom");
      }),
    ).rejects.toThrow("boom");
  });

  it("同期 throw する非 async callback でも run() は同期 throw せず reject 済み promise を返す", async () => {
    const lane = createLatestRun();
    // 非 async callback（呼んだ瞬間に throw）。run() が同期 throw すると呼び出し側の
    // `void run(...)` や `run(...).catch(...)` が壊れる。Promise.reject へ正規化されること。
    const p = lane.run(() => {
      throw new Error("sync boom");
    });
    expect(p).toBeInstanceOf(Promise);
    await expect(p).rejects.toThrow("sync boom");
    // 世代は進んでいる（run 開始時に bump 済み）
    expect(lane.current()).toBe(1);
  });
});
