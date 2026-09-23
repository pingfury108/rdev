# rdev

轻量的远程开发代理 CLI：本地只做编辑与日志查看，把构建/运行卸载到远程服务器。

```bash
rdev go build ./...        # 同步代码 → 远端同名目录执行 → 输出/退出码实时回传
```

## 核心模型

- 服务器是一组命名 context（类 kubectl），`current` 决定命令落在哪台机器
- 远程目录 = `root/<本地项目目录名>`，自动推导，**零项目级配置**
- 复用系统 `ssh`/`rsync` 与 `~/.ssh/config`，支持 ProxyJump/Agent/连接复用

## 安装

```bash
cargo install rdev-cli     # 装出的二进制叫 rdev
```

要求：本地与远端均有 `rsync`（远端缺失时 `rdev server setup` 可自动安装），远端有 bash 兼容的 login shell。

## 30 秒上手

```bash
rdev server add dev user@192.168.1.100   # 添加服务器（首个自动设为 current）
rdev server setup                        # 远端装机：rsync + mise
cd your-project
rdev go build ./...                      # 开跑
```

## 命令

```bash
rdev <cmd>...                # 同步 + 远端执行（主路径）
rdev sh <<'EOF' ... EOF      # 同步 + 从 stdin 执行一段脚本（免转义）
rdev shell                   # 同步 + 进入远端项目目录的交互 shell
rdev sync                    # 只同步
rdev config                  # 打印配置文件路径
rdev server add/use/ls/rm    # 服务器管理与切换
rdev server setup [name]     # 远端幂等装机
```

## 工具链（mise 集成）

项目根放 `mise.toml` / `.tool-versions` 声明工具版本，远端首次执行自动安装：

```toml
[tools]
go = "1.22"
java = "temurin-21"
```

## License

MIT OR Apache-2.0
