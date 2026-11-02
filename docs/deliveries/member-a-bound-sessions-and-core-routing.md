# A26/A27：实际会话、Core 及媒体权限桥隔离交付

2026-11-02，wnfhsj88。用户明确转交 D 的 Core/媒体桥及主线保留依赖的**隔离实现**；没有据此启用产品、批准密码协议、迁移设备、合并 main 或部署。整体验收仍为 81.5%、20/169；剩余工作继续，不把本包写成九步全部完成。

集成 main 基线 `9b24d43dcc74a7d8ff7f28e4aab005bec65c0ed3`，正常合并在本人分支进行。PR169 已由负责人合并，其 SDK 状态适配原样承接，之前的 PR144/153/157/160/161/165 不重做。

本包代码提交：

- `16cef8b61eae5354143dcc73faf0d0528cbebc86`：标准 Noise 候选会话库，未接默认运行。
- `9ccc005641269954afaa77f557f8927d22b28b7a`：同 Auth/Core 的恢复绑定、真实 GATT Core 端口、浏览器候选后端及检查。
- `fac050053cffea5d82b8d21a30367327462a87c9`：实际普通 NKN 身份、成员证明、原 Core/媒体权限桥及失败检查。

D 原始恢复执行及 application host 来自 `7008728412d92db5934bc862bab29a7615528fda`，通过正常 merge 保留原始作者和提交，不将这些实现署名给本账号，也不修改 D 的分支/PR154。后续本人提交只补真实会话、适配及检查。

## 已实现行为

`crates/recovery-session` 使用固定 snow 0.10.0 的标准 `Noise_XX_25519_AESGCM_SHA256`，原生和实际 WASM 都运行相同协议。实际随机 DH、非导出浏览器 Ed25519 签名、设备 Identity 签名共同绑定：purpose、双方公钥、代次、客户端随机 nonce、三个 Core 绑定值及最终 Noise hash。身份签名在 Noise 加密记录中传输；客户端收到并验证加密 binding ACK 后才允许应用消息。

BLE 恢复和普通 NKN 使用不同 purpose/domain/action 集合。应用正文最多 1024B、记录最多 2048B、一个在途请求、严格方向/序号、唯一请求 UUID、最多 128 个请求、总会话最多 300 秒，并服从原 Core/Root 更短期限。握手总预算 10 秒，不逐包续期；取消、错误、重放和超时关闭。NKN 成员 bootstrap 也消耗请求配额，所以普通业务最多 127 次。密码库内部 nonce 不由 GATT fragment counter 或路由计数生成。

恢复路径现在经过真实 `SecureRecoverySessions`、原 `RecoveryRequestVerifierV1`、真实 Store grant/同 Auth capability、D 的原恢复执行和有作用域的加密回复。Core 在实际内核准入的同一描述符上进行握手与派发；Auth 锁不跨 DH 或网络等待。独立 worker 超时或调用者取消后仍持有真实描述符和 admission slot，直至实际返回；旧输出不能复活会话。

`ble::CoreRecoveryPort` 是实际有界 Unix 描述符转接，检查内核 Core UID，固定生产连接路径；低权 BLE 进程不读取设备身份、解密业务或自行授权。断开和未完成 future 的销毁会关闭该原始连接。浏览器 `createRecoveryBackend` 调用实际 Web Bluetooth API，固定 UUID、20B attribute/T5 分片、真实存储密钥及 WASM；`requestDevice` 在第一个 await 前调用。它保留原控制器的写前持久标记、原请求查询和不自动重发语义。

普通 NKN 的网络成员算法按固定 nknguard 上游实现兼容校验，另有真实根密钥 Ed25519 签名覆盖完整已验证设备转录、网络及证明。成员根密钥必须独立于普通客户端和设备身份。成员证明不创建 Principal，不代替普通身份签名，更不能把恢复 grant 变成 Owner。

Store 在同一事务读取真实 Principal、唯一活动 Owner、realm 和 line。仅实际已登记的 Owner/Client 可建立普通会话。有效 purpose2 身份及成员签名产生私有 scope 后，Auth 的原普通会话分配器按**同一 Principal/权限**颁发一次来源绑定的 bearer：必须在本次原始 10 秒证明窗口内，一次性 CAS，不可用同一证明反复刷新登录新鲜度。bearer 仅通过加密 bootstrap 返回。所有普通 Auth 调用继续核对原权限，且核对同实例/会话/连接/传输/Owner/客户端代次/realm/line/期限；源断开或撤销立即使其失效。现有 LAN 会话不被改成远程会话。

这解决了把原 Auth challenge 的 `Vec<u8>` JSON 直接塞进 1024B 路由预算的实际尺寸问题：没有增加预算或引入另一份压缩 Auth JSON。本候选 NKN 桥拒绝 Challenge/Authenticate/SessionChallenge/RenewSession；新的登录需要新的完整会话证明。普通业务仍发送原 `AuthRequest`，保留其原 UUID，并由原 `authenticated::handle`、原 provider 和原状态机处理。

`NknTransportPort` 已实现已合并 A26 路由库的真实端口。service 从实际认证加密的 NormalCore/CallMedia action 导出，业务计数只规范化协议固定的前置记录偏移，实际 AEAD nonce 始终由 snow 管理。真实 NKN 来源 UID 与 BLE/Edge/forwarder/network/Core 分离；普通入口不接受 BLE UID。父连接 EOF、取消及到期通过私有 gate 立即使 Auth scope 失效，原 worker 仍持有实际资源至排空。

媒体经过原 Controller、原 Ticket/Authority/Permit，重新核对当前媒体 Owner、peer/call/line/lease/权限及原请求 deadline。既有 Permit 增加一次性、只能收紧权限的撤销 fence；源会话失效使同一 Permit 立即拒绝，不等待续心跳、不延长租约。普通本地媒体路径保持原行为。没有任意 UDP/TCP/LAN 代理。

## 验证边界

相关六个 Rust 组件全部特性回归：677 通过、0 失败，普通 suite 中 2 个显式 UID 测试忽略。两者只在网络关闭的独立容器中用 `--ignored --exact` 执行；忽略不计为普通 suite 通过。严格 Clippy/all-targets/all-features、fmt、diff 检查通过。完整证据应绑定下方最终验证提交，不能沿用旧 PR 的 CI。

实际 NKN UID 编排：Core=1001、NKN=1002、BLE=1006，仅为隔离夹具；七个错误 UID 被内核凭据检查拒绝。原 Core Health 回复、错 pin、错成员密钥、旧密文、旧令牌、篡改、断连、撤销均实际执行，合计 9 次 libtest 执行通过，不冒称九个不同测试函数。

实际 BLE 恢复 UID 编排：真实 Noise、Identity、Store、Auth、原 Core QueryOperation 与实际 BLE Core 端口；错 pin/篡改/旧密文/断连/撤销拒绝。它和普通 NKN 编排分开，不能把二者相加当成真实无线端到端。

Chromium 的 18 项实际 WASM/WebCrypto/IndexedDB 检查通过，包括生产浏览器候选工厂、固定特征/有限分片、密文篡改和迟到设备选择。浏览器使用合成 GATT 与原生密码 peer，业务是明确的合成 echo；真实 Core 业务由前述独立 UID 编排验证。没有把 echo 写成 Core 成功回执。

新增 Core 媒体检查使用实际 Noise scope、真实 Auth/Store 和原 Ticket/Permit；物理语音 backend 是原有私有 D-Bus FakeVoice。源会话关闭后同一 Permit 立即失败，迟到 pulse 被拒绝；没有真实拨号。

可重跑入口：

```sh
cargo test --offline --locked -p rustsimhub-core -p rustsimhub-auth -p rustsimhub-media \
  -p rustsimhub-ble -p rustsimhub-recovery-session -p rustsimhub-nkn-agent --all-features
cargo clippy --offline --locked -p rustsimhub-core -p rustsimhub-auth -p rustsimhub-media \
  -p rustsimhub-ble -p rustsimhub-recovery-session --all-targets --all-features -- -D warnings
python3 crates/core/tests/run_recovery_kernel_uids.py --secure
python3 crates/core/tests/run_recovery_kernel_uids.py --nkn
node configurator/tests/gatt-framing.mjs
```

容器仅使用原固定 Python digest，network=none、只读根、有限 tmpfs/内存/PID/capability；未修改主机 NSS/账户。测试程序仅移除 debug section 后进入容器，报告记录实际执行文件 hash。浏览器 runner 和 WASM 构建入口在 `tools/run-web-recovery-session.py`；既有控制器/cancel/static-update 检查也保持。

## 继续处理的缺口

本包是可审查的实际隔离实现，`crypto_wire_frozen=false`。密码/依赖/内存擦除/CSP 的最终审查、ARM/真实浏览器 BLE/设备验收没有由本账号代签。snow 所需额外 ChaCha feature 是此前记录的 AES-only 上游编译问题处置，实际协商仍固定 AESGCM；保留既有来源/许可及未完成审查说明。

A13～16/A22/A25 仍需含实际 SDK 的完整子进程、独立身份、固定 Root/出口/租约提供者及两次独立 native/ARM 构建；目前普通 Core 宿主的父 deadline 和安装角色由可信宿主提供，不能把测试角色或传入 Instant 写成已验证 Root lease。没有真实 NKN 网络实测。

A27/A30 仍需完成实际 BlueZ 生命周期宿主装配、保守 20B 链接预算与浏览器工厂的产品接线、WASM/JS 的静态制品清单/CSP/PWA 打包。当前默认页面和 daemon 未启用候选后端。

A29 的 queue4/worker5/七阶段 Core Fence/显式迁移兼容仍继续实现；本包没有迁移生产数据库或发邮件。C 组发行组装、主线签名/部署、实际 Root/cgroup/硬件/外发验收仍按真实证据处理，不改全局台账。

无生产身份、通信数据、设备操作、外发消息、链上交易或部署。来源为固定 snow/既有 SDK 候选链及 nknguard 指定算法，新增源码沿用 AGPL-3.0-only；OpenAI Codex 辅助实现与检查。交给 Viper-Boss 审核后继续下一包，不自行合并。
