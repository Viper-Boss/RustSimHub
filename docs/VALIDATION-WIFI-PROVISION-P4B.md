# Owner 确认 Wi-Fi 配置实际验收

2026-10-14。P4增加1点至7/12，总体66.5点、约67%。本轮证明新的有限配置流程、明确保存和Core重启后的自动重连；实际连接使用现有SSID，不声明不同路由器/新SSID、未配置Wi-Fi首启、热点或物理断电验收。

源码9b5f6b07762f7d9dc727f81f92ed21ea19f73467，精确源归档SHA256 72215080d62106507534294a5f6cf58cfeed45ba754a9c7ae77efc22de5ae975。170项本机Linux测试、Clippy和六个ARM64生产程序构建通过，Core fixtures=false；GitHub CI38570475237 success。产物.local/wsl-artifacts-9b5f6b07762f7d9dc727f81f92ed21ea19f73467，本机WSL构建日志.exit=0。

## 实际结果

- 隔离生产Core/Edge通过NAS到设备USB的严格HTTPS进行验收；新独立owner以设备签名挑战完成初始化和登录。原身份、短信/电话业务库没有接入测试。
- 未启用配置的模式拒绝命令；启用后status.read客户拒绝开始，私有意图目录仍为空。
- 新配置以独立UUID/volatile/dbus-client/autoconnect=false创建，实际AP的SSID与候选一致并取得IPv4后才进入ready；同key/同输入返回同trial，改变密码和并发key拒绝。
- Cancel实际恢复原Wi-Fi，候选profile被删除，原profile UUID集合不变。
- 另一独立key使用错误PSK；候选profile和有限NM checkpoint存在时实际SIGKILL Core。Core不在场，NM定时恢复原Wi-Fi、删除临时候选并清空checkpoint。重启后同key不能再次激活；此前取消的key也不能重新执行。
- 正确配置再次进入ready，owner显式确认后返回saved；自身profile持久保存、autoconnect=yes，重复confirm不重复保存。
- Core停止后该profile仍存在，NM自动重新激活它、Wi-Fi到State100；新Core重新登录后旧trial不可恢复，同一次key不可重放。不是NAS代为保存或重连。
- Core/Edge实际UID及CapEff/CapPrm/CapAmb/CapBnd空集合验证通过。
- 验收结束停止自有单元、仅删除本轮候选UUID并恢复原profile；按UID/mode/nlink/hash移除本轮polkit规则。原profile集合、WWAN无线开关不变，旧四服务active、PCMclosed、NM checkpoints=0。没有拨号、短信发送、普通APN或蜂窝互联网测试。

私有报告.private/wifi-provision-9b5f6b0-second.json passed/cleanup_verified=true，fresh_profile_ready_cancel_and_input_idempotency、real_process_kill_rollback_and_no_restart_replay、explicit_confirmation_persists_own_autoconnect_profile、confirmed_profile_survives_core_restart、temporary_policy_removed均true。SSID/PSK只在私有验收进程和NM配置处理，不提交。

第一轮NAS shell提示混入mktemp输出，尚未启动测试服务/网络操作即退出；改为核验最后一行路径后第二轮通过，失败报告保留。一个额外pkcheck权限检查未通过，没有据此声称拒绝；随后读取同UID实际GetPermissions，network-control/settings.modify.system/checkpoint-rollback恢复为auth，未经额外认证不再直接允许（yes）。这不等同于永久硬拒绝no；结果另存.private/wifi-provision-permissions.log。六程序本轮增量ARM release35.48秒，不是冷缓存或等功能性能对照。

接口见[wifi-provisioning.md](wifi-provisioning.md)。网页已接线并通过JS语法检查，真实浏览器未验收。配置模式目前独立启用；同一个完整媒体/历史/NAS Core的网络接线和相关并发控制仍须继续实现与验收。完整系统/开机安装/恢复门禁未解除。
