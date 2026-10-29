# 成员 B：B-13～B-16、B-32 离线负载/比较工具

账号 wnfhsj88（原 wnfhsj），稳定 ID member-b。起始 main `e48072ba9760c18f10c44847a1c6d94863eafd91`；实现完整 HEAD `0a9a51c5c1795f0514cf52805689e18713cc469e`，最终 PR HEAD 见正文。分支 codex/wnfhsj88-performance-tools，Viper-Boss/RustSimHub。

实际交付：tools/performance 下固定白名单的合成负载、仅本次进程树的 CPU/RSS 采样、单调窗口/暖机/取消/身份检查/资源上限/清理、只读比较器与失败测试；docs/performance/offline-tools.md 含五类真实场景的输入和未测边界、预算提案、实际命令及设备补验清单。复用既有真实 Node 功能模块测试，不另造业务负载后端。

验证：

- `python3 -m unittest discover -s tools/performance -p 'test_*.py' -v`：7 项通过、退出 0。实际运行 SMS 负载并取消本次 Call 负载，清理通过；同时测计数倒退、进程身份失效、输入限制、独占私有报告、错误 SHA/单位/短样本/链接等拒绝路径。
- 默认 `python3 tools/performance/measure.py` 输出计划，不启动负载。
- 在上述实现 HEAD、干净工作区，实际两次 `--run sms --seconds 5`：各 50 个样本、84/81 个成功功能轮次，outcome=completed、cleanup_verified=true、source_dirty=false。
- 用两份同 HEAD 报告运行比较器（精确 old/new SHA 均为实现 HEAD）退出 0，重算原始数值，budget_verdict=not_assessed、device_performance_verified=false。这仅验证比较流程，不是新旧产品性能改善证明。
- 之前 1 秒 smoke 正常；短样本不被比较器接纳。测试报告留在 /workspace/setup-logs，未提交机器测量结果或原始输出。
- `git diff --check`：退出 0。

已知边界：包含 Node/测试启动和夹具开销，CPU 的退出/reap 竞态显示为缺样本，RSS 为当前观察总量而非退出进程峰值。单轮/进程数/输出/时间都有上限；错误或无成功轮次不算完成。只枚举本次子进程的 children，不采样生产进程；无需任何设备凭据或联网服务。

真正设备空闲和远程功能负载标 unavailable，media 只指本地媒体生命周期，不能冒充远程。实机预算数字、旧新系统同功能比较、长测、真实呼叫/收件/转发/流量均未运行，也不由本工具批准。B-13/16 的场景/工具/主线交接及 B-32 的报告转换离线交付；STAB-01/06 全范围待主线实机验收，不提高全局进度。

复用仓库已有功能回归，无第三方源码复制/新增依赖；新增代码沿用 AGPL-3.0-only，OpenAI Codex 辅助实现与测试，工时未估造。未动原设备性能脚本、Core、Root、任务书、名册或全局进度。负责人可独立审此包，接受后再安装到既有测量流程；回退实现提交即可撤销工具，无 schema/运行服务变化。
