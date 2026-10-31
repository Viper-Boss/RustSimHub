# A-05～08：持久签到状态机与失败矩阵

账号wnfhsj88，按用户指示持续完成接手的A组任务。主线基线80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d；在本人PR112分支0f94e9ed28efd43544dd4c893e517ce5668a2306上续写，保留原作者/提交。实际实现与受测HEAD：cd3369f7727fd6809b6576f213e862164ef2aeef；之后仅此交付记录，最终PR HEAD见GitHub。

## 实现范围

新增`crates/usage-stats/src/check_in.rs`及真实临时文件测试，通过lib公开独立候选模块，补充STATE-MACHINE说明。没有改现有v1字段/schema、Cargo/lock、Core/HTTP/Root/引擎启动。PR112尚待负责人审核，本包提交可审实际代码，不把上游接口自动记为已批准或自行接入主线。

- Disabled/Due/Prepared/Pending/Confirmed/Failed/Unknown实际转换；每次显式advance最多一个prepare/submit/query，不开后台线程或自行重试。
- 真实FileStore先保存period及pending摘要，当前进程仅一次性ticket可submit。提交Ok只为Pending；任何模糊结果保留原摘要。重启/禁用/过期/跨周期后只有query能力；明确拒绝消费本周期，确认成功才保存last_success；不补发遗漏周期。
- 控制代次/取消、同一提交临界区的确认采纳、旧回复隔离及disable generation持久化。不可Clone统计资源引用独立获取/释放，部分获取失败也清理，不停止其他拥有者。
- 三窗口独立、300秒退避、最多10秒单调/墙钟预算、严格原意图/摘要/有限回复校验。过期待定仍Unknown，不凭本地超时声称链上过期/撤回。
- 保存前/发布后错误使句柄失败关闭并保留文件；恢复核对，不把保存报错当成“肯定没保存”。默认OFF零clock/store/transport调用；实际文件须由宿主在授权后显式打开。

## 对应实现HEAD的真实验证

激活工作区Rust1.98后：

- `cargo test --offline --locked -p rustsimhub-usage-stats`：**76通过、0失败、0忽略**；本批新增23项真实状态机测试，另有现有持久/协议/查询测试。
- `cargo clippy --offline --locked -p rustsimhub-usage-stats --all-targets -- -D warnings`通过。
- `cargo check --offline --locked -p rustsimhub-usage-stats --no-default-features`、fmt和diff检查通过。
- `cargo test --offline --locked --workspace`：**856通过、0失败、6忽略，55个结果段**（默认features）。6忽略项保持其受限/子进程夹具边界，不计通过；未借用其他PR的899等数字。

实际覆盖三窗口、重启和lost reply、错身份/主题/窗口/周期/摘要、重复/额外/超量输入、时钟回拨/冻结与实际迟到、准备/提交/查询中取消、Idle取消/重新开启竞态、部分资源获取/他人引用保留、保存前满盘/发布后同步错误、确认保存失败、代次溢出、真实权限/损坏文件保留和并发只读完整快照。fault wrapper仅在真实FileStore前/后注入错误；transport明确为合成外围端口，未复制生产状态机。真实超时使用可缩短到100ms的预算及150ms迟到，非等待真实NKN。

日志位于setup-logs的`member-a-stats-machine-{package,clippy,production-check,workspace}.log`。原始临时state内合成身份不提交。整仓已有proc-macro-error2 future-compat提示不冒称已修复。

## 真实边界与后续

没有真实NKN交易构造/签名/链上证据适配器；库不导出seed、不自创NKN签名格式。实际adapter须纯准备真实交易、遵守I/O截止、查询真实原交易并只释放自己的引擎引用，没有adapter不能填固定成功。Core近期Owner鉴权、生产调度/开关、Root/ARM、无PID及真实链上退出仍未集成/验收。

取消句柄与已经开始的确认本地提交排序，取消可能等待文件提交临界区，不是硬实时杀进程；实际驱动步/显式关闭随后持久generation，崩溃恢复仍默认OFF且原pending query-only。查询退避未增加跨进程持久配额；宿主保持单实例，不通过重启绕过只读限流。保存晚于本地截止时即便真实确认已持久，当前调用仍Unknown而不冒充及时成功，不回滚真实证据或重发。

这是A-05～08离线实际实现/测试交付，不是STAT项目整机完成或负责人验收；整体百分比/台账保持主线口径。下一包继续独立SMTP/TLS实现，真实公共接口接线留待对应PR审核。

AGPL-3.0-only，无新依赖/第三方代码复制。OpenAI Codex辅助实现/审查/验证，旧作者与历史保持。
