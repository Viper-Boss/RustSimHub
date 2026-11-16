# 独立统计有限RPC内部候选

这是A1.6的默认关闭候选，基于主线`6e1cd19bc9dd05fd150157c422590c1a801cc0b0`。
2026-11-16已接原Root注册、私有owned bridge及`managed.Run`，详见[接线复验](../deliveries/OFFLINE-A-ROOT-SDK-TRANSACTIONS-20261116.md)。
原public解析器继续拒绝交易；Root策略缺省0，需显式策略、编译和原私有launch均为1。
Core参与许可/签名/单一`CheckInMachine`实际接线与可信链确认尚缺，不能视为生产统计上报能力。

## 已实现

`tools/nkn-engine/usagetx`只含请求校验和公钥验签，无签名、种子或网络：

- 三个操作编号10/11/12仍为候选契约；仅显式开启新能力且原注册完全匹配的独立Core观察连接可进入Root交易路径。
- 预检字段严格为version、operation_id、participation_generation、public_key；
  提交增加原intent、digest、transaction；查询增加原intent和digest，禁止夹带交易。
- 请求最多2048字节，原六字段意图最多512字节，签名交易最多768字节。
  拒绝重复/未知/缺失/null字段、尾随值、超过8层嵌套、错误类型及计数溢出。
- 使用原三个主题、窗口、块数及续订周期溢出边界。
  验证原nonce、固定fee=0/空attributes、原六字段meta、规范protobuf字节、Ed25519签名和双SHA256摘要。
  未知字段、非规范编码或改变签名用途不能通过逐字节重建比较。

`managed.usageTransactions`是私有内部组件，不提供URL、RPC方法或任意交易透传接口。
构造需要同一原`Lease`及其`scopedDialer`；默认未链接SDK的构建拒绝构造。
真实SDK地址函数用固定NKN的`ToCodeHash`/`ToAddress`，不是通用Bitcoin地址算法。

| 阶段 | 有限动作及结果 |
|---|---|
| 预检 | 原安装策略的第一个seed，单次`getnoncebyaddr(address)`，取严格u64的nonce/nonceInTxPool较大者。相同操作重复预检返回原值，不重查、不延长原期限 |
| 提交 | 同一操作ID/参与代次/公钥/原nonce；验签和规范字节核对成功后，发送前消耗一次性预检槽，单次`sendrawtransaction(tx)`。节点回传精确摘要只记accepted |
| 查询 | 单次`gettransaction(hash)`；可查询重启前保存的原intent/digest，不要求重新预检。不具有批准的可信链证据/深度规则，始终unknown，不确认、不清槽 |

预检槽只是同一原Root租约内的临时防重复能力，不是第二份持久签到状态。
提交消耗后，丢回复、超时、返回错误、重新预检均不能再发；相同提交只回原accepted/unknown。
冲突操作、代次、公钥、意图或摘要拒绝。没有成功确认策略，消耗的槽在该租约内不重新分配。
未消耗槽过期可由后续显式预检替换；原槽过期不能提交。

每次调用要求有效截止时间，原预检槽总预算最多10秒且不续期；每次I/O再限制1秒，
并绑定原租约取消、内核boot时钟期限、原scoped连接检查。
预检/提交响应最多2048字节，查询最多16384字节，使用limit+1读取检测分块超限。
HTTP200、严格RPC id/版本/结果及JSON边界都必须满足；无代理、重定向、重试或第二seed回退。
生产地址/DNS/接口及能力策略继续由原scoped dialer与Root出口限制共同执行。

## 接线约束与未完成项

1. 只有原`CheckInMachine`确认没有任何持久Pending，才能做nonce预检。重启有Pending时直接查原摘要，不取新nonce。
2. prepare保持本地纯编码/签名；不在prepare内部调用预检RPC。签名来自PR265固定用途候选，不把seed交给SDK。
3. Core原Auth/recent local Owner、realm、身份、公钥、参与许可完整代次和原总预算必须在每个边界重核。
   当前组件只核原Lease、绑定公钥和调用方操作代次，尚未替Core证明当前参与许可有效。
4. 新Root帧、原82字节注册绑定、能力协商、严格owned bridge解析与同一引擎IO锁已接线。
   新策略还要求原status_protocol=1，共享1024帧预算及每操作5秒冷却。原public status/stop协议不扩大，Root策略默认拒绝新能力，300秒上限不变。
   交易必须用独立观察连接，不复用原owner控制连接，也不能在读回复前half-close写端；EOF只取消该观察操作。
5. 完整受管SDK四段补丁链已按原manifest及真实prepare复验；默认SDK和显式交易SDK两组race/vet/本机程序构建通过，SDK/Root ARM64只完成交叉构建。
   编译开启需`nkn_sdk_candidate,nkn_usage_transaction_candidate`，仅`nkn_sdk_candidate`不声明新能力；编译开关不是参与许可。
6. 原摘要/意图/实际交易与可信区块的关系、确认深度、重组处理及消耗槽释放仍待明确/实现。
   节点接受、交易池存在、订阅已存在或任意height/confirmations字段都不能视为确认。
7. 没有调用共享SDK停止/引用释放接口。原管道锁、取消后毒化、不关闭共享管道，以及观察连接EOF不关闭owner已测试；真正Core只释放Statistics引用、Remote/Manual保留的组合验证仍需完成。
8. 长期参与权限/更新/撤销仍需单独审核；不延长300秒换通过。reporting_available仍false。

## 给B的验证入口

保持仓库工具链要求：Go1.27.3 linux/amd64，原SDK/模块锁不变。
默认Go模块无新增第三方依赖。在Linux原生目录导出精确Git树，TMPDIR使用短Linux路径，执行：

```sh
cd tools/nkn-engine
GOWORK=off GOENV=off GOTOOLCHAIN=local GOPROXY=off GOSUMDB=off \
  go test -race -count=1 -timeout=90s -json ./...
GOWORK=off GOENV=off GOTOOLCHAIN=local GOPROXY=off GOSUMDB=off go vet ./...
gofmt -l usagetx managed/usage_transaction_linux.go managed/usage_transaction_linux_test.go \
  managed/statistics_address_linux.go managed/statistics_address_linux_test.go \
  managed/statistics_address_sdk_linux.go managed/statistics_address_sdk_linux_test.go \
  managed/usage_linux_test.go
```

不能删除同源`tools/nkn-build/usage-vectors/vectors.json`：新测试直接读取这15组已固定SDK核对的公开合成向量。
gofmt输出必须为空，不能只看退出0。测试统计须区分顶层用例与子用例。
本机TLS夹具用原Lease.Check、真实scoped dialer和合成CA，只在测试私有launch中允许回环；不改变生产LAN拒绝。
已做SDK/Root ARM交叉构建；完整发行复现、CI接入和部署归B/主线，本包没有改B的入口/工作流。
实际prepare会把原向量JSON按测试相对路径复制到新stage，并记入原inputs哈希，不重新生成测试期望。
在完整校验后的生成stage中，另执行两套互不累加的验证：

```sh
go test -race -mod=readonly -tags=nkn_sdk_candidate -count=1 -timeout=90s -json \
  github.com/Viper-Boss/RustSimHub/tools/nkn-engine/...
go test -race -mod=readonly -tags=nkn_sdk_candidate,nkn_usage_transaction_candidate \
  -count=1 -timeout=90s -json github.com/Viper-Boss/RustSimHub/tools/nkn-engine/...
```

来源：固定SDK/nknguard `c32952d7586676fec10f71a30b22969cb24881f3`的rpc.go、wallet.go，
NKN core `v2.2.1-0.20240509224846-24ade56074a3`的api/common/interfaces.go、common/{common,uint160}.go。
沿用原许可证与来源，不把SDK称作Rust实现。实际结果见[交付报告](../deliveries/OFFLINE-A-BOUNDED-RPC-20261116.md)。
