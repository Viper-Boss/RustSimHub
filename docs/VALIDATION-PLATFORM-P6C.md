# 共用 Core 与空历史首次安装验收

2026-10-15。把此前分别运行的配网与媒体/短信/电话/历史/NAS服务接入同一个Core，并验证无旧NAS数据的启动方式。本轮完善已有里程碑的集成与首次安装边界，不额外增加点数：总体仍66.5点、约67%，80%目标继续。

## 精确版本

| 源码 | 归档SHA256 | 本机与CI |
|---|---|---|
| 182a6ecdd5ebe4e3e3fc4545f6871f274353af5b | b4e4ce2b7619dd8c7f152fdb98837837f4ffa6c202236d97953f5c852f763777 | 172项测试/lint、六程序ARM64生产通过；CI38725177270 success |
| cd4c96e1df09c598438e088e0e1b5add20bb99e8 | 69a2450c5419844975ed1960efd2c27900c444e870a393b93672e4c7aeaa2fa7 | 174项测试/lint、六程序ARM64生产通过；CI38726463632 success |

本机WSL精确归档构建，两个.exit均0；Core fixtures=false，部署前文件SHA和自报source hash匹配。产物.local/wsl-artifacts-完整提交，日志.private/wsl-build-完整提交.log。两次增量ARM release分别34.72/35.68秒，不是冷缓存性能对照。

## 实机组合模式

报告.private/platform-182a6ec-first.json passed/cleanup_verified=true：

- 单个Core通过--serve-platform同时提供短信、电话控制/媒体授权、历史、NAS会话与配网；health准确报告组合功能，media_available保持false。
- 实际收件箱、合成历史查询、call_status与NAS Go客户端读取在新的Wi-Fi试连接期间持续可用；拨号准备请求被拒绝，没有提交拨号或短信。
- 独立模拟D-Bus电话验收进一步证明拒绝发生在创建新意图/消耗旧prepared意图/调用CreateCall前；释放网络锁后原意图可以显式提交，网络锁持有时挂断照常完成。反向在途语音锁也阻止配网开始。
- 新候选profile和NM checkpoint存在时实际SIGKILL共用Core；NM独立恢复原Wi-Fi，重启防重仍有效。新owner登录后实际收件箱响应保持相同；NAS映射保留、旧会话不复用、新签名会话读取一致。
- owner确认配置后持久保存；Core退出后NM仍自动重新激活候选并取得State100，重启后相同key不能再次执行。

## 无旧 NAS 数据的首次安装

报告.private/platform-cd4c96e-empty-first.json passed/cleanup_verified=true、no_legacy_snapshot_required=true：

- 对新私有目录执行migrate init，仅生成空历史结构，没有生成或读取旧NAS/合成快照，没有批准NAS principal或配置NAS桥客户端。
- 本地owner初始化/登录后，组合Core的历史来源为空，实际SIM收件箱可读取，电话控制/媒体授权和配网接口同时启用。
- 同一Core完成Wi-Fi连接、取消、保存、真实进程终止恢复、重启防重、保存后自动重连；本地业务不要求NAS数据库或NAS授权会话。
- NAS在本轮只充当USB HTTPS验收客户端；没有关闭NAS、拔USB或物理断电测试，不能称为已完成完整独立上电验收。

两轮均验证Core/Edge实际非Root UID与空能力集合；仅删除自有候选UUID，原profile集合/WWAN开关不变；按精确hash移除自己的临时polkit规则，自有服务停止，旧四服务active、PCMclosed、NM checkpoints=0。没有普通蜂窝APN、蜂窝互联网测试、新拨号或短信发送。

额外正反对照.private/platform-cd4c96e-empty-positive.json也passed/cleanup_verified=true：读取当前有效line，配网期间同线路的拨号准备被拒绝；取消恢复并确认checkpoint清空后，准备请求成功返回prepared，active_calls仍0，整个测试没有submit_call。这验证新的互斥与恢复点检查不会永久堵住电话准备；测试留下的只是隔离业务库内的未提交意图，不是实际拨号。

## 持续边界

空库初始化以提交/sync/不覆盖重命名发布；已有目标、遗留staging或链接拒绝，测试包含后续合法离线导入。更换归档仍需按新epoch重新审核授权，不能继承旧授权库。

运行器尚未安装为开机服务，也没有启动完整媒体worker或交付网页声音；本轮只验证媒体授权随组合Core启用。完整镜像、恢复入口、物理断电、其他SSID/热点、正式证书、浏览器和长期稳定性仍需继续。临时polkit调试权限不等于已经安装生产策略。

接口与并发边界见[platform-runtime.md](platform-runtime.md)。本轮曾因自动审批系统额度检查失败未执行一次本机构建命令，额度恢复后按原审批流程完成，没有绕过检查；不是实机故障或验收成功证据。
