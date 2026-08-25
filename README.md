# run-cli

一个管理工具箱的命令行工具（Rust）：注册并运行工具。

## 功能

- `run`：解析工具名并执行，工具名之后的参数原样透传（uv run 风格）
- `list`：列出全部工具（config 注册 + bin 目录），支持 `--json` 机器可读输出
- `which`：只解析工具名并打印绝对路径，不运行
- `add` / `remove`：管理 config 注册表（只记录 name → 路径，不复制/链接/删除任何文件）
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

## 工具来源与解析顺序

工具来自两个来源，**config 优先，bin 兜底**（config 对该名字任一候选命中即生效，bin 同名被整体遮蔽）：

1. **config 注册表**（`add`/`remove` 操作的对象）：`config.toml` 中记录 `name → 绝对路径`，可指向**任意位置**的外部程序
2. **bin 目录**（用户手工放置）：解析规则见下文"路径解析规则"

### config 文件位置

`--config <PATH>`（全局参数，须在子命令前）> `RUN_CLI_CONFIG` 环境变量（空值视为未设置）> 平台默认：

- Windows：可执行文件同目录下的 `config.toml`
- Unix：`~/.run-cli/config.toml`

config 位置**独立于** bin-dir（`--bin-dir` 只影响 bin 目录）。文件缺失 = 空注册表；文件损坏（非法 TOML）时所有命令报错（exit 1）。

```toml
[[tools]]
name = "clang"
path = "C:\\LLVM\\bin\\clang.exe"

[[tools]]
name = "echo"
path = "/usr/bin/echo"
```

## 工具箱目录优先级

1. `--bin-dir <PATH>` / `-b <PATH>` 参数
2. `RUN_CLI_BIN` 环境变量（空值视为未设置）
3. 平台默认：Windows 为**可执行文件同目录下的 `bin`**，Unix 为 `~/.run-cli/bin`；无法确定可执行文件路径或 `$HOME` 时回退 `./bin`

bin 目录不会自动创建：`run`/`list`/`which` 在目录不存在时按"空 bin"处理（config 是唯一来源时不报错）；纯 bin 用户（无 config 文件）在 bin 缺失时 `run`/`which` 报错（exit 1）。

## 用法

```
run-cli [-b/--bin-dir <PATH>] [--config <PATH>] <子命令>
```

| 子命令 | 说明 |
| --- | --- |
| `run <tool> [--cwd DIR] [--env K=V]... [args...]` | 运行工具；`--cwd`/`--env` 为 run-cli 自身的选项 |
| `list [--json]` | 列出全部工具（config 注册 + bin 内容，目录带 `/` 后缀，按名称排序） |
| `which <tool>` | 打印工具解析后的绝对路径 |
| `add <path> [--name <NAME>] [--force]` | 注册工具：把 name → 绝对路径写入 config（**不复制、不链接**） |
| `remove <tool>` | 从 config 注销工具（**不删除任何文件**） |
| `completions <shell>` | 生成补全脚本（bash/zsh/fish/powershell/elvish） |

```bash
# 注册与运行（config 指向任意位置的外部程序）
run-cli --config D:\run-cli\config.toml add C:\LLVM\bin\clang.exe
run-cli run clang --version
run-cli which clang          # 打印注册的绝对路径
run-cli remove clang         # 只注销 config 条目，源文件原样保留

# bin 目录手工放置的工具（解析兜底）
run-cli -b D:\tools list
run-cli -b D:\tools run example --help

# Windows 扩展名自动补全，以下等价（按 .exe → .bat → .cmd → .ps1 查找）
run-cli run example
run-cli run example.exe

# 补全
run-cli completions powershell | Out-String | Invoke-Expression
```

`add` 名字默认取源的**全文件名**（`add program.exe` 注册为 `program.exe`），`--name` 可覆盖；重名默认报错，`--force` 覆盖。`add` 仅支持单个文件（目录请逐个注册或手工放入 bin）。

### run 的参数透传规则（uv 风格）

`run` 的工具名是第一个非选项参数，**工具名之后的一切参数原样透传**，包括与 run-cli 自身选项重名的 flag：

- `run-cli run example --help -v x` → 工具收到 `--help -v x`
- `run-cli run example --cwd work` → 工具收到字面量 `--cwd work`（不会被 run-cli 消费）
- `run-cli run example -- --help` → 工具收到 `-- --help`（`--` 也原样透传）

run-cli 自身选项必须放在工具名之前：

- `run-cli run --cwd work --env FOO=bar example -v` → `--cwd`/`--env` 由 run-cli 消费，`-v` 透传
- 全局选项 `--bin-dir`/`--config` 同样必须放在 `run` 之前；放在工具名后会被透传给工具

工具名以 `-` 开头时用 `--` 转义：`run-cli run -- -weird-tool`。

## 路径解析规则

- 工具名按 `config 注册名` 或 `工具箱目录/名称` 解析；bin 支持子目录：`run-cli run tools/example`
- 裸名（无扩展名）按平台扩展名候选搜索（Windows 按上表顺序，大小写不敏感），找不到再回退裸文件名；其他平台精确匹配；config 与 bin 使用同一候选顺序
- 支持绝对路径输入（bin 内）
- **路径边界**：bin 目录的所有操作都限定在目录内。绝对路径与含 `..` 的路径按**解析后位置**判定，指向目录外的一律视为未找到（退出码 127）；`..` 解析后落回目录内的路径可用。**符号链接条目按其自身位置判定**——链接本身在 bin 内即可操作，不要求其目标也在 bin 内。config 注册条目不受此边界约束（用户显式注册授权）
- **注册失效**：config 条目指向的文件被删除/变成目录时，`run`/`which` 报错（exit 1）且**不回退** bin（坏注册要响亮暴露）
- `list` 会跳过**悬空符号链接**与**路径缺失的 config 条目**——一条断链不会让整个列表或拼写建议失效；其余条目照常列出

## list --json 格式

```json
[
  { "name": "clang", "path": "C:\\LLVM\\bin\\clang.exe", "kind": "file", "size": 12345, "origin": "config" },
  { "name": "echo.bat", "path": "C:\\tools\\echo.bat", "kind": "file", "size": 20, "origin": "bin" },
  { "name": "scripts", "path": "C:\\tools\\scripts", "kind": "directory", "origin": "bin" }
]
```

- `origin`：`"config"`（config 注册）或 `"bin"`（bin 目录）；同名时 config 条目优先展示
- 目录条目不包含 `size`

## 退出码

| 退出码 | 含义 |
| ------ | ---- |
| 0      | 成功（`run` 透传子进程退出码） |
| 1      | 运行出错 / 注册失效（config 条目路径缺失）/ config 损坏 / 其他操作失败 |
| 2      | CLI 用法错误（clap） |
| 127    | 工具未找到（config 未命中且 bin 未命中，或 `remove` 的目标未注册） |

注：进程退出码按平台限制为 8 位，子进程退出码超过 255 时会被截断。

另外注意：`run`/`which` 在**纯 bin 场景（无 config 文件）且 bin 目录不存在**时报 1（"bin directory not found"），而不是 127——127 只表示目录存在但工具未找到（或路径解析到工具箱之外）。由于 `run` 会透传子进程退出码，子进程自身退出 127 与"工具未找到"无法区分。

## 平台说明

- Windows：`.ps1` 通过 PowerShell（`-NoProfile -ExecutionPolicy Bypass -File`）运行；config 与 bin 默认都位于 exe 同目录
- Unix：无扩展名补全，config 默认 `~/.run-cli/config.toml`，bin 默认 `~/.run-cli/bin`
- 透传限制：参数透传对 Windows 的 `.bat`/`.cmd`（由系统经 `cmd.exe` 执行）与 `.ps1`（经 PowerShell `-File`）并非逐字节——两者会按自身规则重解析/转义参数（如 `%`、`&`、引号等）。这是平台固有行为，无法完全规避；原生可执行文件（`.exe`）与 Unix 脚本为逐字节透传

## 许可证

[MIT](./LICENSE) © 2026 miyu52