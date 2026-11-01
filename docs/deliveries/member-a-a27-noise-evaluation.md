# A27：固定依赖及标准Noise真实跨端评估交付

账号wnfhsj88，2026-11-01；起始最新main
`ce9ccb8debb61db49860fe795c1f3e471b749f4c`。本包实现及精确受测源码
`a28e2e88f66090c4e91dd80ac5059d65bbe9ad06`，最终评估开始时
candidate_sources_dirty=false。之后仅补本报告及保留上游Apache原文字节的定向
whitespace属性，不改变Rust/JS/Python实现、锁文件、向量或编译制品。
最终完整PR HEAD以PR正文/GitHub回读为准。

分支 `codex/wnfhsj88-a27-noise-evaluation-20261101`，同仓库非fork。
A26 PR144继续独立待Viper-Boss审核，本包不包含其实现或其他尚待合并的D栈。
已合并PR129/128/133/134等保留，不重做或改写作者。

## 实际交付与边界

新增隔离 `tools/dependencies/noise-evaluation/`，固定snow0.10.0及独立Cargo.lock，
实际标准Noise原生/WASM同源码、完整维护者KAT、真实浏览器/原生互通、失败输入、
系统/浏览器熵与CSP、来源/许可/通告快照。具体来源/原失败和主线审批点见
`docs/dependencies/noise-snow-evaluation.md`，机器可读55包证据见同目录candidate JSON。

根Cargo/lock、生产crates、configurator、firmware、Actions均无diff。根Cargo metadata
实际确认评估crate不属于生产workspace，没有引入产品依赖或改默认入口/Scope/DDL。
没有实现应用签名域、设备pin/Ed身份绑定、grant、Core producer/权限桥或WASM产品
接线，production_ready/crypto_wire_frozen仍false，不把标准Noise互通称A27整步完成。

## 最终真实结果

Linux x86_64 / Rust1.98 / wasm32-unknown-unknown / wasm-bindgen0.2.130 /
Chromium154.0.8037.57 / Playwright1.56.0。启用原Rust工具链和隔离评估工具前缀后执行：

`python evaluate.py --run --online-advisories --output /workspace/RustSimHub/work/a27/run-final`
（实际使用隔离Playwright Python，工作目录为评估包；总体退出0。）

| 检查 | 实际结果 |
| --- | --- |
| `cargo test --locked --offline` | 11通过/0失败/0忽略，另0 CLI test/0 doctest |
| `cargo clippy --all-targets --locked --offline -- -D warnings` | 0，严格通过 |
| `cargo fmt --all -- --check` | 0，通过；根workspace格式另检通过 |
| `python -m unittest -v test_inventory.py` | 4通过，真实来源改密文/删版权/损坏archive拒绝 |
| 原生release、wasm release及同版本bindgen | 均0；没有自行拼加密、nonce或签名 |
| 实际Chromium/原生有限probe | 17检查通过；两端initiator/双向加密、tamper/replay/reorder/旧会话/限额/未握手/真实熵拒绝/CSP |
| 维护者KAT | 1固定向量的3握手+2传输消息既定密文，原生及WASM均核对；不是额外5项测试或官方安全认证 |
| 最小特性独立probe `cargo check --locked --offline` | 实际101，snow E0599/KeyInit；作为已复现阻塞，**不记最小构建通过** |
| 55个registry archive | 全部对独立lock checksum匹配；native图41/wasm图44（含host构建/宏依赖） |
| OSV公开包名/版本查询 | 成功，55包本次无匹配；仅时间点查询，非安全审计/批准 |
| Git diff与生产workspace元数据 | 定向保留原许可EOF空行后diff check通过；生产代码/配置/根依赖无变化 |

最终JS14606B，SHA256 `5a4a82445e75bf202c8b34b202b596dc193aa5abb07a9a0c5d9d7b3d2b1e312a`；
WASM152278B，SHA256 `3dac2dc17e804c5459ad00f859e8d14fa7d4533477768ad4a12e1c1007eb944c`。
合计166884B，满足现有单文件512KiB/总2MiB的尺寸上限；未加入PWA或发布。两次增量
构建观察到相同字节，但没有独立空缓存复现构建，不冒称完整可复现发行通过。

没有改生产代码，因此不重跑无关整仓Rust/Go或借PR144的1196结果填本包通过。
本包新检查均直接运行真实依赖/本地进程/Chromium，密码操作没有用Mock代替。

## 真实失败、限制与下一步

- AES-only最小特性真实编译失败，完整日志保留；评估额外开启ChaCha特性获得上游
  有条件导入，源码未修改，55包清单未藏这项取舍。生产采用仍须明确审核或选修复版。
- 初稿封装的较大enum被严格Clippy拒绝，已按建议Box握手状态后原严格检查通过，
  没有allow该lint。CSP检查初稿把`WebAssembly`错误名误按`wasm`子串查找而失败，
  已改为核对真实编译拒绝与实际CSP violation事件，不放宽产品CSP。
- snow明示没有正式审计；可靠密钥内存擦除尚未建立。close/free是本地失效，不是
  所有复制/内存已擦除证明。XX首包payload明文已由真实测试证实，不能放产品口令。
- r-efi5.3.0随包缺独立license文本，声明及缺口保留；不在本次native/wasm选择图，
  不标许可审核通过。工具链/CLI的完整构建依赖未纳入55包；CLI安装保留buf_redux
  0.8.4/multipart0.18.0未来兼容警告，不能据此称构建工具全审通过。
- 严格self CSP真实阻止WASM；仅评估服务器增加特定wasm-unsafe-eval才可运行。
  产品CSP、资源白名单/离线缓存及Android/第二端/ARM运行仍需后续审批与验证。
- A27真正产品协议仍缺应用身份/pin/签名域/转录、Core真实nonce/grant生产者、
  撤销/原期限、最终2048B密文预算与产品互操作；普通NKN与BLE恢复不得合域。

A26 PR144最终HEAD远端run41272686063已独立核实：native-audio实际通过，test实际
在Rootfs原策略字节检查35/1处失败，后续Rust跳过。其PR正文已回写这个准确结果，
不挪用为本包CI。本包CI按自己的最终HEAD回读，主线基线故障不放宽/擅自修权限。

原始日志和制品仅留 `work/a27/`，下列为本轮最终精确运行日志SHA256：

| 工作区日志 | SHA256 |
| --- | --- |
| native-tests.log | `b17a6961656fb704df0ef0bc62341eeb745a4f8307c964ef85f7c6b7fd755db0` |
| clippy.log | `a5f4c585ee974ca44916ac30a98bbc189e067a7e0a6bc6d2e8d6bc525be724af` |
| fmt.log | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| source-check-tests.log | `073ec4f06827ada55eb08987f47711fe5ba002b87d2d9d1d6def2d1dbe35c6d6` |
| native-build.log | `3997fc6790b8063b86c119e91f87ac7395ac6cf26135da637dcb229006bc32dc` |
| wasm-build.log | `92434e198f27984c035c376ec77a7cf9e6ebdde782f8ccb1336ae3329b66ddb7` |
| bindgen-version.log | `fd69e3062d74b1da5a4f88cbc40bdd8cc9bfc885aa74d58cc481641f1169c73c` |
| minimal-feature-probe.log | `1ef4ab2d98ae702923e5d25cb859aed91972ba0b791bc57e1eb5b7fe3b542036` |
| dependencies.json | `917b06fdea5290a73e559c1eada68452206029c60379672f457134e07bfaa424` |
| osv-response.json | `a6d67de35b9c18b9d4898f8d3b8c9f3528fb0239073679fd8e85ce5cfa669757` |
| report.json | `cb0629833817fce18b8511ed450155cb65211409d4d0eefbff63f42f292f5afd` |

来源：锁定官方registry包、mcginty/snow固定提交声明、原许可与随包公开向量；不复制
第三方密码算法或vendor实现。维护者MIT/Apache通知逐字节保留，新代码AGPL-3.0-only，
OpenAI Codex辅助实现、复核与验证，无工时估造或历史作者改写。

没有生产设备/NAS/Root/真实NKN/RPC/交易/短信/电话/邮箱/蜂窝/产品签名安装部署。
只是本地工具安装与隔离测试。由Viper-Boss按精确HEAD审核，不自行合并或部署。
固定P0–P7按main仍约80%，A27评估不增加全局验收比例，剩余9步缺口继续保持。

## 正常同步最新主线后的再验证

主线后来合并PR144/146～152（其中149修复原Rootfs精确pin），本分支正常合并
main `97f9c52fa832591a5e260d3dc4181847a8b3dfe3`，保留全部原作者/实现。
只在本人剩余步骤索引产生冲突，合并保留A26九步入口并更新A27；没有丢掉主线代码。
最新完整受测合并HEAD `582c9ff37efdeb586330dd017df2076202239acd`，
新运行开始candidate_sources_dirty=false；同样实际11 Rust/4来源/17 Chromium检查
及严格Clippy/fmt、native/WASM构建全部退出0，最小AES-only probe仍实际101。
产物JS/WASM字节及摘要与前述精确结果相同；日志在 `work/a27/run-main-synced/`。

另实际执行最新主线Rootfs36项，**36通过/0失败**，根fmt通过；不再把已修复的旧
基线失败当当前阻塞。与最新origin/main比较，根依赖/生产代码/配置无本包diff；
没有将正常合入的主线代码算本人新增，也不以主线旧制品冒充本包测试。
主线最新HANDOFF/PROGRESS声明已验收81.5%保持，本轮离线评估不增加点数。

| 最新运行日志 | SHA256 |
| --- | --- |
| report.json | `e44eedba90e0c96e8d208b293ebc8435424c17a1163376eb20039ec7f089ca51` |
| native-tests.log | `eb4aedf7618beaaa5cee172f2cd977eed35921057cc7190075f44ec76210db39` |
| clippy.log | `59639588a6f1826f129cbcd121edc07279dab94786f5fa5bde6b3439b2e845cc` |
| source-check-tests.log | `da69e597899de3edaa5599286814c9f61db37f88830d5803f94c541d9b6eb4d8` |
| minimal-feature-probe.log | `1ef4ab2d98ae702923e5d25cb859aed91972ba0b791bc57e1eb5b7fe3b542036` |
