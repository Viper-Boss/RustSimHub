# P6AA：维护迁移的包准入与回退基线

2026-10-23。继续100%目标，搁置关机调查。P2D已完成本地回收站，但数据库6不能由旧v5应用读取。本轮修正应用升级准入，设备未访问，未安装或迁移现场数据。整体77%，不为补齐安全边界重复加点。

## 发现的问题

P2D新包最初声明schema_min=5/schema_max=6。它表达了新Core能从v5迁移，却也让仍以schema5工作的旧updater接受候选。新Core启动后写入6，旧确认组无法读库，自动回退会失效。

另一条路径是单独将updater换成6，再尝试暂存候选：旧确认组仍5，新updater过去只检查候选，可能先推进floor/暂存成功，之后回退时才发现旧组不兼容。已经损坏的确认组也有相同的准入问题。

## 实际修正

- 生产updater的make、开发打包器和factory测试打包器均按目标schema声明6..6，旧5制品仍5..5。旧updater/验证器在修改账本之前拒绝6包。库格式读取能力与自动升级准入分开表述。
- `Store::stage`在候选写盘和推进floor之前，验证已有confirmed组的签名、payload摘要与当前schema兼容性。基线不兼容/损坏则拒绝；原账本和版本目录不变。
- 既有candidate会先返回Busy，保持原任务；空安装账本行为保持，首次可信factory安装与维护交付仍走单独流程，不借此接口静默替换confirmed。

未增加“强制忽略schema”选项，不修改签名验证或反回退规则。v5→v6还需要完整兼容6的基线、业务一致性备份、维护迁移及失败恢复验收。不能仅更新updater或Core后宣称完成。此改动让不安全路径明确失败，本轮没有实现现场维护切换工具。

## 验证

完整Linux工作区/all-features/locked 244项通过，包括新增3项：

1. 确认5组后以6重新打开账本，暂存6候选被Schema拒绝；账本逐字节保持、候选目录不生成、floor保持1。再回到5升级器，6包被拒绝，旧基线仍可选择。
2. 已确认组的程序损坏后，暂存新候选被拒绝；账本和候选目录保持。
3. 在兼容6基线暂存6候选，第一次选择候选；重开后回选确认组，floor仍2，同epoch不能重放。

离线Python签名包测试要求6验证器接受6..6，同时5验证器拒绝；系统工具共141项（1项跳过）通过。生产检查和Clippy、发布CI结果由接手入口及私有索引记录，不把前轮ARM制品重标为本轮构建。

代码入口：`crates/update/src/store.rs`、`crates/update/src/main.rs`、`tools/package-development-application.py`、`tools/test-factory-install.py`、`tools/system/test_application_package.py`。对同一数据库保持新收件记录的实际应用回退、完整Native同源签名组及实机部署仍未验证。
