# A包：NKN页面的后端状态年龄门禁

账号yh1030；分支 `codex/offline-a-nkn-freshness-20261116`，同仓库PR到main，不自合并。
起始main `b15ea3f561bf9e2a1268f8df5b6f0973cc83bbbd`，含产品基线 `2ae840fbffff04691ccf8cedd49d554093dd4032`。
实现提交 `180ad16dc4400f57d2a7cd232f4179264cdff37c`，受测tree `578565dc097225a001e83909f677a4b463b40ab4`。
本报告后补，PR正文另记当前完整HEAD。对应A2.4/A4.5、NKN-07/WEB-05的一个实际缺陷，**不是完整A2或A包完成**。

## 问题与修改

原页面已经把后端 `age_ms` 加入文字的陈旧判断，但操作按钮只考虑本页请求/接收后的经过时间。
例如后端采样已4900ms，响应又经过200ms，页面数据实际5100ms；文字显示陈旧，按钮仍允许持久化并提交运行请求。
原自动过期timer也遗漏后端年龄。Core的原Auth/CAS仍是最终授权；本修复不声称绕过Core，但网页不能用陈旧样本引导操作。

仅改 `crates/edge/src/web/nkn-product.js`：fresh门禁统一计入后端年龄、请求耗时及接收后的较大单调/墙钟耗时，
共享原5000ms上限；timer按同一剩余时间调度。恰好5000ms仍依原契约可用，5001ms拒绝。
无引擎时Core返回 `source:unavailable, age_ms:null` 的新鲜报告仍可由Owner明确请求启动，未知计数不变成0。
没有改授权、CAS、退出/换上下文门禁、300秒租约、Remote支持、统计上报、Root/SDK、自动启动或后端协议。

`tools/test-web-nkn-product.mjs`补五组场景：三组后端年龄/响应/本页经过时间的过期组合，
剩余51ms timer，及无引擎新鲜报告仍可显式启动的正向控制。过期时两种启用按钮禁用，
直接调用set也不创建本地Pending、不发送set_nkn_enabled。

## 实际验证

WSL2 Ubuntu-24.04，kernel `6.18.58.1-microsoft-standard-WSL2`，普通UID1000；Node v18.19.1、Rust1.98.0。
正反例分别从固定完整Git tree归档，在新Linux原生source/TMPDIR运行，复用本任务拥有的原生Rust target缓存；
锁文件保持，Rust命令 `--locked --offline`，没有设备/主机网络设置或公网业务请求。

| 输入/命令 | 实际结果 |
|---|---|
| 旧生产+最终测试tree `ac230677fc3bd7f9fd9286aaa3fbe003f4d8e96e`，`node tools/test-web-nkn-product.mjs` | 退出1；`Backend age must be included in mutation freshness`断言，actual true/expected false；不是解析/依赖失败 |
| 正向受测tree，`node tools/test-web-nkn-product.mjs` | 退出0，含五组新增场景和原CAS/回执/Unknown/注销/上下文等回归 |
| 正向tree，`node tools/test-web-nkn-status.mjs` | 退出0，含原状态未知、过期、时钟、序列、权限/取消等用例 |
| `cargo test -p rustsimhub-nkn-agent --locked --offline -- --list` | 退出0，列65项 |
| `cargo test -p rustsimhub-nkn-agent --locked --offline -- --nocapture` | 65通过/0失败/0忽略；0项doc-test不计数 |
| `cargo clippy -p rustsimhub-nkn-agent --all-targets --all-features --locked --offline -- -D warnings` | 退出0 |
| `git diff --check` | 通过 |

旧生产与正向tree中的回归测试blob完全一致，只恢复生产JS即出现行为失败。
负对照在首个新增断言处停止，不声称已分别运行全部新增反例。
正向完整Git归档SHA256 `07ee4d12b5228d9cc95acd8620e8e0097ff114c5cff928e84fff3899d1ebe246`；
负向归档SHA256 `889386b11bba4a86394b8ad4df41b8727369134a0fa7d02b2e82d99d1ac12553`。
原始日志/源码/元数据ZIP位于本机忽略目录 `.local/deliveries/OFFLINE-A-NKN-FRESHNESS-578565d.zip`，
15项/15442字节，SHA256 `bbe7a787a6929071298aeb10c161aab0c1ac9c7b2bce2751e580db816562d1a2`，全条目CRC回读通过。

65项原监督/路由库测试证明其原默认OFF、三引用释放、取消/到期/有界重连及有限路由等模拟契约在该tree仍通过，
不等于生产Root/SDK/Core Auth已集成复验。没有改Rust，因此未重复无关全仓fmt/ARM/生产feature组合；
本批没有跑真实浏览器/CSP/IndexedDB三宽装配，A4的该项仍待后续执行。Node的document是明确合成夹具，不能冒充实际主页。
云端CI提交后单独回读；未修改或绕过B的工作流。没有声称公网/真实NKN连通。

## 来源、集成与下一步

本批为原AGPL-3.0-only页面和测试的小修复，没有新增依赖/锁文件、协议、密码算法或第三方代码。
AI辅助编写，已检查真实页面代码并运行同一测试字节的正反例；实际工时不估造。
B使用本PR准确已提交SHA复验两条Node命令即可；没有新载荷/Root策略/配置迁移。
回退为撤去该JS小改动，会重新出现原过期门禁问题；不能通过延长5000ms掩盖。
后续继续A2的生产Auth/SDK/Root接口核对和A3转发/SMTP本机端点矩阵、A4实际装配验证。
统计可信Root操作先等待PR256接口审核，不因此停止其他独立部分。
未访问设备/NAS/生产通信数据、真实凭据或主线私有目录；未发消息/交易、改他人分支/作者、签名部署或全局看板。
整体已验收进度仍为 **81.5%**。
