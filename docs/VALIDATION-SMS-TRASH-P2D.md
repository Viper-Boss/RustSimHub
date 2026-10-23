# P2D：短信本地回收站与恢复

2026-10-23。按用户要求搁置关机问题，完成独立收件箱的逻辑删除、回收站、恢复和事件同步。代码与离线验证完成；尚未安装到棒子，不能把离线结果当作正式业务验收。整体暂保持76.5%。

## 行为和边界

网页打开短信详情后可移到本地回收站；回收站可分页、查看完整内容及恢复。每次删除/恢复须确认。普通短信列表、详情与未读灯统计排除删除记录；恢复保留原UUID，原已读状态保留。仅同一条短信的状态改变才产生事件，重复设置同一状态不会重复事件。

这是设备范围的可恢复隐藏，不是安全擦除；正文仍在数据库/备份，SIM及modem存储不变。不提供彻底删除或任意modem操作。既有历史导入库仍是独立只读视图，本轮不宣称历史归档删除已实现。

首版只开放给最近5分钟内认证的Owner，同时要求sms.read及既有Owner管理权限。普通Client/NAS即使有全部既有scope也不能管理回收站，未把sms.read升级为修改权；委派删除scope暂不支持。存储单写入者再次核验有效Owner和generation，撤销/换代不能靠旧Principal越权。

删除标记和sms_deleted/sms_restored事件处于同一个SQLite事务。既有inbox_events读取它们；网页补同步成功刷新列表后提交游标，所以恢复较旧UUID也能重新进入从头读取的列表。外部客户端需处理新事件并重建可见列表；本轮没有改造所有旧NAS页面或实现推送。

同一既存UUID/来源重新观察不会复活删除短信。基带对象换代后出现相同正文是否同一物理消息仍有歧义，本轮不靠正文匹配隐藏可能真实新来的短信，不宣称跨所有NAS导入路径的防复活已完成。

网页单任务串行；写请求结果不明时不自动重试并禁用后续修改，提示刷新核对。刷新会重读服务器状态后由用户决定，不自动重发。已确认写入但列表刷新失败，会明确区分。退出、刷新及迟到详情有代次隔离，不把旧短信正文重新贴回新列表。

## 接口及代码

协议2新增sms_management_status、delete_sms、restore_sms、deleted_inbox、deleted_sms_detail；沿用可信HTTPS/Origin/IPC与签名会话。分页1到32条，网页16条；拒绝额外owner、路径及delete_from_sim字段。写操作按sms_id和目标deleted状态幂等，不产生运营商请求。

CLI新增：`simhubctl delete-sms SOCKET SESSION ID`、`restore-sms`、`deleted-sms`，以及`deleted-inbox SOCKET SESSION`、`sms-management SOCKET SESSION`。SESSION是既有私有会话文件，不把token放命令行。

- `crates/storage/src/inbox_management.rs`：事务、Owner复核、回收站查询及持久化测试。
- `crates/storage/src/inbox-management.sql`：schema6迁移。
- `crates/auth/src/lib.rs`、`crates/core/src/authenticated.rs`、`crates/wire/src/auth.rs`：认证和协议调度。
- `crates/edge/src/web/sms-management.js`、`app.js`、`index.html`：操作和会话生命周期。
- `tools/test-web-sms-management.mjs`：实际网页模块的合成DOM回归，已接CI。

## 数据升级及回退

数据库5→6增加inbox_tombstones，并在事务内重建事件表以允许两个新事件，保留原记录、db_epoch、seq及AUTOINCREMENT高水位。失败应回滚迁移；旧v5应用拒绝打开v6，不能用它直接回退。此项属于开发计划6.3的维护迁移范围，**尚不具备可自动回退到现场旧应用的部署条件**。

Core/updater的build-info声明schema6；本地构建元数据核对存储/Core/updater一致。新包声明5..6，打包器按真实构建元数据保留旧制品的5..5，不把旧二进制重标成兼容6。离线包验证器可显式指定schema，系统组合器检查应用和updater一致；新updater以6检查，拒绝只支持5的包。

上机前须用旧版本生成一致性业务备份、验证保留新增消息的迁移/恢复方案，并准备兼容6的完整回退应用组；不能仅替换Core，不能将旧v5数据备份静默覆盖新短信。本轮未改现场updater、签名组、业务数据库或分区。

## 验证及下一步

本地完整Linux workspace/all-features回归通过，包含新增存储4项、认证1项、Core真实帧IPC1项及协议1项。验证持久重开、幂等事件、拒绝Client/旧Owner、删除/恢复事件插入失败的原子回滚、v5内容/epoch/cursor保留及事件序号100→101。

七组网页回归通过：新增回收站覆盖权限、取消、重复操作、分页、恢复、未知写结果、已确认写后的读失败、畸形回复、退出迟到响应、详情乱序；实际app也检查刷新后旧详情不得回显。生产无默认特性检查及Clippy通过。构建、系统工具与GitHub结果在HANDOFF和私有索引补充，不能提前视为成功。

下一步是schema6组的完整候选构建/回退与维护迁移验收，再做新系统实机收发/跨客户端删除恢复；原卡普通数据保持关闭。不需要为本轮离线功能拨号、发短信或恢复关机测试。
