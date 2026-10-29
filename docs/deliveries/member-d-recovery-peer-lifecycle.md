# D-22 局部交付：超时/取消后的验证工作保留

2026-10-30。用户要求继续，核对 #92 已由负责人合并后，从最新 main 建立独立分支。

- 仓库/分支：Viper-Boss/RustSimHub / `codex/gtw-recovery-peer-lifecycle-20261030`，PR 到 main。
- 起始 main：`b7a5dfa8d0d8c762c2284164e9d4371fce3c373f`。
- 前置 #92 合并：`d4d1673c58049f54c4de94b3f3adee6e41379bb2`。
- 实现/受测代码及当前完整40位 HEAD：提交后在 PR 正文记录，后续报告提交只补证据。
- [实现与完整限制](../ble/recovery-verifier-lifecycle.md)。

## 修改和授权边界

仅 Core 的显式 recovery-transport 子模块及本人文档；不修改 Auth/Store/wire/A 密码库、
其他成员实现、全局进度或原有分支。默认 Core 不调用，仍无恢复监听器/验证器配置或
Wi-Fi 派发。无新依赖、锁文件、迁移或 Root 安装材料。

以线程拥有连接/Packet 的方式实现 verify_blocking：固定五秒截止，入口/线程开始/交付
均复核期限；失败为有限错误；超时/取消只停止等待，原资源一直保留到工作真的
结束，迟到输出丢弃。不自动重试、不伪造业务未执行或关闭完成。
独立线程不占 Tokio 的共享阻塞池；成功输出仍需要后续 Auth/grant/连接复核，不是能力。
永久卡住则该 Guard 一直占用；没有强杀、生产监督/cgroup 或可配置任意插件。
仅 unwind 构建的 panic 能映射 Worker；release 的 panic=abort 仍终止整个进程，
普通 panic hook 仍运行，不能记录秘密。本批没有更改 profile/hook 或声称线程是进程隔离。

## 实测

待最终代码完成后补充实际命令/结果；此段不作为已通过证据。

## 数量、未完成和交接

本批新增完整步骤0；累计仍 **21/26 离线包已提交，剩 D-22～26 共5步**。
D-22 增加实际工作保留机制，A 安全协议/互操作、真实跨 UID 准入、GATT 接线仍缺；
D-23 唯一 Wi-Fi 事务/持久 Unknown、D-24 安装许可、D-25 远程媒体和 D-26 联合验收未冒称完成。
原固定 P0–P7 约80%，主线19/169口径不变；不自行更新全局台账或验收。

单 UID user namespace 只读探测成功；尝试多 UID 映射实际因缺 newuidmap 退出127。
没有以不同命名空间内的相同底层 UID 冒充两个角色。没有安装助手/修改主机账户/NSS，
没有设备/D-Bus/NAS/生产数据或真实网络业务/短信/电话、签名或部署。

沿用 AGPL-3.0-only，复用标准库及现有 Tokio/Zeroizing，无第三方源码复制。
OpenAI Codex 辅助实现与实测，无工时虚报/作者改写。源日志仅保留本地 work，不提交生成物。
远程 CI 按最终 HEAD 单独回读，不能以本地通过代替 CI/审核/设备验收。
