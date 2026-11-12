# AirSIM iPhone App 直连（棒子冒充安卓网关）

状态：**只完成协议层**（`crates/airsim`）。网关、音频、网页开关都还没有，设备上没有任何 AirSIM 功能。

## 定位

- 有 NAS：配对交给 NasSimHub，由它连 iPhone（NasSimHub 侧由 Codex 维护）。
- 没有 NAS（只有 Wi-Fi、流量或蓝牙）：棒子自己冒充 AirSIM 的安卓网关，iPhone 上的 AirSIM App 不改代码直接连。

App 侧用“私网 IP 配对”：填棒子 IP、配对端口和网页上显示的六位码。

## 结构

网关放在 Edge 进程里，不新增系统服务：

| 端口 | 用途 |
|---|---|
| 7594 | 配对，只在所有者打开配对窗口的 120 秒内应答 |
| 7592 | 带 HMAC 签名的控制请求（拨号、接听、挂断、短信、状态） |
| 7593 | 通话音频，TCP 原始 PCM |

- Edge 的数据目录只读，每台 iPhone 的密钥存在 Core，由 Core 授权和撤销。
- 音频：棒子内部是 16 kHz。App 现在只说 8 kHz，网关按配置重采样。**采样率做成后台可切换的设置（8 kHz / 16 kHz），默认 8 kHz**；App 将来支持 16 kHz 时切过去即可，不用改代码。`PcmRate` 已在协议层定义。
- 只接受私有网络来源；这是 App 的本地协议，没有 TLS，不能映射到公网。

## 协议依据

- 线格式由 AirSIM App 和它的安卓网关决定（`iOS/AirSIM/VoWLANAuth.swift`、`SamsungPairingCrypto.swift`、`android/.../VoWLAN*.java`）。
- 实现移植自 NasSimHub 的 `internal/airsimproto`，测试向量来自安卓网关源码本身，保留在 `crates/airsim/src/tests.rs`。向量失败就意味着未修改的 App 连不上。
- 2026-11-12 对比上游 `93db925`（10-31，NasSimHub 的依据）与 `1855e43`（11-04）：
  - 未变：请求规范串和签名、配对加密、PCM 握手行、音频仍是 8 kHz。
  - 新增（网关阶段要处理）：早期媒体（`AIRSIMEM1` 握手和 `/api/calls/early-media`，不支持时返回 404 是被接受的）、`/api/local-mode`、`/api/calls/resolve`、`/api/calls/history/ack`、`/api/push/mode`、`/api/feedback`、拨号操作状态证明。

## 已知限制

- 锁屏来电不会响：推送唤醒依赖 AirSIM 的云端中继和 APNs。App 在前台或后台保活时能收到。
- 蓝牙直连（VoBLE）是另一套协议，未做。
- 走蜂窝时需要 Tailscale 之类的组网；蜂窝数据在换卡前不启用。

## 后续顺序

1. 网关：配对、控制接口、Core 侧配对存储和撤销、网页上的开关与六位码。
2. 音频：给媒体进程加原始 PCM 通道，8k/16k 切换。
3. 局域网发现（`_airsim-vowlan._tcp`）。
4. 真机实测章节加入 `docs/testing/TAKEOVER-ACCEPTANCE.md`。
