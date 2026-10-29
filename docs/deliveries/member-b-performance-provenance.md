# 成员B：采样前后来源核验与环境证据

账号wnfhsj88（原wnfhsj，member-b）；Viper-Boss/RustSimHub 分支 codex/wnfhsj88-performance-provenance，PR到main。
起始main：`b7a5dfa8d0d8c762c2284164e9d4371fce3c373f`；实现/受测源码HEAD：`907161d584dbb184870f38fe044242ce5ec522ce`；最终PR完整HEAD见正文，此后只有本报告。
对应B-13～16/B-32离线性能材料，限本人measure/compare/测试及文档，不涉及设备或其他成员接口。

## 两个实际缺口与修正

原工具仅在负载结束后读取Git提交、dirty和摘要。采集期间若HEAD变化，报告会被错误归给结束时的提交。新回归以真实Node负载/合成Git边界复现，原实现把结束的bbbb提交写为来源，起始aaaa断言失败。

现在负载启动前固定Git提交/dirty、运行器/负载摘要与Linux运行环境，清理自身进程后复核。报告保留开始来源，任一可见变化明确source_changed=true；原本completed的结果改为source_changed，取消/其他失败原因保留。dirty取前后或值。Git/Node元数据命令各3秒截止，读取在性能窗口之外。前后检查不声称不可变快照、期间变化又恢复的检测或独立认证。

原比较器只做environment相等判断，两个null/缺失对象也会相等。现在要求完整Linux环境对象、有限版本/架构字符串及正整数clock_ticks，同时保留原相同环境/场景/运行器/窗口/暖机/间隔和样本覆盖检查。缺少来源复核证据不可比较。

报告和比较产物升级schema=3，明确要求source_changed=false；schema1/2应重新采集，不手改版本或补造字段。默认计划仍不执行负载；预算仍not_assessed，设备性能仍false。

## 真实验证

```sh
source /workspace/toolchains/activate-rustsimhub.sh
python3 -m unittest discover -s tools/performance -p 'test_*.py' -v
python3 tools/performance/measure.py --run sms --seconds 5 --warmup 0 --interval .1 --output /tmp/rsh-provenance-v3-a.json
python3 tools/performance/measure.py --run sms --seconds 5 --warmup 0 --interval .1 --output /tmp/rsh-provenance-v3-b.json
python3 tools/performance/compare.py /tmp/rsh-provenance-v3-a.json /tmp/rsh-provenance-v3-b.json --old-sha 907161d584dbb184870f38fe044242ce5ec522ce --new-sha 907161d584dbb184870f38fe044242ce5ec522ce --output /tmp/rsh-provenance-v3-comparison.json
git diff --check
```

11项测试退出0；修复前新增来源/环境子用例失败，红/绿日志保留setup-logs/member-b-provenance-{red,final}.log。源变化单元回归只模拟Git输出，真实Node负载/采样/清理仍执行；没有偷偷改变当前检出或删除断言。环境差异正例仍用完整有效对象验证不等环境拒绝。

两次干净提交实际5秒采样分别77/76轮、各50样本，schema3/completed/source_changed=false、cleanup_verified=true，命令和比较退出0。是同HEAD工具自验，不是旧新系统性能改进或设备预算结论。

另外使用隔离本地克隆检出同一907161d提交，启动真实1秒Node采样；仅在夹具中用外置Node启动标记同步，随后向该克隆追加一条无文件改动的测试提交。报告保留起始907161d、source_dirty=false但source_changed=true/outcome=source_changed，15成功轮数/20样本、清理确认，CLI预期退出2，比较器拒绝。主检出HEAD/工作树未改，临时克隆与测试提交未发布且已删除。1秒仅用于失败边界，不作为有效比较窗口。原始公开数值保留setup-logs/member-b-provenance-real-change.json，未提交生成物。

首次隔离夹具未能捕获子进程启动，目标断言未执行；改为明确启动标记后完成上述真实验证。该次未执行不计通过，未修改产品断言来回避。

## 集成与保留项

负责人按精确HEAD审查后正常合入。无产品依赖/数据库迁移，旧报告只可人工查看或按旧工具解释；当前比较应重新采集。回退本批恢复旧来源/格式检查，不删除已有报告。

未改Rust/页面，本批未跑workspace、浏览器、ARM或设备，不借用旧结果填通过；远程CI按当前HEAD另行回读。真实设备/等功能旧新样本/阈值/长测由主线提供；页面静态路由实际应用、统计读取与完整恢复后端仍见既有交付前置，不将此包计为全32步最终验收。

没有生产数据/设备/NAS访问、真实短信/呼叫/流量、签名/部署/发布或全局进度修改。沿用AGPL-3.0-only，无新依赖/第三方源码复制；OpenAI Codex辅助实现与实测，无工时估造或他人作者重写。
