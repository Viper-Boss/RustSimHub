# D23 身份私有域请求指纹

2026-10-31（Asia/Shanghai）。在已合并#103的执行接口范围内，继#135的显式schema9准入，新增可选Auth `recovery-execution` feature。流程依据仍是[Owner APPROVED记录](https://github.com/Viper-Boss/RustSimHub/pull/103#pullrequestreview-5708198127)；记录明确未逐行技术审核，不当作新实现技术验收或生产迁移授权。

`Auth::prepare_recovery_execution`消费真实`AuthorizedRecoveryRequest`，先核同Auth私有能力/动作/确切正文及当前grant，要求已经显式打开的schema9，计算指纹，再核原能力。返回不实现Debug/Clone/Serde、无公开构造器的`RecoveryExecutionRequest`。它保留原Zeroizing正文分配，只提供可信Core解析所需的借用；不是持久执行permit，也不写数据库或派发网络。

Begin不带target；Confirm/Cancel必须有64位小写hex target。这里只验证target形状和将它纳入承诺，**不解析业务正文、也不证明target就是正文所指trial**；接线Core必须从同一已认证正文解析/验证target。只读、Query、Disconnect不能用此写动作准备接口。`check_recovery_execution`仍核schema9及原能力，断连、撤权、替换verifier、另一Auth实例和单调期限届满均拒绝。

## 固定v2编码

使用已有HMAC-SHA256和Identity Ed25519 seed，以`HMAC-SHA256(seed, b"RustSimHub.RecoveryExecution.Key.v2\0")`派生独立本地去重key。seed导出和派生key使用Zeroizing临时变量；不增加key文件、不复用SIM/会话key，不声称库内部状态/编译器副本均已擦除。Identity与备份仍须按原私有材料保护；数据库指纹不是密码证明，Identity私钥同时泄露后的猜测攻击不在此隔离保证内。

请求MAC按以下顺序编码；`part`为u32小端字节长度再接原字节，整数固定宽小端，无隐含JSON规范化或字符重编码：

1. `b"RustSimHub.RecoveryExecution.Request.v2\0"`。
2. part(realm UTF-8)、设备公钥原32字节、part(Owner UUID UTF-8)、Owner代次u64。
3. part(grant UUID UTF-8)、独立恢复公钥原32字节、grant代次u64。
4. request UUID原16字节（UUID网络字节序）；数据库显示采用规范小写UUID。
5. part动作ASCII（begin_wifi/confirm_wifi/cancel_wifi）、max_trial_ms u32。
6. target存在标记u8；有值时再part(64位hex ASCII)。
7. part(确切已认证正文原字节)，包含其原空白/NUL/二进制差异。

会话ID、连接nonce、Auth实例和期限不进入持久指纹，使同一授权代次下跨重连/进程重开可比较同请求；它们仍由私有能力门禁实际核验，不能用持久指纹延长授权。realm/设备/Owner/grant代次/约束/正文任意变化都改变承诺。此为本地去重编码，不选择A的ECDH/HKDF/AEAD套件或新线协议。

## 实际验证和边界

独立Python标准库参考生成并核验4组固定向量：Begin的合成Wi-Fi正文、Confirm、Cancel、带NUL的二进制正文；Rust读取相同向量比较实际HMAC。参考入口：

```sh
python3 docs/ble/recovery-execution-fingerprint-vectors.py
```

新增12项Auth执行测试和1项Identity派生向量测试，使用真实Identity、Owner签名登录、Store7→9/Backup API及当前grant；仅外部A verifier是cfg(test)合成适配器。覆盖每个持久绑定字段/正文变化、重连及完整Auth/Store重开指纹稳定、旧能力失效、改正文/跨Auth、撤权/verifier变更、确定性过期、动作/target边界、旧7拒绝，以及准备前后真实Backup API库文件逐字相等且不含合成口令。没有计时睡眠、延长租约或改生产错误映射；撤权沿用Storage错误映射。

Auth仅新增已有锁定uuid依赖的一条直接边，无新版本。默认feature和旧grant feature不引入执行接口。整仓实际结果、完整受测HEAD与日志摘要见[交付报告](../deliveries/member-d-recovery-execution-fingerprint.md)。没有生产库/真实口令或设备输入。

尚无reserve、claim、Lookup、一次执行permit、Core唯一Wi-Fi actor接线；不以可比较指纹代替持久账本。后续writer必须原子核当前数据库grant与无锁存活/单调fence，禁止writer取Auth mutex/调用Store/await；副作用继续使用原网络锁/电话协调/40秒/checkpoint及保存前后Gate。A实际安全会话、Root/发行9兼容和设备验收保留。默认启动仍不打开9，未部署，完整步骤计数保持21/26、剩5，整体约80%。
