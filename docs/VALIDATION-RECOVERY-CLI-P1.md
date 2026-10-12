# P1 所有者恢复、CLI 与生命周期验证

日期：2026-10-12。源码检查点：`7f69dd8ac39c09df92bf97d134b2bb989540a83c`。
本轮全部为独立合成数据，没有给实际业务设置 owner、发送短信、拨号、开启蜂窝数据或改写分区。

## 已通过

| 环境 | 检查 | 结果 |
|---|---|---|
| GitHub Linux / Rust 1.98 | fmt、59 项 all-features 测试、Clippy -D warnings | 全部通过 |
| GitHub Linux / Rust 1.98 | core 默认 feature 检查 | 通过 |
| NAS ARM64 / Rust 1.99 | workspace --no-default-features 检查、59 项测试、release、自检、CLI --help | 全部通过，容器退出 0 |
| 棒子 nassimhub 用户 | 独立身份私有备份与 load、旧 DB 备份恢复及授权隔离 | 通过 |
| 棒子真实离线 CLI | 原 owner 签名证明恢复，simhubctl 验证设备 pin | 通过 |
| 棒子真实 UDS / CLI | owner 登录/查询、待批列表、旧客户端隔离、精确重新批准、NAS owner 拒绝、撤销 | 通过 |
| 棒子真实进程 | SIGTERM 退出、同路径重启、旧会话失效 | 通过，socket 清理、writer 可重开 |
| 棒子真实进程 | SIGKILL 退出、遗留 socket 安全接管、旧会话失效 | 通过 |
| 私有权限 | seed、备份、DB/WAL/SHM、会话、fixture/协调文件 | 均 0600；秘密材料属 nassimhub |

CI：[38265061208](https://github.com/Viper-Boss/RustSimHub/actions/runs/38265061208)。
59 项包含 auth 19、core 8、ctl 1、domain 7、policy 1、storage 19、wire 4，
其中一个为子进程故障注入的辅助入口。NAS 精简镜像没有 Clippy；Clippy 在 CI 执行。
现有 GitHub 授权没有 workflow scope，扩展 workspace 默认检查放入 NAS 脚本，保留已有 CI 文件。

## 失败路径与边界

- 假 owner、client/nas 恢复请求、错误签名、重放、错 purpose、失效 generation 拒绝。
- recovery 提交注入失败时 owner 仍 inactive；不会留下部分有效授权。
- 只有原 owner 恢复；客户必须重新证明并被精确批准。旧会话不会随重新批准复活。
- 重新批准前的 generation 改变拒绝；批准的公钥、角色和权限必须与请求一致。
- client 证明前核实设备签名、domain、双方公钥、nonce/instance、purpose、scope；错误 pin 不签名、不创建会话文件。
- 客户端 key load 缺失不生成替代，device/client seed 跨角色拒绝，私有会话不覆盖或跟随链接。
- 已绑定 DB 丢失设备 seed 时不生成新身份；独立身份备份拒绝覆盖并可 load 验证。
- live listener/运行锁冲突拒绝，不抢占旧版 listener；stale owned socket 可接管，普通文件/替换 inode/链接/宽松父目录保留拒绝。
- 真实测试反复启动同一个 socket 路径，SIGTERM 清理，SIGKILL 后确认旧文件仍在、重启成功、旧 token 拒绝。
- 请求上限 8，关闭信号优先，异步请求排空后关闭唯一 DB writer。

早期 ARM64 编译修正了 rustix connect API 名称；早期烟测发现 stale 文件会误导“文件存在即就绪”，
已改为实际 IPC identity 握手。下列结果及日志对应修正后的完整退出 0 流程。
没有安装 Python/jq；烟测用私有 FIFO/JSONL 与真实 Rust CLI 协调。

## 构建与制品

- builder：`rust@sha256:2c3a22f0a5533ea2dd5a16627bc841228151faa2d4de2644ac9987e4a2f1f2fa`。
- ARM64 Rust 1.99.0 (b940084d7 2026-09-28)，Cargo 1.99.0。
- 允许列表源码包：35 个文件，tools/verify-build-source.py 与 Git 提交树字节一致。
- 源包 SHA-256：`7a81288f8f526163305568dfa19f4fd6324382219c3f75c327652a102c2237b7`。
- 本次增量构建：默认检查 3 min 51 s，test 编译 26.90 s，release 2 min 45 s。
- core：4,347,360 字节，约 4.15 MiB，SHA-256 `870b0f622c163c5a417109980eb5278796fa42a3230998f6bfc3121e68caf41d`。
- simhubctl：791,568 字节，约 0.75 MiB，SHA-256 `c1218a4a306ab05a6c54dde9b7c54784fc01aecc8e3310f671f46d8f5d8c8b03`。
- 两者均为 aarch64 PIE、glibc 动态链接、stripped。core 本次启用 dev-fixtures，正式默认构建关闭。
- 本机 `.local/artifacts/rustsimhub-core-recovery-p1` 与 `.local/artifacts/simhubctl-recovery-p1`。
- NAS `/mnt/docker-data/rustsimhub/p0/src/target/release/{rustsimhub-core,simhubctl}`。
- 棒子 `/opt/rustsimhub/dev-recovery-p1/{rustsimhub-core,simhubctl}`；三处哈希一致。

原始日志：.private/recovery-final-build.log、recovery-artifact-metadata.log、device-recovery-cli-smoke-final.log。
旧构建/测试资料保留，新测试目录均由 mktemp 独立创建，不读取原项目业务数据库。
临时 rustsimhub-recovery-p1.service 最终已停止，未安装开机服务。
原 nassimhub-agent、ModemManager、NetworkManager、rmtfs 均 active，Wi-Fi/USB Ethernet connected，gsm disconnected。
源码隐私指定模式扫描与 staged 扫描通过；不能代替通用 secret scanner 或依赖漏洞审计。

## 下一包

P1 还缺网页密码/TLS、最终用户设置 UI、生产开机部署、完整身份加密备份与系统恢复。
已有可信本机 CLI 可用于后续受限业务接口，但尚无真实短信/电话命令。
接下来推进 MM SIM/短信事件、SIM 观察期和对象对账，最终执行点校验权限与持久提交。
P0 完整分区备份/恢复入口、精确内核来源、断电和 I/O 卡死验证仍未闭环；不能刷 rootfs/boot。
