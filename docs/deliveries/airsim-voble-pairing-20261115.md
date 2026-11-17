# AirSIM VoBLE 蓝牙配对与通话

RustSimHub 可以作为 iPhone 版 AirSIM 的 VoBLE v1 外设。iPhone 通过 BLE GATT 发现、配对、拨号、接听和双向传声。语音帧采用 AirSIM 原有的 G.711 μ-law、8 kHz 单声道、20 ms 帧和 AES-GCM 保护。棒子内部通过 Unix socket 连接 Edge 和声卡服务，手机通话期间无需连接棒子的 Wi-Fi 或 USB 网络。

协议依据为 AirSIM 源码 `1855e43` 的 [iOS 配对](https://github.com/kai-wu-cortex/AirSIM/blob/1855e435aa1e207e6939c525c3feea4bba09ee2b/iOS/AirSIM/VoBLEPairing.swift)、[控制与 GATT](https://github.com/kai-wu-cortex/AirSIM/blob/1855e435aa1e207e6939c525c3feea4bba09ee2b/iOS/AirSIM/VoBLECentralController.swift)和[媒体链路](https://github.com/kai-wu-cortex/AirSIM/blob/1855e435aa1e207e6939c525c3feea4bba09ee2b/iOS/AirSIM/VoBLEMediaTransport.swift)。Rust 代码根据线格式独立实现，定值向量使用 OpenSSL 和 Python 生成。

## 运行条件

- 支持 LE Peripheral 和 Advertising 的 hci0，以及支持按连接路由 indication 的 BlueZ 5.82 或更新版本。语音最低协商 ATT MTU 为 100。
- 同源 Core、主 Edge 和蓝牙侧车。主 Edge 使用原有私有配置、`--serve` 启动方式和运行身份 UID 985。签名候选用 `rustsimhub-edge/airsim-voble` 编译主 Edge；该版本要求现有媒体 worker，提供三个本机抽象 Unix socket 并只接受 UID 0 的客户端。系统服务和配置文件可由前一版本继续使用。
- 配对密钥存放于侧车专用的 0700 目录。设备所有者在网页开启一次 AirSIM 配对窗口，终端用该窗口创建名为 `RustSimHub-VoBLE` 的本机桥接凭据。它受现有 Core 网关开关、配对撤销、通话租约和媒体授权约束。

## 构建与连接

```sh
cargo build --release --locked -p rustsimhub-core -p rustsimhub-edge -p rustsimhub-ble \
  --features rustsimhub-ble/airsim-pairing,rustsimhub-edge/airsim-voble \
  --bin rustsimhub-core --bin rustsimhub-edge --bin rustsimhub-airsim-pairing
```

1. 按现有签名更新流程启动包含 VoBLE 的主 Edge。将已验签的 `rustsimhub-airsim-pairing` 安装到 `/usr/local/libexec/`，将仓库中的 `rustsimhub-airsim-voble.service` 安装到 `/etc/systemd/system/` 后执行 `systemctl daemon-reload`。该程序和服务独立于应用包部署。
2. 使用维护终端停止 `rustsimhub-airsim-voble`。在设备网页的「安全策略 → AirSIM 手机直连」为本机桥开启配对窗口，运行 `sudo /usr/local/libexec/rustsimhub-airsim-pairing link /var/lib/rustsimhub/rustsimhub-v1/system/voble`，按提示输入网页的六位码。已有本机桥时从下一步开始。
3. 将上一条命令的 `link` 换成 `pair`。在 120 秒内，从 iPhone 版 AirSIM 的「蓝牙 Agent」选择 `RustSIM`。终端和 iPhone 分别确认相同的蓝牙配对码。终端收到配对成功确认后退出配对命令并执行 `sudo systemctl enable --now rustsimhub-airsim-voble`。以后已有配对的手机可直接从 AirSIM 重连。
4. 可在 AirSIM 取消手机的蓝牙配对，棒子清除该手机的密钥并退出广播。若手机遗失，可停止服务后在维护终端用同一状态路径执行 `revoke`。已撤销后，服务的启动条件会跳过广播。网页可另外撤销「RustSimHub-VoBLE」本机桥的通话授权。以上两种授权可以独立撤销。

## 代码验证与现场状态

- 验证包括 iPhone 线格式、篡改与重放拒绝、BlueZ 发送者和设备隔离、双向 PCM 与加密 BLE 帧、Core 租约、Unix 和现有 TCP 音频身份验证与断连清理。
- UFI003 实机确认运行签名应用 88。BlueZ 5.82、VoBLE 本机 socket、维护 HTTPS 和 7 条音频路由通过检查。已备份应用 87 的数据并核对原有 AirSIM 绑定。
- 真机协商 ATT MTU 185。iPhone 的 197 字节配对帧由 BlueZ 重组后交给应用，修复后收到了 iPhone 对配对成功 indication 的确认，棒子已保存该配对。相同长度范围的写入已加入 D-Bus 回归测试，BLE crate 81 项通过。
- 已用设备所有者开启的一次性窗口注册本机桥。蓝牙服务在棒子上运行，iPhone 重新连接后已确认配对响应，配对凭据保存在设备。
- 实机记录到 iPhone 经 BLE 拉取设备信息、拨出并接通电话，双向都有加密语音帧。30 ms 蓝牙连接中，下行平均约 20 个完整帧/秒，低于 20 ms 音频所需的 50 帧/秒。iPhone 端听到慢放和卡顿，音质未通过验收。
- 两次现场通话时，棒子实际录音和发送均约为 50 帧/秒。第二次现场通话约 29 秒，媒体进程收到 802 个完整上行帧，约 27 帧/秒，手机端仍报告卡顿。第二次通话使用的连接间隔缺少抓包确认。
- 改由 iPhone 发起 MTU 协商后，iPhone 仍通告 185 字节，棒子通告 517 字节。BlueZ 默认配置已恢复到磁盘。另一次实测确认了现有连接的 15 ms 间隔，监测窗口内没有新的通话，尚无该间隔下的帧率数据。整体实机验收仍为 81.5%。
- Rust 1.98.0 锁定依赖下，共 198 项通过，包括 AirSIM 27、BLE 81、Edge 71 和 Core AirSIM 19。三个 ARM64 程序交叉构建通过。相关 crate 格式检查、AirSIM/BLE/Edge 全目标严格 Clippy 和 Core 生产程序严格 Clippy 通过。Core 全目标额外检查在原转发测试出现四处 `useless_conversion` 警告，该文件与基线逐字节相同。

## 原版 iPhone 客户端的音频对照

- 手动协商 15 ms 后，接通期间抓到完整下行约 24 帧/秒、上行约 20 帧/秒。PCM 录音仍约 50 帧/秒。挂断后控制器接口仍持续提交旧的下行语音约 22 秒。
- 通过 USB 保持维护连接，短暂停用棒子 Wi-Fi。蓝牙断开后重连并在通话中改到 15 ms，下行在通话内和随后的积压排空混合窗口中约 30 帧/秒，上行约 22 帧/秒。用户听感仍明显卡顿，Wi-Fi 已恢复。蓝牙在两次挂断附近均记录到 BlueZ 异常退出和自动恢复，尚未定位该异常。
- AirSIM 公共发布 v0.9.8.25 对应 iOS build 176，仍只支持 G.711。棒子侧候选保持 20 ms PCM、原有分片和 AES-GCM。它丢弃尚未发送的旧 PCM，按真实采样时间推进序号，并可通过 `RUSTSIMHUB_VOBLE_TX_PACING=1` 等待下行最后一个分片交给蓝牙控制器后才提交下一帧。发送数据只在内存匹配，不保存抓包。交给控制器不代表手机收到，也不会增加无线容量。
- 110 项 AirSIM/BLE 测试、两 crate 严格 Clippy 和 ARM64 构建通过。签名候选只替换临时蓝牙服务，旧侧车可独立恢复。候选成功启动并广播。五分钟内没有新的通话，定时回退成功，复核原蓝牙、媒体服务和 Wi-Fi 在线。
- **延期项：实机语音卡顿与慢放。** 先验收配对和通话控制，最后回到该问题。保留现有 TestFlight。实验代码保存在提交 `c83823b41e80830527c08110c2eefa78f6760382`，从当前的控制版本中移出。用双向帧率、挂断后还有多久提交旧语音，以及手机的实际听感判断是否修复。

## 2026-11-16 通话控制收尾

- 棒子已验签安装并启用开机启动单元，复核独立重启后恢复广播。真实整机断电上电尚未测试。安装时短时提高 `/run` 的 tmpfs 容量上限，让 systemd 可以重新读取单元，操作后恢复原上限。
- 已部署拨号后立即取消的修复。只有本次拨号拥有者能用本次 iPhone 临时编号挂断，执行时仍使用基带通话编号。撤销配对后先向手机回复，再停止广播。109 项 AirSIM/BLE 测试、两 crate 严格 Clippy、ARM64 构建和实机服务检查通过。撤销测试使用隔离的合成配对，已保留现场手机配对。
- 实机已验收原版 iPhone 认证重连、拨出、接通、终止。完整来电接听、拒接、通话键盘和蓝牙断连的实机验收仍需手机。前台或蓝牙连接在线时，接听和拒接由已认证的 VoBLE 通道处理。App 在后台断连后由 PushKit 唤醒与重新认证需要另有推送路径，不算离线蓝牙验收通过。
- 真机验收顺序：原版 AirSIM 保持蓝牙连接并关闭手机 Wi-Fi，先拒接一次来电，再接听、挂断一次来电。下一次拨打已授权的测试号验证拨号取消和按键回执，最后在通话中短时断开蓝牙，核对通话清理由 Core 租约执行，再验证配对仍能重连。检查过程中避免用重配对代替重连。整体验收仍为 81.5%。
