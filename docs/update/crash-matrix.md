# C14 磁盘写满、异常退出与兼容/回执矩阵

本包沿用已合并的[C13/C14提案](failure-matrix-proposal.md)，在C13代码之后独立提交。
基线main `5238670246e7c6e43184534a7a6ee4b13c293ffd`。只调用真实公开API，未新增生产hook。
执行结果、完整源码标识和未验收项见[成员报告](../deliveries/member-c-nas.md)。

## 普通用户测试

```sh
python tools/fixtures/backup-recovery/wsl_test.py --run --suite update --ref HEAD
```

Windows入口将固定Git源码放到WSL2 Linux原生目录，以普通UID执行C13回归、C14四项普通测试、
更新器包测试、fmt和包all-targets严格Clippy。去掉--run只列计划。

Linux原生隔离源码中可单独运行：

```sh
python3 tools/test-update-failure-matrix.py --run --suite c14
```

驱动核对四个完整函数名及实际通过集合，零失败/零忽略；固定排除tmpfs专项和仅供父测试调用的worker。
默认不带--run只列计划，不发送请求。C13默认suite保持不变。

| 测试 | 真实动作 | 核对结果 |
|---|---|---|
| stage完成后SIGKILL | 子进程原Store成功stage，报告phase/hash并阻塞；父进程验证握手、精确子PID后kill/wait | 磁盘candidate未尝试；重开第一次choose候选、再次重开回退原组，floor2保持 |
| choose后未confirm即SIGKILL | 子进程原choose已成功返回路径、attempt已落盘，再握手终止 | 重开回退原组、清候选，原组内容与floor2保持；不增加尝试预算 |
| 签名/载荷/schema拒绝 | 分别使用异key签名、正确签名但坏payload、正确签名schema99的包 | 各自Signature/Integrity/Schema拒绝；未创建候选、不推进floor，原台账/组字节保持，重开选原组 |
| 真实outbox与回执 | 原Queue公开API创建合成probe任务并claim/finish为Unknown；真实inspect_outbox观察；把真实库构造成既有格式2，再按实际版本调用原check_upgrade | 投影到3后旧fallback被unsupported_outbox拒绝，authorization_verified=false；比较前后数据库/目录不变；正常Queue重开迁移后原回执、payload、attempt、游标保持，显式恢复合成策略后也不重领Unknown |

前两项、签名拒绝覆盖应用包format1/2。outbox用例读取真实业务Store schema与实际outbox版本，
程序metadata是明确合成的比较输入，不冒称真实签名程序的build-info或Root/数据锁准入验证。
该用例对格式2的构造复用主线既有办法（移除history_batches并设user_version=2），不是通过手写JSON替代真实数据库。

子进程只打开本测试临时根；stdout最多读取16行，握手有5秒预算，父级驱动总预算900秒。
Worker持有stdin管道等待，父进程收到原API完成消息后才发送SIGKILL，wait确认实际signal9。
所有者Drop也会终止并回收子进程；没有随机sleep猜中断点、按进程名杀任务或真实payload执行。
worker在普通Cargo日志中标ignored，但由两个父测试实际调用；不是用ignored隐藏业务场景。

## 单列2MiB tmpfs专项

先在同一固定源码的WSL普通用户目录生成测试产物清单（不运行Root测试）：

```sh
cargo test -p rustsimhub-update --test crash_matrix --no-run --locked --offline --message-format=json > /private/artifacts.jsonl
```

再由测试环境Root执行有限隔离准备器，路径必须指向上述实际清单：

```sh
python3 -B tools/fixtures/backup-recovery/update_full.py --cargo-artifacts /private/artifacts.jsonl
```

Root只准备新mount/net命名空间和传播private、恰好2MiB的tmpfs，nosuid/nodev/noexec。
复用主线已有tools/test-forwarding-storage-full.py的ELF检查与降权方法；实际测试以UID65534、
空附加组、有效/许可/ambient能力全零、NoNewPrivileges=1运行。单文件/CPU/进程时限保留已有有限预算。
普通用户WSL入口不隐式提权；本成员实际专项通过WSL root入口显式单列执行，结果与普通日志分开。

Rust专项在任何填充之前核对真实路径、tmpfs类型、容量、UID/能力；拒绝普通目录或错误容量。
先真实安装A1，filler每次最多64KiB、累计不超过核定容量，必须实际观察errno28和可用空间0。
随后原stage失败为Io，原state/A1内容/floor保持；保存可能存在的半成品目录，释放的只有filler，
原Store重开仍选择A1。另一独立可写临时根完成stage/choose对照，不能通过删失败目录后重试来通过。
两种签名包格式分别执行。API把底层错误折叠为Io，errno28归属filler，不谎称stage返回具体errno。

每次挂载和卸载有10秒预算，降权测试30秒、命名空间父级60秒；超时终止精确进程组并等待。
正常路径finally卸载成功才返回0；不修改宿主挂载、网络、设备或生产目录，不填宿主盘。
入口只接受有限测试ELF/产物清单，不是产品任意执行接口。失败退出码和原日志保留；
目录/台账现场在重开和临时目录清理之前核对，tmpfs最终卸载，证据包只含脱敏日志/元数据，不保存数据库副本。

## 未验收的边界

- 本包验证原stage/choose返回后的进程终止；persist内部write/sync/rename精确中断点没有新增接入，也没有据此验收。
- SIGKILL保留内核页缓存，tmpfs不是真实eMMC；不证明硬件掉电、写缓存或fsync的断电持久保证。
- 兼容比较不授予Root维护权限，不执行整机安装，不证明真实ARM/设备恢复。未改签名/schema/生产状态机。
- 两个既有Root metadata专项本轮未运行；Cargo中的tmpfs ignored需看独立专项结果，不能混改原统计。
- 合成请求使用example.invalid且不连接网络，不发送真实短信/电话；数据、密钥、ELF和数据库不进入共享证据包。

项目AGPL-3.0-only及复用源SPDX保留，无新依赖。旧C13结果按原固定源码保留，新增参数/重跑不增加项目权重。
