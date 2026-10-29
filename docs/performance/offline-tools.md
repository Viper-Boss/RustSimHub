# 有界离线性能材料（B-13～16、B-32 / STAB-01、STAB-06）

这是合成功能夹具与测试启动开销的测量工具，不是棒子 benchmark。默认只输出计划；没有设备、账号、SSH、任意 shell、任意 URL 或命令模板接口。

## 场景与真实边界

| 真实验收场景 | 本机可重复负载 | 尚缺的主线证据 |
|---|---|---|
| 空闲 | 不用空转进程伪造空闲，计划中标 unavailable | 实际 Core/Edge/agent 正常服务、无业务窗口及至少稳定采样窗口 |
| 收件 | `sms`：真实 sms-management 网页模块的既有合成回归 | 相同规模的真实收件/SQLite/设备刷新、条数与写盘量 |
| 通话 | `call`：真实 CallTest 的合成音频/IPC 回归 | 实际双向 Opus/PCM、呼叫阶段、并发限制、主叫/被叫及真人声音 |
| 转发 | `forwarding`：真实 ForwardingPanel/worker 边界的既有网页回归 | 相同目标/消息规模、真实 TLS、队列、回执和完全停止证据 |
| 远程 | `remote` 标 unavailable；`media` 仅本地媒体生命周期夹具，不冒充远程 | 批准的相同功能 TURN/NKN/网络路径、实际数据与预算 |

输入规模固定在仓库现有合成脚本中，工具不改样本、产品租约、队列或安全断言。每次最多 100 轮，单轮成功才记 completed_iterations；失败和零轮成功不计有效完成。没有为了 CPU 曲线另造短信/电话/网络后端。

## 执行

```sh
cd /workspace/RustSimHub
python3 tools/performance/measure.py
python3 tools/performance/measure.py --run sms --seconds 5 --warmup .5 --interval .1 --output /tmp/rsh-sms-report.json
python3 -m unittest discover -s tools/performance -p 'test_*.py' -v
```

新报告路径必须不存在，私有模式 0600、独占创建，不覆盖/跟随已有链接。临时日志和进度只在本次 TemporaryDirectory 内，完成/取消后删除；报告只有允许公开的数值、Git SHA/dirty 标记、运行器/负载 SHA256 和环境版本，无命令输出、正文、令牌、设备标识或进程参数。

窗口 .5～20 秒、暖机 0～2 秒、间隔 .05～1 秒；最多观察 32 个自己创建的进程、输出由 RLIMIT_FSIZE 限制到 1 MiB。只从本次子进程的 task/children 递归采样，不遍历其他生产进程。PID 同时绑定 starttime，复用/消失不得沿用原身份；取消/超窗/失败仅清理本次独立 session，最后记录 cleanup_verified。窗口短可用于 smoke，不拿短窗口外推设备结论。

CPU 单位为进程树累计 CPU 秒/单调经过秒×100，可超过 100%；RSS 是当时观察到的页数×系统页大小之和（bytes）。暖机不进样本。退出/reap 竞态或计数倒退是缺样本，不产生负数/假 0；短命进程的已退出 RSS 峰值无法复原。CPU 包含 Node/测试启动和夹具开销，不能称为产品稳定态 CPU。空闲、设备写盘/网络量和实际延迟阈值本工具未测。

## B-32 比较和报告

```sh
python3 tools/performance/compare.py /tmp/old.json /tmp/new.json \
  --old-sha FULL_OLD_SHA --new-sha FULL_NEW_SHA --output /tmp/comparison.json
```

比较器只读显式报告（普通文件、拒绝软链接、≤1 MiB），核验精确 SHA、干净源码、相同运行器/环境/场景、成功且已清理、至少 10 个样本及 ≥4 秒实际跨度、单调时间、有限 CPU、整数字节 RSS、至少 9 个有效 CPU 样本。忽略输入摘要并从原始样本重算。拒绝旧 SHA、单位字符串、NaN/负值、缺样本、未知版本、取消或短窗口；不把这些写成“达标”。

报告只描述数值变化。负载字节是否相同单列；相同功能仍需负责人审查，即使相同哈希也不自动满足旧新系统同功能性。任何比较均 `budget_verdict=not_assessed`、`device_performance_verified=false`；对比器不批准阈值，不填假成功。现有主线 `tools/test-status-performance.py` 是需要设备授权的另一流程，本工具未调用或修改它。

## 拟议设备预算与停止条件（尚未批准）

主线先冻结固定源码/设备/服务集合、样本规模和等功能性，再决定暖机/窗口（建议暖机 30 秒、采样 5 分钟，均为提案）。收件按固定条数/正文长度、通话分准备/接通/声音/结束、转发按固定目标/并发/回执、远程注明直连/中继与网络条件。CPU/RSS/延迟/写盘/网络必须各自有单位、缺样本和失败数；数值阈值由实机基线确定，本包不捏造数值。

任意身份/线路变化、用户取消、未知发送结果、服务崩溃、资源预算或日志上限触发停止。真实发送/普通流量/远程服务器必须由负责人另行授权，不能用本地合成命令推定许可。主线需采集实际棒子与旧系统相同功能数据、批准预算、完成长测后才验收 STAB-01/06。
