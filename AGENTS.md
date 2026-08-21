# AGENTS.md

本文件供在此仓库工作的 AI 代理使用。

## 项目概览

- 工具箱命令行工具（Rust）：从工具箱目录查找并运行工具，跨平台（Windows / Unix）。
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
| `src/cli.rs` | clap 定义与命令分发（薄层，不写命令逻辑），含解析单元测试 | 纯逻辑 |
| `src/commands/` | 每个子命令一个模块（`Args` + `run`）；`mod.rs` 提供共享 `Context`（bin-dir + allow_escape）与 `locate_tool`/`tool_not_found_error` | 纯逻辑 |
| `src/toolbox.rs` | 工具箱目录模型：`Toolbox`（resolve/list/locate/add/remove）、扩展名搜索、containment 校验 | 纯逻辑（含 cfg 分支） |
| `src/runner.rs` | `Invocation`（Direct / PowerShell）、`RunOptions`（--cwd/--env）、执行与退出码透传（参数为 `&[OsString]`，保持透传逐字节） | `#[cfg(windows)]` 分支 |
| `src/suggest.rs` | 未找到工具时的拼写建议（Levenshtein ≤ 2，strsim） | 纯逻辑 |
| `src/error.rs` | 统一 `Error` 枚举（thiserror）+ `code()` 退出码映射 | 纯逻辑 |
| `src/messages.rs` | **全部用户可见文本**（帮助、错误、输出格式） | 纯逻辑 |
| `tests/cli.rs` | 二进制级集成测试（`CARGO_BIN_EXE_run-cli`），覆盖全部子命令 | 双平台 |

设计约定：

- 纯逻辑模块必须可直接单测；新增逻辑优先拆到纯模块并补单元测试。
- 每个子命令对应 `src/commands/` 下的一个模块（`Args` 参数结构 + `run(args, &Context) -> Result<i32, Error>`，`run` 返回透传的退出码）；新增命令时在 `src/commands/` 加模块、`cli.rs` 的 `Command` 枚举加变体 + 分发 + 解析单测 + `tests/cli.rs` 集成测试。
- **用户可见文本**分两类，规则如下：① clap 帮助与命令输出格式**必须**定义在 `src/messages.rs`，调用点只引用它（帮助文本用 `const` 常量、运行时消息用 `pub fn`）；② 错误 Display 文本由 thiserror 属性**集中定义在** `error.rs`/`toolbox.rs`/`runner.rs` 的枚举变体上，禁止在命令模块内联 `format!` 构造面向用户的错误字符串（如确实需要运行时格式化的错误消息，放入 `messages.rs`）。禁止在其他文件出现面向用户的裸字符串。`messages.rs` 声明了 `#![allow(missing_docs)]`，其余模块受 `lib.rs` 的 `#![warn(missing_docs)]` 约束（零警告为准）。
- 错误处理：统一 `Error` 枚举（`error.rs`）。退出码约定：0 成功 / 1 运行出错 / 2 用法错误（clap 自动）/ 127 工具未找到（`EXIT_TOOL_NOT_FOUND`）。"未找到"的富错误消息（含拼写建议与可用工具列表）通过 `commands::tool_not_found_error` 构造，避免在多个命令中重复。
- 退出码经 `ExitCode::from(code as u8)` 截断为 8 位，属平台限制，README 已文档化，不要"修复"。
- 路径边界：`Toolbox::locate` 默认要求解析结果位于工具箱内（`canonicalize` 校验，`ToolboxError::EscapeAttempted`），`--allow-escape` 放开；绝对路径输入同样受边界约束。**不要用 `Path::canonicalize` 的结果直接展示给用户**——Windows 上会带 `\\?\` 前缀，需经 `toolbox.rs::display_path` 剥离。
- `run` 的透传规则（uv 风格，改参数定义时保持测试同步）：`--cwd`/`--env` 是 run-cli 自身选项，**必须放在工具名之前**；工具名（第一个非选项 token）之后的一切参数原样透传（含 `--`、含与 run-cli 选项重名的 flag）。工具名前出现 `--` 时其后第一个 token 视为工具名（转义 `-` 开头的名字）。实现：`cli.rs::split_run_args` 预分割 argv，透传段不经过 clap，解析后手工注入 `args`；重建 argv 时在 tool 前加 `--` 分隔符（`tool` 位置参数因此**不要**加 `allow_hyphen_values`，否则未知选项会被吞成工具名）。**分割器中硬编码的选项集合必须与 clap 定义同步**（全局：`-b`/`--bin-dir`/`--allow-escape`；run：`--cwd`/`--env`）。对应测试：`cli.rs::split_*`、`cli.rs::run_*`、`tests/cli.rs::run_tool_*`。
- `add`：优先 symlink（Windows 无权限时静默降级为复制，目录递归），返回 `(AddOutcome, PathBuf)`；名字必须是纯文件名（禁路径分隔符）。
- `remove`：文件经 `locate` 解析（含扩展名补全）；目录按精确名匹配且需 `--recursive`。指向工具箱外部的 symlink 条目删除的是**链接本身**（绝不动目标），见关键技术约束 7。

## 关键技术约束（踩过的坑，不要重犯）

1. **clap `trailing_var_arg` 只对"未声明的参数"透传**：声明过的选项（如 `--cwd`/`--env`）即使在位置参数之后仍会被 clap 消费，所以 `run` 的 uv 风格透传靠 `cli.rs::split_run_args` 预分割实现，透传段完全不经过 clap；不要在 RunArgs 的 `tool` 上加 `allow_hyphen_values`（会把工具名前的未知选项吞成工具名）。
2. **`use clap::Args;` 与 `pub struct Args` 同名冲突**：命令模块的 Args 结构用全限定 `#[derive(Debug, clap::Args)]`，不要 `use clap::Args`。
3. **`DirEntry::file_type()` 不跟随符号链接**：`Toolbox::list` 必须用 `entry.metadata()`，否则 `add` 创建的符号链接工具不会出现在 `list` 里。
4. **`Path::join` 遇绝对路径会整体替换**：`dir.join("C:\\x")` 得到 `C:\x`，remove/add 等操作必须校验绝对路径或依赖 `ensure_within` 兜底。
5. **Windows 上 `is_file()` 大小写不敏感**（NTFS），`with_extension` 探测顺序即优先级；Unix 精确匹配，`TOOL_EXTENSIONS` 为空。
6. **Windows 符号链接需要开发者模式/管理员权限**：集成测试与 CI 上 `add` 可能降级为复制，测试断言必须兼容两种结果（`linked` 或 `copied`）。
7. **`remove` 指向外部的 symlink 时删除的是链接本身**：`locate` 会对目标做 containment 校验并报 `EscapeAttempted`，但该条目在工具箱内、删链接不会动目标，所以 `remove` 命中 `EscapeAttempted` 时改用 `find_candidate` + `is_lexically_within` 确认条目不逃逸后直接删链接；绝对外部路径仍拒绝。目录 symlink 用 `remove_link`（Windows 目录链接需 `remove_dir`，文件链接 `remove_file`）。
8. **相对工具路径 + `--cwd` 会解析错位（Unix）**：`Toolbox::locate` 对相对 bin-dir 返回相对路径，Unix 上 spawn 时若先设了子进程 `current_dir`，`execvp` 会把相对路径相对新 cwd 解析导致 spawn 失败（Windows 上 Rust std 已按父进程 cwd 绝对化程序路径，无此问题，但绝对化后行为一致更稳妥）。`run` 在 spawn 前必须用 `std::path::absolute` 绝对化（`which` 同样绝对化输出；`std::path::absolute` 不产生 `\\?\` 前缀，与 `canonicalize` 不同）。回归测试：`tests/cli.rs::run_with_cwd_and_relative_bin_dir`。

## 手工验证

- 只读命令（`list`/`which`/`completions`/`run <只读工具>`）可随意冒烟测试；`add`/`remove` 使用临时目录。
- Windows 冒烟可用真实工具（如 `target/bin/wallpaper-cli.exe` 的只读命令 `monitors`）验证 `run` 的完整链路：`add` → `which` → `run` → `remove`。
- 验证逃逸边界：`run ../xxx`（工具箱外真实存在的文件）默认必须报 `EscapeAttempted`（退出码 1），加 `--allow-escape` 后必须可运行。