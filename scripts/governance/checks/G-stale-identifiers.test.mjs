import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { checkStaleIdentifiers, scanStaleIdentifiers, currentVocabulary } from "./G-stale-identifiers.mjs";
import { staleIdentifierDocs, staleIdentifierGuideDocs, staleIdentifierTargets } from "../lib.mjs";
import { runAll, buildChecks } from "../../governance-check.mjs";

describe("G-stale-identifiers checkStaleIdentifiers（規範の散文に残る、現行語彙に無い識別子）", () => {
  // 守りたい対象 = #736 の同クラス。UI スタックを入れ替えた後、スキルの散文だけが旧 API 名を
  // 現行 API として指し続ける形。赤フィクスチャは実際に検出された `createObjectURL`（WebView2 期）。
  const DOC = ".claude/skills/x/SKILL.md";
  const base = { "SPEC.md": "# 仕様\n\n本文\n", "src-tauri/src/a.rs": "fn f() { let x = AtomicBool::new(true); }\n" };
  const run = (prose, extra = {}) => checkStaleIdentifiers(snap({ ...base, ...extra, [DOC]: prose }), [DOC]);

  it("現行語彙に無い識別子は finding（赤）", () => {
    const f = run("Blob は `createObjectURL` で作る\n");
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("createObjectURL");
  });

  it("ソースの非コメント本文に在れば finding 無し（緑）", () => {
    expect(run("フラグは `newValue` を見る\n", { "src-tauri/src/a.rs": "let newValue = 1;\n" })).toEqual([]);
  });

  it("ソースのコメントにしか無いものは finding（由来注記を語彙に化けさせない・#736 実測 11 件）", () => {
    const f = run("`resetForShow()` でクリアする\n", { "src-tauri/src/a.rs": "// resetForShow 相当\nlet x = 1;\n" });
    expect(f).toHaveLength(1);
  });

  it("SPEC.md は語彙源ではない（SSOT と写しが同時に鳴る・ADR の却下 4 失効）", () => {
    expect(run("`folderState` を確認する\n", { "SPEC.md": "- `folderState` は直交する\n" })).toHaveLength(1);
  });

  it("テストコードは語彙を寄付しない（検出器自身のフィクスチャが偽陰性を作らない）", () => {
    const f = run("Blob は `createObjectURL` で作る\n", { "scripts/x.test.mjs": "expect(createObjectURL).toBe(1);\n" });
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("createObjectURL");
  });

  it("コマンドは空白を含むので見ない（除外リストを置かない・行粒度のフィルタ無しで成立する）", () => {
    expect(run("母集団は `gh issue list --search x` から取る\n")).toEqual([]);
  });

  it("コマンドと同じ行に単独で書かれた識別子は見る（行粒度フィルタの沈黙経路を閉じた・#993）", () => {
    const f = run("`npm run governance:check` は `createObjectURL` を見る\n");
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("createObjectURL");
  });

  it("判定対象外の不混入: 単語 1 つ・パス・空白入り・コードフェンス内", () => {
    expect(run("`Glob` と `expand` と `docs/a.md` と `git diff main`\n")).toEqual([]);
    expect(run("```\n`createObjectURL`\n```\n")).toEqual([]);
  });

  it("末尾 () の有無で判定が変わらない", () => {
    expect(run("`interpKind()` を見る\n")).toHaveLength(1);
    expect(run("`interpKind` を見る\n")).toHaveLength(1);
  });

  // 赤フィクスチャは実際に検出された `G12_NO_LAUNCHER_READ`（#825 の PR が消した語・SCREAMING_SNAKE
  // 述語が捕まえた実測例）。緑の対 `NO_LAUNCHER_READ` は下の SNAKE_SRC が合成語彙として供給する
  //（実装側の同名定数は #894 で撤去済み——このフィクスチャは実リポジトリに依存しない）。
  const SNAKE_SRC = { "src-tauri/src/a.rs": "const NO_LAUNCHER_READ = 1;\n" };

  it("SCREAMING_SNAKE も見る（赤）", () => {
    const f = run("読まない理由は `G12_NO_LAUNCHER_READ` へ載せる\n", SNAKE_SRC);
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("G12_NO_LAUNCHER_READ");
  });

  it("語彙に在る SCREAMING_SNAKE は鳴らない（緑）", () => {
    expect(run("読まない理由は `NO_LAUNCHER_READ` へ載せる\n", SNAKE_SRC)).toEqual([]);
  });

  it("判定対象外の不混入: `_` を持たない全大文字（こぶ 1 つ以上の要求と同型）", () => {
    expect(run("`CI` と `TODO` と `README`\n")).toEqual([]);
  });

  // 赤フィクスチャは #975 が名指しした**実在の欠陥**——`snotra-core/src/index_tree.rs` の doc が
  // 存在しないテスト名を「実データの全件で固定する」と引いたまま素通りしていた（反復 10）。
  // 緑の対は同じ形で実在する側の名前で、LOWER_SRC が合成語彙として供給する
  //（実リポジトリの語彙に依存させない——#891 が「フィクスチャが偽陰性を作る」で踏んだ経路）。
  const LOWER_SRC = { "snotra-core/src/a.rs": "fn index_tree_file_key_matches_normalize_file_name_key() {}\n" };

  it("lowercase snake_case も見る（赤・#975 の実在の欠陥）", () => {
    const f = run("両腕の一致は `index_tree_file_key_matches_normalize_file_name_key_over_frozen_v6` が固定する\n", LOWER_SRC);
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("over_frozen_v6");
  });

  it("語彙に在る lowercase snake_case は鳴らない（緑）", () => {
    expect(run("両腕の一致は `index_tree_file_key_matches_normalize_file_name_key` が固定する\n", LOWER_SRC)).toEqual([]);
  });

  it("判定対象外の不混入: `_` を持たない小文字 1 語（こぶ 1 つ以上の要求と同型）", () => {
    expect(run("`expand` と `collapse` と `mount`\n")).toEqual([]);
  });

  it("照合件数を返す（「腐りゼロ」と「照合していない」の区別・#497）", () => {
    const r = scanStaleIdentifiers(snap({ ...base, [DOC]: "`someName` と `otherName`\n" }), [DOC]);
    expect(r.checked).toBe(2);
    expect(r.findings).toHaveLength(2);
  });

  it("3 述語は checked を二重計上しない（先頭文字と字種で相互排他ゆえ順に試せる）", () => {
    const r = scanStaleIdentifiers(snap({ ...base, [DOC]: "`someName` と `SOME_NAME` と `some_name`\n" }), [DOC]);
    expect(r.checked).toBe(3);
    expect(r.findings).toHaveLength(3);
  });

  // 赤フィクスチャは #984 の**実在の欠陥**——`docs/comment-guidelines.md` の第一原則が模範例として
  // 指していた関数を同じ PR が削除し、規範の根拠だけが実在しない名前を指す状態になった（#993）。
  // 緑の対は同じ型の現存メンバで、QUALIFIED_SRC が合成語彙として供給する（実リポジトリに依存させない）。
  const QUALIFIED_SRC = { "snotra-core/src/a.rs": "fn from_material() {}\nfn encode_batch_binary() {}\n" };

  it("型で修飾した形は末尾セグメントを見る（赤・#984 の実在の欠陥）", () => {
    const f = run("模範例は `Engine::new_from_cache` である\n", QUALIFIED_SRC);
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("Engine::new_from_cache");
  });

  it("末尾セグメントが語彙に在れば鳴らない（緑）", () => {
    expect(run("模範例は `Engine::from_material` である\n", QUALIFIED_SRC)).toEqual([]);
  });

  it("`.` を含む修飾形も末尾セグメントで判定する（`.` の除外はトークン全体ではなくセグメントへ当てる）", () => {
    expect(run("参照先は `icon.rs::encode_batch_binary` である\n", QUALIFIED_SRC)).toEqual([]);
    expect(run("参照先は `icon.rs::encode_missing_binary` である\n", QUALIFIED_SRC)).toHaveLength(1);
  });

  it("判定対象外の不混入: 型セグメント単語 1 つ・引数つき・パス様の末尾", () => {
    expect(run("`Section::default()` と `snotra-core::Engine`\n", QUALIFIED_SRC)).toEqual([]);
    expect(run("`HistoryStore::load(top_n)` と `Color32::from_rgb(...)`\n", QUALIFIED_SRC)).toEqual([]);
    expect(run("`src-tauri::indexing.rs` を見る\n", QUALIFIED_SRC)).toEqual([]);
  });

  it("修飾形も照合件数を 1 しか進めない（末尾セグメント 1 つだけを見る）", () => {
    const r = scanStaleIdentifiers(snap({ ...base, ...QUALIFIED_SRC, [DOC]: "`Engine::new_from_cache`\n" }), [DOC]);
    expect(r.checked).toBe(1);
  });

  it("読めない文書は母集団の欠落として finding", () => {
    expect(checkStaleIdentifiers(snap(base), ["missing.md"])[0].message).toContain("母集団の欠落");
  });

  it("母集団は skills / rules / agents の md に限る", () => {
    const s = snap({ ".claude/skills/a/SKILL.md": "", ".claude/rules/b.md": "", ".claude/agents/c.md": "", "docs/d.md": "", ".claude/hooks/e.mjs": "" });
    expect(staleIdentifierDocs(s).sort()).toEqual([".claude/agents/c.md", ".claude/rules/b.md", ".claude/skills/a/SKILL.md"]);
  });

  it("語彙は production のソースだけから作られる（SPEC.md も docs もテストも .json も正本ではない）", () => {
    const v = currentVocabulary(
      snap({
        "SPEC.md": "specWord",
        "src-tauri/src/a.rs": "let codeWord = 1;",
        "docs/x.md": "docWord",
        "scripts/a.test.mjs": "testWord",
        "package-lock.json": "lockWord",
      }),
    );
    expect(v).toContain("codeWord");
    expect(v).not.toContain("specWord"); // SPEC.md は語彙源ではなく検査対象である
    expect(v).not.toContain("docWord"); // 一般の docs は語彙の正本ではない
    expect(v).not.toContain("testWord"); // テストコードは「現に動いている実装」ではない
    expect(v).not.toContain("lockWord"); // .json は生成物・依存メタデータを招くので語彙源にしない
  });

  it("`.yml` は語彙を寄付する（CI が実際に実行する実装ゆえ）が、そのコメントは寄付しない", () => {
    const y = { ".github/workflows/ci.yml": "        run: echo x >> $GITHUB_OUTPUT\n" };
    expect(run("`GITHUB_OUTPUT` へ書く\n", y)).toEqual([]);
    expect(run("`GITHUB_OUTPUT` へ書く\n", { ".github/workflows/ci.yml": "# GITHUB_OUTPUT のこと\n" })).toHaveLength(1);
  });

  it("検査対象は規範の散文 + 開発ガイド + 固定パス、母集団の欠落を判ずるのは規範の散文だけ", () => {
    const withProse = snap({ ".claude/rules/b.md": "", "docs/x.md": "", "SPEC.md": "" });
    expect(staleIdentifierTargets(withProse).sort()).toEqual([
      ".claude/rules/b.md",
      "AGENTS.md",
      "CLAUDE.md",
      "SPEC.md",
      "docs/x.md",
      "snotra-settings/SETTINGS-DESIGN.md",
    ]);
    // .claude/** が 1 枚残らず消えても runAll の「対象 md が 0 件」が鳴り続けること。
    // STALE_EXTRA_DOCS や docs/** が長さを埋める側（staleIdentifierDocs）へ混ざるとこの検知は永久に沈黙する
    const noProse = snap({ "SPEC.md": "", "docs/x.md": "" });
    expect(staleIdentifierDocs(noProse)).toEqual([]);
    expect(staleIdentifierGuideDocs(noProse)).toEqual(["docs/x.md"]);
  });

  it("開発ガイドの母集団は docs/** から歴史記録（adr / superpowers）だけを外す", () => {
    const s = snap({
      "docs/architecture.md": "",
      "docs/design/2026-05-31-x.md": "", // 日付を持つが status: Agreed で architecture.md が現在形で指す先
      "docs/adr/ADR-x.md": "", // 却下案＝もう存在しない案
      "docs/superpowers/specs/y.md": "", // #589 で非規範化された当時の設計
      "docs/z.txt": "",
      "PERFORMANCE.md": "",
    });
    expect(staleIdentifierGuideDocs(s).sort()).toEqual(["docs/architecture.md", "docs/design/2026-05-31-x.md"]);
  });
});

// 既存の `runAll（空母集団の明示 fail）` は `snap({})` に対し `findings.length > 0` しか見ないため、
// **どのガードが鳴ったかを区別しない**——`docs/**` 専用の 0 件検知を丸ごと消しても緑で通る（実測）。
// ここは兄弟母集団（`.claude/**` と `STALE_EXTRA_DOCS` の固定パス）を**非空に保ったまま**
// `docs/**` だけを空にして、その 1 件が鳴ることを固定する。
describe("G-stale-identifiers の母集団ごとの 0 件検知（兄弟が非空でも鳴る）", () => {
  const siblings = {
    ".claude/rules/b.md": "",
    "SPEC.md": "",
    "CLAUDE.md": "",
    "AGENTS.md": "",
    "snotra-settings/SETTINGS-DESIGN.md": "",
    "src-tauri/src/a.rs": "let x = 1;\n",
  };
  const guideMisses = (contents) =>
    runAll(snap(contents)).findings.filter((f) => f.message.includes("開発ガイド（docs/**）が 0 件"));

  it("docs/** が 0 件なら鳴る——兄弟母集団が埋まっていても", () => {
    expect(staleIdentifierDocs(snap(siblings))).not.toEqual([]); // 兄弟は非空（この検知の独立性の前提）
    expect(guideMisses(siblings)).toHaveLength(1);
  });

  it("docs/** が在れば鳴らない", () => {
    expect(guideMisses({ ...siblings, "docs/architecture.md": "" })).toHaveLength(0);
  });

  it("docs/adr/ と docs/superpowers/ だけでは埋まらない（歴史記録は母集団ではない）", () => {
    expect(guideMisses({ ...siblings, "docs/adr/ADR-x.md": "", "docs/superpowers/specs/y.md": "" })).toHaveLength(1);
  });
});

// #984 の実在の欠陥を凍結したフィクスチャ。`f1827b0:docs/comment-guidelines.md` の当該行の骨格で、
// **同じ行に `npm` のコマンドと修飾形の名前が同居する**——2 つの穴が同時に効いていた形である（#993）。
// 片足ずつ戻した測定は `ADR-stale-identifier-detector-scope` の #993 の追記節が持つ（テストからは
// 実装を変異させられないため、ここで固定するのは「両足そろえば赤い」ことだけである）。
describe("G-stale-identifiers の凍結フィクスチャ（#984 の実在の欠陥）", () => {
  const DOC = "docs/comment-guidelines.md";
  const SRC = { "snotra-core/src/a.rs": "fn from_material() {}\n" };
  const LINE = (name) => `模範例は \`Engine::${name}\`。\`npm run governance:check\` が見る\n`;

  it("コマンドと同居する行の修飾形が腐っていれば赤", () => {
    const f = checkStaleIdentifiers(snap({ ...SRC, [DOC]: LINE("new_from_cache") }), [DOC]);
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("Engine::new_from_cache");
  });

  it("緑の対: 同じ形で末尾セグメントが語彙に在れば鳴らない", () => {
    expect(checkStaleIdentifiers(snap({ ...SRC, [DOC]: LINE("from_material") }), [DOC])).toEqual([]);
  });
});

// 上の describe が固定するのは各関数の戻り値までで、**buildChecks がどちらを検査へ渡すか**は見ていない。
// `staleTargets` を `staleDocs` へ戻しても実リポジトリの finding は 0 / 照合 1 のまま変わらないため、
// dogfood テストも証跡の印字も気づけない——軸 B の主張（SPEC.md が検査対象になった）を守るのは
// この describe だけである（配線を戻すと下の赤フィクスチャが緑に落ちる）。
describe("G-stale-identifiers の配線（buildChecks が SPEC.md を検査対象として渡す）", () => {
  const wired = (contents) => buildChecks(snap(contents), {}).find((c) => c.id === "G-stale-identifiers").run();
  // 母集団の欠落 finding を避けるための最小の母集団（規範の散文 1 本 + 開発ガイド 1 本 +
  // `SPEC.md` 以外の固定パス 3 本。固定パスは実在を問わず検査対象なので、欠けると鳴る）
  const prose = {
    ".claude/rules/b.md": "",
    "docs/architecture.md": "",
    "CLAUDE.md": "",
    "AGENTS.md": "",
    "snotra-settings/SETTINGS-DESIGN.md": "",
  };

  it("SPEC.md の現行語彙に無い識別子は finding（赤）", () => {
    const f = wired({ ...prose, "SPEC.md": "- `deadCamelWord` を使う\n", "src-tauri/src/a.rs": "let x = 1;\n" });
    expect(f).toHaveLength(1);
    expect(f[0].file).toBe("SPEC.md");
    expect(f[0].message).toContain("deadCamelWord");
  });

  it("SPEC.md の識別子がソースの非コメント本文に在れば finding 無し（緑）", () => {
    expect(wired({ ...prose, "SPEC.md": "- `liveCamelWord` を使う\n", "src-tauri/src/a.rs": "let liveCamelWord = 1;\n" })).toEqual([]);
  });

  it("判定対象外の不混入: SPEC.md のフェンス内・コマンド span 内・単語 1 つ", () => {
    const spec = "```\n`fencedCamelWord`\n```\n- `gh pr view --json argCamelWord` を見る\n- `expand` する\n";
    expect(wired({ ...prose, "SPEC.md": spec, "src-tauri/src/a.rs": "let x = 1;\n" })).toEqual([]);
  });
});

// 母集団を広げた側も同じ穴を持つ——配線を戻しても実リポジトリの finding は動かないため、
// dogfood テストも証跡の印字も気づけない。**射程拡大の主張を守るのはこの describe だけである。**
describe("G-stale-identifiers の配線（buildChecks が開発ガイドと固定パス文書を検査対象として渡す）", () => {
  const wired = (contents) => buildChecks(snap(contents), {}).find((c) => c.id === "G-stale-identifiers").run();
  // STALE_EXTRA_DOCS の 4 本と docs/** を最小で埋める（欠けると「母集団の欠落」が混じる）
  const base = {
    ".claude/rules/b.md": "",
    "docs/architecture.md": "",
    "SPEC.md": "",
    "CLAUDE.md": "",
    "AGENTS.md": "",
    "snotra-settings/SETTINGS-DESIGN.md": "",
    "src-tauri/src/a.rs": "let x = 1;\n",
  };
  const rot = "- `deadCamelWord` を使う\n";

  it("開発ガイド（docs/**）の腐りは finding（赤）", () => {
    const f = wired({ ...base, "docs/hooks.md": rot });
    expect(f).toHaveLength(1);
    expect(f[0].file).toBe("docs/hooks.md");
  });

  it("固定パス文書 3 本も検査対象（赤）", () => {
    for (const doc of ["CLAUDE.md", "AGENTS.md", "snotra-settings/SETTINGS-DESIGN.md"]) {
      const f = wired({ ...base, [doc]: rot });
      expect(f, doc).toHaveLength(1);
      expect(f[0].file, doc).toBe(doc);
    }
  });

  it("判定対象外の不混入: 歴史記録（docs/adr/・docs/superpowers/）は検査されない", () => {
    expect(wired({ ...base, "docs/adr/ADR-x.md": rot })).toEqual([]);
    expect(wired({ ...base, "docs/superpowers/specs/y.md": rot })).toEqual([]);
  });

  it("SCREAMING_SNAKE の腐りも配線を通って届く（#825 が消した実在の語）", () => {
    const f = wired({ ...base, "docs/hooks.md": "- `G12_NO_LAUNCHER_READ` へ載せる\n" });
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("G12_NO_LAUNCHER_READ");
  });
});
