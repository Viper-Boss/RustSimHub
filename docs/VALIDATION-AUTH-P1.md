# P1 身份、配对与鉴权验证

日期：2026-10-12。源码检查点：`5389eb8ff07475497b36febcf57309e29e5fa049`。
仍为开发原型，没有真实发送/拨号、开机替换服务或刷写系统分区。

## 实际结果

| 环境 | 检查 | 结果 |
|---|---|---|
| GitHub Linux / Rust 1.98 | fmt、47 项 all-features 测试、Clippy -D warnings | 全部通过 |
| GitHub Linux / Rust 1.98 | core --no-default-features --locked 检查 | 通过 |
| NAS ARM64 / Rust 1.99 | 47 项 all-features 测试、release、自检 | 全部通过，容器退出 0 |
| 棒子 nassimhub 用户 | 独立身份、签名配对、scope 拒绝、owner 隔离、重放和撤销 | 通过，合成流程样本 136 ms |
| 棒子独立进程 | 同一身份及 owner 重认证、撤销保留 | 通过 |
| 棒子真实 IPC v2 | 设备签名验证、owner 签名登录、health、未认证/重放拒绝 | 通过 |
| 棒子真实 UID 边界 | root 尝试访问仅允许 nassimhub 的 endpoint | 拒绝，CLI 非 0 退出 |
| 私有资料权限 | 身份 seed/锁、DB、WAL/SHM、fixture/输入文件 | 均 0600，业务资料属 nassimhub |

CI：[38188910592](https://github.com/Viper-Boss/RustSimHub/actions/runs/38188910592)。
47 项为 auth 13、core 4、domain 7、policy 1、storage 18、wire 4；含一个用于子进程故障注入的辅助入口。
NAS 官方精简镜像无 Clippy，不能把 CI Clippy 称为 ARM64 Clippy。
所有实机公钥/请求均为合成 fixture；未给用户实际设备建立正式 owner。

## 关键覆盖

- seed 重开不变；完整 pending 可恢复；损坏/部分 seed 保留拒绝，禁止静默换身份。
- 独立设备随机身份不同；身份与 DB 公钥不匹配拒绝；宽松权限、符号链接、硬链接拒绝。
- transcript 签名验证，双方公钥、权限、purpose 和随机进程 instance 绑定；篡改/弱公钥拒绝。
- 签名证明之前或 owner 批准之前不能登录；窗口关闭不能配对；owner kind 不允许通过配对提升。
- approve 中更改 scope 拒绝并消耗请求；未知/重复/空 scope 拒绝。
- NAS 即使获得 clients.manage 也不能成为 owner；未授权功能拒绝。
- 撤销对现有会话及进行中的认证立即生效；撤销保留至进程重启后。
- 单调期限、过期挑战/会话、owner 最近证明约束、挑战容量与限速失败路径。
- 重启清除短期状态；长期 owner 可再次证明，不能再次 bootstrap 抢占。
- 旧备份恢复隔离所有授权并保留唯一 owner，不能复活已撤销权限。
- schema v1 迁移保留原操作、事件 cursor；注入 owner 写失败不会留下或确认授权。
- 真实 UnixStream 覆盖完整 owner 登录、NAS 配对批准/登录、权限拒绝和撤销。
- 旧 IPC v1 不能在 v2 endpoint 静默降级，额外 role 字段不能被枚举空项忽略。

早期检查发现并修正严格枚举解析、测试签名变量和 Clippy 写法问题；以上为修正后的结果。
tools/audit-source.py --revision HEAD 的隐私模式扫描通过；只是指定模式检查，不能代替通用 secret scanner / 漏洞审计。

## 构建与产物

- builder：`rust@sha256:2c3a22f0a5533ea2dd5a16627bc841228151faa2d4de2644ac9987e4a2f1f2fa`
- ARM64 Rust：1.99.0 (b940084d7 2026-09-28)，Cargo：1.99.0。
- 源码允许列表包 SHA-256：`bf554f8232f7711747d4df93cb3617c7ffdafb0db90fdb82e2a6f9c4cc324d86`。
- tools/verify-build-source.py 对照提交树验证 28 个源文件完全一致。
- release：4,281,824 字节，约 4.08 MiB，aarch64 PIE、动态链接 glibc、stripped。
- 二进制 SHA-256：`760f25ff5d0a6d152ff3117c56fa3a5a304d237bc016042abc21c0feda1a1616`。
- 最终 ARM64 增量 test 编译 15.56 s，release 2 min 47 s；NAS 精简容器退出 0。
- 该产物显式启用 core/dev-fixtures，仅供开发；正式构建关闭，CI 已检查默认 feature。

本机：`.local/artifacts/rustsimhub-core-auth-p1`；NAS：
`/mnt/docker-data/rustsimhub/p0/src/target/release/rustsimhub-core`；棒子：
`/opt/rustsimhub/dev-auth-p1/rustsimhub-core`。三处哈希一致。
最后 Clippy match guard 修正后生成的二进制哈希与已实测产物相同，未重复同一字节的烟测。

原始证据在 .private/auth-final-build.log、auth-artifact-metadata-final.log、device-auth-smoke.log。
旧存储产物及日志由构建归档保留；新功能均在独立私有临时目录测试。
临时 rustsimhub-auth-p1.service 已停止，没有安装开机单元。
原 nassimhub-agent、ModemManager、NetworkManager、rmtfs 均 active，Wi-Fi/USB 以太网 connected，gsm disconnected。

## 实用限制与下一包

身份/配对基础通过不代表 M03 全部完成。缺少网页密码、TLS、客户端工具与带外设置引导、
恢复 owner 的可信离线工具、完整身份私有备份及生产 systemd 生命周期。
备份恢复会隔离 owner；恢复工具完成前不能把此路径称为可用的完整业务恢复。
scope 位图不代表对应电话/短信 API 已实现；真实 Send 必须在最终执行点重新授权并执行持久提交。
下一包推进恢复 owner 工具和受限业务 IPC，再接 MM SIM/短信事件与对象对账。
完整分区备份/恢复入口、精确内核来源、断电/磁盘故障和远程威胁验证仍未完成。
