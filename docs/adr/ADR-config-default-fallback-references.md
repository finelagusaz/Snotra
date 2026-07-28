# ADR-config-default-fallback-references: config 既定の写しを消すとき、何を参照へ寄せ何を寄せないか

## 文脈

#795 で、`config.rs` の `default_*()` が返す値と同じリテラルが手書きでコピーされている箇所を潰した。一致を保つ機構は 1 つも無く（コメントの規範だけ）、`docs/development-principles.md`「config の値は到達性の検出器を持たない」が言う「既定値の偶然の一致」の実例だった。

## 決定

1. **参照の形は `Default` 実装である**（不足していた `AppearanceConfig` / `HotkeyConfig` に新設）
2. **`*Config::default()` 自身の手書きも先に消す**（消費者を向ける先が写しでは意味が無い）
3. **既定を返す式は例外なく `unwrap_or_else` で受ける**（当初は「確保・I/O を伴うものだけ」という使い分けにしていたが、`/simplify` の指摘を受けて全件を遅延へ揃えた。`unwrap_or_else` は決して高くつかない一方、使い分けは**式のコストを読者が毎回再導出する**ことを要求し、しかも将来 `AppearanceConfig` に `String` フィールドが増えれば `unwrap_or` の側が黙って毎フレーム確保を始める。規則は機械的な方がよい）
4. **「同じ数」でも「同じ概念」でなければ参照へ寄せない**

## 検討した代替案と却下理由

- **`default_*()` を `pub` にして直接呼ぶ**: 却下。`docs/development-principles.md`「config の値は到達性の検出器を持たない」が「**lib crate の `pub` 項目に `dead_code` は出ない**」と明記している。公開面を増やすことは**到達性の検出器を失うこと**であり、この issue が告発している欠陥と同じクラスの穴を新設する。`Default` 実装なら trait 実装であって公開関数を増やさない。
- **`LazyLock<Config>` の静的を置き、全 fallback をそこから読む**（`visual.rs` の `DEFAULT_VISUAL` を一般化する）: 却下。`Config::default()` は `default_scan_paths()` で**ファイルシステムを叩き**（`.exists()`）、`default_language()` で **OS ロケールを読む**。窓幅 1 つの fallback のためにその I/O を静的初期化で走らせるのは過剰である。`DEFAULT_VISUAL` が静的なのは「色 5 本 + font_family の `String` を**構造体まるごと**毎フレーム確保する」という別の理由による。
- **`window_width` に `#[serde(default = "…")]` を足す**: 却下。今日は `[appearance]` に `window_width` が無い TOML は **parse 失敗**し、`.bak` 退避 + 既定起動（`RecoveredFromCorrupt`）へ落ちる。足すと「欠落は正常」へ意味が変わり、**受理する config 形式の変更**（後方互換の判断・`/persistence-check` の領分）になる。既定値をリファクタする PR が持ち込んでよい変更ではない。
- **`window_coordinator.rs` の `inner_size()` 失敗時の `600.0` も参照へ寄せる**: 却下（**一度置換してから差し戻した**）。この `600.0` は `appearance.window_width` の写しではない——読み元は **OS の現在サイズ**であり、これは「問い合わせが失敗したときの便宜値」がたまたま既定幅と同じ数なだけである。参照へ寄せると**存在しなかった結合をコード上の主張として新設**することになり、(a) 本当の欠陥（`window_width = 900` のユーザーで `inner_size()` が失敗すると窓が 600 へ縮む）が「対応済み」に見えて隠れ、(b) それを再検討するために起票した issue の該当項が、コードを読む人に届かなくなる。**重複排除の対象は文字列ではなく概念である**（`AGENTS.md`「検証の作法（全タスク共通）」・#500）。
- **`launcher_controller.rs` の `Language::Ja` を `default_language()` へ寄せる**: 却下。`default_language()` は OS ロケール依存で、非 ja 環境では `En` を返す。**定数と一致しえない**ため、寄せると挙動変更になる。
- **`snotra-settings` の `PRESETS` を既定値から導出する**: 却下。**「導けない」からではなく、フィールドの型を変えない判断による**——`PresetDef` の 5 色を `&'static str` から `String` へ変え、`const PRESETS` を `static PRESETS: LazyLock<[PresetDef; N]>` にすれば、Obsidian エントリは `VisualConfig::default()` から**構造的に導出できる**（`preset_matches` の比較 5 箇所も `&p.bg` へ直す）。それを採らないのは、(a) 既定値のリファクタが UI の型定義と全 4 プリセットの表現を変えることになり、(b) 利用者の裁定が「置換＋不変条件のテスト」であり、(c) 導出した先で「Obsidian だけが既定に追従し他 3 つは追従しない」という非対称が型からは読めなくなるためである。**置換で消せない写しはテストで固定する**——UI が使う述語（`preset_matches`）をそのまま呼ぶ形にした（自前で 5 色を比較すると、UI が守る不変条件より厳しい主張になる）。型を変える案は follow-up の検討対象として残す。
- **`governance:check` に「既定値リテラルの重複」検査を新設する**: 却下。正当なリテラル fallback（`AutoUpdateMode::Disabled` の意図的 fail-safe・`Language::Ja`・レイアウト定数・DragValue の値域など）が 20 箇所以上あり誤爆が必至で、置換後は母集団がほぼ空になる。セーフティネットの変更として合意も要る。
