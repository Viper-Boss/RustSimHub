# C-07 · 权限与输入矩阵

基线：main `2f1595aa3b71fbfdefdbc8801ff6ecd844144b85`，已含 C-05 PR #6 和 C-06 PR #7。
本项复用真实 history/session API 和原独立合成 SQLite 夹具，只增加测试；不改变授权器、
会话 900 秒期限、签名、归档 schema、导入器或格式协商规则。

## 新增验证

| 边界 | 实际测试与保留证据 |
|---|---|
| 历史身份/权限 | HistoryService 对未授权 NAS、同公钥错误 kind、inactive、新 generation、缩减 scope 的列表/详情/分页及非 Owner 管理请求拒绝；Owner 显式撤销后旧游标/详情仍拒绝。每阶段比较归档、access、来源和相邻合成当前/身份/Owner 文件字节 |
| 游标/记录 ID | 空、负数、超长数、SQL 片段、路径、NUL、全角数字不能成为 SQL 或权限；分别篡改 epoch、audience、source、entity 均拒绝，分页 0/17/usize::MAX 拒绝；拒绝前后文件保持 |
| 非法来源/缺字段 | BLOB、无效 UTF-8、零 ID、空/NULL 复合键、缺列/表分别走真实 validate/import/preflight。已有有效归档和相邻当前哨兵保持；文本字段未超限但 canonical JSON 超 128 KiB 仍拒绝 |
| 重复复合键 | 同内容重复 sms_contacts：首次 15 新增/1 unchanged，再跑 0/16；改变重复项且先插入排序更早的新短信后整批冲突，原归档 3 条短信和所有文件保持 |
| 合法文本/未知值 | 合法 UTF-8 中的 NUL 作为正文数据保留，不冒充二进制；缺 timestamp/sender 保持 NULL，不合成号码/时间；来源字节保持 |
| 路径/namespace | 相对路径、带 ./ 或 .. 的非规范路径、符号链接文件/目录、NUL 路径、硬链接（含原文件 nlink>1）、非规范目标路径拒绝。namespace 空/63/65 位/大写/换行/路径/NUL 拒绝；默认报告不回显恶意来源 |
| 错误归档版本 | application_id 错误或 user_version=2，真实 import 和 HistoryService 拒绝，不创建 access 或替换已有库 |
| 文件上限 | 临时稀疏来源 256 MiB+1，真实 import 有界读取后拒绝；归档 512 MiB+1，import/reader 拒绝。不截断/重建，核对长度、原有效前缀及另一库保持；不把大文件整体读入测试内存 |
| 会话真正过期 | 真实签名发出会话并核对 expires_at=now+900；仅在 cfg(test) 中把该真实内存会话 deadline 置于过去，再调用原 subject/authorize 必须拒绝，持久授权库不变；新 nonce 可重新签发，旧 token 不复活 |
| 会话错误身份/令牌/时间 | 错公钥、Client/Owner kind、inactive、新 generation、缩 scope、越权写 scope、未知/不合法 token 拒绝且持久库不变；签名时间偏差 ±301 秒拒绝，±300 秒保持现有规则可用；撤销别名后 token 拒绝 |
| 会话库错误版本 | 实际已绑定别名的 legacy.sqlite，错误 app/version 拒绝启动，原文件字节保持，不重新初始化 |

新增历史测试位于 `crates/compat/src/history/tests/input_matrix.rs`（8 项），
会话测试位于 `crates/compat/src/session/tests/input_matrix.rs`（3 项），
由各自既有 tests.rs 接入，复用原 fixture/Request，不提供测试专用替代授权器。
deadline 调整只发生在测试模块可见的真实会话状态；不新增产品时钟注入或延长期限，也不声称等待了 15 分钟。

## 已有覆盖与本项关系

- 原 history/tests.rs：多用户线路/联系人隔离、tombstone、个人标记、来源/类型隔离、撤销、身份/realm/归档绑定。
- 原 session/tests.rs：设备绑定签名、旧 binding epoch、篡改正文、持久 nonce 防重、重启不保留 token 和时钟回退。
- C-05 history/preflight/tests.rs：恶意未知表/视图名不执行、不回显；未知列拒绝，缺值/缺表/UTF-8、100001 行、128 KiB 超限、sidecar、私有模式、重复/冲突及默认脱敏。
- C-06 history_recovery_tests.rs：真实 ABORT、OS 文件大小写失败、进程强杀和报告丢失的恢复重跑，以及 realm/代次/旧映射和相邻当前状态保持。
- 原 lib.rs/Go 桥测试：协议主/次版本、版本错误分类、有限读取、严格 JSON/TLS、错误/超限/未知字段、原始 Go 签名向量；不另造版本授权语义。

完整命令、退出码、对应 HEAD 和失败记录见[交付报告](../deliveries/member-c-nas.md)。
Windows 仅执行该平台适用回归；新增 SQLite/会话矩阵须在 Linux 实际运行，不能用交叉编译冒称通过。

## 边界

这些是离线模块测试，不是 Core/HTTPS 组合鉴权的替代实现或真实 NAS、SIM 及设备验收。
相邻 current.sqlite/outbox.sqlite 是不应被触碰的合成哨兵，不宣称验证了真实 Core 的发送队列。
合法来源 format/产品版本的未知值保留规则沿用 C-05；不能因为某个 SQLite user_version 数字就猜 NAS 发布版本。
不因失败删除来源、清空历史/授权库、改 namespace、放宽权限或自动重试。
来源为上述固定 main 的真实模块和原独立合成夹具，新增源码保留 AGPL-3.0-only，无新依赖。
