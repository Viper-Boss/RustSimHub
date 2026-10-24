# X1E：固定出口与独立转发执行程序

2026-10-24。状态：源码、本机真实进程和ARM64构建/启动元数据验证通过；未安装棒子、未访问实际推送目标。原P0–P7整体仍80%，新增X1单列。X1尚未完整交付。

## 已实现

- `crates/forwarder/src/egress.rs`：Root明确提供网卡名、ifindex、源IPv4/IPv6和至多三个DNS地址。每次创建新socket，实际设置并核对SO_BINDTODEVICE及ifindex，同时绑定源IP。配置错误、内核不支持或网卡变化直接失败，没有不绑定的连接或蜂窝默认路由回退。
- DNS不调用系统解析器或环境代理：固定Hickory 0.26.3解析有限帧，随机事务ID、响应/问题/记录数/长度核对；A/AAAA、有限CNAME及截断后同一服务器TCP查询。DNS总预算3秒，连接总预算6秒，HTTPS既有总预算10秒；没有缓存、后台解析任务或HTTP重试。DNS传输回退不代表重试HTTP。
- `worker.rs` / `main.rs`形成独立`rustsimhub-forwarder`程序，显式`--worker`前台运行。从至多64KiB的私有stdin读取配置；凭据不进argv或日志。实际非Root/真实与有效UID相同，CapEff/CapPrm/CapAmb均为0；CA包要求绝对路径、Root拥有、不可由组/其他用户写、普通文件、不可跟随末端符号链接，至多1MiB/512证书。没有更改系统信任。
- worker组合真实Core Fence、固定出口、rustls和Hyper，每个Claim只尝试一次。SIGTERM/SIGINT取消当前拥有的连接，提交有界回执后退出。未提交失败与已经尝试但未确认分别保留；失败或Unknown结束本轮worker，不盲目重发。
- Core只把进入状态机之前的锁竞争回答为`retry_later`。取得过任务后再次Acquire被拒绝，丢失回复/超时不能映射为Busy。worker只对明确的受理前Busy有界重查；重复查询回执也不重发HTTPS。
- dev-fixtures入口可以使用指定的合成HTTPS目标，与生产feature集隔离。CI增加实际worker进程场景。

## 验证

完整Linux命令：`cargo +1.98.0 test --workspace --all-features --locked --offline`，**416项通过**，比X1D增加12项；转发库51项。严格workspace Clippy和生产Core无默认feature检查通过。新增测试覆盖绑定出口、IPv4/IPv6、DNS/CNAME/UDP截断TCP、坏帧和超时关闭；真实Core锁竞争尚未消耗Claim、丢失回复不可重试。

实际多UID Core CLI测试通过，禁用后重开保持禁用/Unknown，未外发。新增程序测试运行在临时网络命名空间，只有回环网卡；Root/Core/Edge/worker使用不同真实UID，worker三组能力实际为0。临时CA、私钥、合成短信和凭据都在一次性目录，不安装证书、不创建系统账户/服务。

| 场景 | 真实HTTP请求 | 结果 |
| --- | ---: | --- |
| 正常DNS→TLS→Core回执 | 2 | 两个不同delivery ID，各204确认一次；通知不含完整合成正文 |
| 服务端收到请求但不回复 | 1 | 禁用并SIGTERM后服务端观察到连接关闭，保留Unknown，不重发 |
| 网卡名称与ifindex不匹配 | 0 | worker启动检查拒绝；没有DNS或不绑定的回退连接 |
| 使用另一个CA | 0 | 实际TLS握手失败；Core保留Failed，不发送HTTP或自动重复 |

各场景停止后都等待并回收真实worker/Core/测试服务进程。它证明一次性测试进程退出，**不证明尚未实现的已安装服务/触发器在OFF和冷启动时均无PID**；状态字段仍明确`process_state_verified:false`。

本机交叉编译生产Core、Edge、forwarder程序和转发库，四份制品均核对AArch64及SHA-256。QEMU执行实际ARM64 Core和forwarder的`--build-info`，两者均无开发夹具，主库schema6、worker协议4；没有把元数据启动当成ARM64真实转发或整机验收。制品未签名、未安装。

## 本机环境与失败记录

当前WSL主网络空间的独立Python探针显示IPv4 TCP设置SO_BINDTODEVICE(lo)后超时，而不绑定/仅绑定源IP可用；IPv6正常。代码未删除设备绑定。换到只有lo的临时命名空间后，IPv4/IPv6各组合及真实低权限socket测试均通过，完整回归也在该空间运行。

最初的取消测试等待没有截止，已只终止核对过的自有测试进程，补上准备/回收截止。另一轮Clippy发现测试模块位于生产项目之前，已调整。后续本地脚本末行CRLF导致最后一个构建参数错误，仅修正脚本换行后完成构建。进程测试第一轮的收尾管道未消费停止消息导致reset；修正夹具管道收尾后四场景全部通过。原始失败日志保留于.private，不覆盖为成功。

私有证据索引：`.private/forwarding-worker-process-checkpoint-20261024-first.json`。公开CI结果以精确源码SHA对应任务为准；本文件不预先宣称未完成的CI成功。

## 下一步

1. Root固定服务主管：取得当前网络政策、发放能力并启动受限worker；OFF持久落盘后撤销/停止，检查整个cgroup与所有启动触发源，明确停止失败。
2. 开机默认关闭/启用路径、过期或异常退出后的能力更替；独立、有界的未提交失败策略。Unknown永不自动重发。
3. Owner网页开关/有限规则/隐藏凭据/回执；主库与outbox一致性备份、正常签名应用组和Root服务安装。
4. 棒子上网卡绑定、实际服务停止及冷启动验收，真实推送目标需用户配置后单独测试。

本轮无设备/NAS访问、无SMS或电话、无蜂窝数据/PMIC/BLE/引导更改。单条10010/CXYE权限已经消费，不得据旧观测脚本重新发送。

实现参考：[Linux非Root首次绑定设备的变更](https://github.com/torvalds/linux/commit/c427bfec18f2190b8f4718785ee8ed2db4f84ee6)、[Hickory固定版本feature](https://github.com/hickory-dns/hickory-dns/blob/v0.26.3/crates/proto/Cargo.toml)。这些源码参考不能替代实际目标内核/进程权限验收。
