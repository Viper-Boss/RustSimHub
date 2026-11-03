# D25实际 worker 接线与 D26 故障包交付

后续：本文数字绑定原40600e2等历史来源，不覆盖原失败/边界。[正常Core信令Host交付](member-d-normal-remote-host.md)现已补齐D25离线实现，最新累计24/26、剩D22/D23；生产准入与主线/设备验收独立保留，固定80%不变。

2026-11-02。原 Core 媒体协议、真实Core Client/Authority/Permit已接新显式worker v2，强制中继/实际DTLS/Opus、来源断开撤权和唯一PCM线程排空均运行。**D26离线输入/联合故障包本轮提交完成，累计23/26离线阶段已提交，剩D22/D23/D25三项；固定PROGRESS-SCALE仍80%**。不等于生产安全准入、主线审核或设备验收，不改全局台账或分母。

用户已批准#56/#132离线实施。起点#170 `3b1598f2e74fb63064b0f046b1111db2b4c06d1b`；产品实现提交`d5e5c14c359b9aa406ad3ddd060143c65ba4c44d`；实际受测native/ARM/手动来源完整源码 **`40600e2b45d203c815fc9a34ac3e0c80dd2296b5`**，tracked_dirty=false。之后只有本文、SEC证据/复跑说明和隔离负例脚本，无生产Rust/lock变化。为保持同一公共native入口只在一个PR，追加回原#170，不另开重叠入口PR；最终推送完整HEAD见PR正文。

[实现与协议](../remote/worker-v2.md) · [D26矩阵](../remote/member-d-sec06-matrix-20261102.md) · [实际源码/产物/日志机器证据](../remote/member-d-sec06-evidence.json) · [复跑](../../native/audio-alsa/tests/fixtures/README.remote-worker.md)。只改本人media授权/远程适配、native独立模块/feature入口及夹具/文档。原LAN/v1、正常电话/短信/身份、Root服务材料和其他成员实现保持；Native lock/版本没有进一步改变，media仅追加原已有rustix的net feature。原AGPL及SDK上游许可/作者保留，Codex辅助，未改Git作者。

## 完整结果与边界

- 整仓all-features：**1446通过/0失败/6继承ignore**，严格Clippy通过，源码d5e5c14；40600e2的Root crate/lock完全相同。Auth118、BLE171、Core库84/主程序196、media36、wire21都在其中，不重复相加。六个继承专用环境未在此声称已执行。
- 40600e2：Native默认**34/0/0**，feature **42/0/0**（lib31/bin11）；普通Native全目标与两个显式manual目标严格Clippy、Root/native fmt/diff通过。42包含复用source-file unit，不当作42项新增工作。
- 新私有worker矩阵：x86 **38/0/1**。38=23个复用SDK函数+5个源码内unit+10个新增worker目标用例（其中有一个入口拒绝检查）；Root media另新增8个函数（7协议/运输+1live-clock），Native两项新生命周期unit已与标准suite重复，不把重复累计成阶段。
- 原Core单次票据、boot/line/call/revision/lease-generation/principal/generation/peer/scopes/active/expiry/失去Core等13类撤权、错误ticket/boot/peer、实际正常TLS/Coturn/UDP/selected pair/证书/DTLS/双向Opus、取消Prepare的真实pending TLS fd、控制EOF/未读pipeline、进程停机/迟到PCM排空全部通过。Stop只关闭媒体、不改原call/lease；全程不返回release_verified。
- 专用ignored角色入口已由同一runner显式执行：实际Core986/Media984/Edge985（各自CapEff为空）的双向媒体和原Authority撤权、空闲时非法984/986/987来源拒绝；真实非测试CLI另外核了Root拥有合成NSS、固定/run端点/0660、loopback登记拒绝、985解析/unknown-handle拒绝、SIGTERM原inode移除。不是把ignore直接算过。
- 重新实际跑本人Core/BLE联合目标：Core来源1个libtest/13场景、Core应用1个libtest/16场景、BLE Core端口12场景、原GattBackendV1/固定D-Bus policy/真实FD的3角色全部通过，均40600e2/clean。Verifier/Sealer只有cfg(test)注入，缺生产provider仍拒绝，不证明A密码互操作。
- 同一干净40600e2 tracked archive **AArch64 release** 实际交叉构建（Rust1.98/GCC Cortex-A53/真实ARM ALSA与Opus），真正AArch64 ELF **8,534,824字节**，QEMU Cortex-A53同一**38/0/1**通过。这个1项UID入口只在x86真实UID容器执行，不作ARM三角色、整板/物理PCM或生产cgroup预算证明。

x86与ARM普通私有矩阵均network=none/read-only/UID1000/caps空/**128MiB/16pids/2CPU**。三角色夹具Root控制器只持SETUID/SETGID/CHOWN/FOWNER、无CAP_KILL，清理helper先降成子进程同UID；NSS只在临时镜像，无host账户/UID占用或设备安装。Core/BLE旧来源夹具使用原256MiB/32pids，Gatt/policy使用原128MiB/16pids，均仅容器和外部BlueZ/NM/业务Context替代。

x86 fixture image `sha256:cc7086fb6a0e15d17b091decd3ee96e320055059ba77f85ebd8d6739aca8d7e2`；worker测试ELF SHA `849a75826b27a10cf181e38ae32a2c4d7ae74f76946c71101806be5c8a8712a9`、非测试binary `e39bd45988af8f9b81641bef1b9cedd8ff03cb9657da585f60d86024318e7882`；矩阵日志 `d93e17ee4100dab25ccc70c27f425f5b28d6420a9bf0129117c2e4f8516c40b2`、UID/CLI日志 `88e0ab199091181a64f6f593bdf66dd7c93bec74794ac04ff28a76ca90780723`。

ARM archive SHA `2405101c64b0ee307309a7645a49f699eabc51fd6ced1610ad13e0772280ad3e`；ELF `a96675a23113f1eb637c587e6ad7646a5f701bdeddf62abe25e7734e2d222b5c`，QEMU image `sha256:167dce679c9a29712c08138a32c4bd470963c388aac9aa18fc44c3e51d0e7389`，日志 `1e82148d91b95d947608f11505c31f32d29dc0d9fb841a5603d5f5b9bff018fd`，构建日志 `336f87852685f144f8e5cbd5e715a0d40b399cdbab6c405d938f95ef97948ef4`。完整CLI、其余binary/runner/log SHA见机器证据；合成日志不冒充生产数据或现场实耗。

## 保留失败与最小复现

首轮d5e5c14完整私有矩阵**37/1/1**，nonce/refresh用例失败，日志SHA `00e862934499fb21d99edf8c9b5939af37b4800ebf438287e27d27bc95cd7abb`。固定库trace证实：438后构造新permission重试，50ms发送cadence尚未到；原固定60ms总窗口可能先推进30s测试时钟，使新请求在实际发送前超时，正常success随后按原Ignored拒绝。两个短idle观察也未跨过cadence，保持断言失败。

仅cfg(test)记录实际请求/响应opaque id与I/O代次；无待响应且100ms无I/O（超过固定50ms cadence）后才准推进协议时钟，总等待1秒；真实nonce等待期间继续实际service。保留35×30s推进、真实438、Refresh success/后续转发、601s过期拒绝。产品Allocate/Bind/I/O/凭据/Core/帧期限、固定库与原拒绝不改；临时直接tracing诊断入口和manifest/lock变化已撤除。修后完整x86/ARM矩阵均通过，不以单项代替全套。

新live-clock入口的最小隔离负例仅去掉I/O后取时，保留真实Pump与原断言，稳定exit101/1失败（日志SHA `24f88222c48de93d652fa4a09a0ecb3381024f736dea75965bdff8d313471d00`）。正常Pump SHA `7483ec9948f31f0192ff926bf178f8597d20fdfc76c435e366d12d17efeb7123`；仅注入后的SHA `6f320dc99697ba5e79834f7d5689210dbf8beb7db8ff69e832678d3d2476689a`。复现脚本只改私有git archive，不动工作树；这是初始远程候选沿用固定step时间的反例，不捏造旧生产漏洞。正例已在同源整仓执行。

## 剩余三项与未验收

D22的Unix来源/typed能力、D23的唯一Core Wi-Fi actor/持久Unknown/应用主机已有独立交付及本轮真实来源验证，仍缺A生产安全会话、正常pin/签名域/Verifier/Sealer装配与真实产品互操作。最新main `9b24d43dcc74a7d8ff7f28e4aab005bec65c0ed3` 的A27仍明确crypto_wire_frozen=false，独立Noise评估不代替它；不跨包实现A密码或用测试provider进入生产。

D25的实际SDK+worker/原Core媒体协议接线已交，但完整正常身份/媒体preflight→远程信令/消费端→Core/worker链仍未交付，Root登记/服务安装及最终准入由主线；原DTLS `crypto-bigint0.5.5` 的RUSTSEC-2026-0387仍需兼容修复/密码审查，不能用新TLS ring或通过测试把production_admitted置true。Scope/Owner/主线分工保持，三项不冒充完成。

真实Core daemon业务/电话状态到远程host、生产Root/NSS/cgroup/BlueZ/NM/HCI、目标板PCM、外网TURN/蜂窝/两端听音均未运行。无设备、拨号/短信/APN、签名/部署，未代替主线审核/合并。CI最终HEAD另回读；旧#170 run41325796758因Actions付款/额度根本未启动，保留失败，不改workflow或宣称绿。
