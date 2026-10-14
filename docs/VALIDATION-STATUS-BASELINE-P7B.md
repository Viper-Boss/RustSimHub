# 棒子上有限状态请求性能基线

2026-10-14，e927381 生产程序。source SHA256：
`1db98d42495e57d74c97af9753d201f6f2b38ce3a31378276fe65ab7d33293e6`。
没有新电话、短信、网络修改或蜂窝测试。

复用已验收的签名应用服务组，由 Root 选择验证槽位，Core UID 986、Edge UID 985。
使用 owner 已批准的 status.read 健康客户签名登录，实际 inbox/open-pairing 请求拒绝。
CLI 与 Core 的 UDS 请求在 Edge UID 的私有挂载、空能力集合和有限 systemd 单元运行。
观测真实进程四项 UID 和 CapEff/Prm/Amb/Bnd，均满足预期。

200 次顺序状态请求全部成功。计时包含启动 CLI、读私有 session 和本地 IPC/返回，
使用系统 realtime 纳秒时间；额外 grep 验证不在单次计时内。没有把它当作 HTTP/TLS、
网络延迟、Core 内部执行时间、并发吞吐量或手机端响应时间。

| 指标 | 实测 |
|---|---:|
| 样本 | 200 |
| 平均 | 13.255 ms |
| 中位 | 12.585 ms |
| P95 | 15.949 ms |
| P99 | 19.074 ms |
| 最大 | 35.056 ms |
| Core RSS 前→后 | 6,992→7,152 KiB |
| Edge RSS 前→后 | 3,524→3,524 KiB |

Core+Edge 测后 RSS 约 10.43 MiB；不是完整系统内存，未计 MM/NM/rmtfs 和媒体。
同次观察旧 agent RSS 为 15,216→15,200 KiB，但其职责、接口和负载不同，不能据此声称
Rust 更快、节省了某个百分比或不存在内存泄漏。还需要等功能、并发、长时间与媒体负载对照。

`tools/test-status-performance.py`、`tools/device-status-benchmark.sh`；私有原始报告
`.private/status-performance-e927381-first.json` passed=true、cleanup_verified=true。
所有自有服务组/benchmark 单元已停止，旧四服务 active，所有 PCM closed。
