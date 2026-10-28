# D-25/26前置：实际媒体IPC丢确认与失败关闭

2026-10-28，Viper-Boss / Codex。范围是现有媒体授权客户端与Pump的真实失败边界，不是TURN或远程媒体全功能交付。D-25和D-26仍未完成、不减账；用户确认A安全会话保持原分工，不在此包接管密码实现。

## 实际入口与最小修复

- `crates/media/src/local.rs::Client`：真实Unix socket、文件/父目录归属、内核peer_cred、500毫秒RPC截止、请求ID、boot和回复长度校验；仅Redeem/Pulse，不携带拨号/短信命令。
- `crates/media/src/authorization.rs::RemoteAuthority/Permit`：只能由可信Core连接更新；心跳最多1000毫秒，不自行延长真实业务租约，撤销后不能复活。
- `crates/media/src/pump.rs::Pump`：每次enqueue/capture/step复核真实Permit，失败关闭模拟/实际PCM且停止读写。
- 新 `local/boundary_tests.rs` 是cfg(test)私有子模块，无公开测试hook。测试运行实际Client/Permit/Pump，替换的只有外部Core socket端和PCM I/O；不是新写一个授权模型自证。

### 已复现1：请求ID耗尽提前返回，未立即撤销

通过实际Unix socket完成初次Redeem后，在私有测试访问中把Client.id设为u64::MAX，再调用实际pulse。原实现返回Expired，但旧Permit.check(0)仍Ok，预期Denied。0只用于区分“撤销位未置”与自然时间过期，不是修改生产时钟。该边界在正常运行中极难到达，不能包装为日常通话断音原因或无限期越权。

修复只在checked_add失败时先revoke再返回；不绕回ID、不重新Redeem、不重发操作。

### 已复现2：取消正在等待的RPC，但保留Client

真实请求已到达假Core端（计数从Redeem的1变为Pulse的2），外部50毫秒timeout取消实际pulse future，保留Client和Permit。原实现的revoke位于await之后，没有执行，旧许可仍Ok直到自然心跳到期。测试预期立即Denied，修复前失败。

增加私有ExchangeGuard：只有确切有效回复成功更新Mirror才confirmed；错误、future取消或展开时Drop撤销。未poll的future没有发生请求或丢观察，不会误撤销；正常成功心跳继续有效，Client被drop才关闭。没有改变500毫秒RPC、1000毫秒许可上限、通话租约、Core来源验证或采样/队列。

这两处均是有限心跳窗口内的失败关闭修正，不是长期授权泄漏，也没有证据表明它们造成过用户此前的实机声音问题。

## 证据

| 截面 | 源码归档SHA-256 | 日志SHA-256 | 真实结果 |
|---|---|---|---|
| 序号修复前 | `22a844c9c9afc9e8087a70268b333a52778d9a0f5777da0ac05927c72ad358ea` | `84b48b4d044b2cd90c79547d3dca51ba0b7d0839503c46c3d45150c4a262bf23` | 实际Permit为Ok，预期Denied，1项失败 |
| 取消修复前 | `b63a9aae1bf0cf33edb30ce0083b31805e063a50a4e3be20f6753bf7817b76aa` | `67c240a612e03a45d9917f3942d787e919c73a87db5f8acb76667cc14c289c5a` | 实际RPC已发送；取消后Permit仍Ok，1项失败 |
| 两处修复后 | `eadca6afb26852855c96eea68d0273c0481b5f79014c7f25a5d436a67c5b1494` | `f662c36c33b0b62bbe0d6114394d347867d64fadf0fb8d2297427b7e29725a23` | 媒体25+Core170、严格Clippy/fmt全部通过 |

固定代码提交10966ba（完整SHA在交付台账/PR记录），验证环境为Linux/Rust1.98、UID65534/无capabilities、源码/缓存只读、无网络/设备。Unix端点在私有临时目录中，未连接真实Core或PCM；Core170项是独立生产Core回归，不能说本测试假端等于真实Core授权验收。

```sh
cargo test -p rustsimhub-media --all-targets --locked --offline -j3
cargo clippy -p rustsimhub-media --all-targets --locked --offline -j3 -- -D warnings
cargo test -p rustsimhub-core --all-features --locked --offline -j3
cargo clippy -p rustsimhub-core --all-targets --all-features --locked --offline -j3 -- -D warnings
cargo fmt --all -- --check
python3 tools/progress/build.py --check
```

## 新增6个测试入口，16类实际socket失败

矩阵包括错ID、协议、boot、缺字段、错类型、过期/过长/最大u64截止、无效UTF-8/JSON、重复ID、伪role、0/超长帧、截断、断连、超时。每例先取得实际客户端许可，再给Pump放入合成静音帧，失败后验证：许可立即Denied，Pump关闭一次，不再读写、不恢复设备；外部端计数只有Redeem+Pulse，不自动重试。超时假端由测试显式abort并await，不遗留任务。

另测正确回复在失败后不能使Mirror/队列复活、实际错误UID/宽父目录在连接前拒绝且socket未收到凭据、取消已发送future、丢弃未poll future、成功心跳及drop路径。输入都为合成数据，没有录音、真实号码、SIM数据或公网请求。

## 仍欠的完整目标

既有 `native/audio-alsa` 已锁定Opus0.4.0、str0m0.24.1/RustCrypto，具有DTLS指纹核验、Opus与局域网ICE；`codec.rs` 固定16kHz/320样本/20毫秒、RTP48kHz/960步、最大Opus400字节。`rtc.rs`/媒体worker当前明确只允许局域网IPv4/host候选，无TURN。此包不删除这些限制、不更改原生音频、依赖或锁文件，不把受限LAN链路说成远程已支持。

以上原生组件本次只读核对源码，未构建/运行其Opus/DTLS测试；195项不包含原生音频或ARM测试。

完整D-25仍需先审有限TURN客户端/凭据/实际分配绑定、独立远程路线、强制中继/私有TLS与真实Opus/DTLS对端失败测试；不能用SDP自称typ relay、伪grant或WSS/普通代理替代。D-26还需覆盖D-21～25实际新增的Core/BLE/媒体接线，不能用本次25+170项替代尚未存在的模块。当前D-21～26仍6步，20/169、加权10.6%、原80%保持；未安装ARM/设备、未做实际听音或真实TURN。
