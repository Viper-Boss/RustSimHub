# X1X · 转发错误与容量矩阵（待固定提交CI）

2026-10-27，主线FWD-15。SQLite容量/页满、实际OS级ENOSPC、HTTP错误和真实强杀重开均已本机隔离验证；仍需本次固定提交的完整CI，不提前计完成。不访问NAS/棒子或真实提供商，不开启普通数据。

## 容量和事务失败

| 情形 | 实际验证 | 结果 |
|---|---|---|
| 256个未决上限 | 实际ingest和SQLite事务 | Full时游标不前进，不挤掉旧Unknown；既有回执保留 |
| 4096个回执上限 | 一个实际领取并标Unknown的任务，事务填入4095个已确认合成历史回执；通过真实ingest再入队 | Full，游标/队列不前进；Owner明确prune仅删除4095个终态，Unknown及原请求保留；腾位后同一未消费事件可正常入队 |
| SQLite无法分配页面 | 临时真实DB的max_page_count收紧到实际page_count且无空闲页；额外DDL实际返回SQLite DiskFull | 大正文通过真实ingest返回Storage；游标、旧Unknown/原请求不变，无新queued；恢复8192页预算后同一事件可入队 |
| 实际文件系统ENOSPC | 新mount/net命名空间，容量确认为2097152字节的私有tmpfs；真实Queue、合成旧Unknown和完整请求；64KiB固定块占位至实际errno28 | 可用空间0；新ingest返回Storage，游标不前进、新任务不入队，旧Unknown及请求原字节不丢；释放占位后同一事件只入队一次、只领取一次并确认，旧Unknown继续保留 |
| 多目标原子性 | 接收方插入或游标更新实际SQLite失败 | 整个fanout和游标事务回滚；不部分发布 |

工具为`tools/test-forwarding-storage-full.py`，Rust用例`queue::tests::real_filesystem_enospc_preserves_unknown_and_cursor_then_recovers_once`默认ignore，只能由这个明确的隔离步骤执行。工具Root仅建立私有mount/net空间、2MiB tmpfs并降权；实际测试UID65534、CapEff/CapPrm/CapAmb均0、NoNewPrivs=1、CPU/文件上限有界。Rust再次核实路径、tmpfs类型和1–2MiB容量之后才写占位文件；不会填满宿主、NAS或棒子的文件系统。开发测试ELF复制在宿主临时目录，是普通有界开发写入；不宣称宿主完全零写入。

Cargo JSONL最多读取2MiB+1，只选择唯一forwarder库测试ELF，拒绝脚本、目录、超256MiB、组/其他人可写及多硬链接文件。五项准入测试实际通过，直接在原命名空间调用child也被拒绝。拒绝不是降级到普通目录测试。CI新增单独的实际ENOSPC步骤，不把默认测试中的ignored当通过。

## 传输错误和资源边界

| 情形 | 实际覆盖 | 确认/重试语义 |
|---|---|---|
| HTTP错误 | 本机TCP/rustls/Hyper的400/401/403/404/408/413/429/500/502/503/504，携带Retry-After:3600 | 已提交后全部Unknown/Unconfirmed，保留实际status；每例一条连接、一个完整请求，不重试或跟随 |
| 业务错误/重定向 | 提供商JSON错误，302及服务端错误 | 不能把HTTP 200中的业务失败认作确认；不跟随重定向或自动重发 |
| 超长响应 | 固定长度/分块正文、过量或过长响应头、压缩输入 | 正文8192字节、头部8192字节/32个有界；不解压，不复制不可信正文/密钥进日志 |
| TLS/出口失败 | 不可信CA、主机名错误、TLS中取消、错误网卡ifindex，实际socket | 提交前NotSubmitted/Failed，无HTTP，无不绑定出口或蜂窝回退 |
| 权限/超时/主管丢失 | 三阶段真实Core fence，撤销Owner、截止和关闭连接 | 提交前NotSubmitted，进入唯一submit后丢回执或取消为Unknown；不以结果不明作为重投依据 |

每个worker只有一个活动Claim和HTTPS尝试；目标8、规则32、未决256、回执4096，DNS帧4096字节/地址8个，请求32768字节、响应8192字节、总HTTPS截止10秒。现有有限Busy重查只用于明确未受理的Core锁竞争，不是HTTP重发。此处证明代码预算和对应故障路径，不冒充棒子长期内存/性能实测。

## 真实进程强杀和恢复

运行`sudo -n unshare --net python3 tools/test-forwarding-worker-process.py --core target/debug/rustsimhub-core --worker target/debug/rustsimhub-forwarder`。实际Core的Auth/Store/IPC和生产worker，临时CA/DNS/回环TLS、不同真实UID，不装证书/账户/持久服务。

| 场景 | 实际HTTP请求 | 结果 |
|---|---:|---|
| 正常确认 | 2 | 两条不同ID各确认一次 |
| 已收请求、无回执，随后OFF/SIGTERM | 1 | 连接关闭，Unknown1/Confirmed0，无重发 |
| 已收请求、无回执，SIGKILL worker和Core，再重开同一目录并发新lease给替换worker | 2个不同ID | 第一条保留Unknown；原第二条未发送任务正常确认；不重播第一条，不重新播种短信；清理后真实worker已回收 |
| 错误网卡ifindex | 0 | 无DNS/HTTP，拒绝错误出口 |
| 不可信CA | 0 | 一次TLS失败，无HTTP |

强杀用例明确等待第一个完整请求到达接收端，然后分别确认两个进程返回-SIGKILL；重开`new:false`、Unknown1/Queued1及原两条回执，才启动替换worker。TLS夹具拒绝任何重复delivery ID。最终Unknown1/Confirmed1，两个不同请求恰好各一次。此脚本仍不是已安装Root主管的冷启动证明；其状态继续明确`installed_service_verified:false`。

原有`tools/test-forwarding-supervisor.py`另覆盖真实systemd凭据元数据反例/IPC正对照、OFF不启动worker、重复请求不重启健康PID、整个cgroup包含顽固子进程的有界SIGKILL、Owner管理/一次合成probe收尾和凭据删除。最终固定CI必须实际通过这些步骤才能用于本项验收。用户参与的实际冷启动和真实提供商覆盖仍属于FWD-18/19及其他动作，不转成已验收。

## 验证截面和失败保留

- 最初三项容量/TLS新增截面728文件，SHA-256 `a0a1e7931fac9b2c452f4164602957a69b7fb8d305194bc20c4716eed6b90602`；Core157、forwarder78、Wire18共253项，严格Clippy、fmt及生产检查通过。固定`3231adc6545aeb6ca21e6e0ce472a68a524f9969`的[完整CI40418731070](https://github.com/Viper-Boss/RustSimHub/actions/runs/40418731070)两个任务均success。
- OS ENOSPC用例及普通三包测试在730文件`61e68453672abf56137a7da760ff16c084a5f2d84492758c46a5f366675a11db`截面验证：253项普通测试通过、另1项ignored；实际专用tmpfs执行该ignored用例1项通过，不能将两者混算。实际测试ELF SHA-256 `f66e3563cc665b827c74a68d6bd6142f26fbf3874cfba6754e6cc0bb0ed8959f`。
- 最终进程/工具截面731文件`1ca129d23210bfe289927f90ffb48c25a002c526b4518101bf599034bd1000eb`，只读源码和依赖、非Root/空能力/NoNewPrivileges、私有网络与设备的构建通过；五项准入和fmt通过。真实五种进程场景及最新工具执行真实ENOSPC均通过。
- 私有证据：`.private/fwd-enospc-20261027-first/`、`.private/fwd-enospc-harness-20261027-second/`、`.private/fwd-hardkill-20261027-second/`。仅公开汇总与源码哈希，不公开凭据或原始通信资料。
- 首个构建成功后，下一次WSL调用中原运行目录的测试ELF已不可用；该次工具调用在挂载前拒绝，未算通过。后改用持久的有界开发制品目录并重新构建库测试ELF，实际ENOSPC成功。
- 首版强杀后只核对Core重开通过；加强为替换worker恢复后，首次未等到第二条请求。源码核对发现`Store::open`会使线路观测失效，但dev-fixtures重开只对新Owner观察SIM。修正仅开发夹具：每次启动重新观察同一合成SIM，只有首次播种两条合成短信；生产观察器/授权/重试/队列不改。修正后实际强杀、重开、新lease、新worker和剩余未发送任务确认通过。原失败日志保留，未靠延长截止或删除断言掩盖。

本轮只改测试、CI和开发feature夹具，生产转发行为未变；尚未新部署或重新宣称所有生产ARM制品属于本次源码。设备最后确认的应用50、普通数据/转发OFF保持，无新短信/通话/推送、关机、分区或成员任务改动。原P0–P7仍80%，台账13/169、加权6.4%，FWD-15等待本次固定CI后再结项。
