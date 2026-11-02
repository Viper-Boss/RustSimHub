# 应用71安装进度发布验证

安装记录[PR175](https://github.com/Viper-Boss/RustSimHub/pull/175)已正常合并 `9b24d43dcc74a7d8ff7f28e4aab005bec65c0ed3`。实际安装产品固定d78dc2f3/归档8fb960e0，后续文档提交不冒称另一部署来源；完整设备/备份/验签/健康/未验收边界见[安装记录](VALIDATION-APP71-INSTALL-20261102.md)。

看板统计读取完整主线9b24d43及实际PR元数据，保留真实作者和D线指定归属，没有审核或合并成员PR。28项看板Python、Node交互/统计、JavaScript语法、生成器及diff检查通过。任务状态、169分母和原里程碑权重不变；原P0–P7 81.5%，20/169、加权10.6%；主线代码约90%仅未加权粗估。

公开导出固定来源 `fbb967c0e2eafed8f6cc56cf59bc750d837f3438`，公开提交 `84abdf03ab4ac5a0704711e154122c85b9d8f765`，只保留13白名单文件。Pages运行[41367983541](https://github.com/Viper-Boss/RustSimHub-progress/actions/runs/41367983541)三个job成功，API built提交一致；实际严格HTTPS四文件与发布清单hash一致，本地离线三文件hash一致。[公开看板](https://viper-boss.github.io/RustSimHub-progress/)已更新。

固件文档PR的[CI41367618005](https://github.com/Viper-Boss/RustSimHub/actions/runs/41367618005)两个job因付款/消费上限未启动、0步骤，不当通过。本机固定产品1356 Rust/6原有忽略、13 ARM/原生音频34、27组网页与实际安装证据各自保留；Pages成功不替代固件CI或功能验收。

本证明文档晚于固定导出，不能将它的新提交替换publication.json中的source_head。设备数据、签名材料、私有恢复脚本及日志不公开。后续继续主线自动集成/故障矩阵；人工UI、听音、物理冷启动最后，PMIC/关机继续搁置。
