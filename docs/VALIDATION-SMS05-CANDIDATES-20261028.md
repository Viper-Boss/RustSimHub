# SMS-05：疑似重复与跨启动易用性验收

日期：2026-10-28。源码提交：`adb7bb6e1598e1a2a5a130821c996733d19c4e65`；其包含短信测试提交 `07a98395da961898d199d57c0f65db26315875a5`。本条验收针对台账 SMS-05 的本地业务/离线范围，**没有设备部署或整机冷启动声明**。

## 原验收范围

台账 work：保留不同消息ID及可证明来源，优化候选显示/人工核对及重观察行为。

acceptance：不按正文/号码猜测物理同一短信；已恢复记录可正常查询。该动作原 needs/dependencies 均为空。它不包含 SMS-03 的原始PDU/真实分段、SMS-06的全部故障、SMS-08整机冷启动或真实换卡场景。

## 当前源码的独立证据

| 判据 | 实际执行证据 |
|---|---|
| 同号码/正文/时间但不同来源不合并 | Store 创建两个不同UUID，当前记录仅以 duplicate_candidate 引用旧ID；Unicode组合/分解形式和二进制也保持原文，不做内容归一合并 |
| 删除/恢复只作用当前记录 | 当前ID进入真实tombstone表；旧ID与正文仍可读；当前ID恢复后不换ID，候选边不改 |
| 重观察不重复副作用 | 同一可信来源重复入库返回原UUID，不增加接收事件；先前D-02的70条反向重观察与多页重开矩阵也通过 |
| 跨Store启动保持 | 实际关闭并重开SQLite，删除标记/候选关系/原文/ID保持；Ready重新确认后相同来源仍返回同一ID |
| 备份恢复可查询 | 真实backup/restore保留两份内容及删除标记；旧Owner被隔离拒绝读取回收站，测试通过可信离线恢复入口恢复原Owner后，再读取候选并恢复当前原ID |
| 卡身份隔离 | 新fingerprint下相同正文和来源不关联上一张卡候选；旧记录仍作为历史保留 |
| 页面人工核对 | 当前实际sms-management.js的Node夹具通过：只读两记录对照、明确回收站读取、登录/选择/撤权竞争、恢复与不确定写入锁；不修改候选页逻辑或放宽权限 |

真实业务模块为 `crates/storage/src/inbox.rs`、`inbox_management.rs`、Core Sources/observer、当前网页sms-management.js；新增 `inbox_content_tests.rs` 和首批 `inbox_recovery_tests.rs`。未复制测试专用状态机。本结论不依赖旧版本截图或以前的实机验收来冒充当前HEAD验证。

## 执行与结果

```sh
cargo test -p rustsimhub-storage -p rustsimhub-core --all-features --locked --offline -j3
cargo clippy -p rustsimhub-storage -p rustsimhub-core --all-targets --all-features --locked --offline -j3 -- -D warnings
cargo fmt --all -- --check
node tools/test-web-sms-management.mjs
```

WSL/Linux固定Rust1.98、非Root UID65534、空capabilities、无网络/设备、临时SQLite及私有peer：Core167 + Store61，共228通过、0失败/忽略；Clippy/fmt通过。冻结代码验证归档 `4a26803a5e5fce5765ba2b3bc25eeb9bf3949086d73299e12967527f0143ea6f`，日志摘要 `f9deae6610fb03cb277d091ef84439c374b57255fa34fd99e02734737b8a7cc1`。当前Node短信管理检查通过（含已读节流的主线增量），没有访问设备或真实消息。

被测 `sms-management.js` SHA-256为 `de039c60d4de02c03963acf9dff87cc1bcd058bf78a59ba6d819b34569d60970`，对应测试脚本摘要为 `41cf3e3ece3cd9f323b6b2f603b0052a3e0b21f019fb4127ae46e3872b8749a1`，两者都来自该固定代码提交。

结论：**SMS-05原判据在当前代码上通过，可在本交付分支登记完成，随正常PR审核后进入main。** 不把19个新增测试入口折算为19项功能；只关闭这一条已有动作。其他短信、通话、真实运营商及整机验收保持各自状态。成员步骤归属/PR审核与全局动作状态分开记录，不伪写成员已审或提前改变Git贡献。
