# AGENTS.md

本文件供在此仓库工作的 AI 代理使用。

## 项目概览

- 工具箱命令行工具（Rust）：注册并运行工具，跨平台（Windows / Unix）。
- 双模型解析：**config 注册表**（`config.toml` 记录 `name -> 绝对路径`，`add`/`remove` 只操作它）优先，**bin 目录**（用户手工放置的工具，解析兜底）。
- 单 crate：lib + 薄 binary。`src/main.rs` 只做 `cli::run()` 调用、错误打印（`run-cli: error: {msg}`）与退出码映射。
- 代码注释（含文档注释、测试注释）统一使用英文；命令帮助为英文；README 为中文（用户偏好中文交流）。
- MSRV：Rust 1.88（edition 2024）。

## 常用命令

```powershell
cargo build                    # 调试构建
cargo build --release          # 发布构建（strip + LTO）
cargo test                     # 单元 + 集成测试
cargo clippy --all-targets     # 必须零警告
cargo fmt                      # 必须保持格式化一致
```

提交前必须运行 `cargo fmt`、`cargo clippy --all-targets -- -D warnings`、`cargo test`，三者全部通过。

## 架构与模块

| 模块 | 职责 | 依赖平台 |
| --- | --- | --- |
| `src/cli.rs` | clap 定义与命令分发（薄层，不写命令逻辑），含解析单元测试；全局 `--bin-dir`/`--config`；`run` 透传基于 clap `external_subcommand`（见 `commands/run.rs`） | 纯逻辑 |
| `src/commands/` | 每个子命令一个模块（`Args` + 入口函数）；`mod.rs` 提供共享 `Context`（bin-dir + config-path）、`resolve_tool`（config 优先）、`locate_tool`/`tool_not_found_error`（建议 = config ∪ bin 名称）、`tool_not_registered_error`（remove 用，建议 = config 名称） | 纯逻辑 |
| `src/config.rs` | **config 注册表模型**：`Config`/`ConfigTool`（serde + toml）、`load`（缺文件=空注册表）/`save_atomic`（.tmp + rename）、`lookup`（镜像 bin 的扩展名补全，Windows 大小写不敏感）、`--config`/`RUN_CLI_CONFIG`/平台默认定位（独立于 bin-dir）、`validate_name`、`ConfigError`（Display 集中于此） | 纯逻辑（含 cfg 分支） |
| `src/toolbox.rs` | 工具箱目录模型：`Toolbox`（resolve/list/locate）、扩展名搜索、containment 校验；**不再有 add/remove** | 纯逻辑（含 cfg 分支） |
| `src/runner.rs` | `Invocation`（Direct / PowerShell）、`RunOptions`（--cwd/--env）、执行与退出码透传（参数为 `&[OsString]`，保持透传逐字节） | `#[cfg(windows)]` 分支 |
| `src/suggest.rs` | 未找到工具时的拼写建议（Levenshtein ≤ 2，strsim） | 纯逻辑 |
| `src/error.rs` | 统一 `Error` 枚举（thiserror）+ `code()` 退出码映射 | 纯逻辑 |
| `src/messages.rs` | **全部用户可见文本**（帮助、错误、输出格式） | 纯逻辑 |
| `tests/cli.rs` | 二进制级集成测试（`CARGO_BIN_EXE_run-cli`），覆盖全部子命令 | 双平台 |

设计约定：

- 纯逻辑模块必须可直接单测；新增逻辑优先拆到纯模块并补单元测试。
- 每个子命令对应 `src/commands/` 下的一个模块（`Args` 参数结构 + 入口函数 `execute(args, &Context) -> Result<i32, Error>`，返回透传的退出码）；新增命令时在 `src/commands/` 加模块、`cli.rs` 的 `Command` 枚举加变体 + 分发 + 解析单测 + `tests/cli.rs` 集成测试。
- **用户可见文本**分两类，规则如下：① clap 帮助与命令输出格式**必须**定义在 `src/messages.rs`，调用点只引用它（帮助文本用 `const` 常量、运行时消息用 `pub fn`）；② 错误 Display 文本由 thiserror 属性**集中定义在** `error.rs`/`config.rs`/`toolbox.rs`/`runner.rs` 的枚举变体上，禁止在命令模块内联 `format!` 构造面向用户的错误字符串（如确实需要运行时格式化的错误消息，放入 `messages.rs`）。禁止在其他文件出现面向用户的裸字符串。`messages.rs` 声明了 `#![allow(missing_docs)]`，其余模块受 `lib.rs` 的 `#![warn(missing_docs)]` 约束（零警告为准）。
- 错误处理：统一 `Error` 枚举（`error.rs`）。退出码约定：0 成功 / 1 运行出错 / 2 用法错误（clap 自动）/ 127 工具未找到（`EXIT_TOOL_NOT_FOUND`）。"未找到"的富错误消息（含拼写建议与可用工具列表）通过 `commands::tool_not_found_error`（run/which，建议 = config ∪ bin）或 `commands::tool_not_registered_error`（remove，建议 = config）构造为 `Error::RichToolNotFound`（与领域错误 `ToolboxError::ToolNotFound` 区分，二者退出码都是 127）。
- 退出码经 `ExitCode::from(code as u8)` 截断为 8 位，属平台限制，README 已文档化，不要"修复"。
- **解析顺序（run/which）**：`commands::resolve_tool` = config 精确/补全命中 → bin 目录兜底。config 命中是**整体名字级**遮蔽（config 对该名字任一候选命中即生效，bin 同名整体被遮蔽）。config 命中但存储路径**缺失/非文件 → exit 1 运行时错误，绝不回退 bin**（注册失效要响亮暴露，且无拼写建议）。config 文件**损坏**（TOML 解析失败）→ 所有读 config 的命令直接报错 exit 1，不静默跳过。
- **`add`（config 注册）**：源必须存在且是**文件**（目录报错，v1 仅支持文件）；名字默认 = 源**全文件名**（`program.exe` 注册为 `program.exe`），`--name` 可覆盖且必须为单文件组件（`validate_name`）；源先 `std::path::absolute` 绝对化再存储（相对路径/`subdir/x` 均可，不 canonicalize，避免 `\\?\` 前缀）；**重名默认报错（exit 1），`--force` 覆盖**；重名判定与 lookup 同规则（Windows 上大小写不敏感、裸名按扩展名补全匹配）。绝不触碰 bin 目录。
- **`remove`（config 注销）**：只处理 config 条目，使用与 run **相同**的 lookup（`remove program` 移除 `run program` 会命中的那条）；未注册 → 富 127（建议 = config 名称）；**绝不删除任何文件**（注册源、bin 手工工具都不动）。`-r/--recursive` 已移除。
- **`list`**：合并 config 条目与 bin 条目，按名称排序；**同名去重（config 优先展示）**；JSON 每条含 `"origin": "config" | "bin"`，人类格式带 `(config)`/`(bin)` 后缀；config 条目路径缺失**跳过**（与 bin 悬空链接同一策略）；**bin 目录缺失/非目录视为空**（不报错，config 可能是唯一来源）。
- **config 定位独立于 bin-dir**：`--config <PATH>`（全局，须在子命令前）> `RUN_CLI_CONFIG`（空值视为未设置）> 平台默认（Windows：exe 同目录 `config.toml`；Unix：`~/.run-cli/config.toml`）。config 文件缺失 = 空注册表；`save_atomic` 创建父目录（如 `~/.run-cli`）。

## 关键技术约束（踩过的坑，不要重犯）

1. **`run` 的 uv 风格透传用 clap `external_subcommand` 实现，不要手写 argv 分割器**：`commands::run::ExternalCommand` 的 `Cmd(Vec<OsString>)` 把工具名（第一个非自有选项的 token）之后的一切参数原样捕获（含 `--`、含与 run-cli 选项重名的 flag），无需与 clap 定义保持任何手工同步。注意：① `run` 必须 `disable_help_subcommand = true`，否则 clap 自动生成的 `help` 子命令会拦截名为 `help` 的工具；② `disable_help_flag` 使工具名前的 `--help`/`-h`/`--version`/`-V` 报用法错误（exit 2），工具名后的则原样透传——这是期望行为；③ 旧的 `trailing_var_arg` + 预分割方案已废弃，不要再回退。
2. **`use clap::Args;` 与 `pub struct Args` 同名冲突**：命令模块的 Args 结构用全限定 `#[derive(Debug, clap::Args)]`，不要 `use clap::Args`。
3. **`DirEntry::file_type()` 与 `DirEntry::metadata()` 不跟随符号链接（Unix 上为 lstat 语义）**：`Toolbox::list` 必须用 `fs::metadata(entry.path())`（stat 语义，双平台跟随），否则符号链接工具不会出现在 `list` 里（Windows 上 `DirEntry::metadata()` 跟随链接、Unix 上不跟随，行为不一致，只测 Windows 测不出来）。
4. **`Path::join` 遇绝对路径会整体替换**：`dir.join("C:\\x")` 得到 `C:\x`，locate 等操作必须校验绝对路径或依赖 `ensure_within` 兜底。
5. **Windows 上 `is_file()` 大小写不敏感**（NTFS），`with_extension` 探测顺序即优先级；Unix 精确匹配，`TOOL_EXTENSIONS` 为空。**config 的 `lookup` 镜像同一规则**（`candidate_names` + `eq_ignore_ascii_case`），且裸名查询的候选顺序 = 扩展名候选在前、裸名在后（与 bin 的 `find_bare_candidate` 一致）。
6. **config 写入必须原子化**：`save_atomic` 写 `<path>.tmp` 后 rename，且先 `create_dir_all` 父目录（`~/.run-cli` 可能不存在）。注意 Windows 上 `fs::rename` 遇已存在目标会失败，必须先 `remove_file` 旧文件（留有微小非原子窗口，CLI 场景可接受）；rename 失败要清理 `.tmp` 残留。
7. **config 命中但路径失效 → exit 1，不回退 bin**：注册条目的路径被删除/是目录时，`resolve_tool` 直接报 `RegisteredPathMissing`/`RegisteredPathNotFile`（exit 1），**不**回退到 bin 兜底（否则坏注册会被静默掩盖）；`list` 对该条目跳过（与悬空链接策略一致）。回归测试：`tests/cli.rs::run_registered_missing_path_does_not_fallback_to_bin`、`run_registered_missing_path_exits_1`。
8. **相对工具路径 + `--cwd` 会解析错位（Unix）**：`Toolbox::locate` 对相对 bin-dir 返回相对路径，Unix 上 spawn 时若先设了子进程 `current_dir`，`execvp` 会把相对路径相对新 cwd 解析导致 spawn 失败（Windows 上 Rust std 已按父进程 cwd 绝对化程序路径，无此问题，但绝对化后行为一致更稳妥）。`run` 在 spawn 前必须用 `std::path::absolute` 绝对化（`which` 同样绝对化输出；`std::path::absolute` 不产生 `\\?\` 前缀，与 `canonicalize` 不同）。回归测试：`tests/cli.rs::run_with_cwd_and_relative_bin_dir`。
9. **`add` 存储的是绝对路径**：源相对路径（含 `subdir/x`、`..`）在注册时经 `std::path::absolute` 绝对化——否则之后换 cwd 运行会悬空。回归测试：`tests/cli.rs::add_relative_source_creates_working_entry`、`add_relative_source_with_subdirectory`。
10. **`list` 跳过悬空符号链接与路径缺失的 config 条目**：`fs::metadata` 对断链返回 `NotFound`，`Toolbox::list` 对该条目 `continue` 跳过（其余条目照常列出），其他读错误仍整体报错——避免一条断链让 `list` 与拼写建议（`list_names`）全部失效。回归测试：`toolbox.rs::list_skips_broken_symlink`、`tests/cli.rs::list_skips_config_entry_with_missing_path`。
11. **config 定位独立于 `--bin-dir`**：`resolve_config_path` 只接受 `--config`/`RUN_CLI_CONFIG`/平台默认，**不要**让 bin-dir 影响 config 位置（设计决策，用户明确要求）。
12. **config 文件存在但 bin 目录缺失时，未找到 = 富 127 而非 1**：`map_tool_not_found` 只在**无 config 文件**（legacy bin-only 场景）且 bin 目录缺失时才报 `MissingDirectory`（exit 1）；有 config 文件（即使为空）则 bin 缺失视为"无 bin 条目"，报富 127 建议（config-only 用户不受 bin 缺失困扰）。回归测试：`tests/cli.rs::run_missing_bin_dir_is_runtime_error_not_127` 与 `run_missing_tool_suggests_config_names` 对照。
13. **`remove` 绝不删除文件**：它只从 config 移除条目。手动放置的 bin 工具不在 config 中，`remove` 对其报 127（"not registered"），文件原样保留——不要"好心"回退到删 bin 文件。回归测试：`tests/cli.rs::remove_does_not_delete_source_file`、`remove_does_not_delete_bin_tool`。
14. **信任模型**：config 注册的条目可指向任意路径（用户显式 `add` 授权，不受 bin containment 约束）；bin 目录解析仍保持原有边界策略（`ensure_within` 解析后位置判定 + symlink 按自身位置）。

## 手工验证

- 只读命令（`list`/`which`/`completions`/`run <只读工具>`）可随意冒烟测试；`add`/`remove` 使用临时目录 + `--config <临时路径>`。
- 验证路径边界：`run C:\工具箱外\真实存在的文件`（绝对路径）与 `run ../xxx` 默认必须报 not found（退出码 127）；`run C:\工具箱内\绝对路径` 必须可运行。
- 验证 config 语义：`add` 后 `run`/`which`/`list`（含 `--json` 的 origin）必须一致；删掉注册源文件后 `run` 必须 exit 1 且不回退 bin；`remove` 后源文件与 bin 文件都必须原样存在。
- Unix 验证（含 MSRV 检查）可在 Ubuntu 测试机执行：`ssh myl@192.168.8.7`（免密），把仓库 rsync/scp 过去后 `cargo test`。