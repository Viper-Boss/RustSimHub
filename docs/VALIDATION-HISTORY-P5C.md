# 真实旧通信库离线迁移

2026-10-14。已在NAS定位真实旧通信库，使用SQLite只读backup API取得一致快照并转换为独立历史归档。不是合成数据。本轮增加P5半点，P5为3.5/8，整体64点、约64%。真实数据的在线权限查询仍未单独验收，不能据此声称整个NAS迁移流程完成。

原库以mode=ro打开，无业务SQL写入、服务启动或配置更改。快照转换为自包含DELETE journal格式并quick_check通过；仅在私有目录保存。原库包含的认证信息只留在NAS私有快照中，迁移使用明确的十个历史实体投影。

生产迁移程序仍为已验收a5e4eb7，source archive SHA256为`7f7de34cb6278521e80644ae9d8f606bacc613811a8c5fd5f4a194a6e7859d17`。NAS在固定Debian13 ARM64隔离容器中运行validate、import、再次import、inspect。第一次归档完成，第二次新增为0、全部unchanged；源快照SHA256不变。

独立Python逐条按旧键定位原记录，对归档的每个投影值、必要业务/归属字段、JSON payload摘要进行比较，全部匹配。归档没有principals、认证用户/管理员、短信操作或呼叫操作表。真实库本身没有联系人及号码记录，该部分由P5B合成数据验收覆盖，本轮不声称实测真实联系人。

仅将已经排除认证表的历史归档复制到棒子的独立私有目录，没有复制完整认证库。归档字节SHA256一致；隔离非Root、只读bind、无D-Bus/设备的迁移程序inspect通过，实体计数一致。临时单元停止，旧四服务active，PCM关闭，没有拨号、短信发送或蜂窝测试。

详细源路径、数据数量、源/归档hash、快照、完整比较报告及复现脚本只在`.private/`、`.local/`和NAS私有目录，不提交真实通信数据或数据备份。证据入口：

- `.private/nas-real-history-snapshot.log`
- `.private/nas-real-history-import-second.log`
- `.private/nas-real-history-comparison.log`
- `.private/real-history-a5e4eb7/device-inspect.json` passed=true、cleanup_verified=true

首次上传程序缺执行权限，容器返回126、迁移未执行；设置私有程序0555后通过，失败日志保留。

下一步是原NAS在线认证/重放保护与当前NAS权限的映射、实际协议/PCM兼容，或明确由owner选择真实归档来源及旧用户后做在线查询验收。真实快照里的旧用户不会自动成为新固件的principal；不要为了测试数据便利跳过显式授权，也不要接入原始认证库。
