# A-09/A-10/A-12：NKN最小引擎边界提案

本包先给固定来源、实际依赖清单工具与可复用的有限请求解析器。引擎/主线IPC/Root/网络均未接入；解析器通过不表示NKN运行。用户2026-10-30授权wnfhsj88接手原A-01～30；原协议/新依赖/签名域须先由负责人审核的要求保留。本提案不是已冻结协议。

## 固定上游盘点

只读核对Viper-Boss/nknguard@c32952d7586676fec10f71a30b22969cb24881f3的23个公开文件；完整Git blob/SHA256/字节数见fixed-upstream.json，不用后续main代替。没有把上游Go代码改名或复制为Rust引擎。

| 部分 | 固定源码位置/符号 | 事实与本项目边界 |
|---|---|---|
| NKN多客户端 | pkg/nknclient/client.go Options/Open/checkDelivery | nkn-sdk-go v1.4.8，Open/NewMultiClient本身就联网，后续还做真实自发消息验证；不能在OFF/构造/纯状态查询时调用 |
| NKN状态 | pkg/nknclient/status.go ConnectionStatus/NodeStatus | 节点socket不证明端到端送达；原Endpoint/LastError不能原样公开，NknStatusV1需脱敏/有限错误映射，rtt/节点数未知不编0 |
| 私有DHT | pkg/discovery/dht/dht.go Options/Open/DiscoveryStatus | libp2p/Kad-DHT是独立栈，Open创建身份/监听并可LAN发现/端口映射；首版不跟随NKN隐式拉起，不能把全部功能打包成“最小NKN” |
| 成员证明 | pkg/membership/membership.go Derive/Proof/Verify | 独立网络成员密钥/发现证明；不等于RustSimHub Owner、Core授权或恢复grant，新签名域/撤销须另审 |
| 应用装配 | internal/app/backends_nkn.go、backends_dht.go | NAS应用生命周期/配置负责装配，不导入到棒子Core，不将任意路由/地址下放给本地引擎 |
| VPN/WireGuard | pkg/wireguard/manager.go、userspace/userspace.go | 真实设备/TUN/路由和WireGuard栈；不属于本轮最小发现/信令端口，不复用其安装/任意命令执行器 |
| Android | android/core/main.go run | 从Android交接VPN FD并创建TUN；棒子无需Android启动/GUI/移动FD耦合 |
| 统计 | pkg/usagestats/usagestats.go、chain_nkn.go | 原统计从通信NKN种子派生且有钱包Subscribe；本项目独立统计身份/OFF策略，不沿用AlwaysEnabled或把提交返回当链上确认 |
| 协议/身份 | pkg/protocol/{types,envelope}.go、pkg/identity/identity.go | 只读理解消息和身份，不能直接变成新的Owner/恢复密码协议 |

上游go.mod声明Go1.25.7，实际把nkn-sdk-go和ncp-go替换到本地third_party；SDK版本号和官方go.sum无法认证这些修改。根LICENSE是AGPLv3，两处third_party/LICENSE为Apache-2.0；后续实际提取须保留各自许可/修改来源与精确文件摘要，不拷贝vendor、示例密钥或上游应用目录。

## 本地请求候选：先审最小面

当前仅提供tools/nkn-engine/protocol.Parse纯Go解析函数，没有main/stdio/守护进程、SDK或网络依赖。协议号1为本提案候选；不修改Core/wire或T5恢复协议。后续A-13程序应直接复用此解析器，而非测试另一份逻辑。

单请求上限1024字节，严格单个JSON对象，只接受protocol/id/op三个键。id为36字符小写规范UUID。操作仅status与stop；无start、路由、URL、主机命令、身份种子/密钥路径、广播/订阅/交易。stop只表示未来调用方已有进程的有界收尾，不允许停止主机任意服务。OFF/status不得启动SDK。

```json
{"protocol":1,"id":"11111111-1111-4111-8111-111111111111","op":"status"}
```

stop只把op改为stop。未知/重复字段（含转义别名）、错误类型、截断、尾随第二对象、超量、错误id拒绝为有限错误；未知协议/动作返回unsupported，错误不回显原输入。代码单元测试直接执行交付Parse，验证返回零请求，不能在失败后继续调度。

后续进程帧建议NDJSON、单回复≤4096字节、最多一个未完成请求、3秒应答；EOF/父方退出须取消并释放自身资源，无法确认停止就Unknown。当前没有进程级实现，帧、EOF、取消、强杀/退出预算待A-13～16程序实测，不把纯解析器测试冒充这些结果。

状态应投影已冻结NknStatusProviderV1（T5 III）的明确来源/代次/序号/年龄/未知语义，不直接公开上游节点Endpoint、原始错误、成员标识。无后端明确unavailable，不能固定返回running或NKN已连接。

## 负责人需审核的具体接口选择

1. 接纳本包status/stop/1024B单对象候选，并明确A-13真实启动入口；启动应只由获准监督端提供固定凭据/Root策略，不在本请求通道新增任意seed/URL/路径。
2. 最小实际NKN是否锁定上游本地修改的两套SDK源码；若采用，逐文件保留来源与Apache通知，另审依赖/摘要，不按官方v1.4.8校验和冒称一致。默认不带libp2p、WireGuard、Android或NAS装配。
3. 真正引擎能力需要固定、经审的端点/出站策略及独立身份来源；Root安装、生产密钥、Core授权路由和真实网络执行仍由主线实现/验收。没有这些前置不实现自动连接或虚构公网成功。

原PR41由接手集成PR112承接；A-01～04契约仍待技术审核。A-05～08提交/确认状态机和A-13～16真实Go引擎/ARM制品、A-21～30等后续各包均不因本提案标为完成。现有OFF、权限和有限输入边界不放宽。
