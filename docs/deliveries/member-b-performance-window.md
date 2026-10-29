# 成员 B：完整性能采样窗口与可比较证据

账号 wnfhsj88（原 wnfhsj，member-b）；Viper-Boss/RustSimHub 分支 codex/wnfhsj88-performance-window，PR 到 main。
起始 main `5238670246e7c6e43184534a7a6ee4b13c293ffd`；实现/受测源码 HEAD `06de4aab2041ced33d79f9ea060a94ce5c26bdc3`；最终 PR HEAD 见正文，之后只有本报告。
对应 B-13～16、B-32 / STAB-01、STAB-06 的本人离线工具，不接设备或修改产品业务。

## 已复现问题与修正

修复前真实执行 `sms --seconds 12 --warmup 0 --interval .1`：100轮上限后提前退出，66个样本仅约6.53秒跨度，报告却为 completed。比较器只要求至少4秒跨度，未核对原请求窗口、暖机和采样间隔。此结果不能作为完整12秒窗口证据。

现在提前正常退出也报告 window_incomplete，CLI 退出2；保留已完成轮数/原始样本/清理结果，不增大100轮上限、不空转补时。测量与比较报告升级 schema=2，新增 sample_interval_seconds、measured_seconds；旧schema1缺少证据，拒绝比较并要求重新采集，不补造字段。

比较器要求完整实际窗口、相同请求窗口/暖机/间隔。样本不得超窗，首尾距边界最多1.5个间隔，内部缺口最多2个间隔；原≥10样本/≥4秒跨度/≥9有效CPU、SHA/来源/环境/单位检查仍保持。摘要从样本重算，预算仍 not_assessed、device_performance_verified=false。

只改 tools/performance 的 measure/compare/测试和本人文档。没有扩大采样目标或命令入口，默认仍仅计划；取消/文件/进程/时间预算及只清理自身进程不变。

## 实际验证

```sh
source /workspace/toolchains/activate-rustsimhub.sh
python3 -m unittest discover -s tools/performance -p 'test_*.py' -v
python3 tools/performance/measure.py --run sms --seconds 5 --warmup 0 --interval .1 --output /tmp/rsh-window-fixed-a.json
python3 tools/performance/measure.py --run sms --seconds 5 --warmup 0 --interval .1 --output /tmp/rsh-window-fixed-b.json
python3 tools/performance/compare.py /tmp/rsh-window-fixed-a.json /tmp/rsh-window-fixed-b.json --old-sha 06de4aab2041ced33d79f9ea060a94ce5c26bdc3 --new-sha 06de4aab2041ced33d79f9ea060a94ce5c26bdc3 --output /tmp/rsh-window-fixed-comparison.json
python3 tools/performance/measure.py --run sms --seconds 12 --warmup 0 --interval .1 --output /tmp/rsh-window-fixed-incomplete.json
git diff --check
```

9项单测通过（退出0），含真实临时进程正常早退、不完整窗口、取消清理，以及窗口/暖机/间隔/首尾/内部缺口/旧版本拒绝。修正测试端口拦截范围后，单独在旧主线源码上重跑早退回归，确因 completed != window_incomplete 失败（退出1）；之后新实现通过，不删除断言或放宽产品预算。

两次5秒真实Node SMS夹具运行分别73/77轮、各50样本、实际5.033/5.026秒，均 completed、源码干净、cleanup_verified=true；CLI和比较均退出0。两份为同一HEAD/同一功能的工具自验，不能当作旧新系统性能改进证据。比较产物仍不评设备预算。

12秒复验在100轮后6.536秒结束、65样本，正确 window_incomplete、CLI退出2、清理确认且源码干净；实际调用 compare.load 确认拒绝该报告。此预期失败不是隐藏或重命名为成功。原始数值/报告仅保留工作区临时路径，不提交生成物。

## 集成与未验收

负责人按PR精确HEAD审查；正常合入即可。旧报告仍可人工查看，但不能手改版本交给新比较器。回退本批代码恢复旧报告规则，不触碰设备或已有报告文件。无Rust/网页改动，本批未重跑workspace/浏览器，不引用过去结果为本HEAD通过；远程CI另行回读。

真实棒子/旧新系统相同功能、阈值批准、实际空闲/远程/长测仍由主线采集；不修改全局进度/分母或将本地完成计为验收。既有其他前置见 member-b-usage-and-gates.md，#87/#89仍为独立待审核修复。

没有设备/NAS/真实短信或呼叫/流量/生产数据访问，没有签名/部署/发布。沿用AGPL-3.0-only，无新依赖或第三方源码复制；OpenAI Codex辅助实现和验证，无工时估造或作者重写。
