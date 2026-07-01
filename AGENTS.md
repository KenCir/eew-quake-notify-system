# AGENTS.md

このリポジトリは、Project DM-D.S.S が配信する地震情報を受信し、ユーザーへ自動通知と読み上げを行う Rust 製バックグラウンドシステムです。

## プロジェクトの目的

- 単発 CLI ではなく、常駐するバックグラウンドプロセスとして作る。
- Project DM-D.S.S から地震情報および緊急地震速報関連の情報を受信する。
- ユーザーへ素早く通知し、重複通知を避ける。
- 重要なイベントを設定可能な TTS レイヤーで読み上げる。
- API 認証情報、通知設定、TTS 設定をソースコードに埋め込まない。
- 巧妙な抽象化よりも、信頼性、分かりやすいログ、graceful な復旧を優先する。
- 個人利用かつローカル利用の範囲に限定する。EEW や地震情報の二次配信機能は追加しない。
- 将来的なクロスプラットフォーム対応と GUI 設定画面を見据えるが、初期実装は config ファイルで利用できる状態にする。

## Rust の規約

- `Cargo.toml` に設定された Rust edition を使う。
- コード変更後は `cargo fmt` を実行する。
- 依存関係とツールチェーンが利用可能な場合は `cargo clippy --all-targets --all-features -- -D warnings` を実行する。
- 振る舞いを変更した場合は `cargo test --all-targets --all-features` を実行する。
- async IO が必要な場合は `tokio` を優先する。
- 構造化された API データには `serde` / `serde_json` を優先する。
- 常駐タスクのログと診断には `tracing` を優先する。
- ライブラリやドメインエラーには `thiserror` を使い、`anyhow` は必要に応じて実行ファイル境界で使う。

## アーキテクチャ方針

- システムは小さなモジュールへ分割する。
  - `config`: 実行時設定の読み込みと検証。
  - `dmdata` または `dmdss`: Project DM-D.S.S クライアント、認証、再接続処理、API モデル。
  - `events`: 正規化済みの内部地震イベント型。
  - `notify`: ローカルのデスクトップ通知アダプターのみ。
  - `tts`: 音声合成の抽象化と VOICEVOX 実装。
  - `state`: 重複排除、最後に処理した ID、必要に応じた永続化。
- 外部 API の DTO と内部イベントモデルは分ける。
- 同期的な通知処理や TTS 処理で async runtime をブロックしない。必要なら `spawn_blocking` または worker channel を使う。
- 再接続ループには backoff、jitter、キャンセル、役に立つログを入れる。
- リモート payload が不正な場合でも、誤った通知につながらない限り回復可能なエラーとして扱う。

## Project DM-D.S.S に関する注意

- API 挙動を実装または変更する前に、endpoint URL、認証方式、message schema、rate limit を Project DM-D.S.S の公式ドキュメントで確認する。
- token、subscriber ID、socket URL、環境固有の endpoint をハードコードしない。
- secret は環境変数または git 管理外のローカル config ファイルに保存する。
- sample config には placeholder 値だけを入れる。
- Project DM-D.S.S の利用規約を尊重し、無許可の二次配信とみなされ得る機能を避ける。

## 配信制限

- Discord、Slack、LINE、webhook、email、その他のリモート宛先への通知転送を追加しない。
- 複数ユーザー向け配信、relay、repost、public feed 機能を追加しない。
- 通知はユーザーのローカル desktop environment に限定する。
- 要望された機能が EEW や地震情報をローカルマシン外へ再配信し得る場合は、実装前に確認する。

## 通知と TTS のルール

- 安定した remote ID がある場合はそれで重複排除し、ない場合は保守的な composite key を使う。
- 震度、地域、マグニチュード、最大震度、キャンセル状態などが意味を持って変化しない限り、同じ更新を繰り返し読み上げない。
- 読み上げ文は短く、決定的に生成する。
- 日本語として自然にしつつ、表記や数値は曖昧にしない。
- 初期 TTS エンジンには VOICEVOX を使う。
- 将来の platform 対応に備え、通知と TTS backend は trait の背後で差し替え可能にする。
- GUI 設定画面を実装するまでは、config ファイル駆動を主要な interface とする。

## テスト方針

- parsing、normalization、deduplication、読み上げ文生成を unit test する。
- remote payload にはネットワーク呼び出しではなく fixture JSON を使う。
- live の Project DM-D.S.S service に依存する test は避ける。
- 通知と TTS adapter は mock する。
- 再接続や retry logic の test では deterministic timer または小さく独立した test を使う。

## ファイルと変更の扱い

- 変更はユーザーの依頼範囲に絞る。
- 関係ないファイルを reformat しない。
- secret、local config、log、build output を commit しない。
- 依存関係を追加する場合は、なぜ必要かを説明し、active に保守されている crate を優先する。
