# A22：真实SDK只读状态适配与剩余9步收口边界

账号wnfhsj88，2026-11-01。起始最新main
`172d6659491c20674485ec800b3061cc8090bb9e`，在本人
`codex/wnfhsj88-a-remaining-backend-20261101` 分支推进。PR144/153/157/160/161/165
均已由维护者合并，不重交或改其实现。完整受测源码 **`a5df869217ca73499ddd5b8889f52a21563ad880`**；最终run
candidate_sources_dirty=false、首尾Git/执行输入一致。之后仅本人报告/索引，最终
完整HEAD见PR正文。AGPL-3.0-only，OpenAI Codex辅助；原上游许可/作者保留。

## 本轮真实代码

`tools/nkn-engine/sdkstatus` 在显式nkn_sdk_candidate构建下接受已经由宿主拥有的
实际*sdk.Client。Attach/Read/Retire均不NewClient/读seed/开socket/生成身份/
签名/续权/开关/关闭共享客户端，只从实际SDK锁保护的IsClosed采样。数据源代次
由宿主提供、序号严格递增；取消/旧序号/跨bridge重用/退休/回拨/晚完成拒绝。
观测上下文不是Auth证明，也不把IsClosed等同无PID或Owner OFF。

实际返回既有T5 14字段。Source=engine表示实际SDK状态源；没有Owner/进程/路径/
DHT/字节/RTT/完整节点证据时继续unknown/null，不将连接对象或开放socket填成
running/connected/0节点。SDK关闭只给有限engine_unavailable；开放只表示该读取
未提供错误，不表示NKN已连接。证据年龄只覆盖这次实际SDK本地读，不重置旧网络
样本。原SDK mutex自身不可被本库强制抢占；超时返回检查不是3秒硬中断保证，
独立的有界宿主进程/停止界限仍是必要前置。

bridge新增内部ServeWithSource，保留status/stop输入、1024/2048B及32帧边界。
序号/代次/枚举/年龄/规范u64/RTT/节点上限先验证，非法或私有provider数据不序列化，
错误只有status_unavailable且不回显原错误。LastError改为内部*string以支持既有
T5的null；原默认unavailable仍同形状。stop不读取新样本、不关闭共享SDK；静态
SDK链接标记不当授权/就绪。主入口仍调用原Serve且无source，不新增生产启动或
恢复入口。真实宿主接入此候选仍须接口/授权审核。

## 实际验证（Go1.27.3、Node、Linux amd64）

| 命令/范围 | 实际结果 |
| --- | --- |
| 正常module go test -race ./... -count=1 | 10通过：旧7+新bridge3；默认cmd无测试、不启动SDK |
| 新实际SDK集成go test -tags=nkn_sdk_candidate -race -run=^TestSDKSource | 4通过/0失败 |
| 同阶段actual bridge/protocol -race | 10通过，非另外10项新增 |
| Go vet（实际SDK适配/bridge/新真实测试） | 退出0 |
| 两个实际open/closed样本→原生产nkn-status.js parser/text | 通过；14字段与unknown/null/有限错误保持 |
| native/ARM64含真实SDK+本适配的测试probe | ELF62/183编译；native制品重跑上述4项通过；ARM未运行 |
| 实际默认frontend native原生进程矩阵 | 9组通过：EOF/unavailable/stop实际退出/截断/未知与秘密字段/帧限/部分读2秒/SIGTERM130/阻塞写2秒 |
| Python编译、gofmt、git diff --check | 退出0 |

四项新SDK测试实际覆盖：
1. 已连接自己loopback夹具的真实SDK，查询不增加dial/RPC；关闭SDK后不虚报进程退出/OFF。
2. Retire后来源拒绝，但另一拥有者的显式有界loopback RPC仍返回合成42；没有关闭共享SDK。
3. nil/0代次、取消、重放与新bridge隐式重置旧序号拒绝，不用payload升级为可信上下文。
4. 真实SDK→实际bridge→原帧输出，status/stop不增加网络/关闭SDK，输出不含SDK身份/端点。

bridge三项新增覆盖内部source正例/stop不采样、私有错误/错误枚举/陈旧/坏u64/
模拟来源拒绝、取消及非法start在source读前拒绝。fake仅用来攻击实际bridge验证器，
四项SDK集成使用原真实模块和原SDK fixture，不复制另一个状态机。第一轮新fixture
缺右括号在编译阶段失败，修正后最终干净SHA完整通过；原失败保留，不改算成功。

运行命令：

```sh
python3 tools/nkn-build/sdk-status/evaluate.py --run \
  --verified-sdk-stage "$PWD/work/a13-security/run-final/backport-stage" \
  --output "$PWD/work/a22/run-final"
```

先设置工具README中的GOMODCACHE/GOCACHE/GOPATH；默认plan不运行/联网，重跑
换NEW output。源码stage不是盲信cache：独立核验31个SDK/NCP及两层补丁，51个
Gorilla文件从官方archive+backport逐项还原摘要，严格验证go.mod/sum及原测试源，
拒绝链接后才遍历。只复制验证源码，不复制旧可执行文件。执行GOPROXY/GOSUMDB=off、
mod=readonly，无依赖下载/升级或cache改写；测试正向仅自己loopback夹具，其他
拨号在DNS/socket之前拒绝。每个子进程有界，只回收自己进程组。

| 含SDK/本适配测试probe | 字节 | SHA256 |
| --- | --- | --- |
| amd64 | 19592393 | `63535bcb7d9f91d7baed79d21e1925a50b67ccb1a291a21200931031d0c1b501` |
| arm64 | 18216438 | `5e1cb1fcce7a51fceba8198c2511e3e6e8fd2e9ddc4715752d7eb67763fc5cea` |

这些不是完整引擎或独立可复现发行，不重复计旧probe/前端交付。默认真实frontend
依旧SDK未链接/unavailable；没有把库附件或测试源码变成新的生产启动格式。

## 全部剩余任务逐项核对

用户要求完成全部剩余任务；这里明确列实际不可绕过的下一输入，而非继续把9步
统称完成。当前main与GitHub核对：PR165只有所有者流程批准，明确未作独立技术/
安全审核、未批准生产启用；D的PR154仍draft，不能替其改Core/grant/binding。

| 步骤 | 已有真实代码 | 完整收口仍需要 |
| --- | --- | --- |
| A13 | 既有有限stdio、实际SDK fork/补丁及本状态适配 | 负责人批准SDK派生/兼容选择；明确可信独立身份、固定启动及出口producer后接真正backend，不收网页seed/URL |
| A14 | 既有前端两次独立双架构构建；实际SDK/适配probe双架构 | A13真正引擎后的两次完整独立构建与来源审核；ARM实机由主线 |
| A15 | 既有真实frontend 9组进程矩阵及SDK实际失败/race | 含实际backend的父退出/失联/敏感输入/全进程矩阵；Root/cgroup真实清理由主线 |
| A16 | 31 SDK/51 Gorilla源证明、补丁/许可/实际模块通告/真实掩码修复 | 对派生补丁、1MiB兼容、完整发行清单的具体技术接纳；不以流程合并推定安全批准 |
| A22 | 本次真实SDK→stdio→既有JS解析；无额外IO/假数据 | 可信宿主launch代次/Owner及进程producer、Core只读鉴权/注册；不能拿SDK旗标做Ready/权限 |
| A25 | 既有真实Rust监督库/31失败测试、三引用/预算/重连 | 固定Root账户/策略/出口与证明序列化批准、真正producer及Root/cgroup环境验收 |
| A26 | 既有实际双服务有限路由/撤销/重放/完整binding | 普通NKN记录签名域/成员及会话producer批准，D的普通Core/媒体业务桥；不能沿BLE恢复权限 |
| A27 | 标准Noise实际Rust/WASM/Chromium与固定来源向量 | 具体特性/密钥/设备pin/转录/nonce/purpose/恢复公钥/grant绑定批准，再实现应用协议；crypto_wire_frozen=false |
| A29 | 既有真实SMTP/TLS/18邮件测试及EHLO修复 | PR120的outbox4/worker5/SMTP阶段兼容明确批准、负责人生产queue/Core Fence接线与获准邮箱实测 |

A30组合收口受这些输入约束；C组装、主线持钥签名/安装/公网/设备及最终签认归
主线。全部事项的具体旧提案已在docs/nkn/member-a-root-routing-proposal.md、
docs/ble/member-a-secure-session-proposal.md、docs/email/smtp-compatibility-proposal.md。
本包不新设这些Auth/密码/Root/数据格式，因而没有混在状态PR里擅自冻结。

依据任务书T6："核心密码协议和数据迁移的‘接口审批未完成’明确阻止生产接线"；
A26注明Core桥由D实现，T6保留真实Root/部署/最终验收。用户原先要求仍为提交
Viper-Boss审核、不自行合并或部署。不能用默认允许或编造provider绕过这些缺口。

本轮不改Cargo/根lock、crates、Core/wire/Auth、firmware、D/C范围、生产配置或
全局台账。不读取生产资料，无真实NKN/邮件/短信/电话/链/蜂窝/NAS/设备/Root/
签名/安装；测试与身份均合成。已验收81.5%/20 of169不变。远端CI精确最终HEAD
另行回读，不借PR165的检查结果。完整日志work/a22/run-final/及default-native/保留。

| 最终工作日志 | SHA256 |
| --- | --- |
| run-final/report.json | `58d3b9e193260df358dfc910378cdc536515bc2510053e30b199e2d73d0e2a1e` |
| run-final/sdk-source-tests.log | `1f89d5c42953d1a82a99e9b2de54ae0f4b0d51e25770ec87cbeb6799d4499801` |
| run-final/bridge-protocol-tests.log | `67353c5cee61e998a4a0ccef37e6799c9e3843d6dab3b6021c54140b9b9f5aef` |
| run-final/production-js-projection.log | `678a7ec94f2edab5391544c2a32a9eb519c5b2e655bb8fc400aaf17c39b2bd2e` |
| run-final/native-source-tests.log | `6008a8b365c342bcd0a4f16a262cd055420c855ec7dc2edb55df13d48186cd64` |

本分支提交前正常同步最新main `a5df2c15eb377659d7b7a48d827eea6cbdeeae96`，
其新增仅主线交接/进度/既有更新器Root部署记录；无本包源码或接口重叠。相对受测
SHA的tools/nkn-engine及tools/nkn-build/sdk-status diff为空，执行输入hash保持；
不因无关文档同步重复累加测试。本人未执行该主线记录中的部署。
