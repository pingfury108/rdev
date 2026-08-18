---
name: rdev
description: rdev 远程开发代理 CLI。当用户要在远程服务器上执行构建/运行命令（如 gradle 构建、go/rust 编译、docker 构建等重负载任务）、同步代码到远程、进入远程项目 shell、管理/切换 rdev 服务器，或排查 rdev 报错（rsync 缺失、shell 探测、远端环境）时使用。核心用法：本地命令加 rdev 前缀即在远端同名目录执行。
---

# rdev — 远程开发代理

本地只做编辑与日志查看，构建/运行卸载到远程服务器。

## 心智模型

- 服务器是一组命名 context，`current` 决定命令落在哪台机器
- 远程目录 = `root/<本地项目目录名>`，自动推导，无项目级配置
- 在 git 子目录执行时，以仓库根为同步源，远端 cd 到对应子目录

## 首次使用引导（务必按序检查）

用户想用 rdev 时，先确认环境就绪，不要直接执行命令：

```bash
rdev server ls
```

1. **没有服务器**（输出 `no servers...`）→ 引导添加：
   - 问用户远端地址；优先建议复用 `~/.ssh/config` 里已有的 Host 别名，或直接 `user@ip`
   - `rdev server add <name> <host>`（首个自动成为 current，自动探测连通性/rsync/login shell）
   - 前提：用户能徒手 `ssh <host>` 免密登录（密钥/Agent）；不能则先解决 SSH 本身
2. **有服务器但 current 不对** → `rdev server use <name>`
3. **add 时警告 rsync 缺失，或远端是新机器** → `rdev server setup`（装 rsync + mise，可能提示 sudo 密码）
4. **就绪** → 进入下方的日常使用

## 日常使用

```bash
rdev <cmd>...                  # 主路径：rsync 增量同步 → 远端 login shell 执行 → 退出码透传
rdev shell                     # 同步后进入远端项目目录的交互 shell（调试/手动操作首选）
rdev sync                      # 只同步不执行
```

远程目录自动推导为 `root/<本地项目目录名>`（在 git 子目录执行时以仓库根为源、cd 到对应子目录），无需任何项目级配置。

重型命令优先建议用 rdev 跑：gradle 构建、cargo/go 编译、docker build、大规模测试等；本地 8GB 内存机器尤其如此。

## 服务器管理与配置

```bash
rdev server add <name> <host> [--root DIR] [--force]   # 添加/覆盖
rdev server use <name>         # 切换 current
rdev server ls                 # 列表，current 标 *
rdev server rm <name>          # 删除
rdev server setup [name]       # 远端幂等装机
rdev config                    # 打印配置文件路径（~/.config/rdev/config.toml）
```

配置文件示例：

```toml
current = "dev"
[servers.dev]
host = "dev"                   # ~/.ssh/config 的 Host 别名，或 user@ip
root = "~/rdev"               # 可省略，默认 ~/rdev
shell = "/usr/bin/fish"       # add/setup 自动探测，无需手填
```

## 工具链（mise 集成）

项目需要的开发工具（go/JDK/node/rust/python/gradle...）不由 rdev 管理，委托 [mise](https://mise.jdx.dev)。**声明文件放在项目里、随 rsync 同步，远端首次执行自动安装对应版本，之后零开销。**

### 引导用户配置

当用户的项目需要特定工具链（或远端报 `go: command not found` 这类错误）时，引导在项目根创建声明文件：

```bash
cd <项目根>
cat > mise.toml <<'EOF'
[tools]
go = "1.22"          # 也可以是 node = "20"、java = "temurin-21"、python = "3.12" 等
EOF
```

也可以用 `mise use go@1.22` 生成（若本地装了 mise），但手写文件即可，不依赖本地安装。

### 使用

```bash
rdev go build ./...     # rdev 检测到 mise.toml → 远端自动 mise x -- go build ./...
                         # 首次执行会下载安装 go 1.22（较慢，属正常现象），之后秒级
```

- 兼容 `.mise.toml` 和 asdf 风格 `.tool-versions`，三者任一存在即生效
- 同一台服务器上不同项目可用不同工具版本，互不干扰
- 远端必须已装 mise：`rdev server setup` 会装；未装时报 `mise: command not found`，跑 setup 即可
- 项目没有声明文件时 rdev 零侵入，直接用远端系统环境
- 在 `rdev shell` 里手动执行时，自己加前缀 `mise x -- <cmd>`（或远端 bashrc/config.fish 里配置 `mise activate`，一次性）

## 排障速查

| 症状 | 原因与处理 |
|------|-----------|
| `fish/bash: Unknown command: rsync` 或 rsync exit 127 | 远端无 rsync → `rdev server setup` |
| `mise: command not found` | 项目声明了 mise 但远端没装 → `rdev server setup` |
| 远端 `go/java/...: command not found` | 项目未声明工具链 → 引导创建 `mise.toml`（见“工具链”节） |
| 每次执行输出远端 profile 的报错噪音 | 远端 `~/.bashrc` 等 source 了不存在的路径；`ssh host 'grep -n <关键词> ~/.bashrc ~/.bash_profile ~/.profile /etc/profile.d/*'` 定位后删除或加 `[ -f ... ] &&` 守卫 |
| `no server selected` | 未添加/未切换服务器 → `rdev server add` 或 `rdev server use` |
| 远端换了 login shell 后环境不对 | shell 缓存在配置里 → `rdev server add ... --force` 或 `rdev server setup` 刷新 |
| 命令含空格/特殊字符行为异常 | 不应发生（参数逐个 shell-quote）；若复现则为 rdev 本身的 bug |

## rdev 本身出问题或需要新功能

源码与设计文档（SPEC.md）在 `~/codes/rdev`，引导用户到该仓库下处理，不要在当前项目里临时改 rdev。
