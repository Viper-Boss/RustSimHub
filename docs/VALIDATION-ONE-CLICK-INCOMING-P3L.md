# 一键来电与双向听音测试

当前：2026-10-24正式epoch31已完成一键真实来电/双向短通话验收，整体79.5%。下方“尚未构建/等待双向”段落为实现与交付时的历史记录。

2026-10-23。用户反馈手动选择/准备/提交/连接声音步骤过多，明确要求一键测试。当前整体78.5%，本功能尚未构建交付或实机验收，不提前增加双向语音分数。

## 行为与接口

首页新增“一键来电测试 · 自动接听30秒”。点击后请求麦克风、正常签名登录、核对旧操作及线路空闲；只在本地准备声音，等待两分钟内第一次新来电。用户看到准备就绪后，用另一部手机拨入。测试自动核对具体呼叫、准备并只提交一次接听，确认ACTIVE后连接声音，提交接听起27秒启动一次挂断；不续租、不重试，沿用30秒Core租约保护和最多3秒只读结束确认。

等待期间音轨禁用，不预留远端媒体会话、票据或PCM；原远端预连接绝对30秒门槛保持。只有接听已获得精确租约并确认ACTIVE后，才能沿用本地准备建立真实DTLS/Opus/PCM声音。不改变授权、音频静音选择或录音策略；无录像、录音和普通蜂窝数据。

Core现有call_status增加live_calls摘要，最多64条，来自刷新成功的当前实时目录，按ID排序；目录不可用时为空，保存的历史/reconciling记录不加入。字段沿用CallSummary，不含号码、源路径或凭据，仍要求calls.read。没有新RPC命令、schema变更或权限扩展；实际接听/媒体/挂断仍逐次核验既有身份、scope、对象代次、线路、lease和单次票据。

测试记录初始实时ID，之后只允许同线路新ID、observed、direction=1、ringing_in/modem_state=3且唯一活动呼叫进入接听；再次读取精确详情后才准备。多个/其他呼叫、目录无效、线路改变、来电提前结束、权限失效都停止。没有通过历史页、相同号码或reconciling行猜测可接听对象。手动选择器也只保留observed且可操作状态，历史列表仍可查看旧记录。

接听前持久保存submit_started，重复点击共用同一个CallTest运行锁。Unknown接听不再次提交；仅返回精确目标/线路的有效租约才允许一次清理。不匹配或丢失的租约不允许猜测其他呼叫并挂断；由Core租约到期清理。退出/取消立即停麦克风，迟到许可或目录回复不得触发接听；关闭页面不恢复自动接听。

## 现有验证与尚缺

新增生产JS用例验证本地静音等待、唯一新来电、一次accept/hangup、无dial/renew、两分钟等候超时、权限/目录/线路失败、历史ID/多个呼叫/来电消失、持久化失败、Unknown或错租约、取消及迟到回复。已有10010单次提交、异常清理、收据归档回归保持通过。实际app接线验证两个按钮分发各自模式；旧reconciling记录仍展示、从选择器排除。

Core新增真实fake D-Bus测试：鉴权拒绝、当前ringing_in摘要、缺少号码/源路径、对象消失后仅历史保留、目录不可用且内存仍有陈旧对象时live_calls为空，全部零接听/拨出/挂断。该测试与Core严格Clippy已通过，新增JS已接CI。尚需完整固定提交Rust/ARM/Native、CI、原信任签名组升级与用户两端实际说话。

手动双向观测已停止，未捕获新来电、ACTIVE或PCM，不能计双向成功；其临时证书已移除。c126ddf的P2F修正已通过251项Rust/八ARM/Native26/CI39974512953，私有epoch28—30候选保留但未暂存/部署；一键来电将以新的固定源码和更高独立epoch交付。现场仍e0f843b/confirmed25、67条短信、累计10010为44，普通数据关闭，关机继续搁置。


## 已交付，等待双向听音

固定935ea9d的252项Rust、八ARM、同源真实ARM Native26项、八组网页及完整[CI39980085479](https://github.com/Viper-Boss/RustSimHub/actions/runs/39980085479)通过。沿用原信任签名epoch31—33，独立验签、逐文件摘要、schema5/篡改拒绝通过；正常暂存/一致性备份/整组选中并确认epoch31。原身份、唯一Owner、67条短信、Unknown收据、TLS和独立Root音频/网络组件保留；实际HTTPS五项资源逐字节匹配源码，七路仍开启，普通数据关闭。用户已收到新首页按钮的测试步骤，真实双向仍待反馈；累计10010仍44。

首次确认校验器因recovery.used存在拒绝，未执行confirm，失败报告保留。该标记在本次升级前两小时的开机早期已存在，初始只读preflight也记录了它。独立校验mtime早于preflight、标记内容摘要保持、升级备份时间之后无app-recover日志、当前候选精确运行且健康，随后正常确认；未删除标记或重置预算。本轮没有自动回退，当前boot的一次恢复预算已在开机早期消耗，不能声称预算未用。

开机早期日志新发现：13:24媒体依赖失败、Edge预检查失败，13:25自动整组恢复，13:26服务可用。P6AG证明保存Wi‑Fi场景最终自动恢复业务及真实声音，未证明首次启动完全无失败；该启动时序问题仍待修。其0.5点保留在最终自恢复可用的边界，不增加无故障冷启动或稳定性分数。

私有入口：oneclick-incoming-source-20261023-first.json、oneclick-incoming-native-20261023-first.json、oneclick-incoming-originaltrust-verified-20261023-first.json、oneclick-incoming-business-backup-20261023-first.json、oneclick-incoming-business-health-20261023-first.json、oneclick-incoming-upgrade-confirm-20261023-second.json（成功，first拒绝保留）、oneclick-incoming-confirm-investigation-20261023-first.json及oneclick-incoming-checkpoint-20261023-first.json。

## 实际来电与双向短通话验收（2026-10-24归档）

用户按一键流程完成测试，回复“测试已结束，电话已挂断；观察接通后接音 1.9 秒，正常”。问题明确询问手机能否听到电脑讲话及电脑能否听到手机讲话；正常反馈按该次两端短通话验收记录，没有录音。

首次只读监测捕获的是前一通未接来电，结束后退出，未看到ACTIVE或PCM；该报告的passed只代表结束和PCM关闭，不能用来证明双向。后续独立只读检查找到了第二通新来电：15:56:56 UTC响铃、15:56:57接受并ACTIVE、15:57:24挂断。两个持久操作分别为单次accept和hangup，精确绑定同一呼叫/对象代次/线路与客户端，均submitted/resolved，没有新增拨出。原操作收据逐行保持，包括旧Unknown。

该通媒体日志captured=1238、sent=1238、received=1246、played=1239；约25秒声音，19帧丢弃、1次可恢复xrun。结束时收到客户端停止，RTC Closed与电话挂断在同秒发生；不能把正常结束的Closed解释为提前故障。随后六个PCM全部关闭、Core/Edge/Media仍active，普通gsm断开，保存Wi-Fi仍连接。原唯一Owner、67条短信、删除/恢复事件及发送记录逐行保持，confirmed/floor31无候选，临时Windows证书已移除并核对不存在。

P3新增1点到15.5，总79.5%。只验收一次局域网双向短通话；拒接、远端挂断、DTMF、多客户端竞争、远程TURN、长期和欠载改善仍未验收。10010累计44，无新增拨出或短信发送。

私有独立入口：oneclick-incoming-postinspect-20261024-second.json、oneclick-incoming-acceptance-20261024-first.json。首份postinspect因本地诊断序列化/已存在目录失败保留，第二份成功；没有重拨或更改设备状态。原监测有限结果保留，不补写为ACTIVE通过。
