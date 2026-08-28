//! デシリアライズ後の後処理——レガシーキーの移行・正規化・不正値のフォールバック。
//!
//! **`Config` をデシリアライズする経路は [`Config::apply_migrations`] の適用要否を明示的に
//! 判断する**（迂回すると旧版データの移行漏れが起きる。判断の指針は
//! `snotra-core/CLAUDE.md`「Config のデシリアライズ経路」）。
//!
//! 移行は系統ごとの private fn へ分けてあり、**呼び出し順は [`Config::apply_migrations`] が
//! 持つ**（真の順序依存はそこのコメントが名指しする 1 対だけである）。
//!
//! **検出はここの責務ではない**——不正値を見つけて報告するのは `super::validate` で、ここは
//! 補正する側である（責務分離の経緯は #437）。

use crate::hotkey::HotkeyConfig;
use crate::opener::normalize_openers;

use super::schema::{
    InstantAction, default_fuzzy_history_cap_ratio, default_recent_limit, default_result_limit,
    default_visible_rows,
};
use super::{Config, ScanPath};

impl Config {
    /// Migrate legacy `paths.additional` entries into `paths.scan` with `.lnk` extension.
    fn migrate_additional_to_scan(&mut self) {
        if self.paths.additional.is_empty() {
            return;
        }
        let lnk = ".lnk".to_string();
        for path in self.paths.additional.drain(..) {
            let key = path.to_lowercase();
            if let Some(existing) = self
                .paths
                .scan
                .iter_mut()
                .find(|sp| sp.path.to_lowercase() == key)
            {
                // Same directory already in scan — merge .lnk into its extensions
                if !existing
                    .extensions
                    .iter()
                    .any(|e| e.eq_ignore_ascii_case(&lnk))
                {
                    existing.extensions.push(lnk.clone());
                }
            } else {
                self.paths.scan.push(ScanPath {
                    path,
                    extensions: vec![lnk.clone()],
                    include_folders: false,
                });
            }
        }
    }

    /// Apply post-load migrations: legacy field migration, normalization, invalid hotkey fallback.
    /// Returns true if any changes were applied.
    /// Called by `load()` (auto-save on change) and import (caller decides when to save).
    pub fn apply_migrations(&mut self) -> bool {
        // 呼び出し順は挙動不変のため元のまま固定する。(1)→(5) は真の順序依存
        // （additional→scan で追加された scan エントリを scan 正規化がまとめて dedup する必要がある）。
        // それ以外は独立だが、diff 最小化のため元の並びを保つ。
        let mut changed = false;
        changed |= self.migrate_legacy_additional_paths(); // (1) additional → scan（(5) の normalize より先）
        changed |= self.migrate_legacy_count_params(); // (2) #388 改名マイグレーション
        self.resolve_count_param_defaults(); // (3) None → Some(default)。(2) より後（補完前提）
        changed |= self.sanitize_fuzzy_history_cap_ratio(); // (4) 範囲外値の補正（#437）
        changed |= self.paths.normalize_scan_paths(); // (5) scan path dedup・正規化。(1) より後必須
        changed |= self.normalize_openers(); // opener ターゲットの正規化・具体度ソート
        changed |= self.migrate_instant_legacy_commands(); // 旧 `command` 単一文字列 → `Url`
        changed |= self.fallback_invalid_hotkey(); // parse 不可・system shortcut のデフォルト復帰
        self.dedup_instant_command_names(); // #638: 意図的に changed へ寄与しない（下記 doc）
        changed
    }

    /// #638: 重複名 instant コマンドの先勝ち正規化（in-memory のみ）。実行時解決は
    /// first-match（`commands/instant.rs` / egui `execute_instant_selected` の `find`）のため、
    /// 先頭を残せば実行される action は従来と同一で、候補リストの表示だけが実行と一致する。
    /// 設定 UI は保存時に重複を拒否する（`validate`）ため、ここに来るのは手編集 config のみ。
    /// **意図的に `changed` へ寄与しない**——`load_from_dir_reporting` は changed=true で
    /// `save_to_dir` するため、寄与させるとユーザーの手編集行（重複定義）をファイルから
    /// 消してしまう（spec 2026-07-23 決定 2: 書き戻し禁止）。
    /// 前提条件: この非寄与が防ぐのは「dedup が唯一の変更」のときの書き戻しだけ。他の
    /// レガシー移行が同じロードで changed=true を返す場合、その正当な書き戻しに dedup 済み
    /// 内容が含まれる（実行される action は先頭定義のまま不変・SPEC §19.2 に明記）。
    fn dedup_instant_command_names(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.instant_commands.retain(|c| {
            if c.name.is_empty() {
                return true; // 空名は dedup 対象外（検出=validate / 補正=migration の責務分離。
                // validate の !name.is_empty() エラーを migration が隠さない）
            }
            let keep = seen.insert(c.name.clone());
            if !keep {
                eprintln!(
                    "[config] duplicate instant command name '{}' ignored (first definition wins)",
                    c.name
                );
            }
            keep
        });
    }

    /// (1) 旧 `paths.additional` を `paths.scan` へ移行する（`.lnk` 拡張子付き）。
    fn migrate_legacy_additional_paths(&mut self) -> bool {
        if self.paths.additional.is_empty() {
            return false;
        }
        self.migrate_additional_to_scan();
        true
    }

    /// (2) 件数 config キーの改名マイグレーション（#388）。各新フィールドへ legacy を集約する。
    /// take() で legacy 層を常にクリアし、新フィールドが None（= 新キー未明示）のときだけ
    /// get_or_insert で補完する（新キーが明示されていれば上書きしない＝新優先）。
    fn migrate_legacy_count_params(&mut self) -> bool {
        let mut changed = false;
        // visible_rows ← [appearance].max_results（1層）
        if let Some(v) = self.appearance.max_results.take() {
            self.appearance.visible_rows.get_or_insert(v);
            changed = true;
        }
        // result_limit ← [search].top_n_history（中間）> [appearance].top_n_history（最古）。
        // 両 legacy 層を take() で常にクリアし、search 側を優先する（.or は両引数を評価する）。
        if let Some(v) = self
            .search
            .top_n_history
            .take()
            .or(self.appearance.top_n_history.take())
        {
            self.search.result_limit.get_or_insert(v);
            changed = true;
        }
        // recent_limit ← [search].max_history_display（中間）> [appearance].max_history_display（最古）
        if let Some(v) = self
            .search
            .max_history_display
            .take()
            .or(self.appearance.max_history_display.take())
        {
            self.search.recent_limit.get_or_insert(v);
            changed = true;
        }
        changed
    }

    /// (3) 件数 legacy 移行より後で None → Some(default) に解決する。apply_migrations() 呼び出し後は
    /// 常に Some(v) が保証され、設定画面の DragValue::get_or_insert が no-op になり has_changes() の
    /// 誤発火を防ぐ。旧 legacy フィールドへの get_or_insert は行わない（take 後の再 Some 化で
    /// skip_serializing が無効化されるため）。既定値補完は `changed` に寄与しない（常に実行される
    /// no-op 相当の後始末のため、挙動不変のまま元の実装に合わせて戻り値を持たない）。
    fn resolve_count_param_defaults(&mut self) {
        let _ = self
            .appearance
            .visible_rows
            .get_or_insert_with(default_visible_rows);
        let _ = self
            .search
            .result_limit
            .get_or_insert_with(default_result_limit);
        let _ = self
            .search
            .recent_limit
            .get_or_insert_with(default_recent_limit);
    }

    /// (4) fuzzy_history_cap_ratio が不正（非有限 or [0.0, 1.0] 範囲外）なら既定値へ補正する。
    /// `Config::validate()` は同条件で問題を検出するが補正はしない（検出=validate / 補正=migration
    /// の責務分離。旧 `SearchConfig::sanitize()` の直接処理をここへ移設、issue #437）。
    fn sanitize_fuzzy_history_cap_ratio(&mut self) -> bool {
        let ratio = self.search.fuzzy_history_cap_ratio;
        if !ratio.is_finite() || !(0.0..=1.0).contains(&ratio) {
            self.search.fuzzy_history_cap_ratio = default_fuzzy_history_cap_ratio();
            true
        } else {
            false
        }
    }

    /// 旧 `command` 単一文字列 → `Url` へ無改変移行（自動分割しない＝ゼロ回帰）。
    fn migrate_instant_legacy_commands(&mut self) -> bool {
        let mut changed = false;
        for cmd in &mut self.instant_commands {
            if let InstantAction::Legacy { command } = &mut cmd.action {
                let url = std::mem::take(command);
                cmd.action = InstantAction::Url { url };
                changed = true;
            }
        }
        changed
    }

    /// parse 不能またはシステムショートカットと衝突するホットキーを既定値へ補正する。
    fn fallback_invalid_hotkey(&mut self) -> bool {
        let reason = match self.hotkey.parse() {
            Ok(parsed) if !parsed.is_system_shortcut() => return false,
            Ok(_) => "system shortcut".to_string(),
            Err(error) => error.to_string(),
        };
        let default_hotkey = HotkeyConfig::default();
        eprintln!(
            "[config] invalid hotkey detected ({reason}: {}+{}), falling back to default ({}+{})",
            self.hotkey.modifier, self.hotkey.key, default_hotkey.modifier, default_hotkey.key,
        );
        self.hotkey = default_hotkey;
        true
    }

    /// `Config::default()` に `apply_migrations()` を適用した「正規化済み既定値」を返す。
    ///
    /// `Config::default()` は一部フィールドを `None`（sentinel、明示未設定を表す）のまま返すため、
    /// 読み込み経由（`load()` は必ず `apply_migrations()` を通す）で得た `Some(v)` な Config と
    /// フィールド単位の `PartialEq` を比較すると、`None` を `Some` に解決する順序（DragValue の
    /// `get_or_insert` 等）次第で結果が変わりうる。この関数は正規化を呼び忘れる余地を型レベルで
    /// なくし、`Config::default()` の生値ではなく常にこちらを「比較可能な既定値」として使う。
    pub fn normalized_default() -> Self {
        let mut config = Self::default();
        let _ = config.apply_migrations();
        config
    }

    pub fn normalize_openers(&mut self) -> bool {
        let normalized = normalize_openers(&self.openers);
        if normalized != self.openers {
            self.openers = normalized;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests;
