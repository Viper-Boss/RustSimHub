# 热点内重扫：受理拒绝与实时能力验证

2026-10-29，主线热点配网范围；D任务仍暂停，其他成员接口/归属不变。草稿[PR #69](https://github.com/Viper-Boss/RustSimHub/pull/69)，最新固定实现`d4283ab2353e4261874730e1e50d53079a2a37b2`。

## 修复的问题与最终行为

此前保护请求因冷却、预算或试连冲突未被消费时，没有明确结果，Core只能超时猜测；内存中的“已见编号”也无法作为持久拒绝证据。本批用独立持久决定区分未执行和执行不明。

| 模块 | 已实现的动作 |
|---|---|
| 协议 | 新增有限`Refused`，必须有`cooldown/limit/conflict/stale/window_closing/superseded/cancelled/capacity`之一。无快照，不声称AP/HTTPS已恢复，必须保留原完成时刻。`Unknown`仍专指缺乏执行/恢复证明，不拿Refused掩盖已消费的操作。 |
| Root持久拒绝 | 单个不可覆盖的`.rescan.refusal`同时记录完整intent和原因，在发布响应之前原子落盘。不会写硬件intent或active标记，不占8次无线操作预算。同一编号被拒绝后永远只读该回执，取消后再提交同一Scan也不能执行。 |
| 有界存储 | 每父轮最多8次实际扫描、30秒冷却、32条拒绝；总目录512条保护和父关闭证明空间预留。无法持久写拒绝时保留父Unknown/隔离，不把写失败当成许可。已消费但缺终态的旧操作会阻止新受理，不能被一个新拒绝遮掉。 |
| 实时能力 | 独立保护Capability含完整父Binding、新鲜采样、剩余无线次数、阻挡原因和原时域的冷却结束点。投影当前试连/保存冲突、窗口不足及存储上限；查询不会推进冷却时间。旧Root缺槽/字段时，后续Core须按不支持处理。 |
| 执行器 | 结构有效但过时/临近截止的请求收到明确未执行回执。请求在试连/保存期间到达时，结束后不会自动扫描，而是返回superseded；首次看到就有试连冲突的请求持久拒绝，不再使用临时内存队列。 |
| 丢失回执 | 无论成功、Unknown或Refused，恢复查询仅重发持久结果，不调用网络会话。拒绝回执发布失败仍保留本地决定；原父回合保持Unknown，不自动撤隔离。 |

Refused只证明这次重扫未执行，不证明当前热点可达、试连成功或Wi-Fi已保存。Ready/Failed中的HTTPS证据仍绑定原intent/boot，历史查询的新采样时间不是新恢复证明。

## 实际验证与来源

现有WSL隔离沙箱：普通UID65534、源码/工具链只读、无网络/设备、空capabilities；系统D-Bus地址强制指向不存在路径，NM模型仅使用测试自己的Unix socket。无设备命令或远程测试。

```text
cargo test -p rustsimhub-setup-ap -p rustsimhub-core -p rustsimhub-edge --all-features --all-targets --locked --offline -j3
cargo clippy -p rustsimhub-setup-ap -p rustsimhub-core -p rustsimhub-edge --all-features --all-targets --locked --offline -j3 -- -D warnings
cargo fmt --all -- --check
```

首轮366项通过；补充拒绝响应丢失后的只读查询/同ID再提交反例后，最终**367项通过、0忽略**，Clippy/fmt通过（含原试连→确认保存回归）。新增10项覆盖有限拒绝协议及字段组合、实时能力时钟/剩余次数、单文件拒绝/重启/不可变更、已消费Unknown不能改成拒绝、冷却原时间、拒绝数量/链接限制、实际Root循环的过时/取消/临近截止/冷却/次数耗尽、试连期间到达的扫描和拒绝发布失败。

最终测试归档SHA-256：`d249441e86848d7cf8be736d5e6e86a21d66e9f3579d7c1adcb35e24804603fb`；日志SHA-256：`fbdfb27b7decd69ba065d6428d919f8f1b78c9d473af770f29ebada25f8e225f`。560个代码/支持文件逐Git blob比对与固定实现一致。原始日志、归档与证明仅在`.private/`，不发布到公共看板。

## 下一步Core接口（已定边界，尚未实现）

1. `SetupApScanBegin/Status/Cancel`输入只有现有token、父`operation_id`和客户端固化的`scan_id`；在wire严格UUID/字段校验，在authenticated路由和`setup_ap_control::authorized`接入。不能接受接口、SSID、密码、命令、Root Binding或自定截止。
2. Begin/Cancel复用真实Owner近期登录与NetworkManage，前后复核身份代次、realm、设备公钥和实例。Status仍需原Owner及原设备域，但不以查询延长近期登录。公共匿名首次认领路由不能扫描；先初始化Owner，再接管原AP，保持Binding/父截止/网络独占许可。
3. 新`setup_ap_control/rescan.rs`持久化完整Owner/父intent/提交阶段，不存token或密码；先写不可覆盖intent与pending，再发布一次Scan。Core重启、丢回复、退出/撤权只发同一Cancel/Query，不重发Scan或另造编号。
4. 在现有Session网络独占许可及锁内，与station pending、commit pending、活跃通话和其他测试互斥。新扫描必须看到精确父Ready及新鲜独立Capability；旧Root不支持时在提交前拒绝。正在重扫时，试连/保存也须检查同一pending，不能只做单向互斥。
5. `Session`增加重扫状态，覆盖Runtime恢复、首次Owner接管、tick、父关闭、取消和shutdown。已消费未知请求保留pending和协调许可；不能凭查询超时、slot暂时消失或父状态Starting推断未执行，也不能把进度查询实现为新Start。
6. Status只接受完整intent/Owner代次/原父轮次及当前采样；原boot终态和当前采样时域分开。`Refused`投影`state=refused`、`authoritative_result=true`、`execution_started=false`、有限原因；`Ready`才允许更新新快照，空结果保持空。历史记录不声称当前AP已恢复。
7. 配合Root终态清理当前槽，保留不可覆盖Core回执；查询/归档未知不能偷偷清pending或重复发送。涉及旧编号、错误Owner/nonce、跨设备、并发、撤权、重新登录、丢回复和重启的失败路径均须真实Auth/生产文件测试。
8. Core与Edge契约接通后，网页只呈现“一次重新扫描→重连原热点→自动查询同一编号”；明确冷却/冲突/失败，不让用户再找手动调试按钮。最终仍须真实手机验收。

## 设备、范围与进度

本批没有新ARM候选、网页交付、签名、上传、安装或设备只读检查。应用70仍固定04360b9，仅含开热点前扫描；设备仍是先前确认的69，本批不能声称重新核验了7服务或短信数量。用户目前不方便手机测试，不能自动开热点或更换身份/网络。

原P0–P7仍81.5%，行动台账20/169、加权10.6%；AP-07继续进行中。Root这一层的源码验证不代替完整配网、设备交付或手机体验。
