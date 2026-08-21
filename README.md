# run-cli

一个管理工具箱的命令行工具（Rust）：从工具箱目录中查找并运行工具。

## 功能

- `run`：解析工具名并执行，工具名之后的参数原样透传（uv run 风格）
- `list`：列出工具箱内容（文件与目录），支持 `--json` 机器可读输出
- `which`：只解析工具名并打印绝对路径，不运行
- `add` / `remove`：管理工具箱中的工具
- `completions`：为 bash / zsh / fish / powershell / elvish 生成 shell 补全
- Windows 扩展名自动补全：`.exe` → `.bat` → `.cmd` → `.ps1`（大小写不敏感）
- `.ps1` 工具自动通过 PowerShell 运行
- 工具未找到时给出拼写建议（Levenshtein ≤ 2）

## 安装与构建

```bash
cargo build --release
cargo test
```

需要 Rust 1.88+（edition 2024）。

## 工具箱目录优先级

1. `--bin-dir <PATH>` / `-b <PATH>` 参数
2. `RUN_CLI_BIN` 环境变量（空值视为未设置）
3. 默认 `./bin`

## 用法

```
run-cli [-b/--bin-dir <PATH>] [--allow-escape] <子命令>
```

| 子命令 | 说明 |
| --- | --- |
| `run <tool> [--cwd DIR] [--env K=V]... [args...]` | 运行工具；`--cwd`/`--env` 为 run-cli 自身的选项 |
| `list [--json]` | 列出工具箱内容（目录带 `/` 后缀，按名称排序） |
| `which <tool>` | 打印工具解析后的绝对路径 |
| `add <path> [--name <NAME>]` | 加入工具箱：优先创建符号链接，失败时降级为复制（目录递归） |
| `remove <tool> [-r/--recursive]` | 移除工具；移除目录需 `--recursive` |
| `completions <shell>` | 生成补全脚本（bash/zsh/fish/powershell/elvish） |

```bash
# 列出工具
run-cli -b D:\tools list
run-cli -b D:\tools list --json

# 运行工具（参数全部透传）
run-cli -b D:\tools run example
run-cli -b D:\tools run example --help
run-cli run --cwd D:\work --env FOO=bar example -v

# Windows 扩展名自动补全，以下等价（按 .exe → .bat → .cmd → .ps1 查找）
run-cli run example
run-cli run example.exe

# 解析与管理
run-cli which example            # 打印绝对路径
run-cli add C:\tools\x.exe       # 加入工具箱（链接或复制）
run-cli remove example           # 移除

# 补全
run-cli completions powershell | Out-String | Invoke-Expression
```

### run 的参数透传规则（uv 风格）

`run` 的工具名是第一个非选项参数，**工具名之后的一切参数原样透传**，包括与 run-cli 自身选项重名的 flag：

- `run-cli run example --help -v x` → 工具收到 `--help -v x`
- `run-cli run example --cwd work` → 工具收到字面量 `--cwd work`（不会被 run-cli 消费）
- `run-cli run example -- --help` → 工具收到 `-- --help`（`--` 也原样透传）

run-cli 自身选项必须放在工具名之前：

- `run-cli run --cwd work --env FOO=bar example -v` → `--cwd`/`--env` 由 run-cli 消费，`-v` 透传
- 全局选项（`--bin-dir`/`--allow-escape`）同样必须放在 `run` 之前；放在工具名后会被透传给工具

工具名以 `-` 开头时用 `--` 转义：`run-cli run -- -weird-tool`。

## 路径解析规则

- 工具名按 `工具箱目录/名称` 解析，支持子目录：`run-cli run tools/example`
- 无扩展名时按平台扩展名候选搜索（Windows 按上表顺序，大小写不敏感），找不到再回退裸文件名；其他平台精确匹配
- 支持绝对路径输入
- **默认禁止逃逸**：解析结果必须位于工具箱目录内（`..` 组件、绝对路径指向外部都会被拒绝）；需要放开时使用全局选项 `--allow-escape`。注意：符号链接指向工具箱外部同样会被视为逃逸
- `remove` 删除的是工具箱条目本身：即使某条目是指向外部的符号链接，`remove` 也只会删除链接而不会触碰其目标（`--allow-escape remove` 可删除工具箱外的绝对路径文件）

## list --json 格式

```json
[
  { "name": "echo.bat", "path": "C:\\tools\\echo.bat", "kind": "file", "size": 20 },
  { "name": "scripts", "path": "C:\\tools\\scripts", "kind": "directory" }
]
```

目录条目不包含 `size`。

## 退出码

| 退出码 | 含义 |
| ------ | ---- |
| 0      | 成功（`run` 透传子进程退出码） |
| 1      | 运行出错 / 工具箱操作失败 |
| 2      | CLI 用法错误（clap） |
| 127    | 工具未找到 |

注：进程退出码按平台限制为 8 位，子进程退出码超过 255 时会被截断。

## 平台说明

- Windows：`.ps1` 通过 PowerShell（`-NoProfile -ExecutionPolicy Bypass -File`）运行；`add` 默认尝试符号链接（需要开发者模式/管理员权限），失败时自动降级为复制
- Unix：无扩展名补全，`add` 直接创建符号链接
- 透传限制：参数透传对 Windows 的 `.bat`/`.cmd`（由系统经 `cmd.exe` 执行）与 `.ps1`（经 PowerShell `-File`）并非逐字节——两者会按自身规则重解析/转义参数（如 `%`、`&`、引号等）。这是平台固有行为，无法完全规避；原生可执行文件（`.exe`）与 Unix 脚本为逐字节透传

## 许可证

[MIT](./LICENSE) © 2026 miyu52