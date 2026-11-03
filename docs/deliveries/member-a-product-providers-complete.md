# A22/A27/A30 三组产品候选收尾交付

2026-11-03，账号 wnfhsj88。承接负责人已合并的 PR178、PR184、PR187；本包没有
重做它们或修改其分支。最新 main `6965b62` 已正常合入。初始产品装配提交
`a7afda8`，运行闭环修正及本文所述最终实现为
`5ad537d9dbf4c3fba61778d81f79746c02d0d793`。完整制品来源、哈希、测试和限制见
[验证清单](../nkn/product-providers-validation.json) 与
[SDK/Root 新制品清单](../dependencies/nkn-product-programs-candidate.json)。

用户此前明确转交 Core/媒体桥及 Root/密码/SMTP 依赖的隔离代码实现。本包将最后
三组可独立实现、隔离验证的工作收口，交 Viper-Boss 审核；不转交部署、生产签名
或最终验收权限，不改全局台账。整体仍为 81.5%、20/169。

## 本包完成的代码

| 组 | 最终行为 |
| --- | --- |
| A22 | 实际 SDK stdio→Root 独立只读观察连接→原 T5→同 Core 的 Owner 状态/控制 API→实际 Edge 页面；原三个引用、CAS、精确回执和退出证据分别保留 |
| A27 | 实际 BlueZ 生命周期/回调/PeerLease 与按需 Core 连接；同 Auth/Store 的安全恢复 listener；配置器实际 WASM、安全 backend、预登记和限定 CSP/PWA 静态制品 |
| A30 | 实际客户端 SDK→隔离节点→Root 管理的生产 SDK→真实成员/Noise/Auth→原普通 Core Health，以及同 Core 的 BLE UID/Noise/grant/schema9 原请求查询；三引用释放和原实例退出完整验证 |

接口、固定路径、Root-owned 私有材料格式、两个明确 Core 前缀、独立 BLE 程序及
配置器构建方法见[产品候选说明](../nkn/product-providers-candidate.md)。所有新入口
默认关闭。没有自动生成生产成员 secret、自动迁移 schema、广告时提前消耗握手
截止、自动跟随新 BlueZ、丢回复后重发或自动发布。

Root 观察必须是另一条实际 Core UID 准入的连接；观察 EOF/超时不能释放原 owner
FD 或停止共享 SDK。实际进程 epoch、原授权 generation 和保守样本年龄分开，
无来源的 DHT/路径/流量/节点数保持 unknown/null。新页面不将释放一个引用写成
进程已退出，也不将 not_found 写成未执行。

真实组合联调暴露并修正了三处运行问题：监督器与控制入口争用导致 Unknown；
短暂 Auth 锁争用被握手误当作认证失败；广告阶段预连 Core 会提前消耗十秒截止。
现在监督器与控制通过原动作锁串行化，排队后重查原 Owner；原连接工作者在原
截止内等待 Auth，取消仍保留实际 FD/准入槽直到工作结束；第一次真正 Auth 帧
才连接 Core，取消或断开后不重连。组合验证明确制造 Auth 锁争用并确认通过。

## 实际验证

| 验证 | 结果 |
| --- | --- |
| Core 库/二进制、BLE、Edge、wire 原生 suite | 524 通过、0 失败、4 项专用入口仍为 suite ignored |
| 不含 dev-fixtures 的生产候选特性检查、默认 Core/Edge/BLE 检查 | 通过 |
| 四包全部 target/feature 严格 Clippy、fmt、diff | 通过 |
| 实际 SDK 与全部引擎包 Go/race | 通过；候选预期和 legacy PRNG 控制仅用于隔离测试，不改变产品环境 |
| 实际 Root/SDK、Rust Owner 与内核 cgroup/nft 五场景 | 通过；独立观察 EOF、重复读 stale、三引用、明确停止、owner EOF、原租期及策略篡改证据保留 |
| 完整组合的三个原生测试入口及两个实际 SDK | 通过；普通 Core Health、真实 BLE UID 恢复查询、Auth 争用、只释放 Remote 留两引用、最终退出确认 |
| SDK/Root native/ARM 两轮各自新构建缓存 | 八份 ELF，四组逐字节一致；ARM 未执行 |
| 实际组装配置器 Chromium 154 | 限定 CSP、真实 WASM、非导出 CryptoKey/IDB、用户手势、静态离线缓存及 18 项原生/WASM/WebCrypto 互操作通过 |
| 生产 JS 接收实际 SDK/Root/Core 捕获样本 | 通过；这是序列化兼容证据，不会使历史样本重新变新鲜 |
| 网页防重/Unknown/回执/时钟/代次/退出登录、既有配置器与 SMTP 页面回归、静态 builder | 通过；builder 三项检查通过 |

普通 suite 的专用 ignored 入口没有伪装成 suite 通过；Root 原入口及新增组合入口
另在独立 UID/网络/cgroup 命名空间执行。原其他 UID 失败矩阵保留其已合并交付的
证据，不将本次正向组合代替那些全部场景。Core/media 的真实授权失败测试仍包含
在本包 Core suite 内，不把合成无线后端当作真实电话。

关键命令及实际本地日志位于 `work/remaining-full/product-5ad537d-*`：

```text
CARGO_INCREMENTAL=0 cargo test --offline --locked -p rustsimhub-core --all-features
CARGO_INCREMENTAL=0 cargo test --offline --locked -p rustsimhub-ble --all-features
CARGO_INCREMENTAL=0 cargo test --offline --locked -p rustsimhub-edge -p rustsimhub-wire --all-features
CARGO_INCREMENTAL=0 cargo check --offline --locked -p rustsimhub-core --no-default-features --features product-providers-candidate,email-production-candidate
CARGO_INCREMENTAL=0 cargo clippy --offline --locked -p rustsimhub-core -p rustsimhub-ble -p rustsimhub-edge -p rustsimhub-wire --all-targets --all-features -- -D warnings
python3 tools/nkn-build/sdk-managed/run_kernel.py --inputs work/product-combined7-inputs --tools work/nkn-root-kernel-tools --output work/NEW --combined
python3 tools/build-recovery-configurator.py --output work/NEW --wasm-bindgen /workspace/toolchains/wasm-session-cli/bin/wasm-bindgen
node tools/test-web-nkn-product.mjs work/product-combined-5ad537d/report.json
```

SDK/Root Go 源码冻结于 `7505a025f34f75beed9075cf343b71f38c106eed`，后续实现提交只
修改 Rust/页面/夹具/文档，未改变 Go 产品源码。新制品是新的独立验证 stage 和缓存，
没有修改或借用旧 PR 的制品哈希。Core/kernel 程序是实际模块的显式测试程序，
不是已签名发行程序。此前 SDK 依赖/许可/通告可达性边界保持，不能把源包扫描
无可达匹配解释为正式安全审计。

## 保留的验收边界

内核场景使用真实 UID、实际 SDK、实际 Root helper、实际 cgroup/nft 和隔离协议
节点；网络状态文件与 systemd 单元形状由本容器夹具提供。没有执行真实 PID1
服务安装、真实 NetworkManager 生产者、手机 RF 或公开 NKN 网络验证，没有真实
邮件、蜂窝流量、ARM 运行、生产签名或部署。正式应用/launcher/Root 单元参数、
回退兼容和发布装配仍由主线对精确制品审核，不把手动候选启动当作正式安装通过。

原九步不能因本轮代码收口而标为全部验收。真实设备矩阵、正式密码/依赖审核、
主线签名装配及最终签认继续保留，所有合并和部署都由 Viper-Boss 决定。
