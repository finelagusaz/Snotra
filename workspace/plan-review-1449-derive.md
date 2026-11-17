# #1449 の変更の独立導出（plan.md / research.md / adversarial-*.txt は読んでいない）

導出の元: issue #1449 の本文と 2 つのコメント、`src-tauri/src/egui_shell/font_stack.rs`（HEAD = 77eb5899 系の作業ツリー）、
依存クレートのソース（epaint 0.37.0・fontdb 0.24.0・memmap2 0.9.12・windows 0.62.2）。コードは編集していない。

## 0. 前提として確かめた事実（以後の判断の根拠）

- epaint 0.37 `FontData::from_blob(font: Blob, index: u32)`。`Blob = Arc<dyn AsRef<[u8]> + Send + Sync>`
  （`epaint-0.37.0/src/text/font_data.rs:10,40`）。`from_static` / `from_owned` も中で `from_blob(Arc::new(..), 0)` を呼ぶだけ（同 :31-36）。
  → **`from_blob` は index を引数で取る**。今の `uf.index = face_index` の代入は `from_blob(blob, face_index)` へ畳める。
- memmap2 0.9.12: `impl AsRef<[u8]> for Mmap`（`lib.rs:1107`）、`MmapInner` は `unsafe impl Send/Sync`（`windows.rs:531-532`）。
  → `Arc<Mmap>`（または Mmap を持つ自前の型）はそのまま `Blob` に入る。
- memmap2 の Windows 実装は、写像の後で**ファイルハンドルを `DuplicateHandle` で複製して `Mmap` の中に持つ**（`windows.rs:204-223`、drop で `UnmapViewOfFile` → `CloseHandle`、`:510-525`）。写像オブジェクト（section）のハンドルは直後に閉じる（`:198`）。
  長さ 0 のファイルは写像を作らず、ハンドルも持たない（`:166-181`）。
- fontdb 0.24 は既定 feature で `memmap` が有効（`fontdb-0.24.0/Cargo.toml` の `default`）。src-tauri は `fontdb = "0.24"`（既定 feature）。
  - `Database::with_face_data` は `Source::File` のとき**その場で `File::open`（std 既定の共有モード = READ|WRITE|DELETE）して写像し、クロージャの間だけ使う**（`lib.rs:907-912`）。今の `resolve_font_family` はその写像を `data.to_vec()` でヒープへ写している（`font_stack.rs:147-149`）。
  - `Database::face_source(id) -> Option<(Source, u32)>`（`lib.rs:700`）が公開されている。
  - `Source::Binary(Arc<dyn AsRef<[u8]> + Sync + Send>)` / `Source::SharedFile(PathBuf, Arc<..>)` の Arc は **`Blob` と同じ型**（`lib.rs:866-879`）。
  - `load_system_fonts`（Windows）は `%SYSTEMROOT%\Fonts`・`%USERPROFILE%\AppData\Local\...\Fonts`・`...\Roaming\...\Fonts` を走査し、**非 symlink のファイルは `read_dir` のパスをそのまま** `Source::File` にする（`lib.rs:402-422, 311-327, 236`）。
    → user_font のパスは `C:\WINDOWS\Fonts\HackGen....ttf` のような綴り（`SYSTEMROOT` の大小文字に従う）。jp_font の候補は `C:/Windows/Fonts/YuGothM.ttc`（`font_stack.rs:159`）。**同じファイルでも文字列は一致しない。**
  - fontdb 自身の `make_shared_face_data` は `std::fs::File::open`（共有モードを絞らない）で写像する（`lib.rs:757-760`）——issue の決定（`FILE_SHARE_READ` だけ）を満たさないので**使えない**。
- windows 0.62.2: `GetFinalPathNameByHandleW` / `GetDriveTypeW` / `FILE_SHARE_READ` / `VOLUME_NAME_GUID` / `FILE_NAME_NORMALIZED` は `Win32_Storage_FileSystem`（src-tauri で有効化済み）。**`DRIVE_FIXED` は `Win32_System_WindowsProgramming`（未有効）**にある。feature を足すか、値 3 をローカル定数にするかを決める（後者を推す。feature 1 つのために API 面を広げない。ただし定数の出典を doc に書く）。
- 共有モードは std だけで指定できる: `std::os::windows::fs::OpenOptionsExt::share_mode(FILE_SHARE_READ.0)`。CreateFileW を直接呼ぶ必要は無い。

## 1. 変更が必要なファイルとシンボル

### `src-tauri/src/egui_shell/font_stack.rs`（本体。ほぼすべてここ）

| シンボル | 今（行） | 変更 |
|---|---|---|
| `JP_FONT` | `Mutex<Option<&'static [u8]>>`（:20） | `Mutex<Option<egui::epaint::text::Blob>>`（以下 `Blob`。**綴りは確定**: `egui-0.37.0/src/lib.rs:446` が `pub use epaint;`、`epaint-0.37.0/src/text/mod.rs:23` が `font_data::{Blob, ..}` を re-export。`egui::text` には `Blob` が無い（`lib.rs:471-474`））。**錠を読み込みの間も持つ形（#1452）は保つ**。 |
| `font_definitions` | `(Option<&'static [u8]>, Option<(&'static [u8], u32)>)`（:59-104） | 引数を `Option<Blob>` / `Option<(Blob, u32)>` に。`FontData::from_static(..)` → `FontData::from_blob(blob, index)`（:66, :77-78）。 |
| `ResolvedFont` | `(&'static [u8], u32)`（:107） | `(Blob, u32)`。`Copy` でなくなるので `map.get(name)` のヒットは `.clone()`（Arc の clone）にする（:134-135）。 |
| `USER_FONTS` | family 名 → leak 済みスライス（:121） | family 名 → `(Blob, u32)`。never-clear は残す（理由は §3）。 |
| **新設: 写像のキャッシュ**（例 `FONT_FILES: Mutex<HashMap<PathBuf, Blob>>` を持つ型） | 無い | **ファイルのパス → Blob**。user_font と jp_font の両方がここを通る。キーは §5-2。 |
| **新設: 1 ファイルを開いて写像する関数**（例 `map_font_file(path) -> io::Result<MappedFont>`） | 無い | `OpenOptions::new().read(true).share_mode(FILE_SHARE_READ.0).open(path)` → 置き場所の判定（§4）→ `unsafe { Mmap::map(&file) }` → `MappedFont { map, file }` を返す。 |
| **新設: 写像か複製かを決めて Blob を返す関数**（例 `load_font_file(path) -> Option<Blob>`） | 無い | 写像に成功すれば写像、開けない以外の理由（置き場所が不適・写像失敗・長さ 0）なら `std::fs::read` → `Arc<Vec<u8>>`（今の方法）。開けない（存在しない・共有違反）ときの扱いは §5-4。 |
| `resolve_font_family` | `with_face_data` + `to_vec` + `Box::leak`（:147-149） | `db.face_source(id)?` で `(Source, face_index)` を取り、`Source::File(path)` → `load_font_file(&path)`、`Source::Binary(a)` / `Source::SharedFile(_, a)` → `a`（そのまま Blob。複製しない）。**`with_face_data` は使わない**（§5-1）。 |
| `jp_font_bytes` | `std::fs::read` + `Box::leak`（:157-173） | `CANDIDATES.iter().find_map(|p| load_font_file(p))`。戻り値 `Option<Blob>`。 |
| `resolve_stack` | `(Option<&'static [u8]>, Option<ResolvedFont>)`（:177） | `(Option<Blob>, Option<ResolvedFont>)`。`font_covers_cjk(bytes, ..)` へは `(*blob).as_ref()` を渡す（:183）。 |
| `configure_japanese_font` / `prefetch` / `font_family_from_config` | | シグネチャ不変（呼び出し元 `view.rs:534,648`・`results_view.rs:564,594`・`main.rs:229` 経由の `mod.rs:27` は無変更で済む）。 |
| テスト `font_definitions_*`（:232-308） | `&'static [u8]` のダミー | `Arc::new(..) as Blob` へ。`font_definitions_registers_both_fonts_without_copy` は `Blob` の `as_ptr` と比べる形でそのまま意味を保てる。 |
| 新設テスト | | §6。 |

根拠: 変更が要る型・関数の一覧は `git grep -n "'static\|Box::leak\|from_static" src-tauri/src` が `font_stack.rs` の外に 0 件（フォント関連）であること、`configure_japanese_font` 以外の公開シンボルが無いこと（`mod.rs:26-27`）から、波及は `font_stack.rs` の中で閉じる。

### `src-tauri/Cargo.toml`

- `[target.'cfg(windows)'.dependencies]` か `[dependencies]` に **`memmap2 = "0.9"`** を足す。Cargo.lock は既に `memmap2 0.9.12`（fontdb 経由・`Cargo.lock:2918`）を持つので、新しいクレートは入らない（lock の dependencies 欄に 1 行増えるだけ）。直接依存にする理由を隣にコメントで書く（`ttf-parser` の行と同じ流儀）。
- windows の feature は増やさない（`DRIVE_FIXED` をローカル定数にする場合）。足すなら `Win32_System_WindowsProgramming`。

### 新しい `.rs` ファイルを作るか

推すのは**作らない**（`font_stack.rs` の中に 60〜90 行で収まる）。作るなら `//!` と `src-tauri/CLAUDE.md`「モジュール構成」の追記が要る（`AGENTS.md` の表・`governance:check` が照合）。
Win32 を呼ぶが、ファイル API はスレッドを問わないので `PlatformBridge`（`.claude/rules/src-tauri.md`）を通す対象ではない——と計画に一言書いておくと、レビューで問われない。

### 触らないもの（確かめた上で）

- `snotra-settings/src/font.rs`（`from_owned` で読む）: 別プロセス（設定サイドカー）であり、issue の受け入れ（本体の PrivComm）の外。共有モードの観点では、サイドカーは `std::fs::read`（読み取りアクセス・全共有）なので、本体が `FILE_SHARE_READ` で持っていても開ける。
- `snotra-egui-runtime/src/raster.rs` の `font_bytes`: アトラスのテクスチャの話で無関係。
- `view.rs` / `results_view.rs` / `visual.rs` / `main.rs`: 呼び出し点の名前だけで、型は見ていない。

## 2. 変更で偽になる散文

### `font_stack.rs` の doc（すべて書き直し対象）

- `//!` :4-5「フォントのバイト列は `Box::leak` で `'static` にし、`JP_FONT` と `USER_FONTS` に 1 度だけ置く（never-clear。消すと、次の解決が同じバイト列をもう一度漏らす）」→ 偽。写像（失敗時は複製）を `Blob` で持ち、パスをキーに 1 度だけ写像する、へ。never-clear の理由も変わる（§3）。
- `JP_FONT` :17-19「錠は読み込みの間も持つ——…13 MiB を 2 度読まない（#1452）」→ 写像経路では「読む」ではなく「写像する」。錠を持つ理由（2 回開かない）は残るので言い換え。
- `font_covers_cjk` :37-38「`YuGothM.ttc`（13.26 MiB）+ egui のグリフ機構を丸ごと積まずに済む」→ 写像後は 13.26 MiB は private commit ではなくなる。jp を積まないことで今も省けるのは、epaint の解析・グリフの分と、写像（ファイルの施錠）そのもの。数値の主張は弱める。
- `font_definitions` :51-58「**両フォントとも `&'static [u8]` で受け取り `from_static` で積む。**」「静的参照で積む形はそのまま保っている」→ 偽。`Blob` で受け取り `from_blob` で積む、へ。0.36 の経緯（`from_owned` で 2 倍）は、`ptr::eq` テストが守るものの説明として残してよい。
- `ResolvedFont` :106「`'static` バイト列」→ 偽。
- `USER_FONTS` :109-120「`Box::leak` を素で置くと 1 回の font_family 変更で 2 回漏れる」「never-clear は leak の重複を避ける一意性の要請であって memory safety の要請ではない（`Box::leak` はそれ自体が `'static` を生む）」→ 偽。leak が無くなる。キャッシュの役目は「`fontdb` の全走査（15〜20 ms・PERFORMANCE.md #1452 節）を 2 Context と再切替で 1 回にする」へ移る。
- `resolve_font_family` :123-128「(`'static` バイト列, face index) を返す」「バイト列を `'static` へ leak するのは `font_definitions` が `from_static` で積むため」→ 偽。
- `jp_font_bytes` :154「遅延ロードして `'static` 借用を返す」→ 偽。:156「13.26 MiB は確保されない」は写像後は意味が変わる（もともと確保ではなくなる）。
- `resolve_stack` :178-180「jp を先に読むと、CJK をカバーするフォントでも 13.26 MiB が常駐する。この順序が削減の実体である」→ 写像後は 13.26 MiB は private commit にならないので「削減の実体」の主張は弱まる。順序が守るものは、jp の写像（YuGothM の施錠）と epaint の解析・グリフ分、写像できない場所での複製。書き直す。
- `prefetch` :194「解決の結果も漏らす回数も変わらない」→「写像する回数」へ。
- テスト :295-297 の「常駐 2 倍化に対する退行テスト」: 意味は保てる（Blob の番地で縛る）。文言の `静的スライス` を直す。

### `PERFORMANCE.md`（現在形の主張だけ）

（:37 の「遡及して補完しない」は出所（機体名など）の補完を禁じる規約であって、偽になった現在形の主張を放置する根拠ではない。下の 3 か所は**偽になる**ので処置が要る。処置の形は :139-142 に既にある節頭の引用ブロック「この節の〜は現運用点に当てはまらない——機構〜だけが生きている」に合わせ、本文は書き換えずに注記で「#1449 以降は当てはまらない」と指すのを推す。）

- :139-142 の注記「機構（`from_static` / cmap カバー判定 / Context 数への比例）だけが生きている」→ **偽になる**（`from_static` が `from_blob` に替わる）。注記の 1 語を直すか、「`from_static`（#1449 で写像の `from_blob` へ）」と添える。
- :199-201「`JP_FONT_BYTES` の `OnceLock` は set-once・never-clear（`transmute` による `'static` 化の健全性の根拠）」→ 型名・根拠は**既に現行と違う**（現在は `JP_FONT: Mutex<Option<&'static [u8]>>`・`Box::leak`）。#1449 で「`'static` 化の健全性の根拠」は完全に無くなる。「一度読むと…解放されない」は、キャッシュが never-clear のままなら真（写像のまま残る）。
- :225-226「leak は family 名をキーにしたキャッシュで共有する…漏れは distinct family 数で頭打ち・解放はされない」→ **偽になる**（leak が無くなる。キャッシュは残り、写像は解放されない）。
- :2728「**フォントのバイト列は 1:1 で private commit に載る。**」→ **偽になる**（写像経路では private commit に載らない。複製に倒れたときだけ載る）。同節の :2733「フォントの leak 1 block」は計測の記述なので、節頭の注記で足りる。
- :3413「jp_font は 2 スレッドが同時に 13 MiB を読まないよう、錠で読み手を 1 つに絞った」: 錠は残るので偽にはならない（写像では「読む」が「写像する」になるだけ）。
- **新設**: 「採用: フォントファイルを `FILE_SHARE_READ` で開いて写像する（#1449）」節。規約（:33 出所・:35 機体名）に従い、日付・機体・release・font_family 別・標本数で、アイドル/表示/検索/hide の PrivComm と mapped、windows_create を書く。代償（ファイルの施錠）と、写像しない場所の判定もここか doc に。

### そのほか

grep の範囲（切り詰めなしで引き直した）: `git grep -n -E "Box::leak|フォントの leak|13\.26|10\.21|fs::read\(|jp_font|user_font|font_family"` と `leak|漏` × `font|フォント` を、`workspace/`・凍結 3 ディレクトリ・`Cargo.lock`・`font_stack.rs`・`PERFORMANCE.md` を除く全体で。フォントのバイト列の持ち方に触れる散文は、`font_stack.rs` と `PERFORMANCE.md` の外に無かった（ヒットは `SPEC.md:101,442,710`・`scripts/manual-smoke.ps1:160`・`scripts/measure-memory-stages.ps1:8`・`scripts/visual-input-metrics.ps1:172`・`src-tauri/Cargo.toml:28`・`view.rs` / `results_view.rs` / `visual.rs` / `mod.rs` / `main.rs` の font_family の配線で、どれも持ち方を述べない）。`heap_trace.rs` と `snotra-core/tests/memory_footprint.rs` にもフォントの leak を前提にした記述は無い。

- `SPEC.md:101`: jp_font の有無の話で、バイト列の持ち方には触れない。偽にならない。
- `SPEC.md:710`（font_family の節）: 偽にはならないが、**利用者から見える挙動の変化（Snotra の動作中は、写像したフォントファイルを更新・削除・改名できない）を書く場所はここ**。書くかどうかは判断点（§5-6）。
- `src-tauri/CLAUDE.md:31`「`font_stack.rs` — フォント解決と `set_fonts` 登録」: 偽にならない（新ファイルを作らない限り）。
- ルート `Cargo.toml:11-12` のコメント: 偽にならない。
- `scripts/measure-memory-stages.ps1:8` の例（2026-07-25 実測）: 日付つきの歴史。触らない。
- `docs/architecture.md` / `docs/build-commands.md`: フォントのバイト列の持ち方に触れる記述は grep で 0 件。
- `docs/adr` / `docs/design` / `docs/superpowers`: 除外。

## 3. `unsafe`（`Mmap::map`）の根拠と、それを満たす実装の形

`Mmap::map` の契約: 写像が生きている間、**下にあるファイルの内容が変わらない（切り詰められない）こと**。破られると、Rust の `&[u8]` が指す不変であるべき記憶が変わる（UB）。切り詰め・削除の場合はページの読み込みが失敗して `EXCEPTION_IN_PAGE_ERROR` で落ちる。

### 満たすための条件（4 つ）

1. **書き込みと削除を拒む共有モードで開いたハンドルが、写像より長く生きる。**
   - `FILE_SHARE_READ` だけで開くと、その後の `GENERIC_WRITE` / `FILE_WRITE_DATA` / `FILE_APPEND_DATA` / `DELETE` を要求する open は共有違反で失敗する（他プロセスでも同じ）。名前の変更・削除も `DELETE` アクセスが要るので拒まれる。
   - **開けた時点で、書き込み・削除のアクセスを持つ既存のハンドルは無い**ことも同時に保証される（あればこちらの open が共有違反で失敗する）。
   - **ハンドルの寿命を memmap2 の内部に頼らない。** memmap2 0.9.12 は `DuplicateHandle` でハンドルを中に持つ（`windows.rs:204-223`）ので、それだけでも共有の制限は続く。だが、これは実装の詳細で契約ではない。共有アクセスはファイルオブジェクトの最後のハンドルが閉じた時点（cleanup）で外れ、section が残っていても外れる、という Windows の挙動を考えると、**自分で `File` を持つ**のが根拠を版に依存させない形になる:
     ```rust
     struct MappedFont {
         map: memmap2::Mmap, // 先に宣言 = 先に drop（写像を解いてからハンドルを閉じる）
         _file: std::fs::File,
     }
     impl AsRef<[u8]> for MappedFont { fn as_ref(&self) -> &[u8] { &self.map } }
     ```
     フィールドの drop 順は宣言順なので、`map` を先に置くと「ハンドルが閉じて共有の制限が外れた後も写像が残る」瞬間が無い。
2. **写像の寿命は `Arc` が決める。** `Blob` に入れて epaint へ渡すので、写像は最後の `FontData` / `FontsImpl` が手放すまで生きる。`'static` は不要になり、**never-clear は memory safety の要請ではなくなる**（今の `USER_FONTS` の doc と同じ立場が、`JP_FONT` 側にも及ぶ）。
3. **キャッシュの寿命（never-clear）の理由は「同じファイルを 1 回だけ写像する」と「fontdb の全走査を 1 回にする」へ移る。** 消しても安全性は崩れないが、消すと、次の解決で同じファイルの写像が 2 つ目になる（物理ページは section が共有するので commit は増えないが、仮想アドレスとハンドルが増える）し、fontdb の走査をもう一度払う。代償: 別の font_family へ切り替えても、前のフォントファイルはプロセス終了まで施錠されたまま（issue の決定が受け入れた代償の範囲。doc に書く）。
4. **`Send` / `Sync`**: `Mmap` は `unsafe impl Send + Sync`（`windows.rs:531-532`）、`File` は `Send + Sync`。`MappedFont` は自動で両方になり、`Arc<MappedFont>` は `Blob`（`Send + Sync` 要求）へ unsize できる。static の `Mutex<HashMap<_, Blob>>` も `Sync`。`unsafe impl` は書かない。

### `// SAFETY:` に書くべき内容（散文の骨子）

- `file` は `FILE_SHARE_READ` だけで開いた。開けたので、書き込み・削除のアクセスを持つ他のハンドルは今は無く、`file` が生きている間は新しく作れない（共有違反）。
- `file` は `MappedFont` の中で写像と一緒に持ち、写像より後に閉じる（フィールド順）。
- 置き場所は §4 の判定で「ページの読み込みが失敗しうる場所」を除いた。
- **残る経路（受け入れる）**: カーネルモード（フィルタードライバ・生のボリューム書き込み）、共有モードを強制しないファイルシステム（§4 で DRIVE_FIXED 以外を除いても、仮想ドライブ製品が DRIVE_FIXED を名乗る可能性）、外付け HDD（DRIVE_FIXED）の取り外し、ディスクの I/O エラー。最後の 2 つは Snotra 自身の実行ファイルのページにも同じく当たる。
- ⚠ 確信が持てない点: 他プロセスが**ハンドルを閉じた後も書き込み可能な写像（section）を持ち続けている**場合、こちらの `FILE_SHARE_READ` の open が失敗するか（Windows 8 以降の `IoCheckShareAccessEx` が user-writable な section 参照を見る、と記憶しているが未確認）。フォントファイルでは事実上起きない。**`// SAFETY:` は「書き込み・削除のアクセスを持つ既存の**ハンドル**は無い」と書き、「既存の書き手は無い」という全称にはしない**。実機確認の項目として残す。

### 錠の順序（`/race-check` の対象）

先読みスレッド（`prefetch`）と main / results の `setup`・font_family の切り替えが、3 つの錠を取る。

- `USER_FONTS` の錠は fontdb の全走査の間も持っている（:133 で取り、関数の終わりまで）。そこから写像のキャッシュの錠を取る: **`USER_FONTS` → 写像のキャッシュ**。
- `JP_FONT` の錠は読み込みの間も持つ（:165）。そこから写像のキャッシュの錠を取る: **`JP_FONT` → 写像のキャッシュ**。
- `resolve_stack` は `USER_FONTS` を手放してから `JP_FONT` を取る（`resolve_font_family` の戻りでガードが落ちる）ので、`USER_FONTS` と `JP_FONT` を同時に持つ経路は無い。
- **写像のキャッシュの錠から他の錠を取る経路は作らない**（逆順が無いので、デッドロックしない）。
- **写像のキャッシュの錠は、開く・判定する・写像するまでの間も持つ。** 持たないと、2 スレッドが同じファイルを同時に引いたとき、両方が外れて 2 回写像する（「1 回だけ写像する」が破れる）。持つ時間はマイクロ秒〜ミリ秒（写像はファイルを読まない）。複製に倒れた場合は `std::fs::read` の間も持つことになる——今の `JP_FONT` が 13 MiB の読み込みの間も錠を持つのと同じ扱いで、先読みと `setup` の重なりで 2 度読まないことのほうを取る。

## 4. 「ページの読み込みに失敗しうる場所」の判定

### 推す判定（許可リスト・ハンドル起点）

1. 上の `FILE_SHARE_READ` で開いたハンドルについて `GetFinalPathNameByHandleW(h, FILE_NAME_NORMALIZED | VOLUME_NAME_GUID)` を呼ぶ。結果は `\\?\Volume{GUID}\Windows\Fonts\YuGothM.ttc` の形。
   - **ネットワーク上のファイル（UNC・割り当てたドライブ）はボリューム GUID を持たないので、ここで失敗する**（⚠ 実機で `\\server\share` のファイルに対して失敗することを 1 度確かめる。成功した場合でも次の段で落ちる）。
2. 先頭の `\\?\Volume{GUID}\` を取り出して `GetDriveTypeW` へ渡す。**`DRIVE_FIXED`(3) のときだけ写像する。**
   - `DRIVE_REMOTE`・`DRIVE_REMOVABLE`（USB メモリ・SD）・`DRIVE_CDROM`・`DRIVE_RAMDISK`・`DRIVE_UNKNOWN`・`DRIVE_NO_ROOT_DIR` は写像しない。
3. **どの段の失敗も「写像しない」へ倒す**（API の失敗・バッファ不足・想定外の形）。誤り方の向きは常に「写像できるのに複製する」側で、逆（写像してはいけないのに写像する）にはしない。前者の代償は今と同じ PrivComm、後者の代償はクラッシュ。

ハンドル起点にする理由: パス文字列から `GetVolumePathNameW` で根を取る方式は、symlink・junction・`subst`・マウントポイント越しに別のボリューム（ネットワーク）へ行くファイルを取り違える。開いたハンドルが指す実体で決めれば、その誤りが無い。

副産物: 同じ呼び出しの結果（`\\?\Volume{GUID}\...` の正規化パス）は、**写像のキャッシュのキーにそのまま使える**（§5-2）。

### 判定の誤り方（残るもの）

- 外付け USB HDD・SSD は `DRIVE_FIXED` を返す → 写像し、抜けば落ちうる。フォントは普通 `%SYSTEMROOT%\Fonts` か `%LOCALAPPDATA%\Microsoft\Windows\Fonts` にあるので、まず当たらない。
  - もっと厳しい代案: 「`%SYSTEMROOT%` と同じボリュームのときだけ写像する」。システムボリュームのページ読み込みが失敗すれば OS ごと止まるので、根拠が最も強い。代償は、プロファイルを D: に置く構成で user_font を写像しないこと。**推すのは `DRIVE_FIXED`**（issue の決定の文言「ネットワーク上のファイルなど」に合い、判定が 1 つの API の値で閉じる）。どちらを採るかは計画で明示する。
- 仮想ディスク（VHD を接続したもの）・BitLocker の外付け: `DRIVE_FIXED`。前者は取り外しで落ちうる。受け入れる残余として doc に書く。
- **クラウド同期のプレースホルダ**（OneDrive の「オンラインのみ」など）は `DRIVE_FIXED` の上にあっても、ページを読み込むときに取得が走り、失敗しうる。**採ることを推す**: 開いた `File` の `metadata()?.file_attributes()`（`std::os::windows::fs::MetadataExt`）が `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`(0x0040_0000)・`FILE_ATTRIBUTE_RECALL_ON_OPEN`(0x0004_0000)・`FILE_ATTRIBUTE_OFFLINE`(0x1000) のどれかを持てば写像しない。費用は 1 回の属性取得で、誤り方は写像しない側。
- 判定は純粋関数 `fn maps_on(drive_type: u32, attributes: u32) -> bool` へ切り出し、Win32 の呼び出しと分けておくと単体テストできる（`.claude/rules/rust.md`「純粋な導出を切り出す」）。

## 5. 設計上の判断点と推す答え

### 5-1. user_font の解決で fontdb の `Source` をどう扱うか

推す: **`with_face_data` をやめ、`face_source(id)` で `Source` を取り出して分岐する。**

- `Source::File(path)` → 自前の `load_font_file(&path)`（共有モードを絞った写像、失敗時は複製）。
- `Source::Binary(a)` / `Source::SharedFile(_, a)` → `a` をそのまま `Blob` にする（型が同じ。複製しない）。`load_system_fonts` だけを使う今の経路では到達しないが、`match` を網羅にしておけば fontdb の版上げで variant が増えたときにコンパイルが教える（`Source` は `#[non_exhaustive]` ではない——⚠ 版上げ時に確認）。
- `with_face_data` を使い続けて「クロージャの中で受け取ったパス」を取る手は無い（クロージャはバイト列しか受け取らない）。また `with_face_data` は内部で全共有モードの写像を作って捨てるので、使えば 1 回余分に開いて写像する。
- fontdb の `make_shared_face_data` は共有モードが決定に合わない（§0）ので使わない。
- face index は `face_source` の戻り値の `u32` を使う（TTC の面指定。今の `uf.index = face_index` と同じ意味）。

### 5-2. 写像のキャッシュのキー（jp_font と user_font が同じファイルのとき）

問題: user_font のパスは fontdb が組んだ `C:\WINDOWS\Fonts\X.ttc`、jp_font の候補は `C:/Windows/Fonts/X.ttc`。文字列をキーにすると**同じファイルでも外れる**（issue コメントが挙げた「user_font と jp_font が同じファイル」はこの経路）。

推す: **開いたハンドルから得た正規化パス（§4 の `GetFinalPathNameByHandleW` の結果）をキーにする。**
- 手順: 開く → 正規化パスを得る → キャッシュに在れば、今開いたハンドルを捨ててキャッシュの Blob を返す → 無ければ写像して入れる。開く費用はマイクロ秒で、開き直しは問題にならない。
- 2 回目の `FILE_SHARE_READ` の open は、1 回目のハンドル（読み取りアクセス・READ 共有）と両立するので失敗しない。
- 複製に倒れた場合（判定が「写像しない」）も、得られていれば同じ正規化パスで複製をキャッシュする。これで「同じファイルが 2 本常駐する」経路が複製側でも閉じる。正規化パスが得られなかった（ネットワーク等）ときは、入力パスの文字列をキーにする（外れても今と同じ）。
- 代案（もっと簡単）: 入力パス文字列をキーにする。**写像どうしなら、外れても物理ページと commit は section が共有するので PrivComm は増えない**（仮想アドレスとハンドルが 1 つ増えるだけ）。外れて害が出るのは複製に倒れた場合だけ。issue の決定「パスをキーにして同じファイルを 1 回だけ写像する」を文字どおり満たすには正規化が要る。
- ⚠ 「ファイルのパス」を正規化パスと読んでよいかは、決定の文言の解釈なので計画で明示する（ファイル ID = ボリューム番号 + ファイル番号 をキーにする手もあるが、`std` の該当 API は unstable で、`GetFileInformationByHandle` を別に呼ぶことになる）。

user_font と jp_font が同じファイルで、しかも user_font が CJK を覆わない、という組み合わせは実際には稀（CJK を覆わない和文フォントは jp の候補に入っていない）。キャッシュがあることで、この場合も Blob は 1 つで、epaint の `FontData` が 2 つ（`user_font` / `jp_font`）同じ Arc を指す。

### 5-3. `JP_FONT` と `USER_FONTS` を残すか

- `USER_FONTS`（family 名 → `(Blob, u32)`）は残す。役目は fontdb の全走査（15〜20 ms）を family ごとに 1 回にすること。写像のキャッシュとは別の概念（family 名の解決 vs ファイルの写像）なので、1 つにまとめない（「片方だけが変わる将来」: 別名から同じファイルへ解決する場合、family のキーは 2 つ・ファイルのキーは 1 つ）。
- `JP_FONT` は残す。錠を読み込み（写像）の間も持つ形は #1452 の不変条件で、先読みと `setup` が重なっても候補の探索を 2 度しない。中身が写像になっても理由は残る。写像のキャッシュがあるので `JP_FONT` を消して毎回 `CANDIDATES` を引く手もあるが、存在しない候補を毎回開きに行くので推さない。
- 「失敗はキャッシュしない」（:17・:118）は保つ。写像のキャッシュも成功だけを入れる。

### 5-4. 開けないときの戻り方

決定: 「開けない・写像できないときは、いまの方法（読んで写す）へ戻る」。

- `FILE_SHARE_READ` での open が**共有違反**（他が書き込み・削除のアクセスで開いている）で失敗した場合: `std::fs::read`（全共有）なら開けるかもしれない → 複製へ戻る（決定どおり）。そのとき書き手がいるので読み取った内容は途中かもしれないが、今の挙動と同じ。
- **存在しない**場合: `std::fs::read` も失敗するので `None`（jp_font の次の候補へ）。2 回開きに行くのを避けたいなら、`ErrorKind::NotFound` のときは複製を試さずに `None` を返す。
- 長さ 0 のファイル: memmap2 は空の写像を返す。今の `std::fs::read` も空の `Vec` を返して `Some` になる。挙動を変えないなら空の Blob を返す。⚠ 今でも空のフォントを `jp_font` として積んでいる（epaint の解析が失敗する）ので、ここで直すかは別件。

### 5-6. 施錠を利用者向けの文書に書くか

「Snotra の動作中は、写像したフォントファイル（user_font と、併載するときの Yu Gothic）を更新・削除・改名できず、フォントのインストーラーが失敗しうる。別の font_family へ切り替えても、前のファイルは終了まで施錠されたまま」は、利用者から見える挙動の変化である。書く場所は `SPEC.md` の font_family の項（:710）。推す: **1 文を足す**（issue の決定が代償として受け入れたものを、仕様の側にも残す。`AGENTS.md`「開発ワークフロー」の 3 層分担で、挙動を変える変更は `SPEC.md` と同じ変更で合わせる）。システムフォント（`C:\Windows\Fonts`）はもともと利用者が書き換えないので、実害は `%LOCALAPPDATA%` に入れたユーザーフォントの更新に限られる、と添える。

### 5-5. テストの書き方

- 共有モードと写像は**ファイルシステムに触れるテスト**になる。`font_covers_cjk_accepts_japanese_system_font`（:335-348）と同じく、システムフォントが無い環境では理由を出して skip する形に合わせる。
- 一時ファイルを使うテストは、`std::env::temp_dir()` が `DRIVE_FIXED` にあることを前提にできない（runner は普通 fixed）。判定が「写像しない」に倒れたら skip 理由を出す。
- グローバルな static のキャッシュをテストから触ると、並行に走る他のテストと状態を共有する。**キャッシュは型にして、static はそのインスタンス**にし、テストは自分のインスタンスを作る。
- Win32 の判定（`GetDriveTypeW`）そのものは単体テストの対象にしない（`.claude/rules/src-tauri.md`）。純粋な `maps_on(drive_type)` を表で縛る。

## 6. 必要なテストと実測

### 単体テスト（`font_stack.rs` の `mod tests`）

1. 既存 4 本の `font_definitions_*` を `Blob` へ移す。`font_definitions_registers_both_fonts_without_copy` は番地で縛る形を保つ（`from_blob` の後も `data.bytes().as_ptr()` が渡した Blob と同じ）。
2. **写像した内容がファイルと一致する**（文字の表示が変わらない、の下限）: `YuGothM.ttc`（無ければ skip）を写像経路で読み、`std::fs::read` の結果とバイト一致。**写像経路を通ったこと**（複製に倒れていないこと）も assert する——戻り値に種別（写像 / 複製）を持たせるか、`MappedFont` を返す関数を直接呼ぶ。これを欠くと、判定が常に「写像しない」に倒れても全部緑になり、PrivComm が下がらないことに実測まで気づかない。
3. **共有モードが効いている**（`unsafe` の根拠そのもの）: 一時ファイルを写像して持ったまま、(a) `OpenOptions::new().write(true).open` が `raw_os_error() == Some(32)`（ERROR_SHARING_VIOLATION）で失敗、(b) `std::fs::remove_file` が失敗、(c) `std::fs::rename` が失敗。写像を drop した後は削除できる（片付けと、ハンドルが写像と一緒に閉じることの確認を兼ねる）。
4. **同じファイルを 2 つの綴りで引くと 1 つの Blob**: `C:/Windows/Fonts/YuGothM.ttc` と `%SYSTEMROOT%\Fonts\YuGothM.ttc`（大小文字・区切りが違う）で `Arc::ptr_eq`（無ければ skip）。テスト用のキャッシュのインスタンスで。
5. **置き場所の判定**: `maps_on` が `DRIVE_FIXED` かつプレースホルダ属性なしのときだけ真。drive type 0〜6 の全部と、3 つの属性ビットそれぞれを表で縛る。
6. 存在しないパス → `None`（候補を次へ進める挙動の保存）。
7. （任意）`resolve_font_family` が `Source::Binary` を複製せずに返す: `Database::load_font_data(bytes)` で作った db から引く形にすると、システムに依存せず書ける——が今の関数は db を内部で作るので、`Source` から Blob を作る部分を純粋関数に切り出したときだけ書ける。

### 実測（受け入れ）

- `npm run measure:memory:stages`: **変更前後 × font_family 2 構成**（`Segoe UI`＝既定・jp 併載 / `HackGen Console`＝CJK を覆う）。各段（アイドル・表示・検索後・hide 後）の PrivComm。見込みはアイドルで −14.18 / −10.21 MiB（`PERFORMANCE.md:2721-2731` の帰属から）。**同時に mapped（`VirtualQuery` の MEM_MAPPED）が同じ額だけ増えることを見る**——PrivComm が下がっただけでは「積んでいない」のか「写像へ移った」のかを区別できない。ばらつき 0.6〜0.8 MiB（同 :2694）以下の差を読まない。
  - **今の計測コードは mapped を出さない**（`scripts/measure-memory.ps1` が出すのは PrivWS と PrivComm だけ。`VirtualQuery` / `MEM_MAPPED` は `scripts/`・`src-tauri`・`snotra-core` のどこにも無い。2026-08-22 の帰属表の mapped 列は撤去済みの一時的な足場で測った値）。見る手段を計画で決める: (a) Sysinternals VMMap の「Mapped File」欄で該当フォントのパスを目で見る（足場が要らない）、(b) 一時的な足場を立てるなら、撤去までを新設の時点で決める（`docs/development-principles.md`「観測するとき」）。推すのは (a)。単体テスト 2（写像経路を通ったことの assert）と合わせれば、「下がった理由が写像である」まで閉じる。
- `npm run bench:startup` の windows_create: 前後 A/B。jp_font の 13 MiB の読み込み（2.8〜3.2 ms・`PERFORMANCE.md:3410`）が消えるので悪化はしない見込みだが、ページフォルトが epaint の解析時へ移るので測る。
- `npm run check:colors` と、カテゴリ D の目視スモーク（`font_stack.rs:11-12` が「フォント登録に触る変更では省かない」と言っている）。
- **施錠の実地確認**（代償の確認）: Snotra を動かしたまま、ユーザーフォント（`%LOCALAPPDATA%\Microsoft\Windows\Fonts` のもの）を消そうとして「使用中」になること。Snotra を落とせば消せること。
- **写像経路が実機で選ばれていること**: `C:\Windows\Fonts` のファイルを `FILE_SHARE_READ` で開けるか（フォントキャッシュサービスなどが書き込みアクセスで開いていれば共有違反で複製へ倒れ、削減が無言で消える）。上の単体テスト 2 が開発機で通れば足りる。⚠ trace に `font:source kind=mapped|copied` を出すかは任意（出せば `measure:memory:stages` の標本ごとに確かめられる）。
- `cargo test -p snotra`・`clippy -D warnings`・`cargo doc`（doc を大きく書き換えるので、`docs/build-commands.md` カテゴリ A）。

## 7. 指摘（やりすぎ・簡単にする方向）

- 写像のキャッシュのキーを正規化パスにするのは、写像どうしなら物理ページを section が共有するので**メモリの上では効かない**。効くのは複製に倒れた場合とハンドル・仮想アドレスの数だけ。ただし判定（§4）で `GetFinalPathNameByHandleW` を呼ぶなら追加の費用は無く、そのまま使うのが最も簡単。
- 外付けドライブまで除く厳しい判定（バスの種別を `IOCTL_STORAGE_QUERY_PROPERTY` で見る等）は、フォントの置き場所の実態から見て費用に見合わない。推さない。
