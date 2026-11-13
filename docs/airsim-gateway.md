# AirSIM iPhone App 直连（棒子冒充安卓网关）

状态（2026-11-13）：协议、配对/短信和端口修正已由Claude记录安装至应用77，默认未启用；该记录不是本轮实时探测。通话控制已在main（PR237）。本分支接入PR238媒体PCM并补齐Core授权、Edge7593和8k/16k转换，尚未装机验收；真实iPhone和局域网自动发现仍待完成。

## 定位

- 有 NAS：配对交给 NasSimHub，由它连 iPhone（NasSimHub 侧由 Codex 维护）。
- 没有 NAS（只有 Wi-Fi、流量或蓝牙）：棒子自己冒充 AirSIM 的安卓网关，iPhone 上的 AirSIM App 不改代码直接连。

App 侧用“私网 IP 配对”：填棒子 IP、配对端口和网页上显示的六位码。

## 结构

网关放在 Edge 进程里，不新增系统服务。设备上有两个 Edge 进程，只有面向局域网的那个在自己的非回环地址上绑定这些端口（不绑定通配地址，因此不在蜂窝接口上）：

| 端口 | 用途 |
|---|---|
| 7594 | 配对，只在所有者打开配对窗口的 120 秒内应答 |
| 7592 | 带 HMAC 签名的控制请求（拨号、接听、挂断、短信、状态） |
| 7593 | 通话音频，TCP 原始 PCM |

- Edge 的数据目录只读，每台 iPhone 的密钥存在 Core 私有目录的 `airsim.json`（0600），由 Core 授权和撤销。
- 网关在 Core 里是一个普通客户端身份：所有者启用 AirSIM 时创建，种子不出 Core；停用 AirSIM 或在客户端列表撤销它，所有已配对 iPhone 立即失效。
- Edge 只转发 App 的明文请求（`airsim_pairing`、`airsim_control`、`airsim_poll`），验签和业务都在 Core。
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

1. 同源构建后安装候选，验证真实iPhone配对、通话与PCM；应用77本身不含本次完整声音接线。
2. 局域网发现（`_airsim-vowlan._tcp`），以及有独立资源/授权后再做远程与VoBLE。
3. 按 `docs/testing/TAKEOVER-ACCEPTANCE.md` 记录设备/手机真实结果，不能用合成通话或QEMU代替。

## PCM授权与生命周期

- Edge只在已有USB/Wi-Fi本地地址上绑定7593，不用通配地址；setup-only回环实例不开这些端口，蜂窝不作为隐式回退。
- 手机先发送原`AIRSIMVWL1`签名行。Core验证私网来源、配对密钥、防重放和该手机自己的通话占用，再给Edge短时、仅内存保存的控制能力；配对密钥和网关bearer不出Core。
- 手机的就绪标记为原协议`AIRSIMREADY`。拨号中最多等待90秒，只发送静音并丢弃上行；真正ACTIVE后才经原`CheckMediaLease`/`IssueMedia`签发一次性凭据。重复心跳不重新签发，结果丢失就关闭，不靠重发恢复授权。
- 本机`pcm.sock`仍由媒体UID984拥有，只接受Edge UID985；检查文件/父目录与实际Unix peer UID。媒体进程兑换Core凭据后才打开ALSA，沿用250ms许可心跳，与WebRTC共用唯一会话槽。
- 本机使用16kHz单声道S16LE、20ms帧；手机按Owner选择使用8k/16k，默认8k。16k直接传，8k使用有界线性插值/相邻样本平均，不是自动采样率协商。
- 上行本地队列最多2帧，只取最新帧，接收后满60ms丢弃；半帧过期就关链，不能丢半帧后错位播放。拨号前麦克风字节排空并保持帧边界。TCP/Unix请求有限缓冲，慢写40ms超时关闭。只约束本机队列/阻塞，不声称网络端到端帧龄已经实测。
- 250ms续查配对、启停、网关principal、原通话占用和媒体许可；2秒不续查的能力过期。挂断/hold/后端未知、撤权、改采样率、TCP断开或媒体EOF均关闭；无分离后台泵、自动拨号或自动重连。
- 新`airsim_pcm_open`/`airsim_pcm_pulse`仅走Edge→Core内部IPC；浏览器API拒绝，不能伪造peer或索取凭据。手机只收到公开Ready和PCM，不收到内部hello/接受字节/凭据。

对照本机NasSimHub的`internal/airsimgw/pcm.go`（原就绪标记、拨号等待和关闭语义），复用RustSimHub现有Auth/Store/CallLease/媒体许可；无新增外部依赖或签名域。真实ALSA+iPhone组合、长通话、断网和第二手机抢占仍需验收。
