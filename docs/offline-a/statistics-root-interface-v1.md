# 统计交易接线接口提案 v1（待负责人审核）

2026-11-16后续：已按主线允许的离线候选方向实现原Root/私有SDK通道、能力协商和拒绝测试，见[实际交付](../deliveries/OFFLINE-A-ROOT-SDK-TRANSACTIONS-20261116.md)。
新增操作仍非生产批准：默认策略/编译/launch禁用；当前实现不允许confirmed，Core状态机和长期策略仍待。下文保留原提案截面及未来确认契约，不把提案当实现。

本提案对应任务书A §4.1、A1.1～A1.11，关联STAT-01/02/03/06/07/09。基线为
`b15ea3f561bf9e2a1268f8df5b6f0973cc83bbbd`。这是**实现前的接口交付**，不是已实现的Root能力，
不是生产签名或长期权限批准。当前 `reporting_available:false` 保持。任务书A/B的冻结接口不变。

## 当前事实与实现门禁

- Core `nkn_root/usage.rs` 的操作7～9只有汇总查询；Root `usage_linux.go` 返回87字节原注册快照及
  `RSUC\x01 + u32be长度 + 有界JSON`。SDK bridge现有操作没有统计签名或发送。
- Root策略只认识 `status_protocol` 和 `usage_query_protocol`，均最多1；现有生产路径不因本提案开放新操作。
- 固定SDK是 `Viper-Boss/nknguard@c32952d7586676fec10f71a30b22969cb24881f3` 中的SDK/NCP，
  以原 `source-manifest.json` 逐文件验证。NKN core依赖保持原go.mod/go.sum固定版本。
  `rpc.go:SubscribeContext` 最后调用 `SendRawTransactionContext`，不能用它实现纯prepare。
- PR255提出的单身份串行规则仍需合并审核：任一窗口Pending未决，其他窗口不能准备交易。
  本提案不把未决超过24小时当作nonce释放；关闭/重启也不能释放。
- 新增Root/SDK操作须在本接口PR获审核后实现。未识别版本、缺策略、缺字段、能力不足一律不可用，
  不回退旧SDK便捷接口，不改变原300秒有限Owner授权，不启动额外引擎。

## 信任与身份放置

选择：统计种子及Ed25519签名留在Core既有受保护 `FileStore` 中；SDK进程只收已签名且有界的交易。
Root不读统计私钥、不接受调用方指定密钥路径，也不提供通用签名接口。传输身份种子继续使用原独立Root路径。
Core内只增加用途固定的 `prepare_subscription`，输入为库内验证的 `CheckInIntent` 和本次预检nonce；
只允许三个本产品主题、空identifier、fee=0、空attributes，meta为原意图的确定性公开JSON。
固定上游构造器会随机生成32字节attributes；候选显式在任何签名/摘要缓存建立前清空，
这不是SDK默认行为，须由向量及负责人审核确认，不可在提交时再改attributes。
不对调用方提供任意字节签名、任意fee/目标/主题/属性/公钥的入口。具体编码由固定上游跨实现向量证明后再落地。

准备、预检、提交、原交易核对均必须在同一当前Owner/realm、参与许可generation、Root注册generation、
boot deadline、policy hash、随机nonce和独立统计公钥下。许可文件不是Root capability；
每次实际操作前和返回后重新验证当前授权，不能只在创建适配器时验证。
OFF/撤权/过期/换realm/取消时不创建身份、不签名、不启动、不提交；已有Pending和旧身份保留供以后显式授权核对。

## 有限协议与协商候选

新增Root安装策略字段 `usage_transaction_protocol:1`，缺失/0表示禁用，其他值拒绝加载。
Root安装策略仍绑定固定源码、引擎可执行SHA256、Core pin、统计公钥、单个usb0或wlan0和既定HTTPS seeds。
SDK握手同时声明该能力1；Root策略、实际SDK握手和Core编译能力三者全为1才可用。不能仅凭收到请求宣称支持。
原82字节 `RSCN\x01` 注册绑定头保持，候选操作10/11/12依次为预检/单次提交/原交易查询。
**编号须经负责人确认后使用**；不会覆盖既有0～9或以失败后尝试多个版本探测。

请求均为：82字节原头 + `RSTX\x01` + u32be JSON长度 + 一个JSON对象。
统一长度上限2048字节；拒绝0长度、超长、短读、重复/未知字段、null、非整数、尾随数据和非法枚举。
请求不接收路径、UID、URL、RPC method、shell、任意签名消息或延长deadline。

| 操作 | JSON有限字段 | 成功含义 |
|---|---|---|
| 10 preflight | `operation_id` UUID；`participation_generation` u64；`public_key`小写64hex | 返回该公钥对应地址的下一nonce；不是预留、签名或提交 |
| 11 submit | 同上；`intent`原六字段对象；`digest`小写64hex；`transaction`小写hex，解码后≤768字节 | 只表示原交易单次尝试被节点接受；不能表示链上确认 |
| 12 query | 同上；`intent`原对象；`digest`小写64hex | 只核对原摘要和原意图，证据不足返回unknown |

`participation_generation`只作绑定与回传，**不是Root独立相信的Owner凭证**；权限仍来自原有限注册及Core实时检查。
Core只有在preflight得到公钥/注册完全匹配的nonce后才进入纯本地prepare；prepare本身没有HTTP/SDK调用。
nonce预检和后续步骤共用原CheckInMachine的单一AttemptBudget；不重置10秒总预算。
当前nonce失效只能失败并保留摘要，不能在submit里获取新nonce、重签或改原交易。

响应：原87字节注册快照 + `RSTX\x01` + u32be长度 + 一个≤2048字节严格对象。
共同字段为 `version:1, operation_id, participation_generation, public_key, status`，完全回传请求绑定。
preflight仅在 `status:"ready"` 时另带 `nonce` u64；不把缺失/null nonce当0。
submit status只有 `accepted|unknown|unavailable|denied`，不得出现confirmed。
query status只有 `confirmed|unknown|unavailable|denied`；confirmed另带与原意图完全匹配的固定确认字段。
向原库返回的仍是现有严格 `{intent,digest,status}`，不能直接信任SDK的status字符串。
Core只在原租约/许可代次仍有效时转换；迟到响应不跨代写成功。

Root↔SDK沿用原长度前缀私有stdio、同一有限注册和串行请求锁；bridge增加三个有限枚举，
拒绝原status/usage帧与新响应混用。最多1024帧、每操作每5秒限流沿用原主管约束，超时/错帧毒化本次管道。
SDK没有统计种子，不能自行签名或续订；Root没有任意网络代理能力。

## 交易、确认与失败语义

1. preflight只允许固定第一HTTPS seed的 `getnoncebyaddr`，地址由绑定公钥推导，读取nonce与池nonce取原SDK语义。
2. Core先构造并验证真实NKN签名交易，准备摘要由NKN真实无签名交易编码的链哈希算法得到，
   不使用意图JSON哈希；原Pending落盘成功后才消费一次提交ticket。
3. submit仅允许 `sendrawtransaction`。Root/SDK先反序列化并重新计算摘要、验证签名及固定订阅payload，
   公钥、主题、duration、meta、identifier、fee、nonce/attributes均匹配本次准备结果。
   preflight返回的nonce只能绑定于当前租约内一个临时预检记录；它不是第二套持久签到状态或链上nonce预留。
   租约丢失后该临时记录失效，只能使用原Pending做query，不能补做submit。
   不能把原JSON直接透传为RPC参数，不能对RPC失败自动再次发送。
4. query只允许 `gettransaction` 及实现真实确认所必需的固定块读取方法。具体RPC字段/方法名
   **必须从固定NKN源码核实后补证据，当前未完成，不开放confirmed**。
   须验证交易摘要、签名与完整原payload，并证明该交易在实际确认区块中；仅mempool存在、
   send返回摘要、主题已有订阅、height字段或JSON `confirmed:true` 均不充分。
   只信单节点自述不能称为独立链验证；生产信任模型/所需确认深度由负责人确认后明确。
5. 原持久Pending仅有摘要和意图；重启后不能凭它重新生成或再次发送原交易。
   只query。若prepared后尚未submit就崩溃，则保持unknown而不是补发或新nonce。
6. 关闭/撤权只取消本次统计引用及拥有的IO。Remote/Manual引用存在时共享引擎保持。
   错误不清除Pending，不把结果unknown改failed后重新提交。

## 网络与资源预算候选

- 每Attempt最多10秒，受原Root boot deadline及调用方更早截止限制；单次RPC≤1秒，所有RPC共享剩余预算。
- preflight/submit响应体各≤2048字节；query交易/块证据分别≤16KiB/256KiB，
  最多一个交易和一个确认块、总≤272KiB。超限返回unknown，不截断后当成功。
  大区块可能无法核对，这是明确不可用边界，不自动扩大或无限翻页。
- 原2048字节库回复只放核验结论，不嵌入原始块/任意日志。内容长度先检查再分配，读取中执行截止与取消。
- 沿用固定出口的scopedDialer：仅原usb0或wlan0；禁环境代理、重定向、LAN/回环/链路本地/多播目标、
  普通蜂窝、任意RPC透传、改seed及失败后换节点。测试fixture单独允许回环，生产不能启用该构建模式。
- 单统计身份只有一个Pending；一个在途请求。速率限制不足/预算耗尽不给额外线程或后台重试。

## 长期权限候选与必须批准的决策

本批实现只能使用最近Owner授予的原有限≤300秒注册；到期后明确不可用，许可仍可显示为“已保存，未运行”。
不自动续Root权、不采用AlwaysEnabled，不把远程token或NAS身份换成Owner。
长期运行另需负责人批准一种新的、用途固定的本地授予：绑定Owner/realm/统计公钥/披露版本和许可代次，
每24小时最多三个固定订阅，撤销立即取消，重启可恢复但不能扩大权限，授权及软件升级都有可审计回执。
这是候选方向，未定义完整签名/存储契约前不提供生产启用路径。

待负责人明确：D1采用Core内固定用途签名；D2上述三操作/编号/版本与预算；
D3实际链确认的信任来源及深度；D4长期权限单独设计审核（本批保持不可用）。
其中D3证据尚未完成，不能以合并提案或本机向量通过视为统计上报已完成。
等待审核期间继续A2～A4独立工作，不接管B的安装/Rootfs/CI文件。

## 给B的兼容与验证输入

本PR没有新增程序、载荷、策略字段实现、wire命令或安装格式。旧Core/Root/SDK组合行为保持；
未来只要缺任一新能力，统计提交不可用，原汇总查询/Remote/Manual照常按已有授权运行。
B使用本PR已提交完整SHA导出源码，不执行上传的不明二进制，不修改冻结coordination接口。
未来实现须交给B以下回归：缺能力/错版本/旧Root拒绝；超限/短读/重复字段；错误公钥/realm/generation；
取消/到期/EOF；prepare零网络；持久化失败零submit；原交易只提交一次；重启query-only；
伪造确认/错payload/池交易拒绝；预检nonce竞争；统计释放不终止另两引用。
这些是待实现验收条件，不能作为本PR已运行测试计数。
