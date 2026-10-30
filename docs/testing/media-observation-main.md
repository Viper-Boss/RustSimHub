# D-25/26前置：媒体观察取消与计数耗尽的主线收敛

2026-10-30。从当前main独立接回旧PR #55 的两文件代码修复，不合入其旧GATT/看板依赖，
不改旧分支或自动关闭旧PR。不是新的TURN/远程媒体实现，也不重复扣D步骤。

原始代码提交 `10966baef286f37cb93eea33e0d00c5c29201a13` 的作者、作者日期、消息保留，
通过 `git cherry-pick -x` 保留来源；新提交的committer为实际GTW。只增加本人的当前验证报告。

## 复现和修正

在真实main `local.rs` 上只接同一6项测试模块，退出101：4通过、2失败。
两项失败均为旧 Permit.check(0) 返回 Ok、期望 Denied：

- 已发出pulse后等待future被取消，但Client/Permit仍保留；原代码不立即撤销镜像。
- 私有请求计数被夹具置u64::MAX，checked_add失败直接返回Expired；原许可仍可用。

这不是无限许可：原有自然心跳截止仍在。计数穷尽不是常见正常路径，也不是实机断音诊断。
修正使用私有 ExchangeGuard：只有正确、有效回复更新成功才confirmed；其他返回/取消Drop撤销。
计数溢出同样先revoke再返回。未poll future不误撤权；成功pulse仍有效；Client Drop仍撤权。

保留500ms RPC、1000ms媒体镜像上限、原通话租约/票据/peer/call/line/代次关联，
不改wire协议1、scope、采样/队列/重试策略。失效后的正确迟到回复不能恢复镜像或队列PCM。

## 实际测试与边界

6个测试入口调用实际Client/RemoteAuthority/Permit/Pump和真实UnixListener/UnixStream，
只替换外部Core端与PCM设备。16类错误回复覆盖错ID/protocol/boot/字段/类型、过期/超长/
最大截止、无效JSON/重复字段/伪role、零/超长/截断/断连/超时；错误后pump只关一次且不再I/O。
另有取消、计数耗尽、未poll、成功、Drop、晚正确回复、错预期UID/父目录权限拒绝且不交票据。
错预期UID测试用实际socket metadata/内核来源比对，未创建不同UID进程或假SO_PEERCRED。

修复后本机媒体28项通过、严格Clippy/fmt通过；完整整仓结果见[交付报告](../deliveries/member-d-media-observation-main.md)。
日志只含合成信息。没有modem/ALSA/BlueZ/TURN、公网、电话或实际音频。
ARM/native、设备通话/两端听音、真实TURN/DTLS互操作仍未运行。

旧#55仍由负责人决定如何收敛/关闭；本PR在main上独立审核，不冒称旧PR已合并。
默认行为只在上述真实观察失败时提前撤销，没有新服务或安装/数据格式变化。
