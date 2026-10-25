// race-boundaries.mjs の分類関数を、フォールトインジェクションフィクスチャ（赤）と正常フィクスチャ（緑）の
// 両方向で検証する。各フィクスチャは「守りたい対象 1 件が入力に現れること」と
// 「判定対象外が入力に混じらないこと」の入力集合検算を兼ねる（.claude/rules/safety-nets.md）。
//
// 赤フィクスチャの出所は #805 の着手前ゲートで実際に測った差分である——
// 「文書の散文」「コメント中の語」はどちらも実リポジトリで誤検出を出した形をそのまま写した。
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
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
  listUntracked,
  readerFor,
  parseNumstatZ,
  diffMismatches,
  readTrackedRows,
  DiffShapeError,
  deletedBinaryPaths,
} from "./race-boundaries.mjs";
import { execFileSync, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const kindOf = (kinds, ord) => kinds.find((k) => k.ord === ord);
const run = (rows, opts) => classify(selectPopulation(rows, opts));

describe("種別表の不変条件（序数は書き換え不可・ADR-race-check-simplification 却下 4）", () => {
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
    "@@ -1 +2 @@", // 1 行消して 1 行足す（以前の `-1,0` は `-` 行を持てない、git が出さない形だった）
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

  it("hunk の本文が `++ ` / `-- ` で始まってもヘッダと誤らない", () => {
    const d = ["+++ b/x.rs", "@@ -1 +1,2 @@", "--- old", "+++ evil", "+thread::spawn(|| {});"].join("\n");
    expect(parseDiff(d)).toEqual([
      { file: "x.rs", sign: "-", text: "-- old" },
      { file: "x.rs", sign: "+", text: "++ evil" },
      { file: "x.rs", sign: "+", text: "thread::spawn(|| {});" },
    ]);
  });

  it("context 行は宣言の両側から 1 行ずつ減らし、本文の行にしない", () => {
    const d = ["+++ b/x.rs", "@@ -1,2 +1,2 @@", " keep", "-old", "+new", "+++ b/y.rs", "@@ -0,0 +1 @@", "+b"].join("\n");
    expect(parseDiff(d).map((r) => `${r.file}:${r.sign}${r.text}`)).toEqual(["x.rs:-old", "x.rs:+new", "y.rs:+b"]);
  });

  it("hunk の中の `\\ No newline at end of file` は止めずに読み飛ばす", () => {
    const d = ["+++ b/x.rs", "@@ -1 +1 @@", "-a", "\\ No newline at end of file", "+b", "\\ No newline at end of file"].join("\n");
    expect(parseDiff(d).map((r) => `${r.sign}${r.text}`)).toEqual(["-a", "+b"]);
  });

  it("宣言より本文が短い壊れた hunk は、次のファイルのヘッダを本文にせず止める", () => {
    const d = ["+++ b/x.rs", "@@ -0,0 +1,3 @@", "+a", "diff --git a/y.rs b/y.rs", "+++ b/y.rs"].join("\n");
    expect(() => parseDiff(d)).toThrow(DiffShapeError);
  });

  it("hunk の宣言を越えた行は本文として読まない", () => {
    const d = ["+++ b/x.rs", "@@ -0,0 +1 @@", "+a", "\\ No newline at end of file", "+++ b/y.rs", "@@ -0,0 +1 @@", "+b"].join("\n");
    expect(parseDiff(d).map((r) => `${r.file}:${r.text}`)).toEqual(["x.rs:a", "y.rs:b"]);
  });

  it("削除されたファイル（+++ /dev/null）の行は旧パスで残し、入れるかは selectPopulation が決める", () => {
    const gone = ["--- a/src-tauri/src/b.rs", "+++ /dev/null", "@@ -1 +0,0 @@", "-    thread::spawn(|| {});"].join("\n");
    const rows = parseDiff(gone);
    expect(rows).toEqual([{ file: "src-tauri/src/b.rs", sign: "-", text: "    thread::spawn(|| {});" }]);
    expect(kindOf(run(rows), "①").hits).toHaveLength(0);
    expect(kindOf(run(rows, { includeRemoved: true }), "①").hits).toHaveLength(1);
  });

  it("新規ファイル（--- /dev/null）の後に削除されたファイルが来ても、旧パスを持ち越さない", () => {
    const d = ["--- /dev/null", "+++ b/new.rs", "@@ -0,0 +1 @@", "+a", "--- a/old.rs", "+++ /dev/null", "@@ -1 +0,0 @@", "-b"].join("\n");
    expect(parseDiff(d).map((r) => `${r.file}:${r.sign}${r.text}`)).toEqual(["new.rs:+a", "old.rs:-b"]);
  });
});

describe("deletedBinaryPaths（削除されたバイナリは patch の 1 行だけが名指す）", () => {
  it("削除されたバイナリだけを拾い、変更されたバイナリは拾わない", () => {
    const d = ["Binary files a/keep.ts and b/keep.ts differ", "Binary files a/v.ts and /dev/null differ"].join("\n");
    expect([...deletedBinaryPaths(d)]).toEqual(["v.ts"]);
  });
});

describe("parseNumstatZ（`diff-index -M --numstat -z` の形は一時リポジトリで実測した）", () => {
  it("通常・rename・バイナリを新しいパスで引く", () => {
    const out = "2\t3\tplain.rs\u0000" + "1\t0\t\u0000old.rs\u0000new.rs\u0000" + "-\t-\tb.rs\u0000" + "1\t0\t既存.rs\u0000";
    expect([...parseNumstatZ(out)]).toEqual([
      ["plain.rs", { add: 2, del: 3 }],
      ["new.rs", { add: 1, del: 0 }],
      ["b.rs", null],
      ["既存.rs", { add: 1, del: 0 }],
    ]);
  });

  it("空の出力は空の Map", () => {
    expect(parseNumstatZ("").size).toBe(0);
  });

  it("形の崩れた項目は黙って捨てずに throw する", () => {
    // DiffShapeError で投げる——素の Error だと main が exit 3 でなくスタックトレースで落ちた（セルフレビュー）
    expect(() => parseNumstatZ("garbage\u0000")).toThrow(DiffShapeError);
    expect(() => parseNumstatZ("1\t0\t\u0000old.rs\u0000")).toThrow(DiffShapeError); // rename の新しいパスが欠けた
  });
});

describe("diffMismatches（patch と numstat を別経路で照合する）", () => {
  const add = (file, n) => Array.from({ length: n }, () => ({ file, sign: "+", text: "x" }));
  const del = (file, n) => Array.from({ length: n }, () => ({ file, sign: "-", text: "y" }));
  const ns = (add, del = 0) => ({ add, del });

  it("名前と行数がそろえば空（削除されたバイナリ・ソースでないバイナリは問わない）", () => {
    const rows = [...add("a.rs", 2), ...del("a.rs", 1), ...add("既存.rs", 1), ...del("gone.rs", 4)];
    const numstat = new Map([["a.rs", ns(2, 1)], ["既存.rs", ns(1)], ["gone.rs", ns(0, 4)], ["v.ts", null], ["icon.png", null]]);
    expect(diffMismatches(rows, numstat, new Set(["v.ts"]))).toEqual([]);
  });

  it("削除されたファイルの行が patch から落ちたら捕まえる", () => {
    expect(diffMismatches([], new Map([["gone.rs", ns(0, 4)]]))).toHaveLength(1);
  });

  it("削除されていないファイルの削除行が丸ごと落ちたら捕まえる", () => {
    expect(diffMismatches([], new Map([["a.rs", ns(0, 2)]]))).toHaveLength(1);
  });

  it("quote・接頭辞で化けた名前を捕まえる", () => {
    const rows = add('"b/\\346\\227\\242.rs"', 1);
    expect(diffMismatches(rows, new Map([["既.rs", ns(1)]]))).toHaveLength(2); // 化けた名前と、落ちた本物の両方
  });

  it("patch から行が落ちたファイルを捕まえる（色・外部 diff で +++ が読めない形）", () => {
    expect(diffMismatches([], new Map([["a.rs", ns(3)]]))).toHaveLength(1);
  });

  it("追加行数の食い違いを捕まえる", () => {
    expect(diffMismatches(add("a.rs", 1), new Map([["a.rs", ns(2)]]))).toHaveLength(1);
  });

  it("削除行数の食い違いを捕まえる（--include-removed で落ちる形）", () => {
    expect(diffMismatches([...add("a.rs", 1)], new Map([["a.rs", ns(1, 1)]]))).toHaveLength(1);
  });

  it("バイナリ扱いのソースを捕まえる（-diff 属性で中身が見えない）", () => {
    expect(diffMismatches([], new Map([["y.rs", null]]))).toHaveLength(1);
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

describe("listUntracked（未追跡ファイルを 1 件ずつ・quote せずに挙げる）", () => {
  // 実物の git を返す fake。`--short` を素で打つと、新ディレクトリは `?? newdir/` の 1 行に畳まれ
  // `core.quotePath=true`（git の既定）では日本語パスが `"\346..."` になる——どちらも SOURCE_EXT に合わず
  // 中のファイルが黙って落ちた（#1300 の作業中に一時リポジトリで実測。追跡の変更と同居すると exit 0 で ① 0 件）。
  // 引数の列を丸ごと固定する——`--porcelain=v2` は未追跡を `? path` で出すので、フラグの有無だけを見る
  // fake では全件が落ちる変異を緑のまま通した（824ec52c のレビュー）
  const fakeGit = (args) => {
    expect(args).toEqual(["status", "--porcelain", "-z", "--untracked-files=all"]);
    return " M lib.rs\0?? newdir/a.rs\0?? 新規.rs\0?? notes.txt\0";
  };
  const run = (...a) => fakeGit(a);

  it("新しいディレクトリの中のファイルを落とさない", () => {
    expect(listUntracked(run)).toContain("newdir/a.rs");
  });

  it("日本語パスを quote 付きで落とさない", () => {
    expect(listUntracked(run)).toContain("新規.rs");
  });

  it("追跡ファイルとソース拡張子でないものは入れない", () => {
    expect(listUntracked(run)).toEqual(["newdir/a.rs", "新規.rs"]);
  });
});

describe("readTrackedRows（plumbing の 2 経路を照合する）", () => {
  const PATCH = ["-c", "core.quotePath=false", "diff-index", "-M", "-p", "-U0", "mb"];
  const NUMSTAT = ["-c", "core.quotePath=false", "diff-index", "-M", "--numstat", "-z", "mb"];
  const fakeGit = (patch, numstat) => (...args) => {
    if (JSON.stringify(args) === JSON.stringify(PATCH)) return patch;
    if (JSON.stringify(args) === JSON.stringify(NUMSTAT)) return numstat;
    throw new Error(`想定外の引数: ${JSON.stringify(args)}`);
  };

  it("引数の列を固定し、照合がそろえば行を返す", () => {
    const rows = readTrackedRows(fakeGit("+++ b/a.rs\n@@ -0,0 +1 @@\n+x\n", "1\t0\ta.rs\u0000"), "mb");
    expect(rows).toEqual([{ file: "a.rs", sign: "+", text: "x" }]);
  });

  it("削除されたバイナリは patch の 1 行で知り、照合で問わない（変更されたバイナリのソースは止める）", () => {
    expect(readTrackedRows(fakeGit("Binary files a/v.ts and /dev/null differ\n", "-\t-\tv.ts\u0000"), "mb")).toEqual([]);
    expect(() => readTrackedRows(fakeGit("Binary files a/v.ts and b/v.ts differ\n", "-\t-\tv.ts\u0000"), "mb")).toThrow(
      DiffShapeError,
    );
  });

  it("食い違えば DiffShapeError（main が exit 3 にする）", () => {
    const quoted = '+++ "b/\\346\\227\\242.rs"\n@@ -0,0 +1 @@\n+x\n';
    expect(() => readTrackedRows(fakeGit(quoted, "1\t0\t既.rs\u0000"), "mb")).toThrow(DiffShapeError);
  });
});

describe("本物の git に出力の形を変える設定を注入する（fake は作者の想定の写しで、git の実際を見ない）", () => {
  // 6 設定とも、porcelain の `git diff` では行が黙って落ちた（#1300 の作業中に実測）。
  // 日本語名（quotePath）・cwd の外（relative）・rename と編集・`++ ` で始まる追加行を置き、サブディレクトリから打つ
  const script = fileURLToPath(new URL("./race-boundaries.mjs", import.meta.url));
  const CONFIGS = [
    ["core.quotePath", "true"],
    ["diff.relative", "true"],
    ["diff.mnemonicPrefix", "true"],
    ["diff.noprefix", "true"],
    ["color.ui", "always"],
    ["diff.external", "true"],
  ];
  const SPAWN = "fn b() { thread::spawn(|| {}); }\n";
  let top;
  let emptyConfig;
  // global / system を外して git の既定から始める——開発機の `~/.gitconfig` が `quotepath false` だと、
  // noprefix の行は旧実装でも緑で通った（変異注入で実測）。setup 側の git も同じ環境で打つ
  const env = (extra = {}) => ({ ...process.env, GIT_CONFIG_GLOBAL: emptyConfig, GIT_CONFIG_NOSYSTEM: "1", ...extra });
  const g = (...a) => execFileSync("git", ["-c", "user.name=t", "-c", "user.email=t@t", ...a], { cwd: top, stdio: "pipe", env: env() });
  const write = (f, s) => fs.writeFileSync(path.join(top, f), s);
  const run = (extra, args = []) =>
    spawnSync("node", [script, "--base", "main", ...args], { cwd: path.join(top, "sub"), encoding: "utf8", env: env(extra) });
  const inject = (key, value) => ({ GIT_CONFIG_COUNT: "1", GIT_CONFIG_KEY_0: key, GIT_CONFIG_VALUE_0: value });

  beforeEach(() => {
    top = fs.mkdtempSync(path.join(os.tmpdir(), "rb-git-"));
    emptyConfig = `${top}.gitconfig`; // リポジトリの外（未追跡として母集団へ混ざらないように）
    fs.writeFileSync(emptyConfig, "");
    g("init", "-q", "-b", "main");
    g("config", "core.autocrlf", "false");
    fs.mkdirSync(path.join(top, "sub"));
    for (const f of ["既存.rs", "top.rs", "sub/s.rs"]) write(f, "fn a() {}\n");
    write("mv.rs", "fn m1() {}\nfn m2() {}\nfn m3() {}\nfn m4() {}\nfn m5() {}\n");
    write("r.rs", "fn r() { thread::spawn(|| {}); }\nfn keep() {}\n"); // 1 行を書き換える（削除 1・追加 1）
    write("d.rs", "fn d() { thread::spawn(|| {}); }\n"); // ファイルごと消す
    write("n.rs", "fn n() {}"); // 改行で終わらない——patch の hunk の中に `\ No newline` が入る
    fs.writeFileSync(path.join(top, "v.ts"), Buffer.from([0x47, 0, 0x40, 0])); // 消すバイナリのソース（照合で止めない）
  });
  afterEach(() => {
    fs.rmSync(top, { recursive: true, force: true });
    fs.rmSync(emptyConfig, { force: true });
  });

  const change = () => {
    g("add", ".");
    g("commit", "-q", "-m", "init");
    for (const f of ["既存.rs", "sub/s.rs"]) fs.appendFileSync(path.join(top, f), SPAWN);
    fs.appendFileSync(path.join(top, "top.rs"), "++ x\n" + SPAWN); // patch では `+++ x` になる行
    g("mv", "mv.rs", "sub/moved.rs");
    fs.appendFileSync(path.join(top, "sub", "moved.rs"), SPAWN);
    write("r.rs", "fn r() {}\nfn keep() {}\n");
    write("n.rs", "fn n() { thread::spawn(|| {}); }");
    fs.rmSync(path.join(top, "d.rs"));
    fs.rmSync(path.join(top, "v.ts"));
    write("sub/u.rs", "fn c() {}\n"); // 未追跡 1 件——color/external が exit 0 になる形
  };

  for (const [key, value] of CONFIGS) {
    it(`${key}=${value} でも追加行がすべて入る`, () => {
      change();
      const r = run(inject(key, value));
      expect(r.stderr).toBe("");
      expect(r.status).toBe(0);
      // 既存 1・top 2・s 1・moved 1（rename を検出して追加行だけ）・r 1・n 1・未追跡 u 2（末尾改行の後の空行も数える）
      expect(r.stdout).toContain("判定対象 9 行");
      expect(r.stdout).toContain("① worker の spawn: 5 件");
      for (const f of ["既存.rs", "top.rs", "sub/s.rs", "sub/moved.rs", "n.rs"]) expect(r.stdout).toContain(`    ${f}  /thread::spawn/`);
    });
  }

  it("--include-removed では書き換えで消えた行も、削除されたファイルの行も入る", () => {
    change();
    const r = run({}, ["--include-removed"]);
    expect(r.stderr).toBe("");
    expect(r.status).toBe(0);
    // 既定の 9 行に、r と n の書き換えで消えた行が 1 行ずつと、削除された d の 1 行
    expect(r.stdout).toContain("判定対象 12 行");
    expect(r.stdout).toContain("① worker の spawn: 7 件");
    for (const f of ["r.rs", "d.rs"]) expect(r.stdout).toContain(`    ${f}  /thread::spawn/`);
  });

  it("ファイルを消しただけのブランチを「基準が誤っている」と誤診しない（廃止の検査が映す）", () => {
    g("add", ".");
    g("commit", "-q", "-m", "init");
    g("rm", "-q", "d.rs");
    const plain = run();
    expect(plain.stderr).toBe("");
    expect(plain.status).toBe(0);
    expect(plain.stdout).toContain("① worker の spawn: 0 件");
    const removed = run({}, ["--include-removed"]);
    expect(removed.status).toBe(0);
    expect(removed.stdout).toContain("① worker の spawn: 1 件");
    expect(removed.stdout).toContain("    d.rs  /thread::spawn/");
  });

  it("中身を見られないソース（-diff 属性）は exit 3 で止め、0 件の報告を作らない", () => {
    write(".gitattributes", "top.rs -diff\n");
    change();
    const r = run();
    expect(r.status).toBe(3);
    expect(r.stdout).toBe("");
    expect(r.stderr).toContain("top.rs: バイナリ扱いで中身を見られない");
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

describe("readerFor（ルート相対のパスを cwd に依らず読む）", () => {
  it("cwd がルートでなくてもルート相対で読む", () => {
    const top = fs.mkdtempSync(path.join(os.tmpdir(), "rb-"));
    fs.mkdirSync(path.join(top, "sub"));
    fs.writeFileSync(path.join(top, "sub", "a.rs"), "thread::spawn(|| {});\n");
    try {
      // vitest の cwd はリポジトリルートで、top とは別の場所である
      expect(readerFor(top)("sub/a.rs")).toBe("thread::spawn(|| {});\n");
      expect(readerFor(top)("sub/none.rs")).toBe(null);
    } finally {
      fs.rmSync(top, { recursive: true, force: true });
    }
  });
});
