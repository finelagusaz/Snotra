# research — ホットキーバリデーション Rust/TS 乖離の解消 (#409)

## issue の要約

ホットキーバリデーションの Rust/TS 乖離。TS `ui/src/lib/hotkeyValidation.ts` の `FORBIDDEN_MAIN_KEYS`（CapsLock・IME/かな・Lang/Hangul/Hanja 系の単体キー）に対応する拒否が Rust `Config::validate()` に無く、SPEC §7.4 line 343 の「フロントでも同じリストでガード」が不正確。

**決定方針**: **(c) 孤児コード削除 + SPEC 訂正**（ユーザー判断・2026-06-30）。乖離の根本である死蔵 TS コードを除去し、SPEC を as-built に戻す。

## 調査結果（意図の証跡で裏取り）

ホットキー検証の **live 経路は 3 段**で、いずれも `FORBIDDEN_MAIN_KEYS` を持たない:

1. **snotra-settings キャプチャ UI**（`snotra-settings/src/hotkey_input.rs:108`）: `snotra_core::is_system_shortcut` で即時拒否。`egui_key_to_config_name`（`hotkey_input.rs:142-208`）は **英数 + F1-12 + 一部特殊キー（Space/Enter/Tab/Home/End/PageUp/PageDown/Insert）のみ**をマップ。CapsLock/Eisu/Kana/Convert/Lang/Hangul/Hanja は egui::Key に無く **キャプチャ不能**。
2. **`Config::validate()`**（`snotra-core/src/config.rs:1094` + `is_system_shortcut:1243`）: 空 modifier/key・system shortcut 完全一致・Win+* ワイルドカードを拒否。`FORBIDDEN_MAIN_KEYS` 相当は無し。手動 TOML 編集でのみ到達しうる。
3. **実行時登録**（`src-tauri/src/platform/hotkey.rs:24` `parse_vk` → `register:46`）: `capslock`/`kana` 等は len≠1 かつ match 非該当で **`parse_vk` が 0 を返し `RegisterHotKey` が失敗** → `hotkey-registration-failed` 通知。

**TS `hotkeyValidation.ts`**: `isHotkeyInvalid`/`formatHotkeyLabel` を呼ぶ live パスが `ui/src` に存在しない（`git grep hotkeyValidation` の結果は定義 + 自身の test + SPEC.md:343 + ui/CLAUDE.md:35 のみ）。**孤児コード**。

## 既存パターン（出自）

git 履歴: `57267b4「ホットキーをキーを押したら設定できるようにした」`で**フロント側にホットキー設定 UI があった時代**に追加された。設定が snotra-settings（egui 別バイナリ）へ移管された後、TS バリデータだけが取り残された。SPEC §7.4:343 の「フロントも同じリストでガード」はその移管前の陳腐化記述。

既存 `validate()` の設計哲学: **競合する有効キー**（system shortcut・Win+*）は validate で拒否、**解釈不能キー**（vk=0）は `parse_vk`→`RegisterHotKey` 失敗に委ねる。`FORBIDDEN_MAIN_KEYS` は後者のバケツ（`"foobar"` と同じ vk=0）であり、validate に入れる対象ではない。→ issue 原案 (a) は既存哲学と非対称、(c) が整合。

## 影響範囲（全 tracked ファイル横断で確定）

`git grep -i "hotkeyValidation|hotkey_validation"` の参照（自身を除く）= **SPEC.md:343 / ui/CLAUDE.md:35 の 2 箇所のみ**。barrel `index.ts` 無し。`docs/`・`e2e/` 参照無し。

- **削除**: `ui/src/lib/hotkeyValidation.ts`、`ui/src/lib/hotkeyValidation.test.ts`
- **SPEC.md:343 訂正**: フロントガード節を削除し実態に
- **ui/CLAUDE.md:35 削除**: モジュール構成エントリ除去

## 技術的制約

- TS/doc 変更のみ。Rust コード・IPC・Win32 への波及なし（`Config::validate` / `hotkey_input.rs` / `hotkey.rs` は**現状維持**＝意図的に触らない）。
- 削除の安全性: `git grep` が import 文を exhaustive に捕捉済み（live import ゼロ）。typecheck（カテゴリ B）が最終 backstop。

## 未解決の疑問

なし。設計判断はユーザーが (c) を選択済み。死蔵は git grep で exhaustive に確定。
