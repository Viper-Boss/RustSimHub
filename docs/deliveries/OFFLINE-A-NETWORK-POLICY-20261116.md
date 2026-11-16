# A4：网络限额原ID持久记录与Unknown核对

限额请求的响应丢失时，旧`savePolicy`会清空内存pending并允许新ID保存；注销/刷新也丢失该标记。
本修复先持久记录原意图并读回，再允许一次业务提交；响应丢失、403、写入/读回失败及重开不会自动换ID重发。
同原PR263分支交付，前一笔会话隔离修复保留，不自行审核/合并。

## 固定来源与范围

- 账号yh1030；分支`codex/offline-a-network-web-20261116` → main。
- 本子包起始HEAD `32b1474e773775dda49436f00d39dabd42d8e171`，已含main `6e1cd19bc9dd05fd150157c422590c1a801cc0b0`和任务书产品基线`2ae840fbffff04691ccf8cedd49d554093dd4032`。
- 实现HEAD `e287d87007f246ec6af281bad2fe81f8309bb5cd`；受测tree `1b47e5cfb5169f8107e6fdbb74c7ed4b8f9d2920`。
- 完整Git归档SHA256 `b3da7fab84ed44b4bd71339f67e06c47377c1291d4da11b716c2aa1f29217a84`。报告补充提交只改本文及原两份A4材料，产品/测试字节保持；PR正文另写当前完整HEAD，避免循环引用。
- 关联WEB-02/05/06/07和NET-03/04；本包补一个已有真实缺口，不声明全部A4/93项完成。
- 产品改动仅`crates/edge/src/web/network.js`、`app.js`的必要装配/登录隔离、`index.html`的回执提示/本机未知归档按钮、`style.css`的长ID换行。
- 测试仅原network suite、新`tools/test-web-network-policy.mjs`、实际主页fixture/assembly。Core/Root/wire/schema/网络权限及依赖/锁文件不变；未接管B、configurator、热点、蓝牙/AirSIM/native/media或全局台账。

## 实际行为

1. IndexedDB键按**签名挑战已核验的设备公钥pin**隔离，不采用可编辑的地址或pin输入框。保留原ID、限额参数及计量中断确认布尔值；不保存token/种子/密码。
2. 原状态库的单个读写事务比较整份旧记录，防止同一浏览器存储中的两个页面各自覆盖pending。写入完成后严格解析并完整读回；失败就不发POST。
3. 注销/pagehide只清当前显示及忙槽，原记录保留；登录先停止旧网络面板，旧保存/读/归档的epoch、设备pin和本轮代次必须仍有效。新页面读取只查询`network_diagnostics`，不补发原设置。
4. 只有**原ID、三个策略字段、policy_applied=true、有效boot/采样元数据全部匹配**才归档为`observed`；原意图要求确认计量缺口时，还必须观察到`usage.uncertain=false`。相同策略但不同ID、相同ID不同策略、应用标记缺失/false均不确认。
5. 该回执只表示某次快照中原策略与应用标记已核对，不能冒充以后其他客户端的最新配置。当前设置单独在刷新状态中显示；迟到的失败响应不能覆盖已经核对的本机回执。
6. 未明结果保留pending并阻止本页新保存。用户可明确确认“保留未知记录并解除阻止”，在同一事务保存`unknown`摘要；这只改本机记录，不查询、不撤销、不重发。之后须修改表单再明确保存新意图，旧Unknown仍保留，计量缺口勾选清除。
7. 最多64条归档。只可移除最旧的`observed`摘要，绝不自动删除Unknown；全64条Unknown时拒绝新保存/归档，留维护处理。损坏、重复ID、跨pin、字段/范围异常均拒绝，不回显原损坏数据。

**现有接口限制保留：** Root监测仅发布最后请求ID和当前策略，没有完整历史请求查询。当原请求已被另一客户端覆盖且没有可核对证据时，保持Unknown；不据当前策略相同推定原操作成功。
本机CAS只覆盖同一IndexedDB，不宣称不同浏览器/profile或旧网页已拥有服务端CAS。旧客户端不会消费新浏览器摘要，不能据此保证旧客户端自身不再重复提交。
后端最近Owner五分钟/Scope及Root原校验照旧；关闭限额或本机归档不会启用蜂窝、恢复APN或更改主机路由。

## 最终验证

Linux均在WSL2 Ubuntu24.04、普通UID1000、原生Linux source/target/TMPDIR，固定Rust/Cargo1.98.0、Node24.22.1，原锁/offline。

| 实际命令 | 退出码/实际边界 |
|---|---|
| `node tools/test-web-network-policy.mjs` | 0；原ID先落盘、丢回应/重开、写入/读回失败零POST、精确内容/应用标记/缺口确认、CAS双页面、pin隔离、注销/迟到、有限Unknown归档和损坏拒绝 |
| `node tools/test-web-network.mjs` | 0；原渲染/输入/图表/权限及会话忙槽断言保持；原成功用例补真实匹配策略/应用标记，不以last_request_id单独判成功 |
| 其余27组原Node | 全0；wifi、nkn-product、nkn-status、usage-stats、usage-participation、forwarding、forwarding-history、configuration-state、event-sync、daily-sms、sms-management、daily-phone、call-test、incoming-call-test、operation-receipt、deadlines、indicator、clients、nas-bridge、diagnostics、test-center、test-center-component、console、console-fencing、session、app-imports、app-imports-negative |
| `cargo test -p rustsimhub-edge --locked --offline -- --nocapture` | 0；71通过/0失败/0忽略 |
| `cargo clippy -p rustsimhub-edge --all-targets --locked --offline -- -D warnings` | 0 |
| `cargo fmt --all -- --check`、`git diff --cached --check` | 0 |
| `node tools/test-web-app-assembly.mjs` | 0；Windows已安装Edge Chromium157.0.4341.60/Playwright1.65.2，真实原主页/ESM/CSP/IndexedDB/合成Owner，320/390/1280均通过；真实NKN模块404负对照保留 |
| `node tools/test-web-console-browser.mjs` | 0；同tree实际控制台三宽、长中文/65短信/注销与权限状态原断言保持 |

实际浏览器每种宽度均：一次限额POST后响应体丢失→检查真实IndexedDB已保存原ID→真实reload/重新签名登录→只读恢复原ID且POST总数仍1→相同ID错误策略/未应用保持pending→匹配策略/应用标记后，同事务归档observed。
随后原网络403、在途主动测试注销/同意清理、统计/NKN等流程继续通过。所有API有限合成、仅回环，无真实接口探测或公网。
Node用例中的原子存储故障/双页面是可控端口；不把它写成真实磁盘ENOSPC或两个真实浏览器并发事务。
真实浏览器确实执行原装配/IndexedDB存取/刷新，未用合成HTML替代原页面。没有重跑任何热点专项。

## 严格负对照与原失败

- 最终测试字节/新增HTML与CSS保持相同，只把`app.js`与`network.js`还原为子包起始HEAD。负对照tree `781473831ca3197a893f0473b5195eab8ad9542e`，Git归档SHA256 `bc6f8909916db2a3133ba784846d71a2156bb36e54ccd7b636c64122a1b5bad2`。
- Node实际退出1：`Original ID must be durable before a lost-ack POST`，持久记录undefined而已POST的原ID存在。
- 真实浏览器实际退出1：`Original policy ID must be durable before a lost-ack POST`，并非缺导入、页面元素、启动/编译错误。首个新断言退出，不冒称所有新增用例各自都做了独立负对照。
- 最初浏览器夹具在响应头前直接断TCP，Chromium实际透明重发同一原ID三次，“恰好一次”等待失败；原日志保留。这是传输重放边界，不能从一次JS调用推定绝对一个HTTP请求，也没有把三次同ID说成新ID。最终夹具改为明确写出HTTP头和部分JSON后断响应体，准确验证一次提交后结果未知；没有更改产品fetch或放宽原ID断言。
- 后一次新增reload使原调试details收起，原统计按钮click超时。夹具恢复刷新前明确展开的support-panel，原按钮/业务断言全部保留；不改产品来迁就夹具。
- 初次交付读取将8个大文件一次输出，JSON被工具截断、解析失败，尚未创建远端树/更新分支；改为逐文件读取后按Git tree精确相等核验。不计产品或测试失败，原测试归档不变。

最终私有证据ZIP61项、1,179,588字节，CRC逐项检查通过，SHA256
`0967bfce23e0b2b74acb18c7f3596064e879467b3eb0e9ccc2d965dcadb13963`。
包括最终结果、正反源码摘要、截图和上述原失败；原始日志只在本任务忽略目录，不上传用户真实数据或二进制。

仍未运行：真实设备/Root网络守护进程端到端、不同浏览器全局竞争、真实掉电/ENOSPC、ARM/B发行候选、生产签名/安装、普通蜂窝/流量或公网。云端CI单独回读，不把本机通过冒充云端成功。
新代码/文档AGPL-3.0-only，原来源和许可保持；AI辅助实现/测试/审计。
下一步继续A1统计签名与有限提交适配、剩余27项NKN/统计/远程逐ID审计及短信/电话明确缺口。热点专项延期，A包尚未全部完成；整体已验收 **81.5%** 保持。
