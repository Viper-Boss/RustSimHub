# 主线无设备开发检查

这是标准库 Python 开发辅助入口，不是发行证据、健康评估或生产确认授权。
来源：当前 `.github/workflows/rust.yml`（起始 main
`e3762ec9d302a23077825a7db3cc591a2d5dce39`）、既有 `tools/test-web-*.mjs`、
`configurator/tests/*.mjs` 和 `tools/nkn-engine` 默认离线测试。
新增代码遵循仓库 AGPL-3.0-only，AI 辅助实现，没有引入第三方 Python 依赖。

```sh
python3 tools/mainline-checks/run.py --list
python3 tools/mainline-checks/run.py --group ordinary --output .local/mainline-checks/report.json
python3 tools/mainline-checks/run.py --group native-audio --output .local/mainline-audio/report.json
python3 tools/mainline-checks/run.py --group root --group root-netns --group root-systemd --output .local/mainline-root/report.json
python3 tools/mainline-checks/run.py --group runner --output .local/mainline-runner/report.json
```

每次使用全新的输出目录；已有目录（即使只有其他文件）也会拒绝覆盖。仅允许仓库
真实 `.local/` 下的相对 JSON 路径，拒绝源码位置、已有目录和符号链接组件。
报告、每阶段原始日志及环境版本均留在该私有目录，不自动上传或发布。
公开报告应人工提炼合成结果，不复制整份 JSON、原始环境、真实数据或凭据。

## 固定组与能力

| 组 | 内容 | 前置 |
|---|---|---|
| ordinary | fmt、全仓严格 Clippy、工作区 default/all-features 各一次、production check、CI 普通 Python/shell 语法、27 组 Node 网页、4 组 configurator、Go 默认离线测试、实际生产兼容 CLI | Linux、Git、Rust/Cargo 1.98.0 rustfmt/clippy、Node、Python、Go（环境为 1.27.3）、C/C++、OpenSSL、已有夹具的 cryptography |
| native-audio | 原独立 manifest 测试、严格 Clippy | ALSA/Opus 开发库与 pkg-config |
| root | 原 Board helper、AP 单元解析、材料 CLI、启动器和 chroot | 以真实 EUID 0 运行本运行器；board 还需私有 mount namespace、ext4/loop/udev 等原夹具前置 |
| root-netns | 原 tmpfs ENOSPC、更新状态、元数据进程、维护、签名格式、worker IPC/真实进程 | 以真实 EUID 0 运行本运行器、真实 netns；ENOSPC 另需 mount namespace |
| root-systemd | 原线程耗尽、转发主管/managed 与四个 OFF 时点 | 以真实 EUID 0 运行本运行器、真实 PID1 systemd/cgroup、netns/mount、managed 另需 polkit |
| runner | 合成小进程和临时 Git 仓库单测 | 仅证明运行器，不计入产品通过数 |

不自动安装依赖。先按仓库环境说明准备工具；本云环境每个 shell 先加载
`/workspace/toolchains/activate-rustsimhub.sh`。依赖下载只按已有 Cargo.lock / Go
模块清单，不升级、改锁、关断言或放宽产品约束。系统 Python 依赖缺失时按原夹具
说明准备，运行器本身只用标准库。`--list` 给出每条准确参数、超时、依赖和组别；
清单是提交内的 Python 常量，新增测试必须显式评审清单，不在运行时按 glob 造命令。

Rust 默认和全部特性存在重复，分别计数，不能将两者之和当成独立测试数。
Rust 既有 ignored 单独报告；Root 专用项通过自己的原驱动执行，普通组成功不代表它们成功。
Python discovery 返回 0 项视为失败；有 skipped 的阶段为 `not_run`，不以退出 0 宣称全部通过。
Go 要实际出现测试通过记录；`tools/test-web-recovery-session.mjs` 是浏览器专用 ES 模块，
既有入口为 `tools/run-web-recovery-session.py`，需准备 native peer/WASM 和真实 Chromium；
它在 `--list` 与报告的 `excluded_inventory` 中显式为 `not_run`，不冒充 Node 检查或 GATT 验收。
该排除项不计入普通组阶段数，也不自动启动浏览器驱动。Node 仅使用审阅过的固定、非空合成脚本，不使用可匹配 0 项的过滤器。

CI 的 shell 拼接被拆成独立 argv 阶段；`sh -n` 的多个脚本拆为逐文件检查（后续位置参数不会被 sh 自动解析）；Cargo JSON 制品输出由运行器直接接 stdout，
不经 shell 重定向。`{run}` 与 `{user}` 仅为清单内固定占位符，分别替换本轮私有目录
与实际 UID 对应用户名。以 Root 直接运行时仅省略 `sudo -n`，不改原夹具流程。
生产兼容阶段只从 `git archive --format=tar HEAD` 计算固定来源摘要，传给原构建所需的
`RUSTSIMHUB_BUILD_SOURCE_SHA256`；报告记录摘要和计算命令，不提供自定义环境入口。
该阶段的二进制只用于已有有限合成 CLI，不启动业务服务。

## 结果与来源

每阶段记录完整 HEAD、dirty/受测代码变更标志、变更路径与内容摘要、argv、cwd、
起止 UTC、退出码、执行状态、日志摘要和测试计数。开始前/阶段前后核验 Git 来源；
来源中途变化使阶段失败，后续项未运行。脏来源可诊断，但顶层不能 `passed`。
未跟踪源码也算变更，`.local`/`.private` 和其他 Git 忽略项不作为源码身份；
因此必须遵守仓库规则，不把受测代码藏在忽略目录。没有自动 Git 提交、重试或合并。

顶层同时显示 `passed / failed / not_run` 阶段数（不是测试总数）。必需检查缺失、
依赖失败或夹具跳过均不能整体通过。运行退出码 0 仅代表所选组完整通过；1 为组失败
或未运行，2 为参数/输出/运行器错误。不同组可以重复 `--group`，选择顺序始终为清单
顺序。中间报告也将尚未开始的已选阶段计入 `not_run`，不会提前报整组通过。不选择某组不等于它已通过；跨组完整结论应同时选择或汇总所有所需组。

每个实际子进程用独立 session/process group、关闭 stdin、无 shell 启动。Linux
subreaper 收养孤儿后代，`/proc` 父子关系和启动时间配合 pidfd 追踪本阶段存活后代，
包含原夹具自行创建的新 session。pidfd 防止误发信号给复用 PID；不收养本轮开始前
已存在的子进程。超时或输出超限先 TERM（0.5 秒）再 KILL
（最多 2 秒），等待原进程并逐 PID 回收后代；父进程提前退出但遗留子进程也判失败并清理。
回收不完整时停止后续执行，不重启同一项。每阶段合并日志上限 2 MiB、摘要尾部
16 KiB；超限即失败，不会默默丢弃后继续报绿。仅管理本轮启动的进程树，不清他人缓存。
这不是恶意程序沙箱；被审阅夹具不得访问真实设备或外部网络。Python/内核需要 Linux pidfd 支持。
Root 组须以 EUID 0 启动整个运行器（可由操作者使用既有 sudo），以保证对 Root 后代也能有界终止；
仅检测到 sudo 可用不推定普通 UID 有权清理 Root 子进程。本环境无 sudo，保持未运行。

CLI 不接受任意命令、shell、清单文件或环境赋值。正常继承预先准备的工具链/代理环境，
不打印全环境或读凭据。不提供 NAS、SIM、GATT、NKN SDK 网络、真实浏览器、ARM 运行、
签名部署、关机、刷写等入口。原签名夹具只生成隔离合成密钥/材料，不使用生产密钥。
不重造已有夹具，不改变 Root 规则、租约、权限或资源限制；原夹具断言失败原样保留。
