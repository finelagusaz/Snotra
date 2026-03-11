# 実装計画: Issue #236 インポートエラーメッセージ改善

## 変更ファイル一覧

| ファイル | 変更内容 |
|---|---|
| `snotra-settings/src/tabs/backup.rs` | `first_line()` 削除 + `localize_toml_error()` 追加 |
| `snotra-settings/src/i18n.rs` | JA 向けエラー文字列 3 本追加 |

`snotra-core` は変更なし（`from_toml_str` は既に全文を返している）。

---

## 実装順序

### フェーズ 1: `first_line()` 切り捨てを排除

`backup.rs` の `handle_import_result` で、パースエラー・バリデーションエラーに使っている `first_line(&e)` を削除し、全文を表示する。

**変更前（backup.rs:213）:**
```rust
Err(e) => {
    return (Some(format!("{}{}", tr.status_import_failed(), first_line(&e))), true, None);
}
```

**変更後:**
```rust
Err(e) => {
    return (Some(format!("{}{}", tr.status_import_failed(), localize_toml_error(&e, tr))), true, None);
}
```

バリデーションエラー（backup.rs:220）も同様に `{:?}` → より可読なフォーマットに変更。

### フェーズ 2: `localize_toml_error()` 追加

`backup.rs` に以下のヘルパーを追加:

```rust
/// toml 1.0 のエラー文字列を言語に応じて整形する。
/// - En: そのまま返す
/// - Ja: 先頭に行番号を含む日本語サマリーを付け、その後に英語原文を続ける
fn localize_toml_error(msg: &str, tr: &Tr) -> String {
    use snotra_core::config::Language;
    if tr.0 == Language::En {
        return msg.to_string();
    }

    // 1行目から行番号を抽出: "TOML parse error at line N, column M"
    let line_num: Option<u32> = msg
        .lines()
        .next()
        .and_then(|first| first.split("at line ").nth(1))
        .and_then(|rest| rest.split([',', ' ']).next())
        .and_then(|n| n.trim().parse().ok());

    // 視覚コンテキスト行（| / ^ で始まる行）を除いた最後の行がエラー本文
    let desc = msg
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with('|') && !t.starts_with('^')
        })
        .last()
        .unwrap_or(msg);

    // パターンマッチで日本語サマリーを生成
    let ja_summary = if desc.contains("missing field") {
        // missing field `target` → 「"target" が必要です」
        let field = extract_backtick(desc).unwrap_or(desc);
        tr.err_toml_missing_field(field)
    } else if desc.contains("invalid type") {
        tr.err_toml_invalid_type()
    } else {
        // 構文エラー等: 汎用メッセージ
        tr.err_toml_parse_error()
    };

    // 行番号を付けて日本語サマリー + 英語原文を返す
    match line_num {
        Some(n) => format!("行 {}: {}\n\n{}", n, ja_summary, msg),
        None => format!("{}\n\n{}", ja_summary, msg),
    }
}

/// バッククォートで囲まれた最初の語を抽出: `target` → "target"
fn extract_backtick(s: &str) -> Option<&str> {
    let start = s.find('`')? + 1;
    let end = s[start..].find('`')? + start;
    Some(&s[start..end])
}
```

### フェーズ 3: `i18n.rs` にエラー文字列追加

`Tr` に以下のメソッドを追加:

```rust
pub fn err_toml_missing_field<'a>(&self, field: &'a str) -> String {
    match self.0 {
        Language::Ja => format!("\"{}\" が必要です", field),
        Language::En => format!("missing field \"{}\"", field),
    }
}

pub fn err_toml_invalid_type(&self) -> &'static str {
    match self.0 {
        Language::Ja => "値の型が違います",
        Language::En => "invalid type",
    }
}

pub fn err_toml_parse_error(&self) -> &'static str {
    match self.0 {
        Language::Ja => "構文エラー",
        Language::En => "syntax error",
    }
}
```

### フェーズ 4: バリデーションエラーの表示改善

`backup.rs:220` は現状 `{:?}` Debug 表示:

```rust
// 現状: ScanPathEmpty { index: 0 } のような生の Rust 型が出る
format!("{}{:?}", tr.status_import_validation_error(), errors[0])
```

`app.rs` にはすでに `config_error_message(&ConfigError, &Tr) -> String` 関数があり、
各バリアントを `tr.*()` で翻訳済みテキストに変換している。この関数を共有する。

**変更手順:**

1. `app.rs` の `config_error_message()` を `i18n.rs` の `Tr` メソッド（`format_config_error`）に移動
2. `app.rs` は `tr.format_config_error(&e)` に書き換え（挙動変わらず）
3. `backup.rs:220` も `tr.format_config_error(&errors[0])` に変更

**なぜフェーズ4が必要か:**

インポートの主要ユースケースは「別マシンへの移動」「手書き編集」「マシン復元後」であり、
スキャンパスの不整合・型ミスマッチなどのバリデーションエラーはインポート時にこそ起きやすい。
`ScanPathEmpty { index: 0 }` という生の Debug 出力はユーザーに意味を伝えない。

---

## 不変条件

- `first_line()` 関数はエクスポートエラー（`handle_export_result`）でも使用中。そちらは I/O エラーで改行を含まないため変更不要（変更しない）
- `state.message` は次の操作開始時にクリアされる（`state.message.clear()` はボタン押下時）。多行エラーでもこの動作は維持する
- `from_toml_str` の戻り値型は変更しない。callers への影響なし
- egui `Label` は `\n` を改行として扱う。スクロールエリア内でラベル高さが増えても問題なし

---

## テスト方針

- `snotra-core` の既存テスト (`cargo test -p snotra-core`) が継続してパスすること
- `cargo check -p snotra-settings` でコンパイルエラーなし
- 手動動作確認:
  1. 存在しないファイルを選択 → ファイルアクセスエラー（英語・変更なし）
  2. 構文エラーのある TOML を import → 「行 N: 構文エラー\n\n(英語原文)」
  3. `max_results = "ten"` を含む TOML → 「行 N: 値の型が違います\n\n(英語原文)」
  4. `[hotkey]` セクションを消した TOML → 「行 N: "hotkey" が必要です\n\n(英語原文)」

---

## SPEC.md 更新要否

なし。エラーメッセージの改善は UX 変更だが、機能仕様変更ではない。

---

## セルフレビュー

### 対称コードパス確認

- `handle_export_result` の `first_line()` は I/O エラー用で改行を含まない → **変更不要（意図的に除外）**
- バリデーションエラー (`validate()` の出力) も `first_line()` を使わず直接表示に変更 → **適用済み**

### YAGNI 違反チェック

- `localize_toml_error()` は 3 パターンのみ。issue で D 以降とされた独自バリデーションは含まない ✓
- トラック 2（ドキュメント整備）は別 issue で対応 ✓

### シンプル化の挑戦

- `Tr::err_toml_missing_field` が `String` を返す（他のメソッドは `&'static str`）。これは field 名を埋め込むため避けられない。フォーマット文字列 `&'static str` + 呼び出し側で format! する案も同等のコスト → 現案を維持
- `extract_backtick()` はシンプルな文字列操作のみ。正規表現不要 ✓

### 破壊不変条件

- `handle_import_result` の戻り値型 `(Option<String>, bool, Option<Config>)` は変わらない → app.rs 側に影響なし ✓
