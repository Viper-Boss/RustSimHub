# 正式更新器：只读账本入口实际交付

2026-11-02，产品b37af31d685ef9a843c5800fc59d1249cdc7fe22、源码归档926075557d1482d67c4e24ce4f4e39b78a16ce215893e1ac0d9a306a6720e211，复用已保存ARM制品，不重新编译或更换来源。PR183固定8ba6594已获用户明确授权，正常合并为74bcdce。代码、1374项Rust、Root隔离及先前临时探针见[代码验证](VALIDATION-UPDATE-READONLY-STATE-20261102.md)。

本次仅替换`/usr/libexec/rustsimhub/rustsimhub-update`：旧摘要ad5dbfa29b2c631d9c4daed107a58627aec71a9529fca54da9ac1f4fb99ebc54，新摘要81d6aaa4c0017a2923da0840fa06eb28abda5c7c568de5b81429b873d903825a；保持Root:Root、0555、单链接。不停止或重启业务服务，不调用choose、stage、admit-stage或confirm，不切换应用组。

## 安装前证据

新鲜只读预检确认应用71、floor71、无candidate/attempted或state.next，原运行选择、12个单元状态/PID及网络保持；USB和app-select为正常active/exited的一次性单元。通话仅查看路径和状态，未发现未结束呼叫；普通蜂窝数据断开，转发PID0，全局应用恢复预算未使用。

固定候选摘要、0500属性与build-info匹配冻结材料。候选在无网络/空能力/只读文件系统的受限临时进程中执行inspect-state，并对当前应用与出厂应用进行真实签名/载荷验证，均通过。没有用旧status查询或初始化账本。

旧正式程序在本机和设备唯一Root维护目录分别备份并回读；恢复脚本也核对摘要。备份与候选均不包含业务数据库、Owner或Wi-Fi凭据。当前/出厂应用的验签不等于给Root程序创建了新的维护签名包：本次是负责人按已受信构建摘要执行的固定目标维护交付，没有新增无人值守Root更新API，也不替代C的材料组装任务。

## 恢复脚本与实际替换

恢复脚本在私有网络命名空间内用真实Root文件系统和flock通过17种隔离场景：合法确认、正常新程序、已是旧程序、账本变化、pending状态、坏备份、未知程序、目标链接、备份硬链接、锁缺失/链接/FIFO/忙碌、无效确认链接/权限及已有恢复临时文件的合法/损坏情况。拒绝或无操作场景的夹具树保持；正常恢复只还原已知旧程序。没有使用模拟systemctl或操作真实服务。

脚本拒绝未知目标、账本变化和不安全备份，不创建缺失更新锁。它只恢复该固定程序，不回滚数据库、计量、应用账本或服务单元，不停止业务，也不清除/消费全局应用恢复预算。隔离脚本测试不等于设备已做过真实故障回退。

安装前启动独立600秒恢复timer。替换时只读打开既有锁、非阻塞取得独占flock，核对描述符与路径inode一致及账本摘要不变；在固定目标旁建立唯一新文件，核对新摘要后原子替换并同步。锁在退出时显式释放。没有通过可写Store执行查询或恢复。

随后正式程序的build-info及受限inspect-state通过，当前/出厂应用再次验签通过；角色健康检查完成后写入本轮Root确认标记并停止恢复timer。此标记只控制这一固定程序的恢复，未调用应用confirm或更改floor。

## 独立只读复核

独立复核再次通过：

- 正式文件与build-info分别匹配81d6aaa4及归档92607555，`read_only_state_inspection_protocol=1`。
- 正式inspect-state成功读到既有账本，trust_binding_verified为true；authorization、application_integrity、runtime_selection和health四项仍为false，不冒称C-26健康评估或维护授权。
- 账本目录的清单、inode、权限、大小、mtime/ctime及账本/公钥内容摘要保持；原选择、原boot、12单元状态/PID/重启计数及相关单元内容保持。
- 普通数据仍断开、转发PID0、全局恢复预算未使用；本轮恢复timer和service均inactive。

没有打开/恢复业务数据库，没有重启电话、短信、媒体、NAS或设备，没有外发、切网、刷分区、修改基带/NV或PMIC。业务服务持续运行，不能将本轮观察当作持锁数据库备份、真实冷启动、故障回退或人工验收。

## 交接

正式维护入口现在可使用`rustsimhub-update inspect-state`；旧status仍保留恢复语义，不能作为纯只读检查。先前代码验证中的“正式程序尚未替换”保留为当时状态，以本记录为最新交付。

PR185首次配网存储修复仍是独立未合并/未部署工作，本次没有把其内容安装到设备，也没有审核/合并成员PR。固件云端CI未执行的事实不变，不用Pages或本机结果冒充云端通过。人工与真实冷启动最后，成员及PMIC范围保持。

原固件验收81.5%、20/169、加权10.6%及主线代码约90%未加权粗估保持；安装现有入口不重复增加验收点。

私有证据：`.private/update-readonly-install-preflight-20261102-first.json`、`update-readonly-install-guard-fixtures-20261102-first.json`、`update-readonly-install-prepare-20261102-first.json`、`update-readonly-install-install-20261102-first.json`和`update-readonly-install-final-20261102-first.json`。备份、脚本、设备标识及原始输出仅在私有目录。
