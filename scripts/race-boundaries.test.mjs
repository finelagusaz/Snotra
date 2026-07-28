// race-boundaries.mjs の分類関数を、フォールトインジェクションフィクスチャ（赤）と正常フィクスチャ（緑）の
// 両方向で検証する。各フィクスチャは「守りたい対象 1 件が入力に現れること」と
// 「判定対象外が入力に混じらないこと」の入力集合検算を兼ねる（.claude/rules/safety-nets.md）。
//
// 赤フィクスチャの出所は #805 の着手前ゲートで実際に測った差分である——
// 「文書の散文」「コメント中の語」はどちらも実リポジトリで誤検出を出した形をそのまま写した。
import { describe, it, expect } from "vitest";
import {
  KINDS,
  SOURCE_EXT,
  isCommentLine,
  parseDiff,
  untrackedRows,
  selectPopulation,
  classify,
  formatReport,
  parseArgs,
} from "./race-boundaries.mjs";

const kindOf = (kinds, ord) => kinds.find((k) => k.ord === ord);
const run = (rows, opts) => classify(selectPopulation(rows, opts));

describe("種別表の不変条件（序数は書き換え不可・ADR-0013 却下 4）", () => {
  it("①〜⑧ が順に 8 件そろう", () => {
    expect(KINDS.map((k) => k.ord)).toEqual(["①", "②", "③", "④", "⑤", "⑥", "⑦", "⑧"]);
  });

  it("⑥ だけが独立した grep を持たない（① の部分集合ゆえ原理的に書けない）", () => {
    expect(KINDS.filter((k) => k.patterns === null).map((k) => k.ord)).toEqual(["⑥"]);
  });
});

describe("isCommentLine（#805 欠陥 2 — コメント中の語を境界と数えない）", () => {
  const cases = [
    // [入力, コメントか]
    ["    /// 次の dispatch 予定時刻（契約②・#737）", true], // 5b5aa8e の実例
    ["    // 前回 dispatch + min_interval（契約②の gate）。", true],
    ["/*! module doc */", true],
    ["     * 継続行", true],
    ["# PowerShell のコメント", true],
    ["        let _ = self.sender.send(SchedulerMessage::Request {", false],
    ["    *ptr = value;", false], // 逆参照の代入をコメントと誤らない
    ["    clicked: std::sync::Mutex<Option<(u64, usize)>>,", false],
  ];
  for (const [text, expected] of cases) {
    it(`${expected ? "コメント" : "コード"}: ${text.trim().slice(0, 40)}`, () => {
      expect(isCommentLine(text)).toBe(expected);
    });
  }
});

describe("parseDiff", () => {
  const diff = [
    "diff --git a/src-tauri/src/a.rs b/src-tauri/src/a.rs",
    "--- a/src-tauri/src/a.rs",
    "+++ b/src-tauri/src/a.rs",
    "@@ -1,0 +2 @@",
    "+    let (tx, rx) = channel();",
    "-    let old = 1;",
  ].join("\n");

  it("追加行と削除行をファイル名つきで拾う", () => {
    expect(parseDiff(diff)).toEqual([
      { file: "src-tauri/src/a.rs", sign: "+", text: "    let (tx, rx) = channel();" },
      { file: "src-tauri/src/a.rs", sign: "-", text: "    let old = 1;" },
    ]);
  });

  it("`--- a/...` ヘッダを削除行と誤らない（接頭辞 `-` の衝突）", () => {
    expect(parseDiff(diff).some((r) => r.text.includes("a/src-tauri"))).toBe(false);
  });

  it("削除されたファイル（+++ /dev/null）の行を母集団へ入れない", () => {
    const gone = ["--- a/src-tauri/src/b.rs", "+++ /dev/null", "@@ -1 +0,0 @@", "-    thread::spawn(|| {});"].join("\n");
    expect(parseDiff(gone)).toEqual([]);
  });
});

describe("selectPopulation（母集団のフィルタ）", () => {
  const rows = [
    { file: "docs/superpowers/plans/x.md", sign: "+", text: "- 遅延 dispatch を request_repaint_after で置き換える" },
    { file: "snotra-egui-runtime/src/repaint.rs", sign: "+", text: "    /// worker が dispatch のたび読む値。" },
    { file: "snotra-egui-runtime/src/repaint.rs", sign: "+", text: "    sender: Sender<SchedulerMessage>," },
    { file: "snotra-egui-runtime/src/repaint.rs", sign: "-", text: "    old_sender: Sender<Msg>," },
  ];

  it("#805 欠陥 1 — 文書の散文を母集団へ入れない", () => {
    const kinds = run(rows);
    expect(kindOf(kinds, "⑧").hits).toHaveLength(0);
  });

  it("#805 欠陥 2 — コメント行を母集団へ入れない", () => {
    // repaint.rs の `/// ... dispatch ...` もコード行ではないので ⑧ は 0 のまま
    expect(kindOf(run(rows), "⑧").hits).toHaveLength(0);
  });

  it("守りたい対象（実コードの Sender）は残る", () => {
    const hits = kindOf(run(rows), "②").hits;
    expect(hits).toHaveLength(1);
    expect(hits[0].text).toContain("sender: Sender<SchedulerMessage>");
  });

  it("削除行は既定で入らず、--include-removed で入る", () => {
    expect(kindOf(run(rows), "②").hits).toHaveLength(1);
    expect(kindOf(run(rows, { includeRemoved: true }), "②").hits).toHaveLength(2);
  });

  it("SOURCE_EXT は .md にマッチせず .rs にマッチする", () => {
    expect(SOURCE_EXT.test("docs/a.md")).toBe(false);
    expect(SOURCE_EXT.test("src-tauri/src/a.rs")).toBe(true);
  });
});

describe("classify（種別ごとの検出）", () => {
  const line = (text) => ({ file: "src-tauri/src/a.rs", sign: "+", text });

  const redCases = [
    ["①", "    thread::spawn(move || loop {"],
    ["②", "        let _ = self.sender.send(SchedulerMessage::Request {"],
    ["③", "        while let Ok(msg) = rx.try_recv() {"],
    ["④", "    clicked: std::sync::Mutex<Option<(u64, usize)>>,"],
    ["⑤", "    app.listen(\"config-changed\", move |_| {"],
    ["⑦", "    let out = fut.await;"],
    ["⑧", "    ctx.request_repaint_after(Duration::from_millis(16));"],
  ];
  for (const [ord, text] of redCases) {
    it(`${ord} が検出される: ${text.trim().slice(0, 44)}`, () => {
      expect(kindOf(run([line(text)]), ord).hits).toHaveLength(1);
    });
  }

  it("1 行が複数種別に当たるならすべてに立つ（SKILL「種別ごとに別に立てる」）", () => {
    const kinds = run([line("    let tx: Sender<M> = make(); tx.send(m); rx.try_recv();")]);
    expect(kindOf(kinds, "②").hits).toHaveLength(1);
    expect(kindOf(kinds, "③").hits).toHaveLength(1);
  });

  it("並行と無縁のコードは 1 種別も立てない（判定対象外の不混入）", () => {
    const kinds = run([line("    let width = rect.width() * scale;"), line("    fn label(&self) -> &str { &self.name }")]);
    expect(kinds.every((k) => k.hits.length === 0)).toBe(true);
  });
});

describe("formatReport（#800 — 費用が結論に依存しない）", () => {
  const meta = { base: "main", mergeBase: "abcdef1234", changedFiles: 1, untrackedFiles: 0, populationLines: 1, includeRemoved: false };

  it("境界が 1 件でも、全 8 種が件数つきで現れる", () => {
    const text = formatReport(run([{ file: "a.rs", sign: "+", text: "thread::spawn(|| {});" }]), meta);
    for (const k of KINDS) expect(text).toContain(`${k.ord} ${k.name}`);
    expect(text).toContain("① worker の spawn: 1 件");
    expect(text).toContain("⑤ Tauri listener / emit: 0 件");
  });

  it("境界が 0 件のときも同じ 8 行が出る（0 側と 1 側で様式が変わらない）", () => {
    const zero = formatReport(run([{ file: "a.rs", sign: "+", text: "let x = 1;" }]), meta);
    for (const k of KINDS) expect(zero).toContain(`${k.ord} ${k.name}`);
  });

  it("0 件の種別もパターンを印字する（「打たなかった」と「打って 0 件」を区別する）", () => {
    const text = formatReport(run([]), meta);
    expect(text).toContain("[パターン: \\.await]");
  });

  it("⑥ は grep が無いことを明示する（空欄で沈黙しない）", () => {
    expect(formatReport(run([]), meta)).toContain("① のヒットを送信の有無で分類");
  });
});

describe("parseArgs（--base の必須化）", () => {
  it("--base を取る", () => {
    expect(parseArgs(["--base", "main"])).toEqual({ base: "main", includeRemoved: false });
  });

  it("--base が無ければ null（呼び出し側が使用法 + exit 2 にする）", () => {
    expect(parseArgs([]).base).toBeNull();
    expect(parseArgs(["--include-removed"]).base).toBeNull();
  });

  it("--include-removed を取る", () => {
    expect(parseArgs(["--base", "main", "--include-removed"]).includeRemoved).toBe(true);
  });
});

describe("untrackedRows（未追跡ファイルは中身が対象）", () => {
  it("全行を追加行として入れる", () => {
    const rows = untrackedRows(["new.rs"], () => "thread::spawn(|| {});\nlet x = 1;");
    expect(rows).toHaveLength(2);
    expect(kindOf(classify(selectPopulation(rows)), "①").hits).toHaveLength(1);
  });

  it("読めないファイルは黙って落とさず、単に行を生まない", () => {
    expect(untrackedRows(["gone.rs"], () => null)).toEqual([]);
  });
});
