# A29：SMTP / queue4 / worker5 / Core 七阶段候选交付

2026-11-03，wnfhsj88。用户已明确转交这些依赖的隔离代码实现；只提 PR 由
Viper-Boss 审核，不自行合并、部署、迁移生产库或发送真实邮件。

实现提交：`f8215c5cd347a1ebac1944748517d422e34c76e6`。正常合入最新 main
`e59dd8df524a7be8bbeed4a51fd49c15891de1a7`，合并提交
`0f2c7bb554fac323dfb3b19e07ab8c0530efe46e`，保留主线所有署名/改动。
随后补齐子进程 UID/能力/NNP 和精确 claim CAS；冻结代码以
`1deb309fc3bdb62095287115a6a9ca7457b50686` 及下方最终证据为准。PR178/184 原交付分支保持冻结；本包
接续 PR184，避免重复已合并 PR。总体 81.5%、20/169 不变，新增实现不等于验收。

具体类型、接口、原权限链、迁移/回退与限制见
[产品候选说明](../email/smtp-production-candidate.md)。不新增外部包，不升级锁文件；
Cargo.lock 只新增 update 对本仓库 policy 的依赖记录。默认程序保留原格式 3/4，
SMTP/维护迁移全部特性默认关闭。独立 SMTP 库/EHLO 修复直接复用。

## 验证命令

Rust 1.98，锁定依赖、离线、CARGO_INCREMENTAL=0：

```sh
cargo test --offline --locked -p rustsimhub-forwarder -p rustsimhub-core -p rustsimhub-update -p rustsimhub-core-launcher --all-features
cargo test --offline --locked -p rustsimhub-forwarder
cargo test --offline --locked -p rustsimhub-core --bin rustsimhub-core
cargo clippy --offline --locked -p rustsimhub-forwarder -p rustsimhub-update -p rustsimhub-core-launcher -p rustsimhub-core --all-targets --all-features -- -D warnings
cargo clippy --offline --locked -p rustsimhub-forwarder --lib --bin rustsimhub-forwarder -- -D warnings
cargo fmt --all -- --check
node tools/test-web-forwarding.mjs
node tools/test-web-forwarding-history.mjs
node tools/test-web-setup.mjs
```

冻结代码的最终结果：

| 配置 | 实际结果 |
|---|---|
| 四包全部特性：Core lib/bin、forwarder、update、core-launcher | 500通过/0失败/8普通 suite 专项忽略 |
| 默认 forwarder | 116通过/0失败/1专项忽略 |
| 默认 Core binary | 195通过/0失败/0忽略 |
| 严格 Clippy 全目标全部特性、默认 forwarder、fmt/diff | 通过 |
| 原页面/邮件候选、历史、main 新 setup 存储守卫 | 三套 Node 脚本通过，合成回复/DOM |
| 真实默认3写入器→UID986候选4子进程 | 9场景通过，所有 child 回收 |
| UID65534、2MiB 私有 tmpfs 的 outbox4 ENOSPC | 另手动执行1项通过，Unknown/游标保持、恢复一次 |

数字按配置分别报告，不把重叠旧测试相加为新增数量。8个 suite 忽略中 ENOSPC
已另执行，其余不计本包通过。详见 [公开验证清单](../email/smtp-production-candidate-validation.json)，
含源代码 HEAD、命令日志摘要哈希、程序/测试 ELF 哈希和真实夹具边界。候选子进程
制品没有发行 source_archive 元数据或签名，不能充当正式 selected 程序；生产迁移
签名门禁会拒绝该测试制品。未执行真实 systemd 维护迁移。

迁移夹具：`tools/test-email4-migration.py`。先用实际默认 Queue 构建
`outbox3_fixture`（dev-fixtures 且无 SMTP 特性），再以 SMTP 特性构建真正
forwarder 子入口；两份输入复制到只读挂载。使用固定 Python 镜像
`python@sha256:2d44738ebf4ce5f63b9d9ea871e9c7cfdc0426f7b0c97cac915fd88ded1d86bb`
和 `--network=none` 的一次性容器，由容器内 Root 父进程编排 UID986 子进程。
没有 host network、宿主数据/服务变更或外部 SMTP。

场景包括提交、封存 memfd exec、拒绝提交、Root EOF、源 SHA 错误、writer 锁占用、
非 Root 父进程、原许可已过期、提交确认迟到。成功后旧默认程序拒绝格式4；失败时
原格式3数据库逐字节保持。所有子进程均回收，无日志正文或地址回显。

## 保留的门禁

本包提供代码/隔离验证，没有宣称实际设备迁移、ARM执行、经授权真实邮箱接收或
系统管理器的整条新维护链验收。8项普通 suite 忽略分别属于 Core UID/Root 专项、
forwarder 有界 tmpfs ENOSPC、update Root/受限 tmpfs 专项，按各独立脚本执行情况
另记，不冒称本 suite 通过。构建曾因缓存耗尽磁盘未运行，清理可重建缓存后重跑；
首次失败日志保留，不混记为业务测试失败或已通过。

A22 的公共 Root/SDK 只读 provider、BlueZ 生命周期和 WASM/CSP/PWA 制品装配、
A30 含实际 SDK/会话/Root 的完整组合继续推进。整体九步未全部验收；主线签名、
组装、硬件、真实外发和最终确认保持原职责与授权边界。
