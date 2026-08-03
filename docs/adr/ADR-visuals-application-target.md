# ADR-visuals-application-target: テーマ 3 値は Context ではなく Ui へ適用する

## 文脈

`[visual]` の**色だけ**を変えた config 適用フレームで、style を経由する 3 値
（`extreme_bg_color` / `selection.bg_fill` / `weak_text_color`）が**そのフレームの TextEdit 描画に
届かなかった**（#751）。egui 0.35.0 の `Context::run_ui` は user callback より前に root `Ui` を作り、
`Ui::new` はそこで `ctx.global_style()` を `Arc<Style>` として掴む。ゆえに callback 内の
`ctx.set_visuals` は現在の pass に届かない。そして「次のフレームが来る保証の無い状況」が
**設定 UI で色を編集している当の状況**と一致するため、入力欄まわりだけが旧色で取り残された。

#751 は修正方針を 3 案挙げたうえで「どれを採るかはこの issue では決めない」と明記していた。
本 ADR はその選択を記録する。

## 決定

**`SearchWindowView::update` の適用先を `ctx.set_visuals` から `ui.visuals_mut()` へ移す。**
`ctx.set_visuals` はこの crate から消える。

## 検討した代替案と却下理由

- **runtime 側に root `Ui` 生成前の style 設定フックを設ける**（#751 の第 2 案）: 却下。
  `snotra-egui-runtime` の `EguiView` trait に新しい口を足す代価を払うが、得られるのは
  `ui.visuals_mut()` と同じ到達性だけである。**同じ結果へ 2 経路を作ると、どちらが正本かを
  以後ずっと決め続けることになる。** なお同じ却下は
  `docs/superpowers/specs/2026-07-28-config-background-color-design.md` が背景色の文脈で先に
  下しており（当時は「#751 は症状未観測ゆえ過剰」が理由）、症状が観測された後も結論は変わらない。
- **色の変化を検出したフレームで `ctx.request_repaint()` を撃つ**（#751 の第 3 案）: 却下。
  issue 自身が「最小の対症療法。1 フレーム遅れは残る」と書くとおり、**旧色を 1 枚描いてから
  直す**という挙動は残る。加えて変化検出のためのエッジ状態（前フレームの色）を view が持つ
  ことになり、「snapshot を `self.` へ保持しない」（#673）と衝突する。**PR #791 が未観測の
  #751 へ同じ対症療法を撃って取り下げられた経緯**もある。
- **`ctx.set_visuals` を残したまま `ui.visuals_mut()` を足す**（両方書く）: 却下。global style を
  読む消費者を 2 段で数え上げ、**0 件**を確認した——このリポジトリ側は
  `Area` / `Window` / `CentralPanel` / `Modal` / `ComboBox` / popup / tooltip / menu が 1 件も無く、
  egui 内部の `global_style()` / `options.style()` 呼び出し点も 3 値を読まない
  （読むのは `interact_radius` / `text_options` / `error_fg_color` / `animation_time` /
  `scroll_animation` / `dark_mode` / `debug.*`）。消費者ゼロの書き込みを残す理由が無い。
- **`ctx.set_visuals` が当該 pass に届かないことを固定する対のテストを置く**: 却下。
  それは egui の**現在の制限**を固定する主張であり、**上流が直した日に緑のビルドが赤になる**。
  依存していない命題の保守税である。置いたのは `ui.visuals_mut()` の伝播を固定する側だけで、
  そちらは egui が壊したときに正しく落ちる。
- **新設した順序不変条件に検知器を置く**: 見送り（受容残余）。この修正は「適用は **visuals を読む
  最初の操作より前**」という順序条件を新しく作った——旧 `ctx.set_visuals` はどこで呼んでも
  当該 pass に届かなかったので位置に意味が無かった。**コンパイラ・ユニットテスト・
  `check:colors`・smoke のいずれもこれを捕まえない。** 検知器を作るには「visuals を読む操作」を
  静的に定義する必要があり、egui の内部実装に依存した脆い判定になる。**規範（コメントと
  `src-tauri/CLAUDE.md`）で守り、破れは目視に委ねる**と決めた。

## 帰結

- `src-tauri/src/egui_shell/view.rs` の反映境界は `ui.visuals_mut()` と `frame.set_clear_color` の
  2 つになる。**style 経由の 3 値と背景色の到達フレームが揃った**（従来は背景だけが同フレーム）。
- **global style はもう 3 値を持たない。** main 窓へ新しく egui コンテナを足すなら、その Ui へ
  自分で visuals を渡す必要がある（`ctx.global_style()` から Ui を作るため既定色で描かれる）。
- 順序不変条件は検知器を持たない受容残余として、`view.rs` の適用点と `src-tauri/CLAUDE.md`
  「テーマ色・font・行高の読みは 1 フレーム 1 回（#673 spec 決定 4）」に名指しで残る。

---

status: Accepted
関連: #751 ・#673（1 フレーム 1 lock）・`docs/superpowers/specs/2026-07-28-config-background-color-design.md`（背景色が style を経由しない経路）
