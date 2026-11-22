# 本机合成负载资源证据 — 2026-11-17

复用原网页生产模块夹具，实际运行独立Windows Job，测量CPU、RSS及启动/夹具/回收耗时。
方法标识 `windows-owned-job-cpu-and-sampled-working-set-v1`。环境Windows AMD64、
Python3.12.14、本机已有Node24.22.1，无新工具链/固件下载或真实业务服务访问。

固定入口：

```sh
python tools/offline-b/measure.py --profile statistics --iterations 8 --seconds 10 --output FRESH_DIR
```

其余profile仅 `network`、`forwarding`、`sms`。对应现有
`test-web-usage-stats`、`test-web-network-policy`、`test-web-forwarding`、`test-web-daily-sms`
夹具，不能传任意命令。每组工作预算最多20秒/16次，另有有界来源检查和回收；
原32进程/1GiB Job限制、每日志1MiB上限保留。每次建立新Job，未启动生产引擎。

## 实际结果与口径

开发字节冻结时四组各8次均退出0，测量缺口/错误0，来源前后digest一致；
当时 `dirty=true` 被如实保留，之后发布文件逐字节核对，提交后干净HEAD另跑并在PR正文绑定。
初次结果如下（CPU百分比是一核口径，RSS用MiB）：

| 合成夹具 | Job CPU秒 / 夹具墙钟秒 | 一核CPU占比 | RSS观测峰值MiB | 夹具耗时中位数 / P95近邻秩ms |
|---|---|---|---|---|
| statistics | 1.078125 / 1.906 | 56.56% | 63.80 | 234.5 / 250 |
| network | 1.140625 / 1.953 | 58.40% | 66.94 | 250 / 250 |
| forwarding | 1.375 / 2.000 | 68.75% | 64.34 | 250 / 265 |
| sms | 1.53125 / 2.344 | 65.33% | 78.27 | 289 / 313 |

CPU来自Job累计用户/内核时间，包含Python引导和实际夹具子进程，不包含父采样器。
RSS来自Job成员当前working set之和，按输出/期限轮询采样；仅记录观测峰值，可能漏掉
采样间峰值；共享页也可能在进程之和中重复计入。Job committed峰值另存，不冒充RSS。
PID列表仅来自自己的Job；打开进程句柄后再次核实Job成员，退出/复用/无权限读为缺口，
不读其他进程内存内容、不把未知当0。源码变化、夹具失败或清理失败仍退出2；
缺采样/缺口/预算未完成退出3，不能强制生成完整测量结论。

8次短样本没有批准的性能阈值；P95只是这8次的近邻秩，不是可靠的产品尾延迟估计。
启动/功能断言/回收墙钟不是RPC或真实消息延迟。未取得相同功能旧系统样本，
没有做优劣对照或回归门限判断。不得跨Windows Job与Linux `/proc`口径直接比较。

## 明确保留的未验收项

- 这不是Core/Edge/SDK真实服务CPU/RSS、设备空闲/整机负载、网络吞吐或短信递送测量。
- 未测功耗/温度、棒子资源、NAS/运营商/真实网络、72小时组合长稳、板上声音。
- STAB-01/02只有本机有限合成负载部分证据；STAB-04/05/06的真实长期、旧系统对照与
  发布阈值仍未完成。`thresholds_approved/board_measured/power_measured/hours_72_verified`
  均为false，不能据此提高整体验收81.5%或准许安装。
- Linux原采样器/压力与恢复矩阵仍需要合格Linux环境；未修改它们的原生音频或其他窗口代码。

原始JSON/日志在任务自己的 `.task-tools/offline-b-measure-20261117-*`，不提交原始环境日志。
此资源JSON只作测试证据，不是替代 `build.json`、`profile-build.json` 或应用/Root包的协议。
