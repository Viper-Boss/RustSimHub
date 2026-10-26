# X1V · 显式历史补发事务（后端阶段）

后续网页、确认码与有限归档验证见[X1W](VALIDATION-FORWARDING-HISTORY-WEB-X1W.md)；以下保留本阶段证据与当时待办。

2026-10-27，主线FWD-12进行中。Core真实Owner接口和持久队列事务已接线；简洁网页、浏览器持久防重和生产ARM交付尚未完成。本记录不计FWD-12完成，不表示已部署或已向提供商推送。

## 当前实现

- `forwarding_history_prepare`：Owner最近五分钟登录后，指定当前配置代次、唯一UUID、业务收件箱分页起点、1～32条范围和1～8个保存目标。读取真实业务分页/详情并应用现有规则，最多64个递送；返回消息ID、目标/规则ID、正文模式、扫描/跳过数量及下一分页游标。不返回正文、发送方、URL、凭据或内容指纹。
- 预览只保存有限元数据及私有指纹，不写jobs、不推进自动转发事件游标、不打开总开关或调用Root服务。OFF时可明确预览，确认提交仍须ON；开启会改变代次并取消旧预览，应重新核对预览。
- `forwarding_history_commit`：同一唯一ID确认。重新验证当前Owner、设备/业务realm绑定、配置代次、SIM、可见短信、匹配规则与完整内容/请求指纹；含full_text的预览还需显式`accept_full_text=true`。全部任务和submitted回执在同一SQLite事务提交，容量不足或中途写失败整批不入队。
- `forwarding_history_cancel`：只取消未提交预览；不承诺撤回已提交或已发出的第三方请求。取消和提交争用同一Core队列锁；已取消的唯一ID不能再入队。
- `forwarding_history_request`：当前Owner只读核对独立批次/逐递送回执。未找到不等于从未受理；已submitted的commit重放只读回执，关闭或换SIM后也不再次入队。Unknown不自动重投。

历史范围使用业务`inbox`游标，和自动转发消费的`inbox_events`游标不同，不能互相替用。范围内其他SIM、二进制、疑似重复、已在回收站、规则不匹配的消息不作为补发来源。每个目标按第一条匹配规则选择正文模式，与普通转发一致。当前代次已有任务，以及跨代次queued/in_flight/Unknown/confirmed的同消息/目标会跳过；旧failed/cancelled只有新代次明确预览确认后才可再次建立任务。没有按正文猜合并，也没有SIM删除或恢复入口。

## 有限状态和兼容性

预览基于单调时钟最多五分钟；同进程不靠系统时间回退延长，Core重开将所有prepared记为expired。规则/开关变更将prepared记为cancelled。最多8个活动预览、64个持久批次，每条保存计划不超过64KiB；队列仍为最多256个未决任务、4096个回执、单递送及原有数据库大小上限。达到批次上限会拒绝新预览；批次归档/清理的网页和有限维护收尾尚未完成，不会偷偷删除Unknown或复用未确认ID。

outbox由格式2提升到3，仅新增history_batches；格式1/2先通过原绑定和完整性校验才事务迁移。保留原任务、请求内容、回执、规则及事件游标；旧in_flight重开为Unknown。旧Core拒绝格式3，不能宣称已支持旧代码回退；FWD-20兼容回退仍待独立验收，本轮不部署。

## 验证记录

首版隔离Linux截面724文件，SHA-256 `78c3f959770ab7f07c3cc27dcfacec150acaf83d488294773d01fe301c042b2e`，Core153+forwarder73项、严格Clippy/fmt、Core/Edge/forwarder生产特性检查通过。全部实际Auth/Unix IPC/SQLite与合成消息，无设备或提供商访问。

随后补充整批写失败回滚、跨代次Unknown不重建、真实委托者拒绝、格式2迁移、单调过期及Wire字段拒绝，并将私有指纹绑定完整短信及请求。最终隔离截面724文件，SHA-256 `812c546cfd63b7c2cc011df9b566b967d3546bb48311e76b14276a1830aa38bb`：Core156、forwarder75、Wire18，合计249项全部通过；同三包all-targets/all-features严格Clippy、workspace fmt、Core/Edge/forwarder生产特性检查全部通过。使用现有nobody身份、空能力、NoNewPrivileges、私有网络/设备及只读源码/依赖，无NAS/棒子访问。固定提交完整CI待核对；这些是最新工作源码隔离证据，尚无本轮生产ARM/浏览器交付证明。

## 后续交付

1. 核对最新测试及固定提交CI，保留失败记录。
2. 把历史补发放在转发区的折叠高级面板，提供有限范围/目标预览、明确确认、取消、查询及保留回执；页面刷新、双标签及丢回复只查询，不自动重提。
3. 实际Chrome桌面/手机、浏览器存储失败和未知回执保护；补有限批次归档，不能静默删除Unknown或缩小范围为只读演示。
4. 同源生产ARM构建及完整CI；是否合并部署与后续实机/回退计划一起安排。真实短信补发仍需Owner在设备页逐批确认，本轮没有外发。
