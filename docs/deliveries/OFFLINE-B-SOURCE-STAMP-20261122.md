# B：升级 harness 的真实源码戳接线

起始 main 为 `de663743bd726e77f7c560139d0f91ea78e8fe57`。
本修补追加到本人 PR270 的 B 运行器；完整受测 HEAD 和组合另在 PR 正文绑定。

## 实际故障

原 B 入口只运行 `cargo test --manifest-path tools/product-upgrade/harness/Cargo.toml
--locked --offline`，没有提供其 README 和原主线运行器已经要求的
`RUSTSIMHUB_BUILD_SOURCE_SHA256`。在新建隔离 Linux 客体，完整依赖缓存就绪后，
原命令实际编译完成，但5项库测试中4通过/1失败：真实 Core 元数据返回
`missing committed source stamp`。该原失败、退出101和进程清理记录保留。

不修改 Core/wire 的校验，也不向测试填常量哈希。B 只消费既有编译接口：

- 清单仅给 upgrade-harness 标记需要 committed source；没有任意环境/命令参数。
- 清除继承的来源戳，从干净 HEAD 的真实 `git archive --format=tar HEAD` 计算 SHA256。
- 归档使用原 pidfd/subreaper 后端，30秒/64MiB上限，耗时计入原检查预算；临时归档不发布。
- 归档后再核 HEAD/tree/工作字节；脏源码不产生戳、过程变更/失败/超量/清理失败不得通过。
- 缺隔离仍是 not_run；报告记录实际摘要、HEAD/tree、归档字节数/命令和清理事实。

## 已执行与待绑定

开发字节在真实 Linux6.12.117/systemd 客体执行原运行器自测：25项通过，0跳过。
新增6项包含真实 Git 归档完整字节对照（不截尾/strip）、继承戳移除、脏源码不启动归档、
归档失败/输出超量/清理失败、归档中源码改变和缺隔离。原进程回收及 TCP 重置回归保留。
负例中的 Mock 只注入错误；真实成功路径另用实际 Git 和原 Linux 隔离后端验证。
Windows首轮另复现Git临时目录WinError32清理占用；复用原已测试的cleanup_owned，
仅等最多0.5秒的共享占用，其他/持续清理错误仍失败，不改产品断言或忽略清理。

完整 upgrade-harness 及最终干净提交的结果在 PR 正文绑定；开发 checkout 的 dirty=true
照实保留，不用旧 HEAD 的归档给未提交源码盖戳。Linux客体只用新虚拟盘/合成材料，
一般公网关闭；Cargo/Go依赖预备完成后测试仍为离线。

安装 harness 另发现的 tempfile 缺 dev-dependency 已独立交 PR278，不夹在此修补中。
其2项通过不等同于升级测试或完整Root安装链。12组材料测试也独立绑定原组合5a62506a，
不叠加成项目验收数量。

不改变原 argv、特性、600秒检查预算、Core/Root权限/已有身份/生产服务或包格式；
不涉及AP专项、BLE/AirSIM/native音频、设备/NAS/真实RPC或链上交易。
ARM/sysroot/SDK stage、完整制品与Root/PID1/物理/设备验收继续按各自前置执行。
无全局进度改动，81.5%保持。沿用项目AGPL-3.0-only、标准Python API及原Linux后端；
Codex辅助实现/实际核验，不编造工时。回退仅撤B运行器的此固定环境接线和相关文档/自测。
