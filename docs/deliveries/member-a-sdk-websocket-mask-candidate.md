# A16：WebSocket真实掩码、通告版本矛盾与上游回移候选

账号wnfhsj88，2026-11-01；从最新main
`3baae0294a8aa611542fb11a0e976fd327457fb4` 新建
`codex/wnfhsj88-sdk-websocket-security-20261101`。已合并PR144/153/157/160/161
均承接，不重做或改其实现。实现/完整受测源码 **`6f4bff255ec2b970d237192574bca09387777dcd`**；最终run
candidate_sources_dirty=false、首尾Git与全部执行输入相同。之后只补本人报告/
公开来源摘要/索引，最终完整PR HEAD见PR正文。未修改全局进度或他人代码。

## 实际发现，而非只看扫描绿灯

GO-2026-6278的Go/OSV记录写websocket v1.5.3已修复，但官方Go sum校验后的
v1.5.3源码仍以math/rand.Uint32生成client mask。tag提交
`ce903f6d1d961af3a8602f2842c8b1c3fca58c4d` 是2024-06-14，通告引用真正修复
`d67f41855da42d7bccd9ef050c49f7e54e783b95` 是2024-07-01。因此直接升级到tag
并看到不再告警，不能证明这个行为已修。固定三版的官方sum/zip/conn.go/许可证
摘要与可取得Git来源见versions.json/公开摘要；原v1.4.2无Origin元数据如实保留。

本包新增一个真实测试：Gorilla客户端经自己创建的net.Pipe完成正常握手，写出
16帧synthetic文本；另一端实际读出masked标志/掩码/解码内容。只在这个独立测试
进程设置GODEBUG=randseednop=0，并先核验Seed控制生效；未修改SDK回归/产品的
运行设置。v1.4.2和v1.5.3的16帧均精确匹配已知math/rand序列，候选不匹配。
这证明所选实际掩码来源，不是随机性统计审计、生产利用或所有环境的熵故障保证。
一个测试三版运行不算三个新测试；原版两项通过表示确认问题，不表示安全。

Govulncheck1.9.0对真正SDK根包分析，原版symbol call chain为NewClient→connect→
connectToNode→websocket.DialContext等；nominal tag不再报告该ID，但真实帧仍有
问题。工具拒绝不含SDK/WebSocket/x/crypto闭包的源扫描，正确归一化local replace
身份，不能对只含stdlib的纯测试wrapper空结果报“安全”。源码静态可达、二进制
符号存在、真实合成帧观察和生产利用是不同证据，本包不混写。

## 66行上游回移候选及兼容边界

直接固定修复commit对应pseudo v1.5.4-0.20240701034025-d67f41855da4还要求
x/net v0.26.0且编译proxy。本包未选这条额外依赖路径；在新work阶段复制官方
v1.5.3全51文件，以66行diff回移上游conn.go及prepared_test.go两个文件。
真实maskRand/newMaskKey代码逐字与固定修复commit比较；保留tag其他代码、
旧自含proxy和全部原BSD通知，不从后续提交顺带拿未审改动。

补丁执行前后核对digest/两个目标/原SHA、fuzz=0、完整51文件tree；其余49源逐字
不变，最终registry cache原tree也保持。只有自己的新派生目录调整read-only副本
权限；不改cache权限/源码。生成go.mod只将websocket v1.4.2选为v1.5.3，并显式
replace到派生目录；**不把修改后的字节冒充官方v1.5.3 sum认证**。根产品依赖未改。

上游仍忽略io.ReadFull返回错误，本包原样保留并明确供审；没有自行发明熵错误
wrapper、wiping或安全保证。标准库crypto/rand掩码来源变更不改变应用加密/签名域/
grant/NKN消息/Root协议，不能据此冻结A27应用密码方案或批准生产启用。

## 真实验证

| 检查 | 结果 |
| --- | --- |
| 默认plan | execute=false/network=false，不构建/开监听/创建stage |
| 三版相同真实帧来源测试 | 各1通过；旧版/tag确认可预测，backport不匹配受控PRNG |
| 三版相同SDK生命周期/配置/RPC七项，go test -race | 各7通过/0失败；原PR157五项+PR161两项，不重复计本人新增 |
| 候选原作者SDK/NCP路由，go test -race | 4通过，原作者保留 |
| 候选Gorilla原TestPreparedMessage，go test -race | 1通过，使用原上游修复测试，不算本人新增 |
| 每版go mod verify；候选Go vet、本人fixture/生产patch gofmt | 退出0；gofmt无输出，上游测试原格式保留 |
| native/ARM64闭包与各自baseline对比 | 模块集合完全相同，除websocket外版本/sum/许可逐项相同；无Pion/x/net/api-webrtc |
| native/ARM64实际编译闭包 | 16/15模块、304/302包；ARM不含x/sys，不能套native固定16断言 |
| native/ARM64测试probe构建 | ELF62/183核对；native制品实际重跑7项及新帧测试，全部通过；ARM未执行 |
| 源码扫描 | 原版实际调用链匹配GO-2026-6278；tag/候选无该symbol报告，但不以此单独认证修复 |
| Python编译、git diff --check | 退出0 |

原x/crypto受影响ssh/openpgp等包未进入此Linux闭包，x/sys匹配的Windows
NewNTUnicodeString也未编译；模块通告不擦除，其他平台/API/后续特性仍需复核。
本次没有批量升级x/crypto/x/sys或称所有依赖安全通过。

实际Go1.27.3。额外三个公开固定module/官方scanner1.9.0只在显式准备阶段下载并
校验；工具版本/sum、许可证摘要与编译后scanner hash仅作工具来源，不是产品依赖。
执行时SDK构建/测试GOPROXY=off/GOSUMDB=off/mod=readonly；精确版本的cache lookup
也禁下载，无静默新模块。在线scanner只在明确--online-scans且提供工具路径时运行，
查询公开module/version。全量输入hash及scans/制品/模块许可在公开JSON中。

```sh
python3 tools/nkn-build/sdk-websocket-security/evaluate.py --run \
  --source-cache "$PWD/work/a13-eval/upstream" \
  --output "$PWD/work/a13-security/run-final" --online-scans \
  --govulncheck /workspace/toolchains/security-eval-1.9/bin/govulncheck
```

需先设置README列出的cache/toolchain变量。重跑用NEW output。所有positive socket
仅自己pipe/本机HTTP+WebSocket，其他拨号在DNS/socket前拒绝，无公网NKN/RPC/STUN/
链/交易；每个子进程有界，超时只回收自身新进程组，IsClosed不代替无PID/cgroup。

| 实际测试probe | 字节 | SHA256 |
| --- | --- | --- |
| amd64 | 19291190 | `97ac51ba59fc40ae8d435f3be185c1dd351d14e4ab361bf43628d6dd528e89ad` |
| arm64 | 17992019 | `e0c66e3405c4d1f71b62506900deeeff0ce9c3cc3c4eb0c3c07f1d3ce13eaea8` |

这不是完整stdio引擎、空缓存独立可复现发行、生产签名或安装；ARM只编译。
-buildvcs=false避免生成stage错误继承父Git。完整来源/日志在忽略work阶段，不提交
vendor/第三方源码/二进制/cache。日志实际摘要如下：

| work/a13-security/run-final日志 | SHA256 |
| --- | --- |
| report.json | `8e16d4f18f93e12515eca6f85f180efab9b5f2fd8d77afcce4f12cb2be4d4175` |
| baseline-mask-test.log | `1781ba446dcfeb585939bb9c755f2b6ebf057b91d21f17f897dcc53dd3948655` |
| nominal-mask-test.log | `54bb3037b3c74068feda75a83f8a61a4df2ddc19b1cbaf7dead5cfc22b34289c` |
| backport-mask-test.log | `1781ba446dcfeb585939bb9c755f2b6ebf057b91d21f17f897dcc53dd3948655` |
| backport-sdk-tests.log | `b9e2537334b1cb6a0711aaf74239df841c33bf8698a11c4ca5afb8b31aa3c218` |
| baseline-source-scan.log | `ec3d2d70935f06c5408dd6e47f8a18a79383cc44cb0114c3a72201166a4e38b7` |
| nominal-source-scan.log | `9654d45c9fda46d2a79458e8c27aa1c5e8eedecf752d8a2fa5925297ab06cc8b` |
| backport-source-scan.log | `edcdcff6741191310082b1482e6f5b1ec36813a59d76d8f0da0ddfa4ef3a9984` |
| upstream-prepared-test.log | `79bb862d9709642b5de70014e3199abb166367db356aedc06353bcf7dcfe24a3` |
| native-mask-test.log | `890cbfaa5d90e15777675ea68603f4e2494e57589c4f7f341bab2b58eac67ffc` |

## 保留的失败与外部边界

探索时registry副本继承read-only目录导致patch无法建临时文件；改为仅使自己副本
可写，未动cache。随后scan闭包验证未正确识别local replace名字而拒绝，修为
精确已知替换归一化，继续要求实际SDK根/三个关键模块。一次硬编码ARM也有16
模块而失败，改为逐架构真实baseline集合/非目标版本逐项相同；实际ARM15为x/sys
未选，并没有放开Pion/x/net断言。最终干净SHA整套再次通过，前述失败不改称成功。

早期govulncheck1.1.4在Go1.27源码SSA panic，升级官方1.9.0后真实SDK根分析有效。
纯测试wrapper源分析只有wrapper/stdlib闭包的空结果未采信；旧二进制symbol观察
只记符号，不冒充函数调用链。上述探索日志保留在work/a13-minimal/reachability。

已合并PR160最终CI41295345019两job均success；PR161最终CI41295821590 native
success、test失败于真实forwarding worker读取回执被Core拒绝。原workspace Test
已过，后续production check/Lint跳过；根因未确定，不说已修或全绿，已补其PR正文。
这些是历史精确HEAD结果，不挪用到本包。当前最终HEAD远端另行回读。

本包供负责人具体审核：66行backport、版本矛盾、1MiB RPC旧兼容选择及两层旧SDK
补丁仍不是生产批准。原tools/nkn-engine继续无SDK/unavailable。A13～16真实backend/
进程/两架构完整发行，A22可信状态，A25独立身份/Root/固定出口/cgroup，A26普通
Core桥/安全会话，A27应用绑定/密码方案，A29生产兼容/Fence/queue及A30组合仍缺；
剩余9步不标全部完成，不假造真实provider或沿用BLE恢复权限。

未改根Cargo/lock、crates、configurator、firmware、Actions、产品wire/Auth/Root/
默认服务/他人任务/全局台账。无生产设备/NAS/真实身份/蜂窝/邮件/电话/短信/
交易/签名/部署，不自行合并。新工具/测试AGPL-3.0-only，原SDK/NCP、Gorilla BSD、
Go工具许可和作者保留；OpenAI Codex辅助，无工时估造。已验收81.5%、20/169保持。
