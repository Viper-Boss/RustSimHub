# B-21～28、B-31 离线配置器候选

成员 wnfhsj88（原 wnfhsj，member-b），起始 main e48072ba9760c18f10c44847a1c6d94863eafd91；实现完整 HEAD 4f060ce7ea6cb2c22ff1caa068383c96e2760e3a。分支 codex/wnfhsj88-configurator，Viper-Boss/RustSimHub，最终 PR HEAD 见正文。

已交付：冻结 RecoveryClientV1 的严格合同/控制器、真实 IndexedDB 元数据 CAS、默认无生产后端的响应式 UI、限白名单/限量/独立版本缓存的 PWA、更新保护、只组装公开白名单的站点构建器，以及实际 Node/Chromium 测试和 docs/ble/configurator-integration.md 的接口/集成方案。身份绑定/密码/GATT/Core 后端均未实现或假装存在。

实际检查（退出均 0）：

- node tools/test-ble-configurator.mjs：真实模块校验写前CAS/冲突拒绝、丢回复重开只查询、规范ID/字段/证据、ready期限、旧连接/断开、30次查询限制、未知记录不归档、Wi-Fi冻结向量与Mock源拒绝。GATT向量仅检查头/长度预算，不称真实传输测试。
- python3 -m unittest discover -s tools/site-build -p 'test_*.py' -v：2 项通过，验证严格输出集合/版本、无测试和Mock产物、链接/超限/未批准清单拒绝。
- python3 configurator/tests/browser.py --output /workspace/setup-logs/configurator-browser：真实 Chromium 1440/390/320 无溢出、Tab/default-deny、离线缓存重开；真实 IndexedDB 跨连接CAS只有一个初始写成功、旧revision失败、禁止秘密字段；真实IDB丢回复后新控制器保留原ID并仅查询；真实waiting SW拒绝pending/其他配置器窗口，允许后激活也不强制刷新。真实点击的 userActivation 到达合成端口回调，pin_mismatch/not_preapproved/revoked 各自显示。
- 首轮更新按钮断言早于异步IDB完成而失败；改为等待既定结果文本，产品逻辑未放松，之后整套通过。截图与报告只在工作区，无生产数据。
- cargo fmt --all -- --check、git diff --check：通过。未改 Rust、业务/session、身份绑定、依赖/锁文件、全局进度或发布配置。

#76/#79 的展示契约尚待审核，候选不导入它们，审批依赖未标完成。主线生产 client/非导出身份绑定、每次授权/撤销、加密与GATT/Core事务映射、硬件/Android/第二端仍是外部前置。本包没有真实蓝牙原生选择器或设备认证、真实配网/流量/恢复试验，不证明完整 BLE4-* 或 BLE5-* 验收。

特别发布前置：冻结 /RustSimHub/configurator/ 与现有 /RustSimHub-progress/ 不匹配。已提供保持根看板的确定性组装器和原 workflow 的最小替换提案；未改 Actions/未新建发布流/未发布。必须负责人确认托管或正式修订路径，再接线发布并核验 HTTPS 字节。B-31 发布验收未完成。

新代码 AGPL-3.0-only，无第三方源码复制；OpenAI Codex 辅助实现与实测，不估造工时或改写他人作者。静态源摘要分版本、记录不含SSID/口令/密钥，未知记录不能静默清空。回退实现提交可撤销候选；元数据若已试用须保留原请求供后续查询，不删除未知记录或身份库。
