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

1. 按现有签名更新流程启动包含 VoBLE 的主 Edge。在设备网页的「安全策略 → AirSIM 手机直连」为本机桥开启配对窗口，运行 `sudo rustsimhub-airsim-pairing link /var/lib/rustsimhub-airsim-ble`，按提示输入该窗口的六位码。
2. 运行 `sudo rustsimhub-airsim-pairing pair /var/lib/rustsimhub-airsim-ble`。在 120 秒内，从 iPhone 版 AirSIM 的「蓝牙 Agent」选择 `RustSIM`。终端和 iPhone 分别确认相同的六位代码。
3. 完成后可退出配对窗口，用相同路径和 `serve` 持续提供服务。手机选择蓝牙通话模式。现有 AirSIM 配对过的手机可以直接从 `serve` 认证重连。
4. 验收时关闭 iPhone 的 Wi-Fi，保持蓝牙。至少分别验证拨出、来电接听、双方连续说话、拒接、挂断、按键和断开蓝牙。断连后再次连接只认证设备，通话清理由现有 Core 租约执行。可在设备网页撤销本机桥，也可在 AirSIM 取消手机配对。终端的 `revoke` 删除手机的本地凭据。

## 代码验证与现场状态

- 验证包括 iPhone 线格式、篡改与重放拒绝、BlueZ 发送者和设备隔离、双向 PCM 与加密 BLE 帧、Core 租约、Unix 和现有 TCP 音频身份验证与断连清理。
- UFI003 实机确认运行签名应用 88。BlueZ 5.82、VoBLE 本机 socket、维护 HTTPS 和 7 条音频路由通过检查。已备份应用 87 的数据并核对原有 AirSIM 绑定。
- 真机协商 ATT MTU 185。iPhone 的 197 字节配对帧由 BlueZ 重组后交给应用，修复后收到了 iPhone 对配对成功 indication 的确认，棒子已保存该配对。相同长度范围的写入已加入 D-Bus 回归测试，BLE crate 81 项通过。
- 本机桥接和关闭 Wi-Fi 后的双向通话仍待完成实测。整体实机验收仍为 81.5%。
- Rust 1.98.0 锁定依赖下，共 198 项通过，包括 AirSIM 27、BLE 81、Edge 71 和 Core AirSIM 19。三个 ARM64 程序交叉构建通过。相关 crate 格式检查、AirSIM/BLE/Edge 全目标严格 Clippy 和 Core 生产程序严格 Clippy 通过。Core 全目标额外检查在原转发测试出现四处 `useless_conversion` 警告，该文件与基线逐字节相同。
