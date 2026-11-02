# 正式Root更新器的锁修复部署（2026-11-01）

## 已处理的差异

应用70和独立网络/灯控程序更新后，正式路径的更新器仍是dfd11e9e75d7b532de5fcaeacbdb9fcd87e33215构建。该版本原本已经支持维护观察与admit-stage；不能称本轮首次提供这些能力。

旧版本到825a5016的更新器生产差异仅是Store锁持有者：StoreLock在结束同步存储操作并销毁时显式unlock，防止dup/fork共享的文件描述残留导致独占锁不释放，并有“旧dup关闭不得解锁后继持有者”回归。其他变化是已合并测试矩阵。选择命令、存储/签名格式和schema没有改动。

本轮把正式固定路径/usr/libexec/rustsimhub/rustsimhub-update更新为冻结825a5016e3aa0524190af12139bf908bdd6378e8的真实ARM产物；源码归档1f0c8a98111d1b8ca22b4cd4db6c369fec7b269d6acdcb0e59cebce46bcbc324。复用此前正式1254文件/1353通过/6项原有专用忽略及ARM构建证据，本轮没有冒称重跑整仓。

## 实际执行与结果

1. 只读确认当前应用70、无候选/无在途state.next、空闲呼叫、配网无在途事务、恢复预算未使用。旧正式更新器SHA和0:0:0555单链接权限符合先前安装记录。
2. 旧程序在棒子和本机分别备份，新程序上传回读，并在真实设备执行build-info；当前应用组与出厂组均由新程序验签通过。使用既有开发信任，不新增私钥或信任根。
3. 注册300秒设备端恢复动作，再停止完整应用组。持实际writer锁归档当前业务/配置/ledger，下载回读SHA，核对归档路径及SQLite integrity_check和现有Owner。
4. 新程序在真实受限systemd单元执行inspect-maintenance及check-maintenance-versions，所需CAP范围/固定版本目录bind明确；实际schema6/outbox3与当前70三角色比较为compatible。
5. 原子替换仅此固定程序，保留0:0:0555与所有原systemd配置。实际停止再启动完整应用组，使用未修改的正式app-select单元调用新更新器boot-select；实际Result=success、ExecMainStatus=0、七服务active，仍选中精确应用70。
6. floor70、confirmed70、无候选/attempted=false及ledger原字节保持；NetworkManager和USB原PID保持。Core正常重启，原身份/设置/受保护业务表保持，原收件记录全部保留，数据完整性通过。
7. 可信HTTPS五个实际网页资源逐字节匹配冻结源码；音频routes_on=7/7，普通蜂窝数据断开，转发MainPID0。两次连续正式status成功，恢复预算没有消耗，设备端恢复定时器及服务最终inactive。

本轮仅应用组服务重启，没有重启棒子/NAS、拨号、短信/推送、切换Wi-Fi、启用数据、写分区/基带/NV或做关机驱动试验。未创建71候选、未改floor、未手工改选择链接/ledger。

## 权限与验证边界

- check-maintenance-versions是诊断比较，本次明确传入当前70作为selected/candidate/fallback三个角色；并非暂存授权或历史组回退。报告signed_programs_verified=true，但authorization_verified、installed_trust_verified、slot_identity_verified、effective_systemd_verified仍为false，按原样保留，不能冒称完成维护发起授权或完整admit-stage。
- 正式boot-select真实运行成功；本轮没有新的候选暂存或故障注入，没有将新版本更新器的启动证明当作冷启动/真实失败回退验收。
- 恢复脚本仅接受固定旧/新更新器摘要和精确ledger，未知程序/损坏备份/ledger变化均拒绝；已确认则不动作。若自动恢复预算已使用，可放回原更新器但不会清marker或再次启动应用组。
- 本机八组隔离脚本测试通过：确认不动作、新程序恢复、旧程序恢复、ledger改变拒绝、坏备份拒绝、未知程序拒绝、已用恢复预算不再启动、链接确认标记不绕过。替身systemctl不等于设备故障回退通过。
- 辅助脚本首次字符串生成在本机解析阶段失败，未创建脚本或连接设备；改为补丁写文件并py_compile后才执行。后续真实维护一路成功，未触发恢复。

私有证据：updater-root-current-20261101-first.json、updater-role-{prepare,install,final}-20261101-first.json、updater-role-guard-fixtures-20261101-first.json。备份、设备数据、脚本和原始日志仅在私有目录。

## 剩余工作

签名Root维护材料/发起授权、干净完整镜像和自动健康证据仍按C等原分工继续；mDNS及其他角色不由本次统一宣称更新。人工热点/灯效/冷启动与其他真实验收保持最后安排。原P0–P7 **81.5%**、行动台账 **20/169**不因本次锁修复部署增加验收分。
