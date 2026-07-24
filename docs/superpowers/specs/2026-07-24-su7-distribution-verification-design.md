# SU7: 配布・検証方針（#580 消化計画）設計

日付: 2026-07-24 / 対象: #532 Phase 2 SU7 のうち配布検証（flip 基準 4 の「#580 核心」）とリリース構成 / 先行: SU6.5（PR #655・ゲート G1〜G4 合格。G4 は PR CI green＝#655 マージを根拠とする）
ロードマップ: `2026-07-21-phase2-softbuffer-migration-roadmap.md` の SU7 行
スコープ確定: brainstorm で「署名鍵の CI 方針」を出発点に検討し、**鍵管理は現状維持・設計対象は検証手順とリリース構成**と確定。初版はマルチパースペクティブレビュー（事実照合 / 手順実行シミュレーション / 完全性・整合 / codex 敵対探索の 4 レンズ）を経て、決定 2・4 を含む本改訂版へ更新した。e2e 後継方針の設計と flip 実装の設計は本 spec の対象外（別設計。ただし順序制約は「リスクと受容」に記す）。

## 背景と言語化

brainstorm とレビューの裏取りで、前提が次のとおり更新された。

- **署名鍵は既に CI に在る**。`release.yml` が `secrets.TAURI_SIGNING_PRIVATE_KEY`（+ password）で署名付き NSIS と `latest.json` を生成し、`.sig` 存在検証・起動スモークを経て draft release へ上げる経路が現役（v0.18.3・2026-07-20 まで実績）。#580 の「鍵がローカルに無い」は正しいが、それは是正すべき欠落ではなく維持すべき状態である。
- **旧版の updater は publish 済みの Latest しか見ない**。endpoint は `releases/latest/download/latest.json` 固定（`src-tauri/tauri.conf.json`）で、draft / prerelease はこの URL に現れない。一方、**検証する側の旧版に署名鍵は不要**——updater が署名検証するのは「ダウンロードした成果物」であり、焼き込み済み本番 pubkey で足りる。ゆえに endpoint を 1 行差し替えた旧版のローカルビルド（`--no-bundle`・portable 起動)で、publish 前に実更新を実弾検証できる（決定 2 の rig）。
- **`create-release.yml` はタグ push が先行する**。workflow 開始時に `v<version>` タグを push してから draft をビルドするため、検証 NG でも同一版番号では焼き直せない。NG 時の扱いは手順に含める必要がある（決定 2 の NG 分岐）。
- **実行環境は Windows 11 Home**。Windows Sandbox / Hyper-V は使えず、素の環境検証にはサードパーティ VM の導入が要る。また検証実機は開発機と同一で、per-user AppData（config / history / index / window）を開発ビルドと共有する。
- 補足: #580 コメント（2026-07-21）の「portable ZIP が生成されない」gap は `bundle.targets` 視点の指摘。`release.yml` は `Compress-Archive` で ZIP（snotra.exe + snotra-settings.exe）を手動生成しており、項目 3 の生成機構は既存。残るは成果物の動作確認のみ。

## 決定事項

### 決定 1: 鍵管理は現状維持。恒久的な新機構は作らない

GitHub Actions secret + draft ゲート（人手検証後に手動 publish）を維持し、鍵をローカルへ持ち出さない。検証専用の恒久チャネルや endpoint override の製品機構は作らない。決定 2 の rig は「旧版ローカルビルドの設定 1 行差し替え」という使い捨ての検証治具であり、製品コード・CI への変更を伴わない。

### 決定 2: 項目 1（旧版 → egui 版の実更新）は publish（Latest 化）の**前**に rig で実弾検証する

初版は「publish 後に実機で検証」としたが、レビュー 3 レンズが独立に同じ欠陥へ収束したため改めた——(a) flip 基準 4「#580 核心を満たしてから切替」に対し、flip リリース自身が検証手段になる循環が生じる、(b) 手動 install 検証は updater ハンドオフ固有の条件（`latest.json` 解決・バージョン比較・署名の暗号学的検証・既存プロセス終了・installer 起動）を一切検証しないため「残余は僅か」という受容根拠が誤り、(c) 検証者が最初の被験者である保証がない。

rig の形: v0.19.0 を **prerelease として publish**（Latest にならず既存ユーザーの updater から不可視）→ worktree に v0.18.3 を checkout し `tauri.conf.json` の endpoint を固定 URL（`releases/download/v0.19.0/latest.json`）へ 1 行差し替え → `--no-bundle` ビルドの旧版 exe を portable 起動 → 実更新を完走させる。これで署名検証を含むハンドオフ全段が publish 前に実弾になり、flip 基準 4 の循環も解ける。検証されない残余は `/releases/latest` リダイレクト 1 点のみ（→「リスクと受容」）。

**NG 時の分岐**: タグは既に push されているため付け替えない。prerelease を破棄（または prerelease のまま放置）し、修正後は**次の版番号で焼き直す**。Latest は v0.18.3 のまま動かないので、ユーザー影響はない。

### 決定 3: install 系検証（項目 6 と、項目 2・3 の成果物接地）は実機のみで回す。VM は見送る

署名付き NSIS / ZIP の**生成**は CI（`release.yml`）が担い、実機が担うのは**実 install での接地**である。config / history をバックアップした上で、上書き install → uninstall → 新規 install の列で検証し、**列の完了後にバックアップを復元してから**後続手順へ進む。実機には残留 AppData がありうるため「完全に素の環境」ではなく、検証中に開発ビルドを並走させると同じ AppData を読み書きして汚染する——この 2 点は手順上の禁止（検証中は開発ビルドを起動しない）と受容で扱う（→「リスクと受容」）。素の環境整備が将来必要になれば別 issue とする。

### 決定 4: v0.18.3 を WebView2 最終版とし、v0.19.0 で flip + WebView2 経路撤去を同一リリースで完結する

初版は「1 リリース分離（フラグ温存 = ロールバック手段）」としたが、レビュー 2 レンズが「フラグの実体は環境変数であり、一般ユーザーが操作できる回復手段ではない」と指摘した。フラグ退避をロールバックの柱に据える主張は撤回し、開き直って構成を単純化する:

- **v0.18.3 = WebView2 最終版**（公開されたまま残る）。**v0.19.0 以降 = egui**。
- ロールバックの実体は「v0.18.3 の NSIS を入れ直す」+ 次 patch。アプリ内のフォールバック経路は主張しない。
- 二経路並行維持のコストは即時に消え、ロードマップ「並行期間を短く」に最も忠実な形になる。
- 初版の分離案が担っていた「egui 側 updater の実弾検証機会」は、**flip 後最初の自然なリリース**（v0.19.x）への更新が同じ機会を提供するため、失われない（手順 7）。

## リリース手順（v0.19.0）

0. **事前確認**: 永続形式（index.bin / history.bin / window.bin / config.toml）の v0.18.3 比差分を確認する。本 spec 作成時点の実測では形式バージョン・フィールドとも差分なし（index v4 / history v3 / window v5）。flip までに永続形式を変える変更が入った場合はこの前提が崩れるため、リリース直前に再確認する（`/persistence-check` の領分）。config / history をバックアップする。
1. `create-release.yml`（workflow_dispatch）で v0.19.0 の draft build。CI が署名・`latest.json` 生成・`.sig` 存在検証・起動スモークまで自動実行する。タグはこの時点で push される（NG 時は決定 2 の分岐）。
2. **draft 検証（実機・検証中は開発ビルドを起動しない）**: draft の NSIS で上書き install → 動作スモーク → uninstall → 新規 install → 動作スモーク → portable ZIP を展開し snotra.exe 起動 + 設定画面（サイドカー snotra-settings.exe）が開くことを確認。動作スモークの合否は「起動・ホットキー表示/非表示・検索と結果選択・設定画面起動・更新チェックが更新なしを返す」の 5 点とする。ここで項目 2・3・6 が埋まる。完了後、バックアップを復元する。
3. draft を **prerelease として publish**（Latest 不変・既存ユーザー不可視）。
4. **rig による実更新検証（決定 2）**: endpoint を v0.19.0 の固定 URL へ差し替えた v0.18.3 ローカルビルド（portable）から実更新 → 署名検証・プロセス終了・NSIS passive・再起動を完走 → egui 既定で起動・スモーク合格。ここで項目 1 が埋まる。
5. 合格したら prerelease を解除して **Latest 化**。一般ユーザーへ開放。
6. 公開後の接地: 公開済み asset（NSIS / ZIP / `latest.json`）を Release ページから取り直し、ダウンロード可能なこと・`latest.json` の url が公開 asset を指すことを確認する。
7. **flip 後最初の自然なリリース（v0.19.x）で egui 側 updater を実弾検証**: check → toast → [今すぐ更新] → Installing 表示（両ボタン disabled を目視・#580 コメント 2026-07-23 の追記）→ `on_before_exit` 保存（history flush・icon cache・設定サイドカー終了）→ installer passive → 再起動。ここで項目 4 が実弾になり、完走で #580 を close する。

### #580 チェックリストとの対応

| #580 項目 | 埋まる場所 |
|---|---|
| 1. 旧署名版 → softbuffer 版の実更新 | 手順 4（rig・publish 前） |
| 2. 署名付き NSIS 生成 | 手順 1（CI 生成）+ 手順 2（実 install で接地） |
| 3. portable ZIP | 手順 2（既存 `Compress-Archive` 成果物の動作確認） |
| 4. `on_before_exit` 保存・再起動 | コード検証済（SU5・PR #647 の `on_before_exit` hook）→ 手順 7 で実弾 |
| 5. 3 更新モード維持 | 検証済（SU5・PR #647 + #580 コメント 2026-07-21）。手順 4 で full モードが再接地 |
| 6. 新規 / 上書き / uninstall | 手順 2 |
| 追記: Installing 目視列（#647） | 手順 7 |

## スコープ外

- **e2e 後継方針の設計**（`e2e/` 基盤喪失への対応・#567 との順序整理・egui 自動回帰 smoke の中身）——独立のトピックとして別途設計する。ただし決定 4 により撤去が v0.19.0 に入るため、**「egui 自動回帰 smoke 最低 1 本 CI」（SU7 受け入れ条件）は撤去 PR と同時か先行**という順序制約が生じる（→「リスクと受容」）。
- **flip 実装そのもの**（既定反転の方式・撤去の範囲・二経路の畳み方）——本 spec の後続として別途 brainstorm する。
- **VM / 素の環境の整備**——見送り。必要が生じたら別 issue。

## リスクと受容

- **`/releases/latest` リダイレクト経路は rig で検証されない**: rig は固定 URL を直指しするため、Latest 化後に旧版が辿る `releases/latest/download/latest.json` の解決だけは実弾にならない。v0.18 系の更新で長期に実績のある GitHub 側機構であり、受容する（決定 2）。
- **実機は素の環境ではない**: uninstall 後の AppData 残留があり、新規 install 検証の純度は VM に劣る。加えて実機は開発機でもある——「検証中は開発ビルドを起動しない」の手順制約で能動汚染は避け、残留による純度低下は受容する（決定 3）。
- **egui 側 updater の実弾が flip 後最初の更新まで遅延する**: 項目 4 の実弾（手順 7）は v0.19.0 リリース時点では未完走。コードは SU5 で検証済み・旧版→新版のハンドオフは rig で実弾済みであることを根拠に受容する。手順 7 の更新が万一失敗した場合、WebView2 撤去済みゆえアプリ内の退避は無く、回復は「v0.19.0 の NSIS を Release から入れ直す」+ 次 patch である（決定 4）。
- **`e2e/` の即時喪失**: 撤去が v0.19.0 に入るため、自動回帰の空白が生じうる。上記スコープ外の順序制約（smoke を撤去 PR と同時か先行）で塞ぐことを SU7 の受け入れに含め、本 spec では受容しない（塞ぐ側に置く)。
