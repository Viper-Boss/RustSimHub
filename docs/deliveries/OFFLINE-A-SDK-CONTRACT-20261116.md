# A包：固定SDK交易向量与有限Root接口提案

## 交付范围与准确来源

- 账号：yh1030，GitHub连接回读确认；同仓库个人分支 `codex/offline-a-stats-contract-20261116` → main，不自合并。
- 起始main：`b15ea3f561bf9e2a1268f8df5b6f0973cc83bbbd`，含任务书产品基线 `2ae840fbffff04691ccf8cedd49d554093dd4032`。
- 实现提交：`43a41dc228fb27102f8910a4508fb2760f351af5`；受测完整tree：`949c2dc2c2a06c08193fa65660aa53e2d1293522`。
- Git导出测试归档SHA256：`b858b8705486ed316f95a307d3f257224457cd37ebda08986e311829007c0bd3`。
  测试从该tree导出 `tools/nkn-build/`，在普通用户Linux原生临时目录执行；不是对可变工作区作宽泛声明。
- 后续报告提交仅增加本文；PR正文记录当前完整HEAD，本文不循环声明包含自己的最终SHA。
- 对应A1.1的离线编码前置、§4.1要求的新Root操作实现前接口PR，关联STAT-01/02/03/07。
  **A1实际生产签名/提交/确认、A0逐动作语义复核及A2～A4尚未全部完成。**
- PR255的状态机nonce串行修复是另一个独立PR。本分支从main开始，没有悄悄把其未合并代码混入本批。

## 改动与结果

1. `tools/nkn-build/usage-vectors/main.go`：标准库独立编码与固定SDK真实构造/签名/序列化/摘要的跨实现核对；
   只使用公开合成熵和原统计域分离，完全不调用SubscribeContext、网络SDK client或真实节点。
2. `vectors.json`：15组可重复向量，三个窗口×nonce 0/1/128/2^32/u64::MAX。
   覆盖两种编码（unsigned-chain及protobuf）、单SHA256签名预哈希、双SHA256交易ID、RPC非反转hex顺序。
   最大签名交易387字节。nonce最大值只证明编码，不保证SDK的int64便捷RPC可接受。
3. `run.py`/README：固定Go/31项上游源码校验、原锁文件、原生Linux独立stage、离线构建与行为负对照、来源说明。
4. `docs/offline-a/statistics-root-interface-v1.md`：Core内固定用途签名、只向SDK交签名交易；预检/单次提交/原交易核对的
   有限候选操作及版本、帧上限、共享预算、来源、公钥/许可代次/原Root绑定、默认拒绝和旧版本兼容。
   D1～D4明确待负责人审核；具体链确认RPC证据及信任模型仍缺，不能开放confirmed或宣称上报可用。

源码核查发现上游NewSubscribeTransaction默认填充32字节随机attributes；候选明确在任何摘要/签名缓存建立前清空。
这不是SDK默认行为；向量证明该候选的编码和签名一致，不证明真实链共识/费用或生产策略已批准。
上游对Hash和VerifySignature有缓存；篡改负例均重新反序列化为新对象，避免旧通过缓存掩盖问题。

## 实跑命令与证据

WSL2 Ubuntu-24.04，普通UID1000；Go严格为1.27.3 linux/amd64，安装在本任务用户专用目录，未改系统Go或全局PATH。
官方Go归档SHA256：`65efbc80fdc1fa2cd2631cad5b481777a68e8725f34e89f50fc6ac714e379b40`。
本批不改Rust生产源码、workspace、Cargo.lock或Node入口，未重复运行无关全仓Rust/ARM/网页测试。

按README执行 `python3 tools/nkn-build/usage-vectors/run.py`，显式提供Go、已核source-cache、module-cache、build-cache和原生work-root。
runner清除环境Go/代理设置，只给固定环境，`GOWORK=off GOENV=off GOTOOLCHAIN=local GOPROXY=off GOSUMDB=off CGO_ENABLED=0`。
实际子命令如下：

| 命令 | 退出码 | 观察 |
|---|---:|---|
| `go version` | 0 | 精确1.27.3 linux/amd64 |
| `gofmt -d main.go` | 0 | 无差异 |
| `go mod verify` | 0 | 所有原锁定模块校验通过 |
| `go build -mod=readonly -o <owned>/vectors .` | 0 | 原固定SDK与NKN core实际编译，非模拟SDK |
| `<owned>/vectors` | 0 | 15组向量全部与SDK一致；输出与提交vectors.json逐字节一致 |
| 同程序 `-negative intent-hash` | 1（预期） | 意图JSON哈希不能冒充链交易摘要 |
| 同程序 `-negative single-hash` | 1（预期） | 单SHA256不能冒充链交易摘要 |
| 同程序 `-negative big-endian` | 1（预期） | nonce错误字节序被真实上游编码拒绝 |
| 同程序 `-negative sdk-attributes` | 1（预期） | 未清空SDK默认随机attributes被拒绝 |
| `go vet -mod=readonly .` | 0 | 本probe实际依赖闭包通过 |

正向每个向量同时执行签名及topic各一次篡改：共30次新对象签名拒绝通过。
这不是45项Rust测试或真实链测试；四种负对照是同一已编译程序的行为断言失败，不是编译/工具缺失失败，也不称旧生产失败。
默认HTTP transport拒绝守卫记录0次调用；未使用OS网络命名空间，不能把此观察扩大为未来生产prepare的隔离证明。
原go.mod/go.sum逐字节保持，31项SDK/NCP固定源码同时核SHA256/Git blob SHA1。
向量文件SHA256：`4e3b33a2beee10c61c3ba08df0f6938ddf6c73e33ddf2890f02c368e6242550e`。

本机原始证据 `.local/deliveries/OFFLINE-A-SDK-VECTORS-949c2dc.zip`：26项/22012字节，
SHA256 `3b03bc12f5edc208da673a9fd8c63004b6463a213ff878a084e8a8d579be1738`，全部ZIP条目CRC回读通过。
含合成向量、准确source tree/归档摘要、源文件和全部正反例日志；未把私有测试目录提交Git。

## 失败、未运行与边界

- 初次WSL直连公开Go代理超时，依赖准备240秒被有界停止，锁文件未变；未运行SDK probe，不算功能失败或通过。
- Windows沙箱内公开依赖下载被拒绝后，通过原审批路径下载原锁文件公开模块到本任务目录；
  224个代理文件/18731223字节，Go原go.sum实际验证通过，再离线构建。没有换版本、关闭产品feature或删断言。
- 不重复固定SDK的网络/race/受管Root全套检查；本批只是纯离线参考probe，非生产SDK运行适配。
- 未测ARM/QEMU、设备、真实公网RPC、NKN共识/费用、提交/确认、长期授权、Root能力安装或B的发行组合。
- 新Root操作、字段、stdio枚举和生产签名器均**尚未实现**；本提案不构成其授权。
- 云端CI须在PR提交后单独回读，不能把此前计费未启动称通过。

## 来源、集成与后续

固定SDK/NCP来自nknguard `c32952d7586676fec10f71a30b22969cb24881f3`，原Apache-2.0 LICENSE和fork说明随临时stage保持；
NKN core `v2.2.1-0.20240509224846-24ade56074a3` 的go.sum及原Apache-2.0顶层LICENSE摘要见probe README。
未vendor上游、未自写椭圆曲线算法、未把Go称为Rust生产实现。新增源码AGPL-3.0-only。
AI辅助编写；已逐段核对固定上游并实际运行上述源码/行为校验，未填写估造工时。

B消费本PR的准确已提交SHA，按README自己导出源码和运行命令；没有新增程序载荷/Root策略/安装格式，不要求改B或CI文件。
回退是撤去这组工具/文档，现有生产统计查询、参与许可和共享引擎行为没有变化。
负责人需审核D1～D4及实际链证据前置；期间继续A2～A4的独立实现/复验。
没有访问设备/NAS、真实通信数据/生产密钥，没有外发消息/链交易、签名安装、自审核合并、改他人作者或全局看板。
整体已验收比例保持 **81.5%**。
