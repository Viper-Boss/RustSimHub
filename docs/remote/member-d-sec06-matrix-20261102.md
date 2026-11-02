# D26：实际新增输入与联合故障离线矩阵

本轮完成D26**测试包提交范围**，不是生产安全验收。累计23/26离线阶段已提交，剩D22/D23/D25；固定PROGRESS-SCALE的80%不变，不修改主线台账/其他成员分工。完整源码与公开脱敏机器证据见[证据清单](member-d-sec06-evidence.json)，新远程协议见[worker v2](worker-v2.md)。

实际受测native/手动/Core来源源码为`40600e2b45d203c815fc9a34ac3e0c80dd2296b5`，clean；整仓1446/0/6及严格Clippy在`d5e5c14c359b9aa406ad3ddd060143c65ba4c44d`，两者Root crate/lock完全相同，40600e2只追加native测试观察/夹具修正。之后的复现脚本/文档不改变生产Rust。标准Native默认34/0/0、显式feature42/0/0（lib31/bin11）；Root Auth118、BLE171、Core库84/主程序196、media36、wire21均在整仓真实执行，不能把子集再加进1446。

| 边界/故障 | 实际模块与执行 | 真实结果及限制 |
|---|---|---|
| 恢复身份/授权/迁移、旧realm/代次/撤销 | Auth/Storage/wire/Core已有实际生产模块；整仓all-features | 持久Unknown、旧scope、单次批准/票据与撤销断言保持；不是新Owner或匿名USB/BLE许可 |
| 来源伪造、重复/乱序、单槽、Drop/Close/重连 | Core recovery_transport/channel，真实临时Unix socket与不同UID | 一个libtest、13组实际场景；Core1001/peer1002，Root/Core及1003～1006拒绝；正常Verifier只有cfg(test)注入，缺provider生产路径仍拒绝 |
| 原应用分派、坏输入、queued取消/超时、sealer迟到/退出、回复前撤权 | Core recovery_body/channel/application与原Core应用Gate；不同UID手动目标 | 一个libtest、16组实际场景；来源、队列/回复围栏均通过。Verifier/Sealer为测试外部替代，不证明A生产密码互操作 |
| 原GATT/固定D-Bus许可/真实FD及Core端口 | BLE生产非cfg(test)库/真实SO_PEERCRED；原GattBackendV1与私有dbus-daemon | Core端口12场景、Gatt/policy3角色均通过；不同UID与缺组/错误角色保留拒绝，不选生产账户或使用真实HCI |
| v2严格输入/凭据/回复形状/取消/错id | media remote_worker真实parser/client；实际Unix fd | 7个协议/运输函数；对象/重复键/未知字段、禁止URL/角色/授权布尔、错session/id、partial交换取消均拒绝；取消即关原fd不重连 |
| 单次票据、boot/line/call/rev/lease/principal/key/scope/active变化、原Core丢失 | 新worker、原local::Client/Authority/Permit、真实Core媒体协议与实际socket | 13类撤权关闭PCM，错误ticket/boot/peer在PCM前拒绝；Core业务Context是外部合成，未跑完整Core daemon/真实电话 |
| 真实强制中继/DTLS/Opus、Core前无PCM、两个方向 | SDK+v2 actor+真实独立Coturn/TLS/UDP/selected pair/证书/原Codec | x86新目标38/0/1；23复用SDK函数+5源码内unit+10worker用例，不能把复用或重复架构算新测试/阶段 |
| EOF/取消Prepare/进程停机、已读/未读pipeline、慢PCM、排空/槽位保留 | dup FD独立RDHUP监视、原授权镜像、真实TLS挂起socket与唯一PCM线程 | 取消Prepare关闭实际pending TLS fd；EOF/停机先撤权、迟到read不write、不detach；物理返回前仍持slot，不伪造Closed或release_verified |
| stale捕获/播放、真实时钟过期 | 原Pump新增live-clock入口/设备wrapper，60ms/4帧与1s Permit不扩大 | 正常模块断言通过；隔离仅去掉I/O后取时的负例保持原断言并确定失败（exit101），不是放宽条件或给旧API捏造漏洞 |
| 非测试生产入口/真实服务UID、NSS/0660/控制端点、无Mock入口 | 实际native普通binary；Root拥有**合成**NSS镜像、actual UID984、Edge985与其他实际UID | loopback登记拒绝、非法984/986/987拒绝、985正常解析/unknown handle拒绝、SIGTERM原inode清理通过；没有生产安装/凭据/PCM |
| ARM ABI/跨线程/时间/取消 | 实际AArch64 release ELF、真实ARM FFI、QEMU Cortex-A53 | 同一源码38/0/1，128MiB/16pids/2CPU、UID1000/caps空、network none/read-only；UID专用入口只在x86执行，不作ARM目标板/真实PCM证明 |

机器证据保留最初37/1/1失败及nonce排空修正：trace证实50ms cadence之前的queued重试被测试的30s跳时取消，随后正常success按原Ignored拒绝。仅测试观察与排空前提修正，所有产品期限/固定依赖/拒绝断言保持。新测试用例不依赖伪造生产uid、解析出payload=verified或取消后丢掉真实工作。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
CARGO_INCREMENTAL=0 cargo test --workspace --all-features --locked --offline
CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
CARGO_INCREMENTAL=0 python3 crates/core/tests/run_recovery_kernel_uids.py
CARGO_INCREMENTAL=0 python3 crates/core/tests/run_recovery_kernel_uids.py --application
CARGO_INCREMENTAL=0 python3 crates/ble/tests/run_core_port_uids.py
CARGO_INCREMENTAL=0 python3 firmware/system/ble/run_policy_kernel_uids.py --rust-gatt --rust-backend
CARGO_INCREMENTAL=0 python3 native/audio-alsa/tests/run_private_remote_worker.py
python3 native/audio-alsa/tests/replay_clock_negative.py
```

六个整仓继承ignore继续明确：C导入子进程、forwarder专用ENOSPC、Root metadata有/无CAP_KILL两项、C process worker和C tmpfs ENOSPC。这里不把它们算已运行，不改他人测试或用本包Root控制器冒充那些专用环境。新增native手动ignore只有UID入口，已按上表单独执行。

仍未运行/未验收：A生产安全会话/pin/签名域/真实浏览器互操作；完整正常身份/媒体preflight→远程信令/客户端→Core/worker装配；原DTLS crypto-bigint0.5.5/RUSTSEC-2026-0387兼容修复与密码审查；生产Root/NSS/cgroup安装、真实BlueZ/NM/HCI、目标板PCM/蜂窝/外网TURN/两端听音。无设备、电话/短信/APN操作，无生产签名/部署。A27在最新main `9b24d43`仍明确crypto_wire_frozen=false，不能用独立Noise评估或测试Verifier代替这些结果。
