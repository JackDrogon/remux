# compact：相邻备份去重

`remux compact` 解决的是定时备份的噪声，不是通用压缩，也不是全局去重。

典型用法是每隔 5–10 分钟拍一次：

```bash
remux backup && remux compact
```

多数时候工作区并没有变成「另一个工作区」，只是 prompt 多了一行、`htop` 刷了一屏、子进程来了又走。若不加 compact，目录里会堆满几乎一样的 snapshot。

## 它做什么

在**当前 socket 的备份根目录**里，先扫全部备份目录的 **mtime**（不是 `created_at`，也不是名字里的时间），并列时再按 `backup_id` 降序，选出最新两份，只**解析**这两份 snapshot。以 `.` 开头的目录（backup 写入中的 `.{id}.tmp-*`）不参与扫描。更旧、甚至已经损坏的第三份不会参与比较，也不会让 compact 失败。

compact 和默认 restore 的「最新」是目录 mtime。`remux list` 不同，按 `backup_id` 降序。`touch` 一份旧备份会让它在 compact / 默认 restore 里变成最新；两份 mtime 相同且一份是手起的名时，字母开头的名字会排在数字时间戳前面。

顺序是：

1. 不足两份：什么都不删。
2. 较旧那份不是自动时间戳名（`YYYYMMDD_HHMMSS`）：直接停，不删，也**不**再验 pane。手起的名（`before-refactor`、`backup_20240101_120000`）是检查点，不是定时器产物。
3. 较旧的是自动名：这时才校验**这两份**的 pane 文件存在且 hash 对得上（和 restore 同一标准）。pane 文本仍不进指纹。任一份校验失败则非零退出、什么都不删，避免新份已经缺 pane、却把旧的完好自动备份删掉。
4. 校验通过后比指纹。较旧被较新覆盖（相同，或较新只多了 session / window / pane，共有窗口的 layout 和根进程不变）：删较旧的，留较新的。较新缺了较旧里的东西（关了 session/window），或共有窗口 layout / 根进程变了：两份都留。

只比这一对。不会顺着历史做 run-length 压缩，也不会跨 socket 比较。`-L sockA` 只看 `~/.remux/backup-sockets/<sockA>/`。

留新删旧：留下的那份带最新时间戳和最新 scrollback。不是原地覆盖同一个目录——只有进入比较且两份 payload 都读得回来之后，才删旧的自动备份。

## 为什么不能比整份 snapshot

磁盘上每次 backup 几乎必变：

| 数据 | 5–10 分钟 idle 会不会变 |
| --- | --- |
| `summary.json` 的 `backup_id` / `created_at` / `manifest_sha256` | 必变 |
| `manifest.json` 里同样的时间字段 | 必变 |
| `pane_table.*.sha256` 和 `panes/*.txt` | 几乎必变（prompt、时钟、`htop`） |
| `command_tree.pid`（子进程） | 开/关一次命令就变 |

因此 `manifest_sha256` 或整目录字节相等在定时场景下几乎永不成立。compact 要比的是一枚**投影指纹**，不是整包。

restore 也不会把记录的进程再拉起来。判重问的是：「再留一份，restore 出来的工作区外形会不会不一样？」不是「当时 pane 里闪过什么」。

## 指纹比什么、为什么

投影来自对这两份目录重新 `read_snapshot_dir` 得到的拓扑，以及 summary 里的 `schema_version`。catalog 选出目录后，比较用的不是第一次 listing 留下的 `BackupEntry.snapshot`。

```text
schema_version.major, schema_version.minor
+ sessions 按 name 排序 [
    name,
    windows 按 id 排序 [
      id, layout,
      panes 按 id 排序 [
        pane_id,
        command_tree 根: pid + name + argv
        （没有 tree 则根 = 空）
      ]
    ]
  ]
```

对应实现：`src/storage/fingerprint.rs` 的 `CompactFingerprint::from_tmux()`。

### 要比的字段

**`schema_version`**

格式契约。1.0 没有 `command_tree`，1.1 有。拓扑和根 pid 看起来一样时，把最后一份旧格式塌掉，等于丢掉「这是旧契约下拍的」这一事实。major/minor 都比。

**session `name`，window `id` / `layout`，pane `pane_id`**

这是 restore 真正会重建的东西：有哪些 session、窗口怎么排、分不分屏。`layout` 字符串里已经带终端尺寸（如 `120x40`），resize 会反映在 layout 上，不必再单独比 `size`。投影时 sessions 按 `name`、windows/panes 按 id 做稳定排序，`list-sessions` 等捕获顺序不影响相等。argv 不排序。

**根进程 `pid` + `name` + `argv`**

根是 tmux 在这个 pane 里拉起的第一个进程，通常是那颗一直活着的 zsh。

- 同一颗 zsh idle 几小时：pid 不变 → 可以塌。
- 关 pane 再开一扇「长得像」的：新 pid → 新备份。
- `respawn-pane` / shell 挂了再拉：新 pid → 新备份。
- `exec python`：pid 不变，但 `name` / `argv` 变 → 新备份。所以三者要一起比，不能只比 pid。

没有 `command_tree`（schema 1.0，或当时进程已经没了）当成根为空；两个空相等，空对上「zsh:18421」不相等。

### 明确不比的字段

**`backup_id` / `created_at` / `manifest_sha256`**

身份和时间。放进指纹，compact 永远不会触发。

**window `name`**

没手改过名字时，tmux `automatic-rename` 会把窗口名换成当前前台进程（`zsh` → `tig`）。根还是那颗 zsh，外形没变。手 `rename-window` 也不比：compact 留新，新名字写在留下的那份里。

**pane `path`（cwd）**

`cd` 只是那颗 zsh 里的状态，不是 pane 换了。restore 仍会写入最新那份的 cwd；判重不关心你在哪个目录闲着。

**`command_tree.children` 整棵子树（含它们的 pid）**

restore 不会重跑 vim / cargo。子进程是定时备份里最吵的一层：`git status`、一次编译、已结束的后台任务都会让「相等」失败，而下一拍它们往往已经没了。完整 tree 仍写在留下的那份备份里，只是判重不看孩子。

**根上的 `foreground`**

一开 vim，zsh 就会从 foreground 变成 false。若比这个，等于把「有没有子进程」从旁门偷渡进指纹。

**session `attached`，window / pane `active`**

attach/detach、点一下别的 pane 就会变，不是工作区外形变了。

**session / pane `size`**

和 `layout` 重复。只信 layout。

**`content_ref`、整个 `pane_table`、pane 文本文件**

scrollback。idle 时几乎每次都变。留下的新备份里仍有最新文本；丢掉的是上一份几乎一样的副本。

## 自动名 vs 手起的名

自动 id 来自 backup 未指定名字时的 `%Y%m%d_%H%M%S`（15 字符，且能解析为合法日期时间）。`is_automatic_backup_id()` 在 `src/backup_name.rs`。

`backup_20240101_120000`、`sprint_demo` 都不是自动名。较旧的那份若是这种名字，compact 直接停，不比指纹，也不验 pane。

反过来：最新的是手起的名、较旧的是自动时间戳，且较旧被较新覆盖，会删掉那份自动备份，留下手起的名。

## 和 restore 的关系

restore 重建 session / window / layout / pane / cwd / 文本，**不**启动 `command_tree` 里的进程。

所以指纹对齐的是「外形是否还是同一套 tmux」，外加「pane 根还是不是原来那个进程」。它刻意比 restore 少看 cwd、窗口名和文本（太吵），又比 restore 多看根 pid（区分「同一扇 pane」和「新开的一扇长得很像的 pane」）。

## 命令结果

| 情况 | 退出 | stdout |
| --- | --- | --- |
| 不足两份 | 0 | `Need at least two backups to compact` |
| 较旧的不是自动名 | 0 | `Previous backup {name} is not an automatic backup` |
| 较旧未被较新覆盖 | 0 | `Latest backups {kept} and {previous} differ, nothing to compact` |
| 已删除较旧自动备份（相同或被覆盖） | 0 | `Removed backup {old} (covered by {new})` |

较旧的是自动备份并进入比较时：读 snapshot、pane 文件缺失/损坏、或删目录失败才非零。较旧的是手起的名：退出码 0，不验 pane。

## 非目标

- 不是 `backup --compact`。backup 只负责拍，compact 只负责比最新一对。
- 不扫整个目录做相邻串压缩。一小时 idle 若从没跑 compact，会留下多份；需要的话对同一目录多跑几次 compact。
- 不做「历史上出现过就删」。隔了一段别的工作再回到同一外形，那是第二次，该留。
- 不提供「把 pane 文本 / 子进程算进去」的严格模式。那是以后的事，不能当默认。

## 代码入口

- 指纹：`src/storage/fingerprint.rs` `CompactFingerprint::from_tmux`
- 选最新两份并删除：`src/actions/compact.rs`
- 自动名：`src/backup_name.rs` `is_automatic_backup_id`
- schema：`storage::read_schema_version`
- 行为测试：`tests/compact.rs`
