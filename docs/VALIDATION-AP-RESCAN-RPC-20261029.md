# 热点内重扫：Root持久通道初版验证

后续更新：本文件记录1e0496d的357项初版验证；下文“受理前拒绝/实时能力未完成”已在后续d4283ab补齐源码/隔离验证，见[367项受理边界验证](VALIDATION-AP-RESCAN-ADMISSION-20261029.md)。Core/网页、ARM/设备交付仍待，初版结果不代表这些完成。

日期：2026-10-29。草稿[PR #69](https://github.com/Viper-Boss/RustSimHub/pull/69)，基于PR #65；D任务继续暂停，其他成员分工不变。

## 本批交付边界

固定实现`1e0496d1ad4f91bd08ea422f77c8f80f11c17503`。仅源码与隔离模型验证，没有新ARM构建、上传、暂存、安装或设备操作。现有70候选仍固定04360b9，只含开AP前扫描；设备仍是先前确认的69。本批没有重新读取设备，不能把此前的7服务/短信数量当作本批最新观测。

| 模块 | 实现与接口边界 |
|---|---|
| `channel/rescan.rs` | 固定Request/Intent/Status/Capability。完整父Binding、scan_id、Owner摘要/代次、请求/截止绑定；Scan/Cancel有限动作，没有接口名、SSID、密码、命令或路径输入。请求至多40秒，父窗口保留至少20秒；执行仍要求父剩余60秒且原心跳有效。 |
| `channel/files/rescan.rs` | 不可覆盖消费intent先于active标记和无线操作；终态先于输出。每父轮最多8次、至少30秒间隔，目录512条有界。缺active但已有intent仍阻止父清理；Unknown不可覆盖、不自动解除隔离或重新消费。固定槽复用原元数据/链接/大小约束。 |
| `executor/configuration/rescan.rs` | 只消费保护通道；Root不代替Core的Owner鉴权。执行期间轮询原租约、停止信号与同一请求，取消仍等待精确恢复。随后验证真实HTTPS、隔离、租约，才写Ready或已恢复的Failed；不明结果保留父Unknown。 |
| 快照发布 | 仅向当前完整Binding、未过期且仍pending的父轮次写快照；空扫描覆盖为真实空结果。旧成功操作的查询不能覆盖已更新的快照，跨父轮/跨启动只返回历史回执。 |
| 重启查询 | 固定只读查询槽接受已消费intent；无终态则落盘Unknown，不创建网络会话。查询只刷新采样时间，保留原完成时间和原boot时域，不延长父租约。历史Ready的HTTPS证据只属于其原intent/boot，不能解释成当前网页可用。 |
| 调度互斥 | 试连、保存、重扫串行；冲突请求不因条件后来改变而自动排队执行。区分扫描完成对试连/保存的时间屏障，与其他工作完成对扫描的屏障；旧保存回执查询使用原完成时间，避免反复推进屏障。 |

## 明确未完成

1. Root受理前拒绝（冷却、预算、冲突等）还没有独立、明确的权威回执；能力槽也未投影全部实时条件。需要补齐，避免Core只能超时等待。不能直接把当前初版当完整可用RPC部署。
2. Core的Owner近期登录/NetworkManage/代次验证、协调锁、持久intent与唯一Begin/Status/Cancel尚未接入。Root保护文件不是公共认证接口。
3. 网页一键重新扫描、断线后同一ID自动查询、能力缺失提示及复制结果尚未接入；现有“读取列表”没有升级成重扫按钮。
4. ARM候选、签名、受控安装和手机全流程仍未做。手机必须验收：附近列表、错误密码返回热点、正确密码试连/保存、退出热点和管理入口。此前用户不方便，本轮没有开启测试窗口。

## 实际验证

在现有WSL普通UID65534沙箱运行，源码/工具链只读、无网络/设备、空capabilities；系统D-Bus变量强制指向不存在路径，底层NM测试仅连接自己的Unix socket模型：

```text
cargo test -p rustsimhub-setup-ap -p rustsimhub-core -p rustsimhub-edge --all-features --all-targets --locked --offline -j3
cargo clippy -p rustsimhub-setup-ap -p rustsimhub-core -p rustsimhub-edge --all-features --all-targets --locked --offline -j3 -- -D warnings
cargo fmt --all -- --check
```

最终357项Rust通过、0忽略，Clippy/fmt通过。17项新增测试包括：完整请求/精确取消、跨boot采样/原完成时间、32条转义SSID大小界限、严格能力、重启/不可重消费、缺active/Unknown保留、30秒/8次/文件数/链接限制、真实Root调度正常空扫描、恢复/HTTPS/授权失败、停止/取消、输出丢失、只读查询不覆盖新快照、冲突后不自动扫描。模型不能证明真实无线驱动和手机表现。

冻结测试归档SHA-256：`312753628eefc31907c159bebbe198f06dec369d117b280945ad63b8ffc1f60f`；最终日志SHA-256：`877857249204550ed17fb4208948ebcb7ad59b8d232348830e26422efd5ddd49`。560个代码及支持文件逐Git blob与固定实现比对一致；完整输出及首两轮失败证据只在`.private/`。

首轮因模块可见性/测试借用错误编译失败；次轮4项原确认保存测试失败，原因是把试连完成屏障错误套给了预先准备的保存请求。保留失败，修正生产调度边界后全部通过，未放宽原保存测试。没有把早先70候选的网页/ARM/音频/验签结果套用到这批代码。

整体原P0–P7约81.5%，本轮行动台账20/169、加权10.6%保持。AP-07未完成。下一步先补受理拒绝/实时能力，再接Core与一键网页；实际部署和手机验收另开受控窗口。
