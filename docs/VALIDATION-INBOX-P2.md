# P2 SIM 观察与收件箱验证

日期：2026-10-13。最终 Rust 源码检查点：`fdaf4c2efbdb7c3b194fda88fc60e131dc217c61`。
当前为只读接收基础；没有发送短信、拨号、解锁 SIM、启用蜂窝数据或改写系统分区。

## 已通过

| 环境 | 验证 | 结果 |
| --- | --- | --- |
| GitHub / Rust 1.98 | fmt、all-features 测试、core 默认 feature、Clippy -D warnings | 全部通过 |
| NAS ARM64 / Rust 1.99 | workspace 默认 feature、all-features 测试、release、自检、CLI help | 容器退出 0 |
| 棒子 / 非 root nassimhub | 固定 MM 接口、唯一 owner、SIM 观察期和 PIN2 operational 状态 | ready，未提交解锁码 |
| 棒子 / 独立随机身份和私有数据库 | 已有完整入站短信导入、摘要、详情和事件读取 | 持久化 1 条真实已有消息，内容仅保存到私有目录 |
| 棒子 / 鉴权 CLI | status_read 不能读短信；sms_read 不能读 line 状态 | 拒绝 |
| 棒子 / 撤销 | owner 撤销 reader 后旧会话读摘要/正文 | 拒绝 |
| 棒子 / SIGTERM | 停止单元、socket 清理、数据库重开 | 通过；startup readiness 重置 unavailable，消息保留 |
| 产物与权限 | 本机/NAS/棒子 SHA256；DB、seed、session 的私有模式 | 哈希一致，所检查文件 0600 |
| 旧业务与网络 | nassimhub-agent、MM、NM、rmtfs；Wi-Fi/USB/蜂窝状态 | 四服务 active；Wi-Fi/ethernet connected、gsm disconnected |

CI：[38298747817](https://github.com/Viper-Boss/RustSimHub/actions/runs/38298747817)。
66 项测试：auth 21、core 10、ctl 1、domain 7、policy 1、storage 22、wire 4；包含既有子进程故障测试辅助入口。
NAS 最终增量默认检查 2.20 秒、测试编译 22.89 秒、release 3 分 05 秒。这是构建耗时，不是系统性能指标。

新增关键测试覆盖单调观察期与丢失重置、PIN2/PUK2 与阻断锁区分、用途隔离指纹、权限撤销、旧 SIM 隔离、等正文消息保留、同源内容冲突、分页/恢复游标、超限输入，以及事务事件失败时正文与线路/outbox 全部回滚。

## 实机发现与修复

初次硬件烟测登录通过，但 SIM 被判 locked。只读确认 UnlockRequired=3 (PIN2)、State=8 (REGISTERED)，并对照 MM 源码修正为允许普通读取；增加回归测试。没有猜测/提交 PIN，没有修改 SIM 或 modem 配置。

初次 CI 发现一个 collapsible_match lint，已修复，最终 CI 全部通过。Git HTTPS 网络重置时使用本机已有 API 同步工具：比较创建树与审计树，校验提交对象、禁止强推。最终源码与 NAS 允许列表源包 39 个文件逐字节一致。

## 可复现入口

```powershell
python tools/package-source.py
python tools/verify-build-source.py
python tools/remote.py --config .private/nas.json --sudo --script tools/nas-reset-stopped-build.sh
python tools/remote.py --config .private/nas.json --upload .local/source.tar.gz --destination /mnt/docker-data/rustsimhub/p0/source.tar.gz
python tools/remote.py --config .private/nas.json --sudo --script tools/nas-start-build.sh
```

仅在构建容器退出 0 后读取产物。上传本轮产物到棒子的 `/var/tmp/rustsimhub-core-inbox-p2` 和 `/var/tmp/simhubctl-inbox-p2`，运行 `tools/device-inbox-smoke.sh`。该脚本需要 modem 中已有完整入站消息；没有消息时会拒绝宣称真实导入通过，不会为测试发送新短信。

脚本建立 `/var/tmp/rustsimhub-inbox-p2-*` 私有随机目录及最长 90 秒的 `rustsimhub-inbox-p2` 临时单元。读取正文重定向私有 detail.json；不打印正文、号码、ICCID 或 token。异常时只停止这个项目测试单元。测试身份和目录不能用作生产 owner 或业务数据库。

私有原始证据：`.private/nas-build-inbox-final.log`、`inbox-artifact-final.log`、`device-inbox-final.log`。初次失败日志另行保留，不抹除证据。短信/身份/会话资料留在原机器私有目录，不进入 GitHub。

## 产物

构建镜像：`rust@sha256:2c3a22f0a5533ea2dd5a16627bc841228151faa2d4de2644ac9987e4a2f1f2fa`；rustc/cargo 1.99.0。
两个产物均为 aarch64 PIE，glibc 动态链接、已 strip。core 含 dev-fixtures，仅用于本轮独立调试。

| 产物 | 大小 | SHA256 |
| --- | --- | --- |
| source.tar.gz | 允许列表源码包 | `02589ce7ceab3450ed729a3290d357ceef30626a0c6249a57b53270995a46103` |
| rustsimhub-core | 4,609,504 字节，约 4.40 MiB | `d3ff1e172548f6dd6e0a5840a93188540964f6acc228b9d99761c8e98bf0bd26` |
| simhubctl | 791,568 字节，约 0.75 MiB | `10941fc92805fb5150843564da3610f2f5e6ab464cef9927c18fadf744584651` |

本机 `.local/artifacts/{rustsimhub-core-inbox-p2,simhubctl-inbox-p2}`；NAS `p0/src/target/release/{rustsimhub-core,simhubctl}`；棒子 `/opt/rustsimhub/dev-inbox-p2/{rustsimhub-core,simhubctl}`。

## 未验收

本轮验证的是**读取并保留已有短信**，不是新短信到达、多段组装、实际拔插/换卡、PIN 锁或 MM 重启的硬件验收。轮询间隔、历史短信归属、同路径复用、超大短信、超过 256 个对象、逐对象故障隔离、信号订阅仍见 sim-inbox.md 的限制。没有做长稳/性能或完整漏洞审计，也没有生产开机部署和完整系统镜像；P0 完整备份/恢复闭环仍未完成。
