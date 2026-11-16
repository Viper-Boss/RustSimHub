# 固定用途统计本地签名候选 v1

本组件实现A1的本地编码/签名与Core授权门禁，沿用[接口提案](statistics-root-interface-v1.md)。
负责人在主线`6e1cd19bc9dd05fd150157c422590c1a801cc0b0`的[审核边界](../deliveries/LEAD-USB-FIRST-OFFLINE-INTEGRATION-20261116.md)允许继续离线候选与拒绝测试；这不是新Root操作、长期授权或生产上报许可。
`reporting_available:false`保持；此处没有SDK/RPC提供者、服务或外部调用入口。

## 实现与调用边界

- `rustsimhub-usage-stats/transaction-candidate`默认关闭，提供`StatisticsIdentity::prepare_subscription`。
  输入只能为原严格`CheckInIntent`、独立身份、已有EnabledPolicy和u64 nonce。OFF先返回None，不读时钟；公钥不同、周期陈旧或时钟不可用均不返回交易。
- 不接受任意消息、topic、fee、attributes或签名公钥。原三个主题/订阅块数及六字段JSON元数据由原类型固定。
  使用原统计身份域分离种子；不读取Owner/SIM/NKN传输种子，不公开种子或通用签名方法。
- 本地构造SUBSCRIBE payload及NKN unsigned-chain字节：type=6、nonce/fee小端、fee=0、显式空attributes。
  签名单SHA256(unsigned)；链摘要为该hash再次SHA256；实际提交字节为独立protobuf Transaction/Program，最多768字节。
  **空attributes是待决候选选择，SDK默认随机32字节并未改动。**
- `PreparedSubscription`仅公开不可变的意图、nonce、unsigned/hash/signature、实际交易字节和真实摘要；不能反序列化成伪准备结果。
- `rustsimhub-core/usage-transaction-candidate`默认关闭，依赖已有完整`product-providers-candidate`及上述usage特性。
  这只使内部候选类可编译，不增加命令行上报开关、wire命令或网页接口；原显式产品启动门禁保持。
- Core `nkn_statistics::SubscriptionSigner::from_current_owner`取得原Auth的本地recent Owner及原有限`NknRootAuthority`，
  绑定当前principal、realm、observer instance与原participation快照/actor。许可不存在/OFF、Client/NAS、错误token或绑定不足即拒绝，绝不创建统计身份。
- 每次`prepare`使用**原CheckInMachine传入的AttemptBudget**；前后检查截止/取消与有限权限。
  当前Auth锁和原Ledger生命周期锁覆盖小型本地准备，检查真实持久许可完整快照，故OFF后ON也不能复用旧许可代次。
  先验证原FileStore仍完整，再签名；结束再次验证原授权。不修改FileStore格式、原300秒限制或库的300秒重查冷却。

## 仍须由后续适配完成的部分

1. 本组件使用现有身份FileStore，**没有实现生产身份首次创建/恢复装配**。初始化仍须在当前授权下完成，不能从传输/Owner身份派生统计种子。
2. Core持有的是原Auth本地有限证明，**不是已经建立的Root注册或SDK就绪证明**。它不启动Root，也不延长已有共享引擎授权。
3. nonce目前为内部Rust调用参数；只有后续有限预检适配才能证明其与原注册generation/nonce/policy hash/统计公钥完全绑定。
   本地签名成功不能当作preflight成功、nonce预留或Root同意发送。
4. 原CheckInMachine仍独占持久Pending→一次submit ticket→原摘要query。此处不维护第二套签到状态、不持久保存可补发交易。
   集成测试调用的submit/query为本机测试端口，**不是生产Root/SDK/RPC接通证据**；生产提交适配仍未实现。
5. 新操作10～12、帧预算/能力协商仍按提案核旧版本/原注册后接入；Root策略默认不开放新能力。原汇总查询与三引用主管未改。
6. 真实链证据、信任来源和确认深度未决，不能输出confirmed；不能把SDK接受、mempool或已有订阅冒充确认。
7. 原有限权限到期后拒绝。长期许可、后台调度、引用释放及SDK共享进程的真实联调仍需后续，不采用AlwaysEnabled或永久化Owner同意。

## 给B的构建/兼容输入

从交付报告/PR完整SHA导出源码，原Cargo.lock及全部SDK/Go锁定文件保持。
默认构建不包含候选签名模块；明确核候选时使用Core `usage-transaction-candidate`，
它要求完整既有生产候选依赖，避免仅root-provider特性使原UID测试缺迁移API。
旧Core/Root/SDK仍无统计提交能力；不因候选编译通过把策略字段或`reporting_available`改成可用。
无新程序/载荷/安装格式、固件策略或迁移；不生成替代build/profile manifest的清单。
撤回候选feature不改原持久格式或清除Pending；Root/RPC接线完成前不能当作可部署上报方案。

实现、源码摘要、默认/候选真实命令、受控故障反例及原失败见[交付报告](../deliveries/OFFLINE-A-LOCAL-SIGNER-20261116.md)。
固定上游原SDK/NCP与NKN core为独立跨实现依据，许可证和来源保持；本候选Rust代码为AGPL-3.0-only。
