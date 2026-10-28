# GATT 离线适配库交付：D-17～20

日期2026-10-28。实现者 Viper-Boss / Codex。包 `crates/ble`（`rustsimhub-ble`），无守护进程、无默认 BlueZ 后端、无生产安全端口，不安装服务、不访问 HCI。D-17～20 是这份可复用库与失败矩阵的离线交付；BLE3-01/04 整项仍有真实后端和设备验收要求。

最新接续PR50的2b13b25及Claude main dfd11e9后，GATT代码be02191保持原样，灯控断言跟随用户新配色。最终归档 `4898875176e75cec26e30f532eb398b7361f64ade94beb604e6d4921800e3c5a`、日志 `d8ba77e0d818a2881b839aa9cd810d82a88b9a8da61ad93aa02440638cdd3d3f`：蓝牙38+灯控24、严格Clippy/fmt、Python进度23和Node看板一致性通过。最新台账20/169、10.6%；下文早期61项/18条完成记录为历史证据，不当成当前计数。

## 接口来源与固定对象

唯一项目协议依据是 `docs/team/interfaces/t5-offline-v1.json` 和 `t5-vectors-v1.json`，基线3eb42bca52768d2f08c81cd8eb860e4fef681d59。实际测试直接读取两个 JSON，逐项核对 UUID、flags、资源上限、帧字节和重组结果，避免复制一套夹具后互相自证。

BlueZ 的 WriteValue request/command/reliable、offset、mtu、device、prepare-authorize 选项见[上游 GattCharacteristic 文档](https://github.com/bluez/bluez/blob/master/doc/org.bluez.GattCharacteristic.rst)；注册应用/广播与 Release 分别参见[GattManager](https://github.com/bluez/bluez/blob/master/doc/org.bluez.GattManager.rst)、[LEAdvertisingManager](https://github.com/bluez/bluez/blob/master/doc/org.bluez.LEAdvertisingManager.rst)、[LEAdvertisement](https://github.com/bluez/bluez/blob/master/doc/org.bluez.LEAdvertisement.rst)。UUID/12字节头/预算是本项目选择，不是 BlueZ 标准，也不是已审核的加密协议。本库没有复制上游实现；沿用 AGPL-3.0-only。

- 固定根 `/org/rustsimhub/recovery/v1`，固定 service0、char0～3 和 advertisement0。Manifest/Advertisement 为零大小类型，没有任意路径/UUID/广播内容参数。
- 四特征值分别为 DeviceInfo Read、AuthExchange Write/Notify、Command Write、Result Read/Notify。仅接受 Write with response；拒绝 command/reliable、非零 offset 和 prepare-authorize。
- DeviceInfo 常量只有协议、产品名、预授权要求和2048字节上限，少于256字节，无公钥、SIM、SSID或设备地址。
- 广播后端只可发 peripheral 和固定服务 UUID；不能顺手 Power/Pair/Trusted 或操作其他对象树。

## 实际模块与接线

| 模块 | 代码做什么 | 主线仍需提供 |
|---|---|---|
| `manifest.rs` | 固定对象、UUID、flags、DeviceInfo | 实际 D-Bus ObjectManager/特征值导出 |
| `ports.rs` | 有类型的 GattBackendV1 / SecurePortV1；Send + owned future；脱敏 Peer/Epoch/OpaqueBytes | 绑定唯一 BlueZ owner 的后端、批准过的真实安全会话 |
| `framing.rs` | 有界重组、硬截止、方向编号、分片生成、确认后推进队列 | 实际 ATT MTU 的可信协商证据、单调时钟 |
| `lifecycle.rs` | 真正异步注册/撤销/Release/断连/迟到补偿；连接代次 | 运行时按 future wake 与 deadline 调度 poll |
| `transport.rs` | 把上述生产库连到注入安全端口；拒绝旧连接结果、限流拥塞、清理会话/缓存 | 真正的 Core 授权/撤销信号和 IPC 来源验证 |

`Transport::new(backend, Some(secure), unique_owner, fresh_boot_domain)` 才能构造。None 在产生任何注册调用前拒绝。生产模块没有 Mock/default secure、环境变量开关或假认证捷径；Mock只位于 cfg(test) 文件。

主线启动后依次 `start`、根据实际连接事件 `connected`，用返回的私有字段 `Connection` 作为本连接令牌；对每个 WriteValue 传实际特征值、严格解码的选项及单调时间。不要从用户 payload 取 owner/device/MTU/epoch。`Peer::from_backend_callback` 只验形状，不证明来源；实际 owner/peer 证明由真实 D-Bus 接线负责。

宿主必须在 future 唤醒和 `deadline_ms()` 到点时调用 `poll`；每轮最多处理8个即时传输动作，更多工作重新唤醒，避免占满主循环。没有后台计时线程，也不假装“不 poll 仍会自动清理”。停止/断线后继续保有该对象，直到 `is_quiescent()`；unknown/pending 不能简单 drop 后另开一个实例。进程被强杀后的实际 BlueZ 资源回收也要另做集成验收。

## 编号、预算和未知结果

帧头固定 little-endian `RB / 1 / kind / id / index / count / total`。初始 attribute20，可信协商后 `min(MTU-3,244)`，MTU<23拒绝；首片冻结当前消息布局，之后协商变化只影响下一条。正文1～2048、最多256片、索引连续、非末片填满；auth/command共用入站编号空间，auth/result共用出站编号空间，各从1开始，到65535后拒绝并要求新连接，绝不绕回0。

每连接只有1条重组中消息，第一片起15秒硬截止，中间片不续期。错头、错长度/类型/片数、重复、跳片和第二条并发消息终止该连接重组；不是“清空再接受相同ID”。总逻辑结果队列最多4条；每轮只有一个安全/通知 I/O，I/O期间继续写入按拥塞关闭受影响连接，不无限堆任务。最多4×2048字节队列、2048重组、一个待决安全请求/结果和一个≤244字节最近确认结果，后端必须另守自己的分配预算。

通知只在确切后端调用成功后推进同一个 FrameTicket；失去确认不丢回执后声称成功、不自动重发。Result Read只返回本连接最近一次已确认的加密 Result 帧；Auth回复不能读成业务结果；断线、停用、撤销/owner丢失会清空缓存和队列。**这不等于加密或业务授权已实现**：Application回复必须由真实安全端口在验证/解密/防重放/Core授权后返回，不能把网络布尔字段转换成这个枚举。回复种类错配也会关闭连接。

注册应用后才注册广播。任一失败会尝试本 epoch 的有限补偿；停止顺序为撤广播→关闭当前安全会话→撤应用。Release和主动撤销协调，不重复清理已确认 Release 的同一广播。调用5秒截止后逻辑状态变 unknown，原 future 仍被保留；即使注册迟到成功也会撤销原 epoch 的资源。不能证实清理则保持 unknown，不自动重启、不全局删除其他广播。

同一 BlueZ owner/设备路径在同次启动中断线重连也会获得不同的私有连接序号，因此旧异步回复/旧断线事件不能重新变成当前结果。序号和 epoch 仅是新鲜性控制，**不是 VerifiedSession/Core能力/加密 nonce**。启动 epoch 在本实例递增且不绕回；跨进程的新 boot domain 必须由可信宿主分配，本库不能证明调用者没有复用 domain。

## 已执行的测试与保留失败

测试调用实际 `Transport`、`Lifecycle`、`Assembler` 和 `Outbox`，仅模拟外部 D-Bus 与安全端口：

- 冻结 JSON/字节向量；6种 ATT预算下1～2048的全部长度；65535个编号耗尽；畸形头/乱序/重复/并发、15秒硬截止、MTU跨片变化、有界队列和确认票据。
- 真实注册/撤销顺序、广播部分失败、5秒超时后保留原 future、迟到成功补偿、Release重复/与unregister交错、关闭会话未完成、清理失败 unknown、旧epoch/owner/第二peer与同peer重连代次。
- 真正接到注入安全端口后再通知：缺安全端口拒绝、端口错误无成功兜底、读取缓存隔离、半包无后续仍过期、迟到加密回复不发送、通知未确认不重试、洪泛/停止/owner丢失清理。

```sh
cargo test -p rustsimhub-ble --all-targets --locked --offline -j3
cargo clippy -p rustsimhub-ble --all-targets --locked --offline -j3 -- -D warnings
cargo test -p rustsimhub-status-led --all-targets --locked --offline -j3
cargo fmt --all -- --check
python3 -m unittest discover -s tools/progress -p 'test_*.py'
python3 tools/progress/build.py --check
```

环境：Linux / Rust1.98、UID65534/无capabilities、无网络/设备、只读源码/缓存、临时输出。已有联调截面 f3bc87bb91f49543a2f649f0dd243eaab75d54c4ac342af538703144244fb31a：蓝牙37+独立灯控23、严格Clippy/fmt及进度Python23通过；日志435ef94d08e88c0f928d177791c759b3a9d9e678642b9fedf7534631b719e0ab。随后新增Release时钟回退先验保护和回归；最终源码/结果另补于交付台账。

最终代码提交 `be0219119add5517161a8429328b5c0d1740641d`：蓝牙**38项**、独立灯控23、严格Clippy/fmt通过。20/6分工和生成看板后的交付归档 `31dcca7e8d1e0eff57781fb962d7f49edf4f168b74d2ccf33fe439656631b523`、日志 `67e8c9bb4a90d4bb89d8edb815adc3da3396b918bf9d71dc3f4d0e5a8c5b7359` 再次通过相同检查及Python进度23项；Windows Node看板一致性通过。此段只补实跑结果，PR完整HEAD仍需独立审核，不把离线自测当设备/密码验收。

失败历史：Windows原生 rustc 缺MSVC link.exe，未宣称原生构建通过，改用既有隔离Linux；早期测试分别被Clippy的 chunks_exact_to_as_chunks / manual_noop_waker 拒绝，改用标准库接口，未关闭 lint。没有真实发包、BlueZ注册、加密互操作、Core批准/撤销、Wi-Fi保存、ARM、安装或设备验收。

## 明确未交付的生产前置

1. A 的标准安全会话/密码套件、transcript/nonce域、真实Rust/浏览器互操作尚待其接口审核。本库 opaque 端口不构成任何替代。
2. D-21～23：Core唯一所有者预批准/撤销/持久迁移、不可伪造可信来源、SO_PEERCRED固定IPC、现有Wi-Fi事务接线。没有临时认领窗口、第二Owner或NAS权限升级。
3. D-24：实际服务账户/UID/Root材料先审，有限服务安装、NSS/cgroup/BlueZ策略；本批未新增系统UID/单元。
4. D-25～26：远程受限媒体/TURN和跨模块实际失败矩阵，不能以这份GATT测试替代。
5. 实际HCI/RF/BlueZ能力、浏览器授权、双端/重启/撤销传播/资源消失和关闭无进程均待真实集成。禁止据此把BLE3-01/04或原固件百分比改成完成。
