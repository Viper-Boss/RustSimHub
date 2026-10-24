# P4F 配网热点的Core控制面与文件通道

2026-10-24。已完成Core所有者授权、网络互斥、持久防重和Core/Root有限文件协议；289项工作区测试、严格Clippy、Core生产检查、Core/配网库ARM64构建和12组网页回归通过。尚未安装或启用，设备上次确认的应用仍为185ec6e/confirmed43，本轮不访问棒子，不开启热点、不切网、不新拨号/发短信、不启用普通数据。整体79.5%。

这一步完成控制侧；Root消费执行程序、首次设置材料/证书、AP到STA切换及网页仍缺。测试中的Root回执明确是合成数据，不代表真实热点闭环已通过。

## 启用及接口

标准--serve-device及其他现有模式保持禁用。新--serve-device-setup-ap在原设备参数后增加AP_INPUT_DIR、AP_OUTPUT_DIR、AP_JOURNAL_DIR三个可信启动参数；目录必须预先按所有权/组/权限准备，不由网络请求指定。没有Root服务就绪证明时不能开始，也没有为Core增加Root或NET_ADMIN权限。

| 协议2操作 | 额外字段 | 权限与结果 |
|---|---|---|
| setup_ap_status | token | 当前Owner及network.manage；查看当前操作或禁用/不可用状态，不返回凭据 |
| setup_ap_start | token、idempotency_key | 最近5分钟登录的Owner及network.manage，Root材料/HTTPS就绪、无活动通话、独占网络协调锁；返回operation_id和有限状态 |
| setup_ap_cancel | token、operation_id | 最近5分钟登录的同一Owner，取消精确操作并等待Root回退证明 |

委派network.manage的Client/NAS也不能调用。请求不接受UID、接口、SSID、口令、窗口时长、nonce或Root参数。数据库保持schema6，操作日志独立保存在Core私有目录，不写SQLite或SIM。

Start在发布提示前重新核验同一真实principal/generation/scopes、业务realm、设备公钥摘要及Core实例。独立的通话空闲检查通过真实控制器重新读取D-Bus目录，拒绝后台失联/过期、正在控制的通话或任意非终止呼叫；普通call_status的缓存展示行为保持。持有同一Core的网络写锁后，新拨号/接听/DTMF和其他试连接不能竞争，挂断仍可清理。

## 文件协议

Core拥有输入目录（2750、Core:Root），只发布hint.json；Root输出目录为2750、Root:Core，状态及就绪材料由Root写、Core读。固定slot文件0640，私有操作日志0600。校验普通文件、精确UID/GID/权限、单链接和限长；拒绝软链接/FIFO/过大文件，通过随机pending文件、同步和原子rename发布。路径只来自可信服务配置。

提示绑定操作UUID、Linux boot UUID、业务realm、设备公钥摘要、Root材料摘要、Core实例、随机nonce和绝对uptime截止；最多8192字节，严格拒绝未知字段。Start只允许sequence1，KeepAlive/Cancel递增且最多1024次，单次确认最多15秒，总期限最多300秒，不通过心跳延长。

Core在Root确认已消费前保持原Start内容，避免Root首次读取时只看到KeepAlive而无法开始。确认后每轮核验真实Owner登录和新读通话空闲状态，再发布短期心跳。身份实例/权限变化、取消、超时或读取失败后不再续确认。已取消会话不能被迟到的Ready或后续KeepAlive复活；Root状态不能用旧序号将Ready退回Starting。

RootLease与Root Journal提供消费规则：持久记录操作已消费和active后才允许后续副作用；同一ID不重放，重启或Unknown不清空active，也不能用新ID绕过。只有绑定同一操作且证明rollback_verified和guard_released的Closed回执才清除标记。Root Journal独占锁和512文件上限均保留；真实Root执行进程仍需接入这些原语。

Core同样先保存intent及pending再发布Start。Unknown保留操作和网络锁。显式模式启动时遇到pending会重新持锁、发布取消旧Start，不能重新激活；只有匹配原boot/realm/实例/nonce/材料的Root清理证明才能释放。完成回执先持久保存，再核对并删除自己的pending，不凭EOF、HTTP成功、时间到或一份不匹配状态当作已恢复。

## 已验证

- 289项Rust工作区测试和全工作区严格Clippy、生产功能检查通过。
- ARM64 Core ELF及配网库归档构建成功，机器类型核对为AArch64；这些是未签名开发产物，没有嵌入冻结源码包证明，不用于部署。
- 12组网页回归通过；此轮没有新增AP网页，验证的是现有日常电话/短信/媒体/网络入口未受影响。
- 真实Auth覆盖Owner、委派Client拒绝、缺少Root/TLS就绪、通话繁忙、重复键只返回原操作、权限/实例失效取消、错误nonce不能释放锁、取消后的迟到Ready、Core重启后只取消/等待精确Closed。
- 真实IPC证明默认模式下Owner也无法Start，无效会话不能查询。真实p2p D-Bus证明空闲检查重新读目录、失去绑定或来电时拒绝，没有Create/Start/Hangup副作用。
- 文件夹具覆盖弱权限、链接、超长文件、持久Unknown、重启、旧幂等键及精确清理；Root Lease覆盖旧boot/realm/nonce/材料、过期/乱序/篡改同序号和取消不可恢复。

私有日志setup-ap-control-20261024-fourth.log/.exit、setup-ap-control-web-20261024-first.json及setup-ap-control-arm-20261024-first.json。第一份定向测试使用了只控制FakeCall.start的标志来模拟目录失败，断言失败；已改为实际断开绑定及真实来电状态，失败报告保留。远端精确源码CI另存，不以本地测试代替。

## 当前限制及下一步

1. 实现Root消费执行进程：连接本协议、持久消费、P4E的NftGuard/NetworkManager/transaction、状态发布及未知结果核对。不能让任意Root脚本、静态允许Gate或无就绪证明绕过。
2. 准备每设备私有AP材料和证书SAN/精确Origin/监听。Availability的TLS/材料就绪必须来自实际安装核验；本轮测试用值不能进入设备。
3. 接首次所有者尚未认领时的可信本地窗口。当前网络API只允许已有Owner，没有新增匿名开热点或所有权重置；原setup.used规则保持。
4. 接AP到STA试连接/确认/回退和隔离模式交接，解决无USB配网、切网访问地址和保存后关闭AP。本轮未移除已有USB维护条件。
5. AP页面、状态投影、可信材料交付、真实手机/浏览器和冷启动验收；当前status只投影当前会话，重启后旧已完成键仍拒绝重发，历史回执查询待界面整合。

最新私有接手索引setup-ap-control-checkpoint-20261024-first.json。当前无需用户配合；用户休息期间只做代码和隔离验证。PMIC/关机继续搁置，普通数据不作隐式回退。

源码：crates/core/src/setup_ap_control.rs及tests.rs，crates/core/src/calls.rs的私有空闲作业，crates/wire/src/auth.rs，crates/setup-ap/src/channel.rs及channel/files.rs。
