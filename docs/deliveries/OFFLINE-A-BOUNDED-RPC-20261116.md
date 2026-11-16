# A1独立统计有限RPC内部候选交付（2026-11-16）

本报告记录下列固定HEAD的初次RPC交付；后续Root/SDK接线与完整构建结果另见[后续报告](OFFLINE-A-ROOT-SDK-TRANSACTIONS-20261116.md)，不把新证据追记成旧源码已具备的行为。

当前完成固定用途公钥验签、原租约内nonce预检/单次提交/原摘要查询的内部组件及真实本机TLS验证。
尚未接生产Root/owned bridge/Core transport，不代表A1或全部A包完成。没有设备、公网节点、实际交易、签名安装或全局台账变化。

## 固定来源

- 实际账号yh1030，本人同仓库`codex/offline-a-statistics-rpc-20261116` → main，不自行合并。
- 起始main：`6e1cd19bc9dd05fd150157c422590c1a801cc0b0`，含产品基线`2ae840fbffff04691ccf8cedd49d554093dd4032`。
- 实现HEAD：`0e2262fadeb62be3c3addb12f8f9cfe319d37def`。
- 最终受测tree：`24b8626d641ed6387381b7968281ae6dd6b7faf6`。
- 完整源码Git归档SHA256：`121c95c8e1632df66d3b84a7d96b6c2d71171d5cbfffea51ac17589b3d32060c`。
- 后续只加本报告与接口说明；完整最终HEAD由PR回读记录，避免报告引用自身提交的循环。
- 10个实现/测试文件，加两文档。相对main不含PR263/264/265产品变化；未改依赖/锁、原Root/wire/API/300秒权限、B/firmware、coordination、BLE/AirSIM/音频、成员审核或全局进度。

## 实现与门禁

详见[有限接口与B接入说明](../offline-a/statistics-rpc-candidate-v1.md)。

原Lease/boot时钟取消与scoped dialer仍为连接门禁；不新建身份或签名、不导出种子，不接受任意节点/方法/交易。
固定首seed，仅三种RPC；无环境代理、重定向、重试或跨seed回退。
严格JSON/2048字节请求、768字节交易、规范protobuf、固定主题/meta/fee/attributes和Ed25519/双SHA256摘要。
预检缓存不延长原预算；发送前消耗唯一槽，丢回复/超时/重复请求不会再发。
单次I/O最多1秒且仍在原期限内，最多读取响应上限+1字节。
accepted只表示节点接收；查询不具有已批准链信任/深度策略，始终unknown，不能解锁Pending或生产上报。

## 实际验证

WSL2 Ubuntu24.04，Go精确1.27.3，普通UID1000，Linux原生source，TMPDIR=/tmp，原模块锁及离线缓存。
同一最终tree另在独立network namespace复验：实际接口仅lo，UID/GID1000，
CapInh/Prm/Eff/Bnd/Amb全0，NoNewPrivs=1。只在该隔离namespace启用回环，无主机网络/服务修改。

| 验证 | 实际结果 |
|---|---|
| `go test -race -count=1 -timeout=90s -json ./...` | 0；44顶层测试通过，0失败/忽略。含子测试184个pass事件，不能把44与184相加 |
| 同源/同命令无外网namespace | 同为44顶层/184含子事件，0失败/忽略；不是额外独立用例数 |
| `go vet ./...` | 0 |
| 新文件及原取消测试`gofmt -l` | 0且输出为空 |
| 固定SDK15向量 | 三窗口×五nonce原字节通过；签名/载荷/摘要/额外字段/不同意图/错误nonce逐项拒绝 |
| 真实TLS/RPC | 首seed、精确方法/参数/原nonce、丢回复单发、无预检/过期/错误绑定/变代/验签拒绝、查询重启、撤权/超时/忙碌与迟到回复通过 |
| 输入/网络负例 | nonce缺失/null/重复/负数/小数/u64溢出、响应错误id/尾随/额外error、2048/16384分块超限、HTTP错误/重定向/错误CA、无期限/已取消、禁任意方法/第二seed通过 |
| 默认关闭 | 默认未链接SDK的构建不能构造RPC backend，零HTTP |
| 实际地址helper与固定SDK钱包 | 单独1测试及vet均0，31个SDK/NCP来源三重校验、原go.mod/go.sum不变。是文件列表最小链接探针，**不是完整受管SDK/runtime构建** |

新增10个顶层用例：7个Transaction、2个usagetx、1个默认关闭；其余是原测试。
地址探针使用实现中的地址函数和SDK账号，私有测试shim只提供原ErrLaunch声明；
两份地址产品/测试文件与最终受测树逐字节核对，不能把shim当作Root授权证明。

两项受控候选mutation均实际断言失败退出1（不是编译失败）：

- 去掉consumed返回分支：实际第二次HTTP，`duplicate submit`失败；变更文件SHA256 `ea11658e09a129c3ea37d2ab342cfc12fb6604831f442c792d5576e961bde6ab`。
- 跳过规范交易验签检查：非法字节触发HTTP，`invalid request reached RPC or consumed reservation`失败；变更文件SHA256 `e89517964cec680d1fd6364f7454f58c2e5263dd739a5644a9510dc8d9aa73f9`。

旧主线没有这个交易客户端，因此不把候选mutation说成旧生产实现的回归失败。

## 原失败保留

1. 首轮tree `fd100b85694eb2473a6c102513fde8f108efe8e5`，归档SHA256 `f433548a0db7792f26fbe650a7dac1f9d409bee15ba98c1738509c2e65efe8a2`：
   深TMPDIR导致AF_UNIX路径超限；原HTTP取消测试服务端不读写、只等待request.Context，客户端已取消但清理仍卡住，90秒测试超时。
   后改用短Linux TMPDIR；原测试只增加独立释放通道，保留原客户端错误/耗时断言，不改产品超时。
2. 第二轮tree `efb51436973529455a4f8a47952b5d666e47ba7e`，归档SHA256 `b19fa747fa33d419d86ae79fba4ef8f1c9d9a24e44ea55875099d6075c3acd1e`：
   新测试构造真实scoped dialer漏初始化连接map而panic。只修测试夹具；产品runtime原本已有该初始化。
3. 修正后tree `1ea3e19d4df2d3304de705fe2c1f8f4dd7aeef46`通过，再补默认关闭/查询上限/首seed负例，最终tree独立全测通过。
4. CI尚无本包的成功证据；本机通过不等于云端通过。固定Go/SDK门禁不降级，不改工作流规避计费限制。

完整日志、失败/正向/隔离环境元数据、固定源码哈希及B接入命令保存在本机忽略目录并另打包；不提交原始私有运行目录。

## 阻塞、后续与完成边界

这部分实现可继续接线，不能把待审核PR265或本包视作已合并。
下一步：原Root注册绑定/新能力默认关闭协商、同一owned bridge有界帧与引擎IO串行，
再把PR265原Auth/参与代次/本地签名候选接原`CheckInMachine`，实测保存Pending后单发和重启只查。
原槽释放及最终确认必须有原交易/意图和批准的可信链证据，不能根据节点高度猜成功。
长期权限仍单独审核，原300秒不延长；reporting_available保持false。
共享Remote/Manual引用保留、完整SDK补丁链/ARM、真实Core→Root→SDK组合、其余A0/A2/A3/A4/A5缺口仍需推进。
热点专用开发与手机验收延期，USB共享事务及其他业务继续；公网/设备/生产签名与最终验收归负责人。

整体验收保持81.5%，原20/169、加权10.6%和原分母/权重不变。本包不改全局台账、不把离线候选当动作验收。
