# B-30 离线诊断展示与支持指引

成员 wnfhsj88（原 wnfhsj，member-b）；起始 main e48072ba9760c18f10c44847a1c6d94863eafd91；实现完整 HEAD 790c29b68c466a074809ee56e38d55c3c5beb5e9；分支 codex/wnfhsj88-diagnostics，Viper-Boss/RustSimHub。最终 PR HEAD 见正文。

交付 diagnostics.js、实际模块回归和 docs/web/diagnostics-integration.md。证书/登录/业务/远程路径独立，未知不推断成功；复制固定白名单元数据，去掉完整地址、公钥、口令、正文等；按钮只查询原操作 ID，具备截止、错 ID/退出/并发隔离，不提交/归档/重试或发现网络。

`node tools/test-web-diagnostics.mjs` 及全部 20 个 tools/test-web-*.mjs：退出 0；git diff --check：退出 0。测试只模拟外部 DOM、复制和查询回调，直接加载真实模块，无设备/网络业务。未运行真实浏览器/设备、ARM64 或全仓 Rust；新增模块未接生产 app/静态路由，主线接受后需映射真实授权 DTO、添加路由并验收证书/地址/远程/Android。

本包只完成 B-30 脱敏组件、查询边界与指引的离线交付，不冒充生产故障诊断或真实远程连通验收。未动业务、会话、权限、全局进度、名册或依赖。新代码 AGPL-3.0-only，OpenAI Codex 辅助编写与验证，无第三方源码复制/虚报工时；可独立回退，无迁移或设备配置变化。
