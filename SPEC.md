# rdev 设计 Spec（v1.0）

> 一个轻量、零配置的远程开发代理命令行工具。
> 本地只做编辑与日志查看，构建/运行卸载到远程服务器。

## 1. 核心心智模型

```
~/codes/my-app  ──rdev──>  <current-server>:~/rdev/my-app
```

- **服务器是一组命名 context**（类比 kubectl），`current` 指针决定命令落到哪台机器
- **目录名即远程目录**：远程目录 = `root/<项目目录名>`，自动推导，无项目级配置
- 项目名取**当前目录 basename**；若在 git 仓库子目录中执行，取仓库根目录名，同步以仓库根为源，`cd` 到对应的远程子目录

一句话：**加前缀即在远端跑，目录名即远程目录。**

## 2. 配置 Spec

唯一配置文件：`~/.config/rdev/config.toml`（遵循 XDG：`$XDG_CONFIG_HOME` 优先；全平台统一，不用 macOS 的 `Application Support`。若旧版本已在平台默认位置生成配置，自动兼容读取）

```toml
current = "dev-linux"

[servers.dev-linux]
host = "dev-linux"              # ~/.ssh/config 别名，或 user@ip
root = "~/rdev"                 # 远程工作区根，可省略，默认 ~/rdev

[servers.build-mac]
host = "admin@mac-mini.local"
root = "~/build"
shell = "/bin/zsh"               # 可选：add/setup 时自动探测，一般无需手填
```

| 字段 | 必选 | 说明 |
|------|------|------|
| `current` | 否 | 当前服务器名；为空时执行类命令报错并提示 |
| `servers.<name>.host` | 是 | 完整复用 `~/.ssh/config`（ProxyJump/Agent/端口等均在其中配置） |
| `servers.<name>.root` | 否 | 默认 `~/rdev`；`~` 在远端展开；禁止为 `""`、`/`、`~` |
| `servers.<name>.shell` | 否 | 远端 login shell 路径；`server add`/`setup` 时通过 `$SHELL` 自动探测，缺省 fallback 为 `bash`；exec/shell 均使用该 shell，保证 fish/zsh 用户的环境配置（PATH、mise activate 等）生效 |

无项目级配置文件。自定义排除通过项目自身的 `.gitignore` 表达。

## 3. 命令 Spec

### 3.1 `rdev <cmd>...`（主路径，隐式 run）

```bash
rdev go run main.go
rdev gradle assembleDebug
```

流程：

```
1. 读配置 → current → Server{host, root}
2. 定位项目：向上找 .git 定根；root basename 为项目名
3. sync：rsync 增量推送（见 §4）
4. exec：单条 SSH 执行（见 §5）
5. 以远端退出码退出
```

### 3.2 `rdev shell`

同步后进入远程项目目录的交互式 login shell：

```
ssh -t <host> 'mkdir -p <dir> && cd <dir> && exec bash -l'
```

始终分配 TTY。退出 shell 后本地以 shell 退出码退出。

### 3.3 `rdev sync`

只执行 §4 的同步，不执行命令。

### 3.4 `rdev config`

打印配置文件路径（`~/.config/rdev/config.toml`）；文件尚不存在时在 stderr 提示引导命令。

### 3.5 `rdev server`（子命令组）

| 命令 | 行为 |
|------|------|
| `rdev server add <name> <host> [--root DIR] [--force]` | 添加服务器；首个自动设为 current；重名报错，`--force` 覆盖；写入后做连通性探测（`ssh -o BatchMode=yes -o ConnectTimeout=5 <host> true`），失败仅警告不阻塞 |
| `rdev server use <name>` | 切换 current；不存在则报错 |
| `rdev server ls` | 列表，current 标 `*`；裸敲 `rdev server` 等同 |
| `rdev server rm <name>` | 删除；若删的是 current，current 置空 |
| `rdev server setup [name]` | 幂等装机：检测/安装 `bash`、`rsync`（按 apt/dnf/yum/pacman/brew 自动选择，sudo 提示密码）、安装 `mise` 到 `~/.local/bin`（免 sudo）；缺省对 current 执行；全程一项一行输出 `ok/missing/FAILED` |

### 3.6 工具链依赖：mise 集成

项目工具链（go/JDK/node/cargo...）不由 rdev 管理，委托 [mise](https://mise.jdx.dev)：

- 项目根存在 `mise.toml` / `.mise.toml` / `.tool-versions` 时，`rdev <cmd>` 的远端命令自动包装为 `mise x -- <cmd>`（声明文件随 rsync 同步，远端首次执行自动安装工具，之后零开销）
- 无声明文件的项目零侵入，直接使用远端系统环境
- 远端无 mise 时命令报 `mise: command not found`，跑 `rdev server setup` 即可

## 4. 同步 Spec（rsync）

```
rsync -az \
  --exclude=<内置规则>... \
  --exclude=<.gitignore 规则>... \
  -e 'ssh <控制连接参数>' \
  --rsync-path='mkdir -p <remote_dir> && rsync' \
  <项目根>/  <host>:<remote_dir>/
```

- **内置排除**：`.git/`、`target/`、`node_modules/`、`build/`、`dist/`、`__pycache__/`
- **`.gitignore` 感知**：读取项目根的 `.gitignore`，非注释、非取反（`!`）行转为 `--exclude`（近似语义；嵌套 `.gitignore` 与取反规则 v1 不支持）
- **不带 `--delete`**：远端多余的文件保留，永不主动删除
- **远程目录自动创建**：通过 `--rsync-path` 在同步同一条连接内完成，无额外 SSH 往返
- 同步静默执行；失败时 rsync 错误直接输出，rdev 以非零退出

## 5. 远程执行 Spec（ssh）

```
ssh <控制连接参数> [-t] <host> \
  'mkdir -p <dir> && cd <dir> && exec <login-shell> -lc <quoted-cmd>'
```

- **单条连接**：建目录、切换目录、执行合并为一次 SSH
- **连接复用**：自动注入 `ControlMaster=auto`、`ControlPath=<cache>/rdev/cm-%C`、`ControlPersist=10m`，热路径开销 <50ms；不影响 `~/.ssh/config` 既有配置
- **login shell**：以配置中探测到的远端 login shell 执行（`<shell> -lc`），保证 PATH 与用户环境配置完整（bash/zsh/fish 均可）；项目含 mise 声明文件时包装为 `mise x --`（见 §3.6）
- **参数安全**：每个参数独立 shell-quote，杜绝转义错误与注入
- **路径展开**：`~` 前缀不做引号包裹，交由远端 shell 展开
- **TTY**：本地 stdout 是终端时加 `-t`（支持交互命令与颜色）；管道场景不加
- **信号**：前台进程组语义，Ctrl+C 经 ssh pty 送达远端进程
- **退出码**：本地退出码严格等于远端命令退出码；ssh 自身被信号杀死时退出 1

## 6. 安全约束

- `root` 禁止配置为 `""`、`/`、`~`（`server add` 时校验）
- 永不默认 `--delete`
- 配置写入前全量校验 TOML 结构

## 7. 已知限制（v1 明确不做）

- 嵌套 `.gitignore`、`.gitignore` 取反规则
- 远端 `bash` 缺失（`rsync`/`mise` 可由 `rdev server setup` 自动修复，`bash` 需手动）
- 远端 login shell 需支持 `&&`（bash/zsh/fish 3+/sh 均可）；`server setup` 脚本强制经 `sh -c` 执行，不受 login shell 影响
- 产物回拉、watch 持续同步、多 profile——预留架构位，见 §9

## 8. 技术栈与模块

Rust 单二进制。依赖：`clap`（derive + external_subcommand）、`serde` + `toml`、`dirs`、`anyhow`。同步与执行直接调系统 `rsync`/`ssh`。

```
src/
├── main.rs     # CLI 树：Run(external) / shell / sync / server(add|use|ls|rm)
├── config.rs   # 配置读写、current 解析、root 校验
├── project.rs  # 项目定位（.git 向上探测）、项目名/相对路径推导
├── sync.rs     # rsync 调用、内置排除 + .gitignore 解析
├── setup.rs    # 远端幂等装机脚本（bash/rsync/mise）
└── ssh.rs      # ssh 调用、shell quote、远程路径构造、控制连接参数
```

## 9. 里程碑

- **M1（本 Spec）**：上述全部命令与安全约束
- **M2（候选）**：watch 同步模式、`rdev pull` 产物回拉、profile 多目标
