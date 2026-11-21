# A3：HTTPS撤权/截止竞态修复，邮件/NAS离线复验

## 来源与交付边界

- 账号yh1030；本仓库本人分支`codex/offline-a-forwarder-20261116` → main，不自合并。
- 起始main `b15ea3f561bf9e2a1268f8df5b6f0973cc83bbbd`，包含产品基线`2ae840fbffff04691ccf8cedd49d554093dd4032`。
- 实现提交`8ec10d95ea2a92385cf72a478c92bd07419c2821`；受测完整tree `a9fbeca7ac01734bce22997d61d07cb5e82cbcd8`。
- 同tree完整Git测试归档SHA256 `e6f4c575674d73aa54bdf288c17b92a70a165018182f15d3f457b8b7aa711dd4`。
- 后续只补本报告、转发README及[20项差距表](../offline-a/forwarding-nas-audit-20261116.md)，产品/测试字节保持。
  PR正文写当前完整HEAD，不在本文循环声明包含自己的最终SHA。
- 主修复对应FWD-13/16/21；复验A3中的12项转发与8项NAS动作，逐项区分已有代码、模拟/本机IO和待现场部分。
  不声明20项全部已验收，也不声明全部A包/93项完成。A1新增Root操作仍待PR256接口审核，其他独立任务继续。

## 具体问题与改动

`HttpsSender`只靠外层`select!/timeout`取消。一个立即就绪的权限回调可在同一poll里撤权，随后仍调用connector；
一个同步阻塞回调可耗尽总预算，返回后仍进入下一IO阶段。旧代码两项回归分别实际失败101，均出现不应发生的连接调用。
这不是用测试等待时间推断网络安全，回归直接检查实际connector次数、后续fence阶段及真实TLS对端没有HTTP请求。

只改`crates/forwarder/src/delivery.rs`：在权限、连接、TLS/HTTP握手返回后，以及提交和读取/确认边界，
重查原取消通道及单调总预算；结束时也不接受迟到结果。总预算仍10秒，不延长权限、不新增重试/线程/后台driver。
提交前取消/到期保持NotSubmitted；原attempted已消费后丢失结果仍Unknown，保留原状态/ID，不再递送。
`delivery/tests.rs`补两个回归函数，覆盖三个阶段×撤权/监督者丢失及三个阶段的ready阻塞。
原SMTP发送器已具有这些边界，直接复用，未重写邮件、队列、Auth或IPC协议。

邮件生产候选已实际存在：Core `email-production-candidate` + forwarder `smtp-production-candidate`，
原outbox4/worker5及七阶段claim fence；默认仍outbox3/worker4。
README补充真实状态及`email_revision`门禁，不把原“尚未实现”历史说明当当前事实；不改变B的打包/安装组合。

## 环境与精确正反例

WSL2 Ubuntu24.04，kernel `6.18.58.1-microsoft-standard-WSL2`；普通测试UID1000，Rust/Cargo1.98.0，
Node18.19.1，Go严格1.27.3 linux/amd64。所有Rust使用原Cargo.lock、`--locked --offline`；源码/TMPDIR/target在Linux原生文件系统。
源自Git归档的新目录，复用本人已有编译缓存，未读取主线私有材料、设备/NAS或生产数据。

负对照tree `4f63370b07e88a8f2f8bb30b3e9bd8edd4bc225a`与正向tree使用**相同最终测试blob**，只恢复旧生产文件。
分别执行：

```sh
cargo test -p rustsimhub-forwarder --locked --offline --lib delivery::tests::ready_fence_cancellation_prevents_the_next_connection_or_http_submit -- --exact --nocapture
cargo test -p rustsimhub-forwarder --locked --offline --lib delivery::tests::ready_fence_exhausting_absolute_budget_cannot_begin_the_next_io_stage -- --exact --nocapture
```

旧生产各运行1项并行为失败101，不是编译错误。每个函数在首个违规场景处停止，不冒称其所有负例组合都已逐一运行。
正向完整suite中的两个函数全部通过。负向归档SHA256 `54777261f4fae9fa949468f14a3a2737ba247a65df67c2788e5a960131f37034`。

## 实跑命令和结果（不同feature/重叠组不累加成独立总数）

| 命令/范围 | 退出码与真实结果 |
|---|---|
| `cargo fmt --all -- --check` | 0 |
| `cargo test -p rustsimhub-forwarder --locked --offline -- --nocapture` | 0；127通过、0失败、1原专项忽略；包括105 lib/4兼容/18 SMTP |
| 同命令加`--features smtp-production-candidate` | 0；129通过、0失败、1原专项忽略；包括108 lib/21 SMTP生产派发 |
| `cargo clippy -p rustsimhub-forwarder --all-targets --all-features --locked --offline -- -D warnings` | 0 |
| `cargo test -p rustsimhub-core -p rustsimhub-compat -p rustsimhub-storage -p rustsimhub-auth --locked --offline -- --nocapture` | 0；417通过、0失败、3原专项忽略 |
| `cargo test -p rustsimhub-core --features product-providers-candidate,email-production-candidate --locked --offline -- --nocapture` | 0；345通过、0失败、6原专项忽略 |
| Core同生产feature、`cargo clippy --all-targets --locked --offline -- -D warnings` | 0 |
| `cargo test -p rustsimhub-compat --features normal-nas-candidate --locked --offline -- --nocapture` | 0；58通过、0失败、1原worker入口忽略；含6项PCM/wire候选契约 |
| `node tools/test-web-forwarding.mjs`、`node tools/test-web-forwarding-history.mjs` | 均0；明确合成document，非真实Chrome三宽 |
| Go1.27.3，`GO111MODULE=off RSH_GOLDEN_FILE=<source>/crates/compat/src/legacy-golden.json go test -race -count=1 -timeout=120s -v ./tools/nas-adapter` | 修正环境后0；15个顶层测试通过，含真实Linux CLI/本机TLS及原Go签名向量 |
| 同Go，`go test -race -count=1 -timeout=120s -v ./tools/nas-adapter/control ./tools/nas-adapter/cmd/control` | 0；10个顶层测试通过；无测试CLI包不计数 |
| `GO111MODULE=off go vet ./tools/nas-adapter/...` | 0 |

SMTP为实际production sender+本机TCP/rustls，覆盖证书/hostname、AUTH能力、命令注入、MIME、七阶段真实queue claim、
拒绝/断线/DATA前后/超大回复/取消/迟到和Unknown不重试。HTTPS覆盖真实编码/认证/原ID、Webhook及Telegram/Bark响应错误、
429与Retry-After、限流/重定向/坏证书/有界正文、实际Owner撤权、队列Unknown及重启。不是公网送达或邮箱收件证明。

Core/compat的原忽略保持：恢复/健康Root专用UID、NKN Root/cgroup/独立进程专用夹具及导入子worker。
本批不冒称补跑所有忽略；导入父测试实际启动自己的worker，但worker入口仍标ignore，不计额外通过。
没有改受保护BLE/AirSIM/Native/Media/更新源码；Core原测试因已有依赖编译相关模块，不代表接管其实现或验收。

## 专用真实内核/进程检查

从同受测tree另行构建default/SMTP测试ELF：`cargo test -p rustsimhub-forwarder --lib --no-run --message-format=json --locked --offline`，
SMTP另加`--features smtp-production-candidate`；从Cargo真实artifact JSON选出唯一测试ELF，不猜最新文件名。

- 运行原`tools/test-forwarding-storage-full.py --test-binary <exact ELF>`，default/SMTP各0，原专项各实际1项通过。
  Root仅创建新mount/net命名空间中的2MiB tmpfs，实际测试为UID65534、空能力、NoNewPrivileges；真实ENOSPC后
  保持Unknown/游标，再释放空间仅恢复原逻辑。没有填满宿主磁盘。原普通suite的ignore标记没有删除。
- 同源码`cargo build -p rustsimhub-core -p rustsimhub-forwarder --features rustsimhub-core/dev-fixtures,rustsimhub-forwarder/dev-fixtures --locked --offline`，0。
  **这是显式测试程序，不是发行制品**，未进入打包或安装。
- 在新`unshare --net`中运行原`tools/test-forwarding-worker-ipc.py --core <exact Core>`，0；四种真实UID权限、
  grant/acquire/撤权/重启、disabled/Unknown保持。只停本夹具的Core及短命客户端，不是已安装systemd服务停止证明。
- 在另一个新net namespace中运行原`tools/test-forwarding-worker-process.py --core <exact Core> --worker <exact worker>`，0。
  confirmed/lost_ack/killed_after_post/wrong_route/wrong_ca五场景通过，全部实际worker回收；
  本机HTTP总请求数分别2/1/2/0/0（含不同新合成事件，不能说整个场景只发一次），原Unknown未重发。
  只在私有namespace启用lo，无真实DNS/接口/代理回退，不安装服务或改宿主网络/防火墙。

所有专用夹具均已正常退出，临时挂载/进程由原夹具清理；未执行设备部署、电话、短信、真实转发或链交易。

## 原始证据、失败和保留项

本机忽略目录 `.local/deliveries/OFFLINE-A-FORWARDER-a9fbeca.zip`，43项/105646字节，
SHA256 `cee3f126d1e726eb9408d92a7ea60d643c2f1a5b5d13d7201049e2e7e78260e1`，全部ZIP条目CRC校验通过。
含同tree代码、完整命令/退出码/测试计数、正反例、Go首轮失败与重测及真实ELF摘要。公开只交合成代码和脱敏报告。

首轮直接Go read suite因未设置README要求的`RSH_GOLDEN_FILE`而失败1；唯一失败为原签名向量文件路径为空。
未改任何断言/向量或产品行为；指定同tree已提交fixture后同`-race -count=1`命令15项通过。原失败日志与原组合summary的false保留，
不覆盖成“首次全部通过”。最终重测单独记录。云端CI需PR后独立回读，不把WSL通过写成云端通过。

未跑：ARM/QEMU、已安装systemd/cgroup关闭/冷启动、真实NAS/公网/推送账号/SMTP收件、设备并发性能、
完整旧NAS UI/未知旧PCM兼容、B的真实迁移/签名发行组合、A4实际主页/CSP/IndexedDB三宽。NAS缺口逐项见差距表。

## 集成、许可和下一步

B按PR当前准确完整SHA导出源码，原锁文件/feature/工具不变，运行上表命令；没有新增载荷、协议或Root安装字段。
回退可撤去本提交的HTTPS边界检查，但会恢复原ready回调竞态；不能延长预算来替代修复。
已有Go NAS/SMTP来源及许可沿用原报告/legacy-golden，新增修改AGPL-3.0-only，没有复制外部服务或将Go宣称为Rust。
AI辅助编写，已逐段检查真实调用及同字节正反例；不估造工时。未改成员作者、其他分支、主线进度或全局看板。
后续继续SMS/通话/网络/AP/LED逐项语义复验和A4真实装配，统计新Root操作等待具体接口审核。
整体已验收进度保持 **81.5%**。
