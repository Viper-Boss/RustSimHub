# D23 Core同正文参数绑定

2026-10-31（Asia/Shanghai）。新增Core可选`recovery-execution` feature及库模块`recovery_body`，消费真实Auth的`AuthorizedRecoveryRequest`，补齐之前Auth准备/查询接口要求Core自行校验的正文语义。没有新增正常AuthCommand、加密会话实现、监听器或网络执行器。

## 明确的本地应用正文v1

`decode_v1`是供可信Core主机调用的**本地应用正文候选**；版本由这份显式API选择，不是宣称A的握手/加密wire已冻结。后续A适配器必须按共同审核的应用接口提供确切已认证body及其动作/请求UUID；本包不选择ECDH/HKDF/AEAD或修改A会话库。默认不开启，新feature也不自动注册服务/部署。以下JSON均不含真实凭据：

| Auth已核动作 | 允许的完整正文结构 |
|---|---|
| BeginWifi | `{"ssid":"SYNTHETIC_NET","passphrase":"SYNTHETIC_PASSWORD"}` |
| ConfirmWifi / CancelWifi | `{"trial_id":"<64小写hex>"}` |
| QueryOperation | `{"target_request_id":"<规范小写UUID>"}` |

动作与本次请求UUID来自Auth私有能力，不从正文的action/request_id/token/channel/uid/verified字段取。拒绝任何未知字段、重复字段、字段缺失/错类型、尾随值、无效UTF-8/JSON；Confirm/Cancel target由同正文解析后交Auth的64小写hex门禁。查询UUID须按现有uuid库重新显示后逐字相等。

Begin复用普通Core已有的`rustsimhub_wire::wifi_credentials_valid`：SSID UTF-8字节/控制字符和WPA2口令规则保持一致，不新造宽松规则。解析使用封闭serde结构，SSID/口令放入无Debug/Serialize的Zeroizing字符串，无serde_json::Value中间业务对象。错误只返回有限Auth Error，不输出serde正文。JSON解析器内部临时缓冲/编译器副本不在擦除保证内，不声称消除了所有内存副本。

## 实际私有接力

1. 解析前先核同Auth的真实能力、确切动作/正文、当前grant及连接/期限。改正文、另一Auth、断连/撤权会先失败。
2. Core私有Write把同正文参数与Auth指纹准备对象一起保留。reserve消费Write，只有新插入返回私有Pending，Existing只有只读快照；参数不能由调用者事后换成另一套。
3. claim消费Pending并保留原参数；Begin只能转换为`CoreRecoveryTrial`，里面是原Auth有界trial授权和同正文凭据。Confirm/Cancel只能绑定这个私有Core trial，不能转成Begin。
4. Core私有Lookup的方法不接额外UUID参数，只使用自己的已认证正文目标，调用原Auth/Store只读查询；NotFound不给重发或执行票据。

Core这些类型无公开字段构造器/Clone/Serde/Debug，没有正常Principal转换。普通外部枚举变体只能装已有私有值，不能从JSON制造它们。写动作仍要求显式schema9；查询的实际Store调用也要求9，旧7不能藉由新feature执行查询/业务。

原始认证正文不做隐含JSON规范化：不同空白/转义即使解码成相同凭据，持久指纹仍不同，同UUID拒绝Conflict。请求ID不自动换新，重连的旧Reserved/Unknown仍只读；丢票据或关闭Core不能重建执行许可。派生trial沿用40秒/grant/原能力最小期限，控制不能延长。

## 实际验证及余项

新增14项真实Core库/Auth/Identity/Store9测试，覆盖私有解析→登记→领取→trial/control/查询的接力、独立Confirm/Cancel、重复/未知/普通AuthCommand伪装、JSON/UTF-8/类型/截断、普通Wi-Fi规则与转义、准确查询目标及快照字节保持、认证字节篡改、断连/撤权/跨Auth、重连旧Reserved与原始正文变体冲突、旧7拒绝、重开Unknown和完成备份无明文SSID/口令。外部A证明适配器仅cfg(test)合成；不冒称真实SO_PEERCRED、A密码、蓝牙或NM路径在本批联调。

feature只增加已有锁定uuid版本的一条Core依赖边，无新第三方版本。普通库/原grant+transport配置仍可编译；原Core业务、唯一ACTIVE/TRIALS、网络/电话锁、USB/NM/WWAN/IMS/APN和默认启动均未修改。精确受测HEAD、整仓结果和环境中断见[本包交付](../deliveries/member-d-core-recovery-body.md)。

这仍是Core业务派发的**参数绑定接点**，未调用原Wi-Fi执行器。下一步须让同一个原actor消费这个私有trial/独立Control，接Normal/Recovery所有权、前后Gate及#139观察，不复制引擎/全局锁。普通USB门禁不得全局移除，Recovery例外须持有真实获核来源。A会话、真实host/Root/UID、发行9兼容和设备联验保留；不把库组件通过当成端到端可安装。完整步骤仍21/26、剩5，整体约80%。
