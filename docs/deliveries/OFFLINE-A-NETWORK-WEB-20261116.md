# A4：网络会话隔离修复与实际网页/配网/灯控复验

交付后回读的新main为`6e1cd19bc9dd05fd150157c422590c1a801cc0b0`，已含负责人整合的PR255～258，并更新任务书为热点暂缓、USB电脑工具优先。
本文AP测试保留为旧截面的实际历史记录；已停止新增热点专用开发/验收，不以延期标完成，后续只推进共享Wi-Fi事务、USB授权与其他日常功能。
下列测试仍精确绑定原受测tree，不冒称已经验证与新main的全部组合；PR263当前可合并，组合复验另做。

- 账号yh1030；同仓库`codex/offline-a-network-web-20261116` → main，不自合并。
- 起始main `b15ea3f561bf9e2a1268f8df5b6f0973cc83bbbd`，含任务书产品基线`2ae840fbffff04691ccf8cedd49d554093dd4032`。
- 实现HEAD `a41addeb320da70d676d7d8b71a23396ddaff453`；受测tree `a00522e061a46ccdfb5479ef5c6e5b077a2e1121`。
- 完整Git正向归档SHA256 `471c08a15ef8382eb17745b40cc65893a382f2ad9fea6a60d4394339b87f7e85`。
- 后续仅加本文和[28项差距表](../offline-a/network-web-audit-20261116.md)，产品/测试字节保持；PR正文另写当前完整HEAD。
- 对应NET-03/04、WEB-02/05/06/07的一个具体缺陷及A4离线复验；不声明28项或A包93项全部验收。

## 问题与实现

网络面板在stop/logout后，旧测试的finally仍会重新启用按钮；旧refresh占用loading可阻塞新会话，旧finally还可能释放新会话的忙槽。
`crates/edge/src/web/network.js`现在让read/test的收尾只处理原epoch，stop释放本会话槽并禁用测试控件；
无样本/无token/权限失败时保持禁用，注销清除旧蜂窝测试同意勾选。没有自动运行网络测试、开启蜂窝或修改后端权限。
`tools/test-web-network.mjs`新增三个会话竞态回归，实际主页浏览器用例新增本机403及在途测试注销/同意清理。

原缺导入反例移除整个loader后，会先在新增UsageParticipationPanel上失败，无法验证原目标NknProductPanel。
`tools/test-web-app-imports-negative.mjs`改为加载真实导入后只删除目标绑定，保留原退出1和准确ReferenceError断言；
六个原正向VM用例与原业务断言不变，没有用空类替代或放宽失败检查。
仅五个源码/测试文件变更：network.js、test-web-network.mjs、test-web-app-imports-negative.mjs、
test-web-app-assembly.mjs及fixtures/web3-web/assembly-browser.py。

## 固定源码的真实结果

Linux测试全部在WSL2 Ubuntu24.04普通UID1000、原生source/target/TMPDIR，Rust/Cargo1.98.0、原lock、offline。
网页ESM使用专用Node24.22.1（系统Node18不改）：官方归档SHA256
`6c172a9ab6718f7b801fb5a7bd486af2ee5e32758086a04b84fcce2aa77095e6`，保留原LICENSE。

| 实际命令 | 退出码和边界 |
|---|---|
| `cargo test -p rustsimhub-setup-ap -p rustsimhub-network-agent -p rustsimhub-status-led --locked --offline -- --nocapture` | 0；241通过/0失败/0忽略：网络16+12，AP180，灯控11+15+7；这些Rust路径从main到最终tree完全不变 |
| 上述三包`cargo clippy --all-targets --all-features --locked --offline -- -D warnings` | 0；没有额外生产feature组合，未修改其Rust |
| Node25套件，见下表清单 | 最终均0，含原业务回执/Unknown/注销等，不把套件数当测试断言数 |
| `cargo test -p rustsimhub-edge --locked --offline -- --nocapture` | 最终tree，0；71通过/0失败/0忽略；当前Edge未声明额外feature |
| `cargo clippy -p rustsimhub-edge --all-targets --locked --offline -- -D warnings` | 0 |
| `cargo fmt --all -- --check`、`git diff --check` | 0/通过 |

Node命令均为`node tools/test-web-<name>.mjs`：

```text
network wifi setup ap-wifi ap-scan-fence configuration-state event-sync
daily-sms sms-management daily-phone call-test incoming-call-test
operation-receipt deadlines indicator clients nas-bridge diagnostics
test-center test-center-component console console-fencing session
app-imports app-imports-negative
```

AP使用原生产客户端和私有P2P NetworkManager对端，覆盖保存、检查点、错密、回退、扫描、改代和丢失回复；
计量覆盖月结/短月/回拨/重启、SIM/boot、缺源、溢出和未知值；灯控只用纯模型、临时文件/FIFO及私有PID1/NM对端。
没有连接宿主system bus或修改主机Wi-Fi、路由、DNS、nft、GPIO、运行服务及设备。

## 实际浏览器（与Linux环境分别记录）

Windows已安装Edge Chromium `157.0.4341.46`，临时目录安装Playwright1.65.2/greenlet3.5.6（PyPI包哈希随pip report保留），
没有改系统Python或下载浏览器。每个测试使用新浏览器/上下文/临时profile，不读取用户Chrome/Edge登录资料或Owner钥匙。
在同一最终Git归档副本运行，设置`WEB3_WEB_BROWSER_PYTHON`/`WEB3_AP_BROWSER_PYTHON`及对应CHROMIUM变量：

| 命令 | 实际结果 |
|---|---|
| `node tools/test-web-app-assembly.mjs` | 0；320/390/1280实际HTML/ESM/原CSP/IndexedDB/真实Ed25519合成Owner登录，权限403、注销在途测试、同意清理、原ID/迟到/Unknown；真实缺NKN模块404在任何API之前失败 |
| `node tools/test-web-console-browser.mjs` | 0；三宽，原列表/长中文/时间/已终止与未知状态/计数/65条合成收件分页、刷新/代次/注销/拒绝及CSP |
| `node tools/test-web-setup-browser.mjs` | 0；三宽11场景：刷新/空结果、同ID重开/保存、能力/冷却/拒绝、离线恢复、精确取消、双标签、未验证返回、错误密码 |

所有设备/网络API是有限本机合成对端，实际生产HTML/ESM不替换为空模块。网络测试只请求本机fixture，
明确断言wlan0/connectivity/allow_cellular=false，不解析/连接真实接口。没有真实互联网测速、拨号、短信、转发或链交易。
已检查320窄屏截图及三宽overflow断言。截图中部分debug区由测试显式展开，不是默认日常页面状态，不能当用户整机视觉验收。

## 失败与负对照完整保留

- 旧生产最终对照tree `22f0dce7c1e7540a7d36f5ab2dfcdd5ef205a870`与正向使用相同最终Node/浏览器测试字节，
  唯一生产差别是network.js。Node实际退出1：`late test completion must not enable logged-out controls`；
  浏览器实际退出1：`Denied network read enabled test controls`，均行为断言，不是编译/启动错误。
  在首个违规处停止，不声称所有负例分别全部运行。较早浏览器对照还实际复现`Late network finally enabled logged-out controls`，
  其测试版本单独绑定，不能冒称它与最终新增403断言完全相同。
- 基线的缺导入反例实际失败在错误的UsageParticipationPanel；已保留日志并修复目标移除方式，最终六组正向/负向通过。
- 首次Edge装配在浏览器启动时进程退出0、Playwright报告TargetClosed，未执行页面断言；相同机器后续控制台/配网及最终三套通过，
  启动原因未确定，不将该首次失败归为产品问题或抹去。
- 第一轮私有执行器在25个Node完成后因命令元组长度错误退出1，Edge Rust未运行；执行器修正后最终25套、71 Edge、Clippy/fmt全部实跑。
  保留独立runner-failure记录，不把第一轮写成全通过。

原始证据：本机忽略目录`.local/deliveries/OFFLINE-A-NETWORK-WEB-a00522e.zip`，113项/1565761字节，
SHA256 `64fc60fab5dde4b3c5ded034780006769d0119b912b0d449e1977b7f9f3363a0`；全ZIP条目CRC回读通过。
包含固定代码/命令/退出码、初始与最终失败、最终截图及源码/工具链摘要；不提交真实凭据、profile、私钥或设备数据。

## 集成与仍缺项

B消费本PR当前完整已提交SHA，自行按上述命令导出/构建；没有新增程序载荷、协议、Root字段、schema或依赖/lock变更。
回退是撤去network.js小修，会恢复旧会话收尾问题；不能用放宽身份或延长权限替代。
沿用原AGPL-3.0-only代码和测试，新修改独立编写、AI辅助并已实际验证；实际工时不估造。
不改B、BLE/AirSIM/Native/Media生产代码、冻结coordination、成员作者、全局进度或权限，不自审核/合并/部署。
云端CI须PR后单独回读，不以本机通过代替。
待做：A0剩余45项逐条语义审计、网络限额Unknown持久回执专项、A2真实SDK/Root组合、A1新可信操作审核/实现，
以及所有设备/AP/无线/灯效/真实消息/公网/长期/断电/原卡、签名发行验收。未因不在本轮就标完成。
整体已验收进度保持 **81.5%**。
