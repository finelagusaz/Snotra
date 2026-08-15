import { fileURLToPath } from "node:url";
import { describe, it, expect } from "vitest";
import { snap } from "../test-helpers.mjs";
import { makeSnapshot, workspaceMembers } from "../lib.mjs";
import { checkWorkspaceLints } from "./G-workspace-lints.mjs";

describe("G-workspace-lints checkWorkspaceLints（rustdoc deny の実効性・#713）", () => {
  // 赤フィクスチャは「cargo が exit 0 で沈黙した入力」そのもの（cargo 1.94.0 で実測）。
  // 述語を弱めた実装はこのどれかで必ず緑へ振れる。
  const ROOT_OK =
    '[workspace]\nmembers = ["a", "b"]\n\n[workspace.lints.rustdoc]\nbroken_intra_doc_links = "deny"\ninvalid_html_tags = "deny"\n';
  const OPT_IN = '[package]\nname = "a"\nversion.workspace = true\n\n[lints]\nworkspace = true\n\n[dependencies]\negui.workspace = true\n';
  const base = { "Cargo.toml": ROOT_OK, "a/Cargo.toml": OPT_IN, "b/Cargo.toml": OPT_IN };
  const member = (contents) => checkWorkspaceLints(snap({ ...base, "a/Cargo.toml": contents }));
  const root = (contents) => checkWorkspaceLints(snap({ ...base, "Cargo.toml": contents }));

  // --- クラス 1（member 側の opt-in）---
  it("緑: 全 member が [lints] workspace = true を持つ", () => {
    expect(checkWorkspaceLints(snap(base))).toEqual([]);
  });
  it("緑: ルート直下の dotted lints.workspace = true（テーブル形と等価・実測 F）", () => {
    expect(member('lints.workspace = true\n\n[package]\nname = "a"\n')).toEqual([]);
  });
  it("緑: 見出し・キーの行末コメントと余分な空白を許す", () => {
    expect(member('[package]\nname = "a"\n\n[lints]   # docgen 検出器の opt-in\nworkspace = true  # 継承\n')).toEqual([]);
  });
  // 赤ケースは件数ではなく **file の並び** で主張する——件数だけだと、クラス 2 が退行して
  // ルート由来の finding が 1 件出た状態でも「1 件」を満たしてしまう
  const memberRed = (contents) => expect(member(contents).map((x) => x.file));
  it("赤: member に [lints] が無い（実測 B・#706 の再現形）", () => {
    memberRed('[package]\nname = "a"\n').toEqual(["a/Cargo.toml"]);
  });
  it("赤: [lints.rustdoc] だけを持つ（workspace テーブルを継承しない・実測 C）", () => {
    memberRed('[package]\nname = "a"\n\n[lints.rustdoc]\ninvalid_html_tags = "deny"\n').toEqual(["a/Cargo.toml"]);
  });
  it("赤: [lints] に workspace = false", () => {
    memberRed('[package]\nname = "a"\n\n[lints]\nworkspace = false\n').toEqual(["a/Cargo.toml"]);
  });
  it("赤: [package] 配下の lints.workspace = true（cargo は警告を出すが exit 0 で通す形・実測 E）", () => {
    memberRed('[package]\nname = "a"\nlints.workspace = true\n').toEqual(["a/Cargo.toml"]);
  });
  it("不混入: version.workspace / <dep>.workspace は opt-in と見なさない（字面一致では常に緑になる）", () => {
    const f = member('[package]\nname = "a"\nversion.workspace = true\n\n[dependencies]\negui.workspace = true\ntauri.workspace = true\n');
    expect(f).toHaveLength(1);
    expect(f[0].file).toBe("a/Cargo.toml");
  });
  it("不混入: ルートの [workspace.lints.rustdoc] は member 側の判定に混入しない（読み取りが <dir>/Cargo.toml に閉じる）", () => {
    const f = member('[package]\nname = "a"\n');
    expect(f.map((x) => x.file)).toEqual(["a/Cargo.toml"]); // ルート由来の finding は出ない
  });

  // --- クラス 2（ルートの rustdoc deny が実効か）---
  it("緑: テーブル形 level = deny と forbid（実測 P/Q）", () => {
    expect(
      root('[workspace]\nmembers = ["a", "b"]\n\n[workspace.lints.rustdoc]\nbroken_intra_doc_links = { level = "deny", priority = 1 }\ninvalid_html_tags = "forbid"\n'),
    ).toEqual([]);
  });
  it("赤: deny → warn へ降格（実測 N）", () => {
    expect(root(ROOT_OK.replace('broken_intra_doc_links = "deny"', 'broken_intra_doc_links = "warn"'))).toHaveLength(1);
  });
  it("赤: [workspace.lints] は在るが rustdoc サブテーブルが無い（実測 R）", () => {
    expect(root('[workspace]\nmembers = ["a", "b"]\n\n[workspace.lints]\n')).toHaveLength(1);
  });
  it("赤: [workspace.lints.rustdoc] が空テーブル", () => {
    expect(root('[workspace]\nmembers = ["a", "b"]\n\n[workspace.lints.rustdoc]\n\n[profile.release]\nlto = true\n')).toHaveLength(1);
  });
  it("赤: rustdoc サブテーブルはあるが broken_intra_doc_links の行だけ消える（実測 R2）", () => {
    // 必須 lint の名指しを落とした述語は**この 1 件だけ**で緑へ振れる（実測で当初仕様が踏んだ欠陥）
    expect(root('[workspace]\nmembers = ["a", "b"]\n\n[workspace.lints.rustdoc]\ninvalid_html_tags = "deny"\n')).toHaveLength(1);
  });
  it("赤: 必須 2 件は deny だが、別の rustdoc lint が warn で足された（意図的な摩擦）", () => {
    expect(root(`${ROOT_OK}private_intra_doc_links = "warn"\n`)).toHaveLength(1);
  });
  it("不混入: [workspace.lints.clippy] が warn でも、rustdoc が deny なら緑（この検査は rustdoc カテゴリだけを見る。clippy 側で見張られているのは disallowed_methods の deny だけ・#950）", () => {
    expect(root(`${ROOT_OK}\n[workspace.lints.clippy]\nall = "warn"\n`)).toEqual([]);
  });

  // --- 母集団の欠落（fail-closed・5 分岐 + member 不読）---
  it("赤: ルート Cargo.toml が読めない → 1 件だけ（同じ欠落を 2 件に増やさない）", () => {
    const f = checkWorkspaceLints(snap({ "a/Cargo.toml": OPT_IN }));
    expect(f).toHaveLength(1);
    expect(f[0].message).toContain("母集団の欠落");
  });
  it("赤: [workspace] セクションが無い", () => {
    expect(root('[package]\nname = "x"\n').some((x) => x.message.includes("母集団の欠落"))).toBe(true);
  });
  it("赤: [workspace] は在るが members 行が無い（他の分岐では踏めない経路）", () => {
    expect(root(`[workspace]\nresolver = "2"\n\n${ROOT_OK.split("\n").slice(3).join("\n")}`).some((x) => x.message.includes("母集団の欠落"))).toBe(true);
  });
  it("赤: members が 0 件", () => {
    expect(root(ROOT_OK.replace('members = ["a", "b"]', "members = []")).some((x) => x.message.includes("母集団の欠落"))).toBe(true);
  });
  it("赤: members に glob 要素（展開器を持たないので母集団の欠落として倒す）", () => {
    expect(root(ROOT_OK.replace('"a", "b"', '"crates/*"')).some((x) => x.message.includes("母集団の欠落"))).toBe(true);
  });
  it("default-members が members より前に在っても members を拾う（[workspace] スコープの回帰ガード）", () => {
    // 載せ替え前の全文正規表現は `default-members = [...]` を先に拾って ["a"] を返していた。
    // 現ルートに default-members が無いため、この 1 件だけが素朴な形への差し戻しを検知する。
    const src = '[workspace]\ndefault-members = ["a"]\nmembers = ["a", "b"]\n';
    expect(workspaceMembers(snap({ "Cargo.toml": src })).members).toEqual(["a", "b"]);
  });
  it("赤: member の Cargo.toml が読めない", () => {
    const f = checkWorkspaceLints(snap({ "Cargo.toml": ROOT_OK, "a/Cargo.toml": OPT_IN }));
    expect(f).toHaveLength(1);
    expect(f[0].file).toBe("b/Cargo.toml");
  });

  // --- 回帰・カナリア ---
  it("CRLF でも判定が変わらない（CI の Windows checkout は autocrlf=true・#595 の同型）", () => {
    const crlf = (s) => s.replace(/\n/g, "\r\n");
    expect(checkWorkspaceLints(snap({ "Cargo.toml": crlf(ROOT_OK), "a/Cargo.toml": crlf(OPT_IN), "b/Cargo.toml": crlf(OPT_IN) }))).toEqual([]);
  });
  it("カナリア: 実リポジトリで緑であり、守りたい対象（snotra-egui-runtime）が入力に現れる", () => {
    const s = makeSnapshot(fileURLToPath(new URL("../../..", import.meta.url)));
    const { members, error } = workspaceMembers(s);
    expect(error, "実 Cargo.toml から members を導出できなかった").toBeNull();
    expect(members, "#706 の当事者が母集団に居ない＝この検査は再発を見ない").toContain("snotra-egui-runtime");
    expect(checkWorkspaceLints(s)).toEqual([]);
  });
});
