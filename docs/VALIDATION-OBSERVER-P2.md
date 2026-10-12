# P2 事件订阅与对象核对验证

日期：2026-10-13。最终 Rust 源码：`31be30a805be626b913b099b3f76823378d518bf`。
本轮接收链路保持只读，没有发送短信、拨号、解锁、启用蜂窝数据、重启旧 MM/基带服务或改写分区。

## 结果

| 环境 | 验证 | 结果 |
| --- | --- | --- |
| GitHub / Rust 1.98 | fmt、73 项测试、core 默认 feature、Clippy -D warnings | 全部通过 |
| NAS ARM64 / Rust 1.99 | workspace 默认 feature、73 项测试、release、自检、CLI help | 构建容器退出 0 |
| 私有 peer D-Bus | 40 个对象含 1 个超限，其余 39 条分批入库 | 通过；坏对象不阻断其他消息 |
| 私有 peer D-Bus | Deleted/Added 同路径、SIM 属性失效、peer 连接断开 | 通过；新 incarnation、旧历史保留、重新隔离、当前连接退出 |
| 单元/故障测试 | 513 路径公平推进、4096 限制、1000 个提示突发、在途取消、信号任务关闭 | 通过 |
| SQLite 单写者 | 任务排队后门禁失效，执行前重新检查 | 通过；拒绝旧状态/正文写入 |
| 棒子 nassimhub 非 root | 固定总线 owner 及 MM 信号订阅、稳定 SIM、周期修复 | subscriptions=true，核对计数增加 |
| 棒子独立私有目录 | 已有入站短信摘要/详情/事件读取及周期后保留 | 持久化 1 条；object_read_errors/storage_errors 均 0 |
| 棒子鉴权 | 只读状态用户不能读短信，reader 不能读状态，撤销后旧会话读正文/摘要拒绝 | 通过 |
| 棒子退出/权限 | SIGTERM、socket 清理、数据库重开、启动 ready 重置、DB/seed/session 0600 | 通过 |
| 旧服务/网络 | nassimhub-agent、MM、NM、rmtfs 继续 active | Wi-Fi/ethernet connected；gsm disconnected |

CI：[38316851912](https://github.com/Viper-Boss/RustSimHub/actions/runs/38316851912)。
73 项为 auth 21、core 16、ctl 1、domain 7、policy 1、storage 23、wire 4，包含既有故障测试辅助入口。
NAS 最终增量默认检查 4.29 秒、测试编译 41.69 秒、release 3 分 23 秒；这些是构建耗时。

私有 peer 测试起初因模拟连接的唯一 destination 路由配置超时；读取所用 zbus 5.19 源码后修正测试端路由，生产匹配规则保持唯一 owner 限制。最终测试与 Clippy 全部通过。失败证据保留在 NAS 自有构建历史中。

## 实机资源样本

tools/device-observer-smoke.sh 在真实 SIM 及已有短信、完成一个 10 秒兜底核对后读取本项目进程 /proc 状态：VmRSS=6420 kB、VmHWM=6420 kB、Threads=3，约 6.27 MiB RSS。
这是独立开发进程的一次短时样本，不能证明负载峰值、长期稳定性或相对旧实现的性能改善。

## 产物与来源

42 个允许列表源文件与提交逐字节一致；本机、NAS、棒子二进制 SHA256 一致。core 是带 dev-fixtures 的开发产物；私有模拟 D-Bus 代码仅 cfg(test) 编译，正式 daemon 没有模拟入口。

| 产物 | 大小 | SHA256 |
| --- | --- | --- |
| source.tar.gz | 允许列表源码 | `94965e0a80a54b97a79bb33fd2c2a740e2f648d1e375a01c9ded62f37f268f16` |
| rustsimhub-core | 4,740,576 字节，约 4.52 MiB | `0138ec3116ae23d57979efc8f8ab872debff060c7ca26fb5f51a5857acc010ed` |
| simhubctl | 791,568 字节，约 0.75 MiB | `10941fc92805fb5150843564da3610f2f5e6ab464cef9927c18fadf744584651` |

镜像仍为 `rust@sha256:2c3a22f0a5533ea2dd5a16627bc841228151faa2d4de2644ac9987e4a2f1f2fa`，不挂载设备、host network 或 Docker socket。两个 ELF 均为 aarch64 PIE/glibc 动态链接、已 strip。

本机：`.local/artifacts/{rustsimhub-core-observer-p2,simhubctl-observer-p2}`；NAS：`p0/src/target/release/{rustsimhub-core,simhubctl}`；棒子：`/opt/rustsimhub/dev-observer-p2/{rustsimhub-core,simhubctl}`。

## 复现与接手

使用现有 tools/package-source.py、verify-build-source.py、nas-reset-stopped-build.sh、nas-start-build.sh 固定来源构建。容器退出 0 后运行 nas-artifact-metadata.sh，再下载和上传上述两个产物到棒子 `/var/tmp/*-observer-p2`，运行 tools/device-observer-smoke.sh。

该脚本要求已有完整入站 modem 消息，创建随机私有目录、随机测试身份和最长 150 秒的 rustsimhub-observer-p2 临时单元；正文只写私有 detail.json。周期后核对无错误及原记录仍在，再验证 scope/撤销、停止单元和重开数据库。最终单元已停止；异常也只停止该项目测试单元。

原始证据 `.private/nas-build-observer-final.log`、`observer-artifact-final.log`、`device-observer-final.log`；正文、会话和设备标识留在原机器私有目录，不进 GitHub。其他私有 fixture 及 NAS history 也保留。

尚未验收真实新消息到达/多段拼接、拔插换卡、PIN/PUK、MM/系统总线重启、4096 真实消息压力、故障风暴/满盘维护和长稳性能。历史消息的实际 SIM 归属及观察间隙内等内容复用仍可能有歧义。详见 observer-events.md；下一步推进发送最终门禁、提交状态与 modem 对账。完整刷机/恢复闭环和生产部署尚未完成。
