# tk Task List CLI 仕様

ステータス: Draft

最終更新: 2026-04-19

## 1. 目的

`tk` は、構造化 task list を管理するための単体 Rust CLI です。

Claude Code の構造化 task list 部分だけを抽出し、Anthropic 依存の意味づけを取り除いたうえで、以下のような実行主体から共通利用できる vendor-neutral なインターフェースを定義します。

- Codex CLI
- Claude Code
- 独自 agent
- CI job
- 人手での運用

最優先の統合形態は subprocess 呼び出しです。任意の agent が `tk ... --format json` を実行し、stdout/stderr と exit code だけで扱えることを前提にします。

## 2. ゴール

- Rust 製 single binary の task list CLI を提供する。
- 元の構造化 task list の有用な意味論を維持する。
  - task 作成
  - status 更新
  - owner 割り当て
  - 依存関係の表現
  - 次に着手可能な task の claim
- 複数 agent / 複数 terminal からの同時アクセスを安全に扱う。
- 安定した machine-readable JSON 出力を提供する。
- vendor-neutral に保つ。
  - Anthropic 用語を含めない
  - Claude 固有の prompt reminder を含めない
  - mailbox や teammate 固有挙動を持ち込まない
- 既定では project-scope の保存先を使い、必要なら shared root を明示指定できるようにする。

## 3. 非ゴール

`tk` は background execution subsystem を扱いません。v1 では以下を明確に対象外とします。

- shell や background process の管理
- 実行中コマンドの output/log capture
- remote session の追跡
- sub-agent transcript の閲覧
- UI overlay, spinner, REPL 固有 panel
- model prompt への自動 reminder 注入
- "task created" / "task completed" のような vendor 固有 hook

これらが必要になった場合は、`tk` 本体ではなく上位ツールとして別建てで実装します。

## 4. 設計原則

- Local-first: 状態は平文ファイルとしてローカルに保存する。
- Agent-safe: 契約面は JSON mode を正本とし、text mode は人間向けとする。
- Explicit over implicit: 隠れた cleanup や prompt mutation を持たない。
- Concurrency-safe: file lock と atomic write を必須とする。
- Portable: Linux / macOS を first-class とし、Windows は best-effort とする。
- Inspectable: task JSON を直接開いて確認できるようにする。

## 5. 基本概念

| 用語 | 意味 |
| --- | --- |
| Root | `tk` が状態を保存する filesystem directory |
| List | 通常は 1 project または 1 workstream に対応する名前付き task list |
| Task | list に属する 1 つの work item |
| Owner | task の担当者または担当 agent を表す free-form identifier |
| Blocker | ある task が claimable になる前に完了している必要がある別 task |
| Claimable | `pending` かつ owner 未設定、かつ未解決 blocker が存在しない状態 |
| Revision | optimistic concurrency のための単調増加 version 番号 |

## 6. 設定解決順

### 6.1 Root path

保存 root は以下の順で解決します。

1. `--root <path>`
2. `TK_ROOT`
3. 最寄り VCS root (`.git` または `.jj`) の `/.tk`
4. 現在ディレクトリの `/.tk`

これにより、既定では project-local storage を使いつつ、cross-worktree や shared directory も明示指定できます。

### 6.2 List ID

active list ID は以下の順で解決します。

1. `--list <id>`
2. `TK_LIST_ID`
3. `<root>/config.toml` の `default_list_id`
4. 検出した VCS root basename の sanitize 結果
5. 文字列 `default`

### 6.3 Default owner

default owner は以下の順で解決します。

1. `--owner <name>` を受け取る command 上の明示指定
2. `TK_OWNER`
3. `<root>/config.toml` の `default_owner`
4. 未設定

### 6.4 Config file

任意設定ファイル:

`<root>/config.toml`

v1 で扱う key:

```toml
default_list_id = "repo-name"
default_owner = "codex"
output_format = "json"
```

CLI flag が常に優先されます。

## 7. 保存レイアウト

```text
<root>/
  config.toml
  lists/
    <list-id>/
      manifest.json
      .lock
      .highwatermark
      tasks/
        1.json
        2.json
        3.json
```

### 7.1 Path rule

- `list-id` は `[a-z0-9][a-z0-9._-]{0,127}` に一致すること
- task file 名は 10 進 task ID + `.json`
- root / list directory は可能な限り user-private permission で作る

### 7.2 Manifest file

`manifest.json` には list-level metadata を保持します。

```json
{
  "schema_version": 1,
  "list_id": "repo-name",
  "title": "repo-name",
  "description": null,
  "created_at": "2026-04-19T12:34:56Z",
  "updated_at": "2026-04-19T12:34:56Z",
  "list_revision": 0
}
```

`list_revision` は list を変える mutation のたびに増加します。

- create
- update
- claim
- unclaim
- delete
- block add/remove
- reset

`.highwatermark` は発行済み最大 numeric ID を保持し、削除後も ID 再利用を防ぎます。

## 8. 永続 task schema

各 task file は 1 つの JSON object を保持します。

```json
{
  "schema_version": 1,
  "id": "12",
  "revision": 3,
  "subject": "Run integration tests",
  "description": "Run the full integration suite after parser changes.",
  "active_form": "Running integration tests",
  "status": "in_progress",
  "visibility": "public",
  "owner": "codex",
  "blocks": ["14"],
  "blocked_by": ["9", "10"],
  "metadata": {
    "component": "parser",
    "priority": "high"
  },
  "created_at": "2026-04-19T12:34:56Z",
  "updated_at": "2026-04-19T12:40:00Z",
  "started_at": "2026-04-19T12:35:10Z",
  "completed_at": null
}
```

### 8.1 Field semantics

| Field | Type | 説明 |
| --- | --- | --- |
| `id` | string | list 内で一意な 10 進文字列 ID |
| `revision` | integer | mutation ごとに増加する task version |
| `subject` | string | 短い実行タイトル |
| `description` | string | 詳細要件 |
| `active_form` | string or null | UI client が進行中表示に使う現在進行形 |
| `status` | enum | `pending`, `in_progress`, `completed` |
| `visibility` | enum | `public` または `internal` |
| `owner` | string or null | 担当 human / agent identifier |
| `blocks` | string[] | この task の完了を待っている downstream task |
| `blocked_by` | string[] | この task の upstream blocker |
| `metadata` | object | 任意 JSON object |
| `created_at` | RFC3339 UTC string | 作成日時 |
| `updated_at` | RFC3339 UTC string | 最終更新日時 |
| `started_at` | RFC3339 UTC string or null | 初回 `in_progress` 遷移時刻 |
| `completed_at` | RFC3339 UTC string or null | `completed` 遷移時刻 |

### 8.2 Validation limit

- `subject`: 1 から 200 UTF-8 characters
- `description`: 0 から 32768 UTF-8 bytes
- `active_form`: 0 から 120 UTF-8 characters
- `owner`: 設定時 1 から 128 UTF-8 characters
- `metadata`: serialized size 最大 65536 bytes

## 9. 派生 field

`list`, `get`, `claim`, `next`, `watch` の JSON response には、永続化されない派生 field を含めてよいものとします。

| Field | 意味 |
| --- | --- |
| `open_blocked_by` | `blocked_by` のうち、参照先 task が未完了なもの |
| `invalid_blocked_by` | 参照先 task が存在しない blocker ID |
| `claimable` | `status == pending` かつ `owner == null` かつ `open_blocked_by` 空、かつ `invalid_blocked_by` 空 |
| `blocked_tasks` | human-readable output 限定の `blocks` 別名 |

派生 field を task JSON file に書き戻してはなりません。

## 10. 状態モデル

### 10.1 Canonical status

- `pending`
- `in_progress`
- `completed`

### 10.2 許可される遷移

既定で許可する遷移:

- `pending -> in_progress`
- `in_progress -> pending`
- `in_progress -> completed`

制限付き遷移:

- `pending -> completed` は `--force` 必須
- `completed -> pending` は `tk reopen <id>` または `tk update <id> --status pending --force` を明示的に要求
- `completed -> in_progress` は reopen を経由すること

### 10.3 Ownership の扱い

- owner と status は独立です。
- claim は、`--start` を付けない限り自動で `in_progress` にしません。
- unclaim は、`--requeue` を付けない限り自動で `pending` に戻しません。

## 11. 依存関係モデル

### 11.1 Invariant

- 依存グラフは DAG であること
- self-dependency は禁止
- `blocks` と `blocked_by` は同一 edge set の対称表現であること
- edge 追加系 mutation は両側を transactionally に更新すること

### 11.2 Blocker semantics

以下の blocker が 1 つでも存在する task は blocked とみなします。

- 参照先 task が存在し、かつ `completed` ではない
- 参照先 task が存在しない

存在しない blocker 参照は invalid graph state として扱います。該当 task は claimable ではなく、`verify` で必ず報告します。

### 11.3 Delete semantics

`tk delete <id>` の既定は safe mode とします。

- 依存 edge に関与している task は削除失敗

`tk delete <id> --detach`:

- 関連する inbound / outbound edge を transactionally に除去
- task を削除
- 影響を受けた task の revision を増加

## 12. CLI surface

### 12.1 Global flag

全 command で共通:

- `--root <path>`
- `--list <id>`
- `--format <text|json|ndjson>`
- `--no-color`
- `--quiet`

`ndjson` は `watch` のような streaming command でのみ有効です。

### 12.2 Command 一覧

| Command | 目的 |
| --- | --- |
| `tk init` | root と list manifest の初期化 |
| `tk dir` | 解決された root/list/task path の表示 |
| `tk create` | task 作成 |
| `tk list` | task 一覧 |
| `tk get <id>` | 単一 task の取得 |
| `tk update <id>` | task patch |
| `tk start <id>` | `update --status in_progress` の便利 alias |
| `tk done <id>` | `update --status completed` の便利 alias |
| `tk reopen <id>` | completed task を `pending` に戻す |
| `tk claim <id>` | owner 割り当て、必要なら start |
| `tk unclaim <id>` | owner clear、必要なら requeue |
| `tk next` | 次に着手可能な task の取得または claim |
| `tk block add <task-id> <blocker-id>...` | blocker 追加 |
| `tk block remove <task-id> <blocker-id>...` | blocker 除去 |
| `tk delete <id>` | task 削除 |
| `tk reset` | list 内の全 task 削除 |
| `tk verify` | graph と on-disk state の検証 |
| `tk watch` | list 変更の stream 出力 |

## 13. Command 詳細

### 13.1 `tk init`

挙動:

- `<root>` がなければ作成
- list directory と manifest がなければ作成
- 既に存在する場合は no-op
- 任意 flag:
  - `--title <text>`
  - `--description <text>`

### 13.2 `tk dir`

挙動:

- 解決された root path を出力
- 解決された list path を出力
- JSON mode では追加で以下を返す
  - `manifest_path`
  - `tasks_dir`
  - `lock_path`
  - `highwatermark_path`

### 13.3 `tk create`

必須:

- `subject`

任意:

- `--description <text>`
- `--active-form <text>`
- `--owner <name>`
- `--visibility <public|internal>`
- `--meta key=value` repeated
- 大きい payload 用の `--json-body <file>`

挙動:

- list-level lock の下で次の numeric ID を採番
- task file を atomic に write
- 初期値:
  - `status = pending`
  - `revision = 1`
  - dependency array は空

### 13.4 `tk list`

任意 filter:

- `--status <pending|in_progress|completed>` repeated
- `--owner <name>`
- `--unowned`
- `--claimable`
- `--all` で `visibility=internal` も含める
- `--limit <n>`

ソート:

- 既定は numeric ID 昇順
- 将来拡張として `--sort updated_at` は許容

### 13.5 `tk get <id>`

挙動:

- 永続 field と派生 field を返す
- 見つからなければ `task_not_found`

### 13.6 `tk update <id>`

サポートする mutation:

- `--subject <text>`
- `--description <text>`
- `--active-form <text>`
- `--status <pending|in_progress|completed>`
- `--owner <name>`
- `--clear-owner`
- `--visibility <public|internal>`
- `--set-meta key=value` repeated
- `--unset-meta <key>` repeated
- `--if-revision <n>`
- `--force`

ルール:

- 指定 field のみ patch
- 何も変わらなければ revision は増やさない
- status が初めて `in_progress` になったとき `started_at` を設定
- status が `completed` になったとき `completed_at` を設定
- reopen 時は `completed_at` をクリア
- `--if-revision` は compare-and-swap として扱い、不一致時は `revision_conflict`

### 13.7 `tk claim <id>`

必須:

- `--owner <name>`。config/env で解決できる場合は省略可

任意:

- `--start`
- `--check-busy`
- `--if-revision <n>`

挙動:

- task 不在なら失敗
- 別 owner に既に claim 済みなら失敗
- claimable でなければ失敗
- `--check-busy` 指定時、同 owner が別の未解決 task を持っていれば失敗
- `--start` 指定時は `status = in_progress` も同時に設定

### 13.8 `tk unclaim <id>`

任意:

- `--requeue` で `status = pending` へ戻す
- `--if-revision <n>`

挙動:

- owner を clear
- `--requeue` があれば status も `pending` に戻す

### 13.9 `tk next`

挙動:

- claimable task のうち numeric ID 最小のものを選ぶ
- `--claim --owner <name>` があれば atomic に claim する
- `--start` が `--claim` と併用された場合は `in_progress` にもする

利用可能 task がない場合:

- JSON mode は `ok: false`, `code = no_available_task`
- process exit code は 3

### 13.10 `tk block add`

形式:

`tk block add <task-id> <blocker-id>...`

意味:

- `<task-id>` は各 `<blocker-id>` に blocked される

挙動:

- 全参照 task の存在を検証
- self-edge を拒否
- cycle 生成を拒否
- `blocked_by` と `blocks` の両方を transactionally に更新

### 13.11 `tk block remove`

形式:

`tk block remove <task-id> <blocker-id>...`

挙動:

- edge を両側から除去
- 既に存在しない edge でも成功

### 13.12 `tk delete <id>`

任意:

- `--detach`
- `--if-revision <n>`

挙動:

- 既定は safe delete
- `--detach` で関連 dependency edge を先に外す
- 必要に応じて `.highwatermark` を更新

### 13.13 `tk reset`

任意:

- `--force`

挙動:

- active list の全 task file を削除
- `.highwatermark` は保持
- `list_revision` を増加
- `--force` なしでは、未完了 task が 1 つでもあれば失敗

### 13.14 `tk verify`

検証項目:

- manifest readability
- task schema validity
- duplicate ID
- asymmetric dependency edge
- missing blocker reference
- dependency cycle
- invalid timestamp

JSON output では stable code を持つ diagnostic 一覧を返します。

### 13.15 `tk watch`

挙動:

- 最初に full snapshot を出す
- 続けて change event を流す
- filesystem watch と polling fallback を併用
- v1 では durable replay 契約は持たない best-effort stream

event type:

- `snapshot`
- `task_created`
- `task_updated`
- `task_deleted`
- `list_reset`

`watch` は `--format ndjson` 必須です。

## 14. 出力契約

### 14.1 JSON success envelope

```json
{
  "ok": true,
  "command": "create",
  "list": {
    "list_id": "repo-name",
    "list_revision": 4
  },
  "task": {
    "id": "12",
    "revision": 1,
    "subject": "Run integration tests"
  }
}
```

### 14.2 JSON error envelope

```json
{
  "ok": false,
  "command": "claim",
  "error": {
    "code": "blocked",
    "message": "Task #12 is blocked by unresolved tasks",
    "details": {
      "task_id": "12",
      "open_blocked_by": ["9", "10"]
    }
  }
}
```

### 14.3 安定性保証

- JSON field 名を machine contract とする
- text output は human-oriented であり minor release でも変わりうる
- NDJSON event shape は v1 release 後に安定面として扱う

## 15. Exit code

| Code | 意味 |
| --- | --- |
| `0` | success |
| `1` | usage error または unexpected internal error |
| `2` | not found |
| `3` | conflict, blocked, busy, または no available task |
| `4` | validation error |
| `5` | storage error または lock timeout |
| `130` | signal interrupt |

## 16. Concurrency と atomicity

- `create`, `next --claim`, `reset`, graph-wide delete は list-level lock を使う
- 通常の single-task `update` は task-level lock を使ってよい
- `claim --check-busy` は unresolved task 全体を見るため list-level lock 必須
- 全 write は以下を満たす
  1. current state を読む
  2. validate
  3. temp file に書く
  4. 可能なら temp file を fsync
  5. atomic rename
- 複数 task file を更新する command は、partial graph corruption を起こさず成功か失敗のどちらかにする

## 17. Agent 統合契約

想定する基本 loop:

1. `tk list --format json` または `tk next --format json`
2. 作業取得時は `tk claim` または `tk next --claim --start --owner <agent>`
3. 詳細取得に `tk get <id> --format json`
4. 進行状態を `tk update`
5. 完了時に `tk done` または `tk update --status completed`

統合側ルール:

- human text output を parse しない
- JSON mode と exit code だけを見る
- hidden reminder や auto state transition を前提にしない
- `revision_conflict` は正常系 retry condition として扱う
- owner には `codex`, `claude`, `ci`, `alice` のような project 内で安定した名前を使う

## 18. Claude Code task tool からの migration

### 18.1 対応表

| Claude Code の概念 | `tk` の対応 |
| --- | --- |
| `TaskCreateTool` | `tk create` |
| `TaskListTool` | `tk list --format json` |
| `TaskGetTool` | `tk get <id> --format json` |
| `TaskUpdateTool` | `tk update <id> ...` |
| `status: deleted` | `tk delete <id>` |
| `metadata._internal` | `visibility = internal` |

### 18.2 意図的な差分

`tk` では以下の Claude Code 固有挙動を意図的に削除します。

- TUI task pane の自動展開
- task tool を使うよう促す hidden reminder
- 全 task 完了後 5 秒での自動 reset
- owner 割り当て時の mailbox 通知
- Anthropic 固有の team 用語

## 19. Rust 実装ガイド

この文書は CLI 挙動の仕様であり crate layout 自体は拘束しませんが、推奨分割は以下です。

- `tk-core`
  - schema
  - storage
  - locking
  - graph validation
- `tk-cli`
  - `clap` による command parsing
  - text formatting
  - JSON envelope rendering
  - watch loop

推奨 crate:

- `clap`
- `serde`
- `serde_json`
- `toml`
- `camino`
- `fs4` または同等の lock crate
- `notify`
- `thiserror`
- `time`

v1 の互換面は CLI の JSON/NDJSON interface のみです。内部 Rust API は変更されて構いません。

## 20. 将来拡張

本 spec の範囲外だが、後続で検討できるもの:

- priority
- label
- delete/reset の代わりの archive
- task comment
- durable event log + replay cursor
- SQLite backend
- HTTP daemon mode
