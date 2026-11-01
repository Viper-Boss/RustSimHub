# A27：锁定 Noise 依赖的真实原生/WASM 评估候选

基于 main `ce9ccb8debb61db49860fe795c1f3e471b749f4c`，账号wnfhsj88。
PR144的A26策略仍独立待审；本包从main开始，不包含或重交PR144，也不依赖D未合并栈。
此前PR129没有可核验snow包/浏览器构建证据；现在补实际来源、失败样例和隔离运行。
**production_ready=false、crypto_wire_frozen=false**，不将标准算法可运行当应用协议批准。

## 固定来源及真实特性阻塞

| 项目 | 已核验事实 |
| --- | --- |
| 注册表 | 同一 `https://crates.io/api/v1/crates/snow` 本轮实际200；此前403记录仍是历史事实，没有绕过网络拒绝 |
| 包 | snow 0.10.0，未yank，声明Rust>=1.85，本轮Rust1.98 |
| 官方下载字节 | 899770B，SHA256 `599b506ccc4aff8cf7844bc42cf783009a434c1e26c964432560fb6d6ad02d82`；Cargo锁和实际缓存archive再次比对 |
| 包内Git声明 | `4bb43f50370bdb3e8b1b57814ac662864db2704f`，mcginty/snow；同提交README已另读核对，不把包内声明当独立签名证明 |
| 许可/审计 | Apache-2.0 OR MIT；原文完整保留，README明示未经过正式审计 |
| 标准测试套件 | 固定 `Noise_XX_25519_AESGCM_SHA256`，直接调用snow，不写ECDH/HKDF/AEAD/Noise算法 |
| 实际最小特性失败 | 仅Curve25519/AES-GCM/SHA2/getrandom：snow default.rs:326/349，E0599/缺KeyInit导入，真实退出101 |
| 评估取舍 | 显式额外启用 `use-chacha20poly1305` 以取得上游有条件导入；**不改上游源码**，不开放套件协商/降级。多出的依赖必须审核，不能声称AES-only最小构建已通过 |

独立最小失败probe没有本项目封装，已锁版本，`cargo check --locked --offline` 必须
实际产生上述错误；主评估的正常构建与这项预期失败分别记录。不能靠忽略编译失败
或改registry源码把“最小依赖已验证”写绿。

## 已执行的标准密码及跨端边界

`tools/dependencies/noise-evaluation/` 是独立、publish=false的Cargo workspace，
不加入根Cargo.toml/Cargo.lock。它只有测试封装、有限本地probe和合成浏览器夹具；
不实现SecureSessionPortV1/签名域/设备pin/恢复grant/Core verifier，不读产品秘密。
生产Core/BLE/网页/PWA/Root默认入口均未改变。

11项Rust测试实际调用同一snow：真实随机独立密钥/握手/双向传输，完整握手之前
不能进入transport，篡改/截断/多字节/反向/重放/乱序/跨会话及关闭后输出拒绝。
首包payload确实明文的断言也通过，不能把Wi-Fi口令放在Noise XX第1消息里。
使用的公知固定私钥只属于随包KAT，不可作为默认会话密钥；普通constructor只能
通过真实系统/浏览器CSPRNG生成独立钥匙。

维护者随包 `tests/vectors/snow.txt` 共408条，本包只抽取目标套件1条，逐字节核对
3握手+2传输消息的既定密文；原native和浏览器WASM都运行该向量。原文件SHA256
`69da433305fd045f6c9f01b656662a389d022688986fd39fbe7af009cd402fd3`，所选JSON
`91bf4be3e0d2499b62a5a11a236992112b31ac0fd77bf26f67e072c1479e9f39`。
期望值来自校验通过的上游包，不由本项目实现生成；不冒称官方认证或正式审计。
4项真实来源检查补测确认改密文、删版权通知、损坏archive均拒绝。

真实Chromium154/WASM与原生子进程共17项检查：两端分别为initiator、双向传输、
Unicode/空/1024B正文、不同新会话转录、双向篡改及浏览器截断/额外字节/重放/
乱序/反向/旧会话拒绝、超限关闭、未完成握手拒绝、实际Crypto.getRandomValues被
调用及拒绝时无降级。真实密文由snow生成/消费，仅外部随机失败输入是合成注入。
没有伪签名、手拼nonce或把T5分片序号当AEAD nonce。

夹具一页 `script-src 'self'` 实际阻止WASM编译并观察到CSP violation；另一页只
追加特定 `wasm-unsafe-eval` 后实际执行成功。只改测试服务器响应，**没有修改
真实configurator CSP**。该CSP扩展仍须负责人按特定资源审核。

## 依赖/制品事实及未决点

机器可读证据在 `noise-snow-0.10.0-candidate.json`：主独立lock含55个registry包，
实际archive全部按lock checksum核验。native选择41个、wasm选择44个依赖图节点
（包括构建/宏依赖，不等于全部链接进WASM）。同时保留声明许可、实际文本hash及
直接/间接关系；不以声明代替许可批准。r-efi5.3.0随包缺独立license文本，明确记
缺口；它不在本次native/wasm选择图中，不谎称已审通过。工具链/wasm-bindgen-cli
的完整构建依赖不在这55包内；CLI锁0.2.130及其安装警告单独记录。

显式OSV查询55个公开包名/版本，本轮无匹配项，时间/原始响应hash保留；不是完整
安全审计或未来无漏洞保证。评估没有根据无匹配结果自动批准依赖、更新生产lock。

初步真实release产物：生成JS14606B、WASM152278B，合计166884B，均低于T5单文件
512KiB/总2MiB；精确最终源码/摘要见本轮交付报告。不把这些文件加入PWA或发布。
Rust与浏览器复用同一状态机，不存在第二份手写JS Noise。

源检查无法建立可靠的私钥内存零化保证；snow保存的数组及其Drop没有因此变成
已审擦除实现。close/free只证明本地入口拒绝，不证明所有内存已擦除。
最终协议还须批准独立密钥生命周期、设备pin/Ed25519身份绑定、完整转录/nonce/
purpose/role、实际Core连接来源与grant撤销、10秒握手/会话截止和2048B最终记录
开销，并实现真正pin/撤销/旧grant等跨端向量。**本包没有实现或测试这些应用权限**。
不把A26路由策略或标准Noise传输加密提升为Owner/Recovery授权。

负责人可先审固定包/上游失败/额外特性与依赖清单，再选修复后的版本或明确接受
该评估特性集合；随后按获审应用绑定实现A27产品库、D的真实producer及主机接线。
无生产数据迁移/安装；回退本包只移除独立评估目录及本人说明，无产品状态变更。

来源/许可：保留snow维护者、固定源码/许可和向量来源，新测试/工具AGPL-3.0-only，
OpenAI Codex辅助。没有复制第三方算法源码、vendor实现、二进制或私有维护材料。
