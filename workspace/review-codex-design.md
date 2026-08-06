# codex による設計レビュー — #900

道具: codex-cli 0.146.0（別モデル）。プロンプトは「同意ではなく反証」を求める形で与えた。
以下は codex の最終回答の全文である（進捗ログ・thinking トレース 4677 行は破棄した）。

---

結論は、**現計画はそのまま実装すべきではありません**。`disallowed-methods` は旧コードへの単純な巻き戻し防止には使えますが、この不変条件の主たる担保としては不完全です。9件への拡張は「不完全なのに過剰」という状態です。

## 1. `disallowed-methods` は最善か

**最善ではありません。確信度: 高。**

強制の階梯では、今回の lint は実質的に「テスト・CI で検出」の段です。通常の `cargo build` では発火せず、CI と hook が明示的に `cargo clippy` を実行して初めて効くためです（[development-principles.md:90](/C:/workspace/Snotra/docs/development-principles.md:90)、[ci.yml:125](/C:/workspace/Snotra/.github/workflows/ci.yml:125)、[post-edit.mjs:308](/C:/workspace/Snotra/.claude/hooks/post-edit.mjs:308)）。`#[allow]` でも回避できます。

より上の段へ登る現実的な方法は、#751 の実際の消費者である `TextEdit` と3値の適用を同じ関数へ閉じ込めることです。`search_input_ui` は既に kittest から実コードのまま駆動できる境界であり、別の順序バグをこの方法で閉じた先例があります（[view.rs:203](/C:/workspace/Snotra/src-tauri/src/egui_shell/view.rs:203)）。

概念コードは次です。

```rust
struct InputVisuals {
    input_bg: egui::Color32,
    selection: egui::Color32,
    hint: egui::Color32,
}

fn search_input_ui(
    ui: &mut egui::Ui,
    input_visuals: InputVisuals,
    buf: &mut String,
    params: &SearchInputParams,
    hint: impl FnOnce(&mut egui::Ui) -> String,
) -> egui::Response {
    // 子 Ui を作る前に、この関数内で必ず適用する。
    let visuals = ui.visuals_mut();
    visuals.extreme_bg_color = input_visuals.input_bg;
    visuals.selection.bg_fill = input_visuals.selection;
    visuals.weak_text_color = Some(input_visuals.hint);

    egui::Frame::new().show(ui, |child| {
        let hint_text = hint(child);
        child.add(/* TextEdit */)
    }).inner
}
```

既存の `hint` クロージャは子 `Ui` の中で呼ばれるため、初回 pass で `child.visuals()` の3値を記録して検査できます。

```rust
let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
    search_input_ui(ui, expected, &mut buf, &params, |child| {
        seen.set((
            child.visuals().text_edit_bg_color(),
            child.visuals().selection.bg_fill,
            child.visuals().weak_text_color(),
        ));
        String::new()
    });
});
assert_eq!(seen.get(), expected_tuple);
```

これは以下を同じテストで落とします。

- `ui.visuals_mut()` を子 `Ui` 生成後へ移す
- `ctx.set_visuals()` へ戻す
- 3値の一部を適用し忘れる

現在のテストは egui 自体の伝播しか測らず、製品の呼び出し位置を守らないと明記されています（[view.rs:1362](/C:/workspace/Snotra/src-tauri/src/egui_shell/view.rs:1362)）。上の案はその残余を閉じます。

これは ADR が却下した案の再提案ではありません。ADR が却下したのは「`ctx.set_visuals` が届かないという上流制限のテスト」と「visuals reader 一覧に依存する静的検知器」です（[ADR:39](/C:/workspace/Snotra/docs/adr/ADR-visuals-application-target.md:39)、[ADR:43](/C:/workspace/Snotra/docs/adr/ADR-visuals-application-target.md:43)）。上の案は製品関数の同一pass出力を測ります。

完全な型表現不能化には、`egui::Ui` を `Deref` しない独自 `StyledUi` で包み、許可した操作だけを転送する必要があります。しかし `Ui::ctx()` が生の `Context` を公開する以上、薄い newtype や型 alias では禁止できません。約700行の `update()` に対して全 egui API を包む費用は、YAGNI 境界（[development-principles.md:110](/C:/workspace/Snotra/docs/development-principles.md:110)）を越える可能性が高いです。確信度: 中。

したがって推奨は、**製品不変条件を上の choke point＋意味論テストで守り、`disallowed-methods` は旧コードへ戻す最頻経路 `Context::set_visuals` 1件だけの補助ガードに下げる**ことです。

## 2. 誤っている前提

### 2.1 9件は「Context 経由の global style 書き込み」を閉じない

**確信度: 高。**

計画は母集団が `Context` の inherent method で閉じると述べています（[plan.md:25](/C:/workspace/Snotra/workspace/plan.md:25)）。しかし、少なくとも次の未列挙経路があります。

```rust
ctx.memory_mut(|memory| {
    memory.options.dark_style = Arc::new(style);
});

ctx.options_mut(|options| {
    options.light_style = Arc::new(style);
});
```

egui 0.35.0 の一次ソースでは、

- `Context::memory_mut`: `egui-0.35.0/src/context.rs:953-957`
- `Memory::options` は public: `egui-0.35.0/src/memory/mod.rs:30-32`
- `Options::dark_style/light_style` は public: 同 `:193-200`
- `Context::options_mut`: `context.rs:1067-1071`

です。

計画は `options_mut` を受容残余として認識していますが（[plan.md:55](/C:/workspace/Snotra/workspace/plan.md:55)）、`memory_mut` は列挙していません。しかも次の実行結果のとおり、`memory_mut` 自体は正当用途があるため一括禁止できません。

```text
$ rg -n -g '*.rs' "\.memory_mut\s*\(" src-tauri/src
src-tauri/src/egui_shell/view.rs:259: ...
src-tauri/src/egui_shell/view.rs:1164: ...
src-tauri/src/egui_shell/view.rs:1207: ...
src-tauri/src/egui_shell/view.rs:1323: ...
```

したがって、9件が守れるのは「選んだ9個の名前付き API を直接呼ばないこと」だけです。`CLAUDE.md` を「`Context` 経由の global style 書き込み全般」へ広げる [plan.md:135-147](/C:/workspace/Snotra/workspace/plan.md:135) は、機構より強い偽の契約になります。

### 2.2 9件の境界基準が一貫していない

**確信度: 高。**

- `set_debug_on_hover` は global `Style` を書きますが `Visuals` は書きません。#751 の3値に限定するなら除外は正しいです。
- `set_theme` は `Style` を書きませんが、callback 内で呼ぶと当該passの見た目が切り替わらないという症状は同型です。
- `style_ui` / `settings_ui` は `Visuals` を変更可能ですが、デバッグ用 UI としての利用まで crate 全体で拒否します。

つまり基準が「Styleへ書く」「Visualsへ書く」「同じ症状を起こす」の間で揺れています。9という件数は、仕様上の自然な境界ではありません。

`style_ui` / `settings_ui` のユーザー操作が常に次passを保証するかまでは今回確定できていません。確信度: 中。ただし、9件集合の定義が曖昧という結論には影響しません。

### 2.3 「`disallowed_methods` では順序を守れない」は正しいが、「順序に検知手段は無い」は誤り

前半は正しいです。メソッド存在 lint は `ui.visuals_mut()` の位置を評価しません。

後半は誤りです。責務を `search_input_ui` の検査可能な境界へ移し、初回passの子 `Ui` を観測すれば検知できます。このリポジトリ自身が「検知手段が無いなら責務移動か観測点を検討する」と要求しています（[development-principles.md:105](/C:/workspace/Snotra/docs/development-principles.md:105)）。

### 2.4 `git status` の受け入れ条件は現在の作業ツリーでは成立しない

計画は「ちょうど3ファイル」を要求します（[plan.md:168](/C:/workspace/Snotra/workspace/plan.md:168)）が、実測は次です。

```text
$ git status --short
?? workspace/
```

`workspace/` 自体が未追跡なので、実装後も「ちょうど3ファイル」にはなりません。

## 3. 過剰な部分

削れる箇所は次です。

- **9件への拡張**  
  既知の回帰姿は `ctx.set_visuals` への巻き戻しです。9件でも汎用アクセサを閉じられない以上、仮想的なAPI名を8件増やす費用に完全性は伴いません。1件 lint＋意味論テストが妥当です。

- **`CLAUDE.md` の禁止範囲拡大**  
  機構より広い契約になるため、過剰というより誤りです。

- **`view.rs` `//!` への「clippyが0件を保つ」追記**（[plan.md:152](/C:/workspace/Snotra/workspace/plan.md:152)）  
  lint の検出力を一切増やしません。`clippy.toml` が削除されてもコメントだけ残り、誤った安心を作ります。

- **`snotra-settings` の個別 clippy 再実行**  
  最終 `cargo clippy --workspace --all-targets` は既存の正当な2呼び出しもコンパイルします。設定スコープが漏れていれば workspace 実行自身が赤くなるため、別コマンドは診断上の便宜でしかありません。

- **除外理由を大量に `clippy.toml` へ収めること**  
  `.toml` コメントは governance 検査外と計画自身が認めています（[plan.md:107](/C:/workspace/Snotra/workspace/plan.md:107)）。守りを増やさず、更新されない否定知識を増やします。

一方、9件を残すなら9件すべての fault injection は削れません。不正パスが無音化しうる以上、1件だけの注入では残り8件を検証できないからです。削るべきなのは検算ではなく、検算を必要にした9件という設計です。

## 4. 新しい負債

1. **egui API 棚卸し負債**  
   egui が新しい Style writer を追加しても lint は緑のままです。5年後の更新者は9件が完全な集合だと誤認します。

2. **上流修正後も消えない禁止**  
   `run_ui` の順序が改善されても lint は赤くならず、正当になったAPIを黙って禁止し続けます。計画にも再評価・撤去トリガーがありません（[plan.md:233](/C:/workspace/Snotra/workspace/plan.md:233)）。

3. **設定ファイルの自己消失を検知できない**  
   `clippy.toml` の削除・空配列化・crate名 typo が緑になります（[plan.md:184](/C:/workspace/Snotra/workspace/plan.md:184)）。「機構が守る」という文書だけが残る最悪の失敗方向です。

4. **将来のルート設定を遮蔽する**  
   crate-local `clippy.toml` は親の設定とマージされません。将来ルートへ共通設定を置くと `src-tauri` だけ黙って除外されます（[plan.md:103](/C:/workspace/Snotra/workspace/plan.md:103)）。Clippy公式文書も、最初に見つかった設定を使って親方向へ探索すると説明しています。また Clippy 設定ファイル自体が unstable と明記されています。[Clippy公式設定文書](https://doc.rust-lang.org/clippy/configuration.html)

5. **正当な `setup` と対照テストを拒否する**  
   `EguiView::setup` は `run_ui` 前に呼ばれるため欠陥を持ちません（[runtime.rs:380](/C:/workspace/Snotra/snotra-egui-runtime/src/runtime.rs:380)）。crate全体 lint はここや `#[cfg(test)]` の正当な書き込みも拒み、`#[allow]` を蓄積させます。

6. **低レベルな抜け道へ誘導する**  
   名前付きAPIが禁止される一方、`options_mut` / `memory_mut` 直書きは通ります。将来の実装者が lint 回避のため、より読みにくく壊れやすい低レベル操作へ流れる可能性があります。

最終提案は、**現計画の9件・3ファイル変更を撤回し、`search_input_ui` に3値適用を吸収して初回passの実コードテストを置くこと**です。`ctx.set_visuals` への単純な巻き戻しも明示的に止めたいなら、`disallowed-methods` はその1件だけを補助ガードとして追加してください。

ファイルは変更していません。最終 `git status --short` も開始時と同じ `?? workspace/` のみです。
