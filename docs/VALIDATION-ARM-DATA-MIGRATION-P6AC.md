# P6AC：真实ARM Core的数据迁移与兼容版本重开

2026-10-23。继续100%目标，在NAS隔离ARM64环境运行三份真实生产Core及合成数据库，验证v5→v6、签名IPC、本地删除/恢复及兼容版本重新打开数据。未访问棒子，未改正式数据；整体77%。

## 输入与方法

| 角色 | 固定源码 | 作用 |
|---|---|---|
| old | 38366a8dac63a127f7aa50882f0c05cb907fc4e2 | 现用业务代码的v5 Core副本，正常读取原库；拒绝v6 |
| new | 9528712c784d80c9326b99f46b1f72971b5714d2 | 新候选Core，真实执行迁移与删除 |
| compat | 8785d3c2d45b7aeb7f434b117ae2ff2a0b892817 | 较早v6 Core，重新读取新增记录并执行恢复 |

程序均从固定构建元数据核对二进制摘要，不从现场复制私有数据。运行基础为P6AB的固定Debian13 ARM64镜像，无网络、无host硬件挂载、无capabilities、只读根文件系统和有界tmpfs。Core使用`--serve-authenticated`，未启用modem观察或发送控制器。

初始v5数据库由38366a8的五份实际SQL构造，包含一条合成中文/emoji短信、seq100、两个合成Principal（Owner及持全部既有scope的Client）。这些Principal是可信测试种子，**不是首次Owner初始化验收**。客户端在工作站对真实challenge transcript签名，验证设备签名、身份pin、purpose、角色、scope和generation，经生产`--request-auth`及本地帧IPC调用业务接口；没有绕过服务端鉴权。

成功汇总为`.private/schema-data-runtime-20261023-third.json`，结合first中的三个已完成正向检查和本轮独立负向验收。first/second均保留false，不能将其包装为完整通过。

## 实际通过

1. old Core成功启动、Owner/Client签名登录；列表、完整正文和旧事件游标与合成种子一致。正常停止后WAL已关闭，保存可读的离线一致性快照。
2. new Core真实打开v5库并迁移6。旧会话token失效；重新登录仍是同一设备身份和Principal generation。原db_epoch不变，旧seq100游标可继续读取。
3. 持全部既有scope的Client删除/读回收站仍被拒绝；Owner可删除，普通收件箱隐藏，回收站正文保持，产生sms_deleted事件。
4. 停止new Core后，向试验库注入一条新的合成接收记录及对应事件；重新运行new Core可见新记录，原删除标记保持。
5. 停止new Core，使用compat Core打开同一v6库。新合成正文可完整读取，原删除消息仍不可见；Owner恢复后，两条原UUID均可见。事件顺序为sms_deleted、sms_received、sms_restored，游标100→103，不回退或重置。
6. old Core的直接存储入口读取该v6库，返回`storage: NewerSchema`，没有生成新身份，数据库逐字节保持。不把“旧版本拒绝”当作可回退到旧v5的证明。
7. 从实际old Core停止后的v5快照构造DDL冲突库。new Core迁移返回`storage: Database`，原库逐字节保持，也未创建身份。此项证明该迁移冲突失败路径，不代表磁盘满或进程中断矩阵全部通过。
8. 正向容器和负向容器已退出并移除；全部试验数据库、版本摘要及失败输出仅私有保存。

## 原始失败与解释

- first：正向三阶段已完成，最后测试脚本要求通用服务入口打印NewerSchema而失败。该入口经Auth将存储错误包装为Storage，错误字符串的假设不成立。first整体保持false；负向测试改用直接调用Store的`--prepare-device-setup`，没有放松产品或错误判断。
- second：构造负向运行镜像时将本地image ID直接用于FROM，被解释为仓库名并请求远程元数据，遭403，测试未执行。未发送私钥或业务数据；失败保留。third改用已固定的本地Debian基础镜像并重新校验程序副本后执行通过。

## 必须保留的限制

这次新增接收记录是**Core停止时的可信SQL合成输入**，不是modem实际收信，也不证明接收适配器、MM重连或换卡去重路径。它证明实际生产程序对已有新增数据的读取、删除保持和恢复行为。

compat是较早v6 Core的单程序兼容检查，没有将其拼成一个新的正式签名组。new与compat这两个提交的Core业务代码相同；不能据此泛化到未来改变数据格式的代码。P6AB签名组选取与本次Core数据检查仍是分开的证据，尚未把完整Core/Edge/Media切换与真实收信合在一次验收中。

离线快照不是业务backup API/restore API的完整验收；恢复后生成新db_epoch及权限隔离仍按既有单元证据记录。本轮也未完成维护安装工具、正式Owner/证书交付、冷启动或真实掉电矩阵。

下一步核对现场的未初始化状态与维护条件，准备兼容v6的完整基线及可信备份/交付流程，再安排正式业务初始化；不将新库直接交给旧v5应用，不把旧备份覆盖候选期间新短信当作回退。关机问题仍搁置，原卡普通数据不启用。

## 数据试验后的现场只读复核

现场补充只读复核：`.private/firstboot-prerequisites-20261023-first.json`通过；固定身份SSH/同boot维护，MM/NM/rmtfs/网络守卫/灯控active，正式Core/Edge/Media inactive。installation.complete、正式identity.seed、业务store.sqlite3和升级账本state.json均不存在，PCM关闭、普通gsm断开且无gsm配置。当前factory epoch13声明5..5。未安装或初始化。后续应按空正式业务的v6首次安装准备，不能把隔离v5迁移试验说成已迁移现场；仍需沿用正确发布信任和交付完整基线、Owner/证书流程。整体77%，关机继续搁置。
