# AirSIM VoBLE 蓝牙配对

RustSimHub 可作为 AirSIM iPhone App 的 VoBLE v1 配对端。候选程序通过 BlueZ 在 hci0 上注册 AirSIM 的服务和广播，设备端与 iPhone 分别确认六位校验码。设备仅在确认完成后保存一台手机的配对凭据。凭据独立于现有 VoWLAN、设备所有者和 BLE 应急配网记录。

协议依据为 [AirSIM 1855e43 的 iOS 配对实现](https://github.com/kai-wu-cortex/AirSIM/blob/1855e435aa1e207e6939c525c3feea4bba09ee2b/iOS/AirSIM/VoBLEPairing.swift)、[iOS GATT 客户端](https://github.com/kai-wu-cortex/AirSIM/blob/1855e435aa1e207e6939c525c3feea4bba09ee2b/iOS/AirSIM/VoBLECentralController.swift)和[分片格式](https://github.com/kai-wu-cortex/AirSIM/blob/1855e435aa1e207e6939c525c3feea4bba09ee2b/iOS/AirSIM/VoBLEProtocol.swift)。Rust 代码按上述线格式独立实现，定值向量由 OpenSSL 和 Python 生成。

## 设备联调

需要支持 LE Peripheral 和 Advertising 的 hci0，以及具备按连接路由 indication 和通过描述符回传确认的 BlueZ 5.82 或更新版本。候选检查实际占有系统总线 `org.bluez` 的版本。行政终端启动，使用应用自有的 0700 状态目录。该二进制通过 Cargo feature 单独构建，默认构建和设备启动流程不加载它。

```sh
cargo build --release --locked -p rustsimhub-ble --features airsim-pairing --bin rustsimhub-airsim-pairing
sudo ./target/release/rustsimhub-airsim-pairing pair /var/lib/rustsimhub-airsim-ble
```

1. 在 120 秒窗口内，从 iPhone 版 AirSIM 的「蓝牙 Agent」选择 `RustSIM`。
2. 核对终端与 iPhone 的六位数字。在终端输入相同数字，再点 iPhone 的「数字一致，完成配对」。
3. 终端收到最后一个 ATT indication 确认后提示已确认。检查 iPhone 的配对状态。让窗口结束或用 Ctrl-C 退出后，可用相同目录和 `serve` 子命令验证已配对手机的认证重连。
4. 停止正在运行的程序后，可用 `status` 查询凭据状态，或用 `revoke` 同时删除当前凭据和重连保留的上一份凭据。要重配同一台 iPhone，先清除 AirSIM 的本机旧凭据。

## 当前交付范围

配对窗口由设备终端控制，网页入口、设备安装和真实 iPhone 验收仍待完成。iPhone 在开始配对前要求控制和媒体特征存在，因此候选按 App 要求声明这些特征，业务写入返回不支持，音频特征无数据。iPhone 配对后可能显示「蓝牙通话已就绪」，本次验收应检查配对和认证重连。目标棒子的 HCI、实际 BlueZ 版本和射频状态待现场核对。

## 验证记录

- 在主分支 `8b54b4142505e23fc063c6c22a737f22e4b8926a` 的源码上开发。BlueZ 测试使用独立的 dbus-daemon、实际 D-Bus 方法调用与 Unix SEQPACKET，模拟 iOS 的分片请求和 ATT 确认。
- 实测待办包括发现和选中广告、两端校验码一致、手机保存凭据、停启后重连、拒绝第二个中心设备、撤销后旧凭据失效。
- 整体验收仍为 81.5%，本次没有设备验收加分。
- Rust 1.98.0 使用锁定依赖验证，AirSIM 24 项、BLE 80 项测试通过，相关 crate 的格式检查和严格 Clippy 通过。使用 Debian AArch64 交叉 GCC 生成了单独的配对程序，未在 ARM 设备执行。
