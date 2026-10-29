# 热点内重扫：Core生命周期与首配HTTP通道验证

2026-10-29。主线负责人范围；D任务继续暂停，其他成员分工/归属保持。草稿[PR #69](https://github.com/Viper-Boss/RustSimHub/pull/69)，固定实现`1909a0af497a8bba57b4294b2a66db93f4d7466c`。

## 现在实现到了哪里

Root的有界暂停/扫描/精确恢复与持久回执保持。本批接上Core认证、单次派发、进度/取消/重启收尾，以及首配HTTP入口的有限转发。**没有新网页按钮或自动轮询，没有新ARM构建、签名、上传、安装、设备操作或手机验收。** 应用70候选仍固定04360b9，仅含开热点前扫描；设备仍是先前确认的69，本批没有重新读取设备健康或短信数量。

| 模块 | 已实现的动作与边界 |
|---|---|
| Wire/Auth路由 | `setup_ap_scan_begin/status/cancel`只接受token、规范父UUID与scan UUID，拒绝接口、SSID、deadline和Root Binding注入。Core异步认证路由接入；普通同步Auth分派明确拒绝，未开启本地配网模式时三个命令也拒绝。 |
| Begin | 真实Owner、最近登录与NetworkManage；通话空闲检查前后复核身份/realm/设备公钥/实例。Session必须持有原网络独占许可；要求精确新鲜Root父Ready/sequence与独立Capability。在派发前短持Auth锁再次核对，不能用旧权限签入新操作。 |
| 持久阶段 | 不可覆盖intent → 内存子状态 → pending → 必要的旧试连Cancel → dispatch标记 → 只发布一次Scan。重复Begin只查询同一编号，Core重启不重发Scan；记录不保存密码或bearer。Root终态快照可能包含附近SSID，属于Owner保护数据，不发布日志/备份。 |
| 双向互斥 | 活跃重扫/pending阻挡试连与保存；试连或commit pending阻挡新扫描。结束已成功但未保存的Ready试连时，必须保存原trial Cancel标记并发布精确Cancel，不能只清`finished`状态；这发生在扫描intent持久之后。能力投影含`will_end_unsaved_trial`，供一键确认明确说明。 |
| 查询与取消 | Status限定原Owner/代次和原设备域，只读取/同步原结果；必要时查询Root或清理已超时原操作，不产生新Scan。Cancel要求近期Owner，绑定原完整intent，不延长截止。Scanning心跳不清Scan/Cancel槽，避免Root误认为请求消失。历史查询不抢占另一活跃扫描的结果槽。 |
| 终态/Unknown | 只接收完整intent与新鲜采样、正确boot的Root状态，终态先持久再清当前槽。Ready/Failed/Refused才能清pending；Unknown保持pending和协调许可，不以查询、旧Ready或父Closed概括证明清理。 |
| Root未受理收尾 | 若原父有精确持久Closed且该扫描从未被Root消费，Root可持久返回`Refused/parent_closed`。没有父证明、错nonce或已消费Unknown不能转为未执行。跨boot只更新采样，不伪造原时域完成时间。Core必须等此子回执才能释放未确认扫描所占许可。 |
| Core重启/关闭 | 恢复原intent后仅发送原Cancel/Query，父回合保持取消/未知，不重建登录、不延长父窗口；若子取消槽损坏，仍尝试发布父Cancel。对旧版本留下的无父pending孤立记录，只在已有精确父Closed和权威子终态时清理，否则拒绝启用而非猜测。 |
| 短暂Busy | 派发后Auth/就绪检查短暂忙碌时，Root只读就绪重查单次最多2秒，仍受原请求截止约束；身份拒绝不重试、无线操作不重试。消费前再次检查原请求时效和当前槽，已取消则持久拒绝，绝不先暂停热点。 |
| HTTP入口 | 首配captive白名单仅增加三个有限命令，保留精确Host/Origin、JSON标记、请求大小/并发/速率边界。Core仍逐次鉴权；白名单通过不等于允许匿名执行。 |

## 返回给网页的语义

- `submitting/scanning/ready/failed/cancelled/unknown/refused`区分尚未获Root消费确认和实际Root回执；`not_found_is_not_proof`不允许客户端把缺记录当成可以重拨/重扫。
- `authoritative_result=true`仅用于Ready/Failed/Refused；Refused的`execution_started=false`，Ready为true，其余不能证明时为null。Refused不声称AP恢复或Wi-Fi保存。
- `ap_return_verified`只对仍活跃原父轮次的Ready/Failed成立；历史查询不会把旧HTTPS证明说成当前热点可达。所有回执`settings_saved=false`，扫描成功不能被解释为保存成功。
- 只有权威Ready返回新快照，空结果保持空；公共回复不含完整Root Binding、nonce、Owner摘要、token或Wi-Fi密码。
- `scan`能力含支持、当前可用、剩余次数、冷却、Root/本地冲突及结束未保存试连的提示。旧Root没有槽或字段、错误父轮次/过期采样按不支持处理。

## 实际验证

现有WSL普通UID65534沙箱：源码/工具链只读，无网络/设备、空capabilities；系统D-Bus强制指向不存在路径，仅测试自己的Unix socket模型。

```text
cargo test -p rustsimhub-setup-ap -p rustsimhub-core -p rustsimhub-edge -p rustsimhub-wire --all-features --all-targets --locked --offline -j3
cargo clippy -p rustsimhub-setup-ap -p rustsimhub-core -p rustsimhub-edge -p rustsimhub-wire --all-features --all-targets --locked --offline -j3 -- -D warnings
cargo fmt --all -- --check
```

最终**409项Rust通过、0忽略**，严格Clippy/fmt通过。相对367项阶段，新增23项（Core16、Root5、Wire1、Edge1），另纳入19项既有Wire测试。复用原试连/保存回归，未放宽原验收。测试覆盖：真实签名Owner与授权客户端拒绝、空成功/明确拒绝、篡改/未来证明、取消心跳、Unknown与父关闭、重启只读恢复、派发失败、Ready试连精确结束、commit/试连互斥、最大转义快照、身份变更、首次Owner原Binding/截止接管、Root跨boot未受理证明、Busy有界/身份拒绝不重试、消费前取消，以及真实captive Router转发/Origin/字段约束。

HTTP路由模型没有安装Core，合法请求返回503仅证明进入转发器、不是404；Core正向用例使用真实Auth与生产保护文件，Root状态显式模拟。Root执行器/网络后端另有模型验证；**这不是Core与Root两个真实服务进程、真实无线驱动和手机的联合验收**。

首轮编译暴露取消路径异步锁作用域、测试可见性和同步Auth拒绝分支遗漏，修复后再测；第二轮测试错误地重复打开已有数据库，改用原Store句柄模拟认证实例更换。各轮失败保留，随后405、408及最终含HTTP路由的409项通过，未将观察超时当作重启测试的理由。

最终归档SHA-256：`9df2a56627bef4f18160cb5842b4be47b4fa21678078de2438b6a29712b1c8f1`；日志SHA-256：`5c5254f9f2a65582874b1d96139207e3bec1c62aa3ece3999f313409882eea5c`。562个代码及支持文件逐Git blob与固定实现一致；原始输出仅存`.private/`。

## 下一步与仍未验收的边界

1. 网页实现一次确认：说明结束未保存试连及短暂断热点，固化设备域/父ID/scan_id后只发一次Begin；不保存token或密码到本地草稿。
2. 同一热点重连后自动只读查询原编号，恢复页/并发页/退出与取消都不得再发Begin；明确冷却、拒绝和Unknown。只有权威Ready更新列表，真实空结果清旧SSID。
3. UI能力缺失和忙碌时给出简短原因；确认保存/试连/测试按钮与扫描互斥。复制结果不泄露凭据或完整Root日志。
4. 新网页需DOM回归与实际浏览器交互，再冻结完整来源、ARM构建/验签和受控安装。现有70验证不能借给新源码。
5. 等用户方便另开测试窗口，验收刷新附近Wi-Fi、错密码回热点、正确试连/保存并关闭热点、恢复正常管理入口。本轮不要求用户操作。

Root真实已消费Unknown仍不会自动变成成功，需按后续受控恢复流程处理。Core/HTTP源码交付不代表一键页面或完整首配已完成。原P0–P7约81.5%，行动台账20/169、加权10.6%保持，AP-07仍进行中。
