# PR190 主线辅助检查工具：接收复核与失败分流

复核固定HEAD：`1d6c9bf258fbde3ea90c6d9436fcb1f6fe0a323c`，报告的实际受测代码为`7950051b25c1c215a70a4b4e5beeedbedbc33da5`。当前产品起点e3762ec与主线6965b62的差异仅为既有三份文档。

## 本轮核对

- PR仅新增任务书允许的`tools/mainline-checks/`四文件及指定测试报告，没有产品Rust/JS、依赖锁文件、workflow或权限改动。
- 从报告受测代码7950051到交付HEAD1d6c9bf确实仅增加指定报告，没有把未测产品改动混进来。
- 已逐段阅读运行器的固定argv清单、来源/输出约束、超时/子进程回收、能力探测与结果分类。没有把开发检查工具当作Root沙箱或发布授权。
- 本机普通UID/Linux独立固定源码克隆实际运行20项工具合成自测，全部通过；固定列表86个检查、1个明确排除项，未知组拒绝。它们只证明运行器，不计为固件测试。
- 当前完整diff检查有一处轻微格式问题：`docs/testing/WEB-CODEX-MAINLINE-REGRESSION-20261103.md:290`末尾多空行。故不能照搬交付说明中的“最终diff检查通过”；已保留非0记录，不因这个问题否定20项自测。此项不涉及产品行为，待正常集成时修正。

本轮没有提交GitHub批准或合并，没有重写作者。实际提交账号wnfhsj88、Git作者GTW沿用交付记录；不将辅助工具算成新增产品功能或A/B/D业务完成。

## 交付报告的有效结论

报告为52阶段passed、3 failed、31 not_run，不是全绿。默认1135项通过、27组Node网页/4组configurator通过及全仓Clippy通过保持其精确来源。三项失败分别为：

1. Core全部特性：`partial_persist_and_failed_compensation_leave_unknown_and_refuse_new_effects`得到Storage；同源单项后来通过，但整组87个已执行测试中有11项失败，首个Storage后其他Busy。原因尚未确定，不能把单项通过覆盖整组失败，也不能自动归因磁盘。
2. 原生音频：`rtc::tests::encrypted_loopback_roundtrip_and_ticket_identity_revocation`得到Bounds；同一二进制单项后来通过。原生ALSA/Opus版本、CPU调度及具体Bounds分支尚未区分，不能假定是编解码、权限或生产网络故障。
3. compatibility-build：真实ENOSPC，后续upgrade-compatibility/worker-inspection没有执行。需要足够空间后按同一来源另起记录接续，保留失败，不能删除断言或旧记录取绿。

29项Root相关能力不可用，加上两个依赖阻断，共31 not_run。真实浏览器/WASM、GATT和设备验收也未完成。不把默认配置通过外推到邮件/NKN/BLE候选功能或Root部署。

## 本机限定诊断

为比较产品相同而环境不同的结果，复用了e3762ec/06cb909c正式来源**已经构建的同一Core库测试二进制**，未重建、修改或降低断言，在普通UID/独立loopback网络中按预定顺序各执行一次：

| 场景 | 实际结果 | 边界 |
|---|---|---|
| Core原失败单项 | 1通过 | 不覆盖网页版失败 |
| Core库整组 | 87通过、0失败、1既有忽略 | 只说明本机此配置此轮未复现；不循环重跑取绿 |
| ARM/QEMU音频同名单项 | 1通过 | 与网页版原生x86环境不同，不证明其Bounds已修复 |

两个环境的结果同时保留。当前没有证据支持修改生产权限、延长租约或宣称某个具体根因已修复；优先接续原模块的可定位诊断与修正。原产品1485项/34 QEMU的成功记录仍真实，但不能抵消新报告暴露的跨环境问题。下一版材料不因此冒充已通过全面发行验收。

私有证据：`.private/pr190-runner-review-20261103-first.json`和20项单测日志，整体复核为false的原因是末尾空行；`.private/pr190-existing-program-triage-20261103-first.json`记录三场景限定诊断。未执行本地Root/完整产品检查工具组，也未改设备。

上次设备只读检查原会话已确认正常退出，记录为应用71/下一epoch72、七单元active、通话空闲、普通蜂窝数据断开、转发PID0、公钥匹配，现场未更改。该观察时间仍是原记录，不把恢复会话后的读文件当成新鲜设备检查；签名/安装前仍需复核时效与现场。

PR190仍未合并，原81.5%、20/169及代码约90%粗估保持；人工测试最后，PMIC继续搁置。
