//! `config.toml` の読み書きと、TOML 文字列との相互変換。
//!
//! **読み込み失敗は種類で扱いを分ける**——不在・内容破損・一時的失敗で保全方針が違う。区分は
//! [`LoadOutcome`] が、各枝が何を保全するかは [`Config::load_from_dir_reporting`] の rustdoc が
//! 正本である（`Err(_)` 一括の first-run 扱いは実データを既定値で潰す・#338/#343）。

use std::fs;
use std::path::Path;

use super::Config;

/// `Config::load_reporting()` の結果区分。UI 文字列を持たない（表示・通知は呼び出し側の責務）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadOutcome {
    /// 正常に parse できた。
    Loaded,
    /// 設定ファイルが存在せず（first-run）、既定値を生成・保存した。
    FirstRun,
    /// 内容が壊れていた（TOML parse 失敗 or 非 UTF-8）。`config.toml.bak` へ退避し既定値で起動。
    RecoveredFromCorrupt,
    /// 一時的・環境的な read 失敗（権限/ロック等）。既存ファイルを退避も上書きもせず既定値で起動。
    ReadFailed,
}

impl Config {
    pub fn load() -> Self {
        Self::load_reporting().0
    }

    /// `load()` と同じ読み込みを行い、結果区分（`LoadOutcome`）も返す。
    /// 退避通知（トレイ）や読込失敗時の保存ガード（設定画面）が結果を判断するために使う。
    /// `config_dir` が解決できない極端な環境では `(default, FirstRun)` を返す。
    pub fn load_reporting() -> (Self, LoadOutcome) {
        let Some(dir) = Self::config_dir() else {
            return (Self::default(), LoadOutcome::FirstRun);
        };
        Self::load_from_dir_reporting(&dir)
    }

    /// `dir`/config.toml を読み込むコア（`config_dir` を注入可能にし統合テストする）。
    /// - parse 成功: migration 後、変化があれば保存 → `Loaded`
    /// - parse 失敗: ログ + `.bak` 退避 + in-memory default（保存しない）→ `RecoveredFromCorrupt`
    /// - read 失敗 (NotFound): first-run。default を生成・保存 → `FirstRun`
    /// - read 失敗 (InvalidData = 非 UTF-8): 壊れた永続データ。`.bak` 退避 + default → `RecoveredFromCorrupt`
    /// - read 失敗 (その他: permission/lock 等): 退避も上書きもせず default → `ReadFailed`
    fn load_from_dir_reporting(dir: &Path) -> (Self, LoadOutcome) {
        let path = dir.join("config.toml");
        match fs::read_to_string(&path) {
            Ok(content) => match toml::from_str::<Self>(&content) {
                Ok(mut config) => {
                    // 正常系: 従来どおり migration → 変化があれば save
                    if config.apply_migrations() {
                        let _ = config.save_to_dir(dir);
                    }
                    (config, LoadOutcome::Loaded)
                }
                Err(e) => {
                    // TOML parse 失敗（ユーザーの構文ミス・破損等）。
                    // 黙ってデフォルトで上書きしない（snotra-core/CLAUDE.md:
                    // deserialize_failed → save() はデータ喪失を招く）。
                    // エラーを可視化し、不正ファイルを .bak へ退避してから
                    // in-memory default で続行する（save() しない）。
                    eprintln!("[config] failed to parse {}: {e}", path.display());
                    Self::backup_invalid(&path);
                    (Self::default(), LoadOutcome::RecoveredFromCorrupt)
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // first-run / ファイル不在: default を生成・保存
                let config = Self::default();
                let _ = config.save_to_dir(dir);
                (config, LoadOutcome::FirstRun)
            }
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                // 不正な UTF-8 = 壊れた永続データ。parse 失敗と同質なので、同じく
                // byte-preserving に .bak へ退避してから default で起動する。
                // canonical path に残すと後続 save() が破損元を上書きし、.bak にも
                // 残らず失われる（parse 失敗との保全方針の非対称を解消）。
                eprintln!("[config] {} is not valid UTF-8: {e}", path.display());
                Self::backup_invalid(&path);
                (Self::default(), LoadOutcome::RecoveredFromCorrupt)
            }
            Err(e) => {
                // permission / sharing violation / ロック等の一時的・環境的 read 失敗。
                // ファイル内容は壊れていない可能性が高く読めないだけなので、退避も
                // 上書きもせず default で起動する（読めないファイルは安全に退避できない）。
                // `Err(_)` 一括 first-run 扱いは一時的 read 失敗で実設定を default に
                // 潰すデータ損失経路になるため避ける。
                eprintln!(
                    "[config] failed to read {}: {e} (running on defaults; file NOT overwritten)",
                    path.display()
                );
                (Self::default(), LoadOutcome::ReadFailed)
            }
        }
    }

    /// Best-effort: 解析不能な config ファイルを `<path>.bak` へ退避（移動）し、
    /// ユーザーが手動復旧できるようにする。結果をログする。panic しない。
    /// 退避に失敗した場合は元ファイルをその場に残し（default で上書きしない）、
    /// ログして default 続行する。
    fn backup_invalid(path: &Path) {
        let bak = path.with_extension("toml.bak");
        match fs::rename(path, &bak) {
            Ok(()) => eprintln!(
                "[config] backed up unparseable config to {} (running on defaults; original NOT overwritten)",
                bak.display()
            ),
            Err(e) => eprintln!(
                "[config] failed to back up unparseable config at {}: {e} (running on defaults; original left in place)",
                path.display()
            ),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let dir = Self::config_dir().ok_or("設定ディレクトリが見つかりません")?;
        self.save_to_dir(&dir)
    }

    /// `dir`/config.toml へ atomic 保存する（`load_from_dir` と対の注入ポイント）。
    fn save_to_dir(&self, dir: &Path) -> Result<(), String> {
        fs::create_dir_all(dir).map_err(|e| format!("ディレクトリ作成失敗: {e}"))?;

        let path = dir.join("config.toml");
        let content = toml::to_string_pretty(self).map_err(|e| format!("シリアライズ失敗: {e}"))?;

        // Atomic write: .tmp → rename
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, content).map_err(|e| format!("書き込み失敗: {e}"))?;
        fs::rename(&tmp, &path).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            format!("リネーム失敗: {e}")
        })
    }

    /// Parse a TOML string into a Config, filling missing keys with defaults.
    /// Does NOT run migration or auto-save (unlike `load()`).
    pub fn from_toml_str(s: &str) -> Result<Self, String> {
        toml::from_str(s).map_err(|e| e.to_string())
    }

    /// Generate a default export filename like `config_202603111430.toml`.
    /// Caller provides local time components (year, month, day, hour, minute).
    pub fn export_filename(year: u16, month: u16, day: u16, hour: u16, minute: u16) -> String {
        format!("config_{year:04}{month:02}{day:02}{hour:02}{minute:02}.toml")
    }
}

#[cfg(test)]
mod tests;
