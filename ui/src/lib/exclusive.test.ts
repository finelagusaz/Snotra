import { describe, expect, it } from "vitest";
import { createExclusive } from "./exclusive";

/** await 中の task を外部から解放できる gate。single-flight の in-flight 状態を制御するために使う。 */
function makeGate(): { gate: Promise<void>; release: () => void } {
  let release!: () => void;
  const gate = new Promise<void>((r) => (release = r));
  return { gate, release };
}

describe("createExclusive", () => {
  it("非 blocked 時は task の戻り値をそのまま返す", async () => {
    const lane = createExclusive();
    expect(await lane(async () => 42)).toBe(42);
  });

  it("in-flight 中の 2 回目は undefined を返し、task を起動しない", async () => {
    const lane = createExclusive();
    const { gate, release } = makeGate();
    let secondStarted = false;

    const first = lane(async () => {
      await gate; // in-flight のまま滞留
      return "first";
    });
    const second = await lane(async () => {
      secondStarted = true;
      return "second";
    });

    expect(second).toBeUndefined();
    expect(secondStarted).toBe(false); // 拒否時は task を呼ばない

    release();
    expect(await first).toBe("first");
  });

  it("task 完了後は解放され、再度実行できる（連続 2 回が両方走る）", async () => {
    const lane = createExclusive();
    const r1 = await lane(async () => "a");
    const r2 = await lane(async () => "b");
    expect(r1).toBe("a");
    expect(r2).toBe("b");
  });

  it("task が reject しても finally で解放され、reject は伝播する", async () => {
    const lane = createExclusive();
    await expect(
      lane(async () => {
        throw new Error("boom");
      }),
    ).rejects.toThrow("boom");
    // 解放されているので次が受理される（戻せない状態に固まらない）
    expect(await lane(async () => "ok")).toBe("ok");
  });

  it("task を同期起動する（同期プレフィックスが呼び出し tick で走る）", async () => {
    const lane = createExclusive();
    const { gate, release } = makeGate();
    let syncRan = false;

    // task の同期プレフィックスで syncRan を立て、その後 await で滞留する。
    const p = lane(async () => {
      syncRan = true;
      await gate;
      return "done";
    });

    // await を挟まずに確認: task の同期部が lane() 呼び出しと同一 tick で走っている
    // （preGen 捕捉・selected() 読みが現行と同 tick で走る不変条件の担保）。
    expect(syncRan).toBe(true);

    release();
    expect(await p).toBe("done");
  });

  it("同期 throw する非 async callback でも解放し、reject を伝播する", async () => {
    const lane = createExclusive();
    // 呼んだ瞬間に throw する callback。primitive の try/finally が inFlight を戻し、
    // 例外は reject 済み promise として伝播すること。
    await expect(
      lane(() => {
        throw new Error("sync boom");
      }),
    ).rejects.toThrow("sync boom");
    // 解放されている
    expect(await lane(async () => "ok")).toBe("ok");
  });
});
