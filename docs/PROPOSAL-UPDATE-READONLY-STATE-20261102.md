# Root只读更新账本检查 v1

本接口属于主线UPD-01的有限Root入口，不接管C-26候选健康评估。旧`status ROOT PUBLIC`仍保留`Store::open`的恢复语义；不能把它当成无副作用的安装前查询。

## 入口与数据来源

新增`rustsimhub-update inspect-state`，只接受这一个参数，Root UID才可运行。生产路径固定：

- 账本目录：`/var/lib/rustsimhub/rustsimhub-v1/system/app`。
- 既有锁：上述目录中的`.update.lock`，普通文件、Root所有、0600、单链接；只读打开并取得非阻塞共享锁。
- 既有账本：`state.json`，普通文件、Root所有、0600、单链接，最多8192字节，复用真实State解析/关系检查。
- 已安装信任：`/usr/share/rustsimhub/release.public`，普通文件、Root所有、0444或0644、单链接，恰好32字节；其SHA-256必须等于账本的key_sha256。

不接受调用方路径、JSON健康结论、包内公钥或浏览器标记。不打开业务数据库、不启动Core/worker、不读取消息或凭据，不使用网络或GPIO。

缺失文件不初始化；任何`state.next`（包括损坏内容和悬空链接）需要由显式恢复流程处理，本入口拒绝并保持原状。不创建锁或versions目录，不重命名/删除状态，不消费候选、改floor、选择版本或确认健康。

## 一致性与报告

共享锁与现有Store独占锁互斥，可在Core业务进程运行时检查，不需要获取业务写入栅栏。读取时及报告前复核目录、锁、信任和账本的文件身份/元数据；锁路径被替换、原描述符与路径不匹配或文件变化均拒绝。Drop显式释放共享锁，复制/继承的描述符不能使锁滞留或解开后续独占锁。

输出为有界JSON：format=1、scope=`application-ledger`、read_only=true、trust_binding_verified=true，state为实际账本结构。authorization_verified、application_integrity_verified、runtime_selection_verified及health_verified均为false。调用方应在持有观察对象期间消费报告；它是时间点观察，不是可稍后重放的维护能力或健康票据。

报告不验证签名载荷、运行exe/PID、实际选择链接、业务schema/outbox或健康，也不授权confirm。C-26后续可消费这份Root来源信息，但仍必须接入自己的精确候选、Root、运行与数据证据；不能仅凭这个JSON标为健康。

## 命令分派兼容

新增入口在通用可写Store分派前处理，错误参数不会退回通用分派。同时为既有Store命令先校验动作与参数数量，再读取路径/信任和打开Store，避免拼写错误或错误参数在返回Invalid前意外恢复state.next。合法boot-select/install-factory/stage/admit-stage/choose/confirm/status行为不变，未知命令不初始化存储。

`--build-info`新增`read_only_state_inspection_protocol:1`；既有maintenance_observation_protocol、应用组格式及schema保持不变。没有新依赖、数据库/签名格式、Root账户/能力或网络API。正式安装和设备支持声明以验证/部署记录为准。
