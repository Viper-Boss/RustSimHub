# B-17～20：回执/配置显示候选与字段适配提案

账号 wnfhsj88（原 wnfhsj，member-b）；起始 main e48072ba9760c18f10c44847a1c6d94863eafd91。独立分支 codex/wnfhsj88-operation-receipts，Viper-Boss/RustSimHub；实现/最终完整 HEAD 见 PR 正文。

#76 测试中心契约仍待审核，因此本包仅提交独立候选组件/源码字段矩阵，不 import 或改生产接线，不宣称依赖审批已通过。B-17 对照 phone/call-test、SMS、forwarding、AP、session 的真实状态，保留来源差异、原 ID 和提交标记；B-18 组件只显示并调用明确 query/archive 能力，不自动归档/提交或统一业务状态机；B-19 将保存与经核验的进程状态分开，原证据未知/陈旧时不填成功；B-20 交 Node 和真实 Chromium 验证及接线提案。

验证：两个实际组件 Node 脚本退出 0；全部 21 个 tools/test-web-*.mjs 退出 0；Chromium 本地夹具 1440/390/320 像素检查无溢出、长帮助、禁用归档、键盘查询和服务未确认展示全部通过；git diff --check 退出 0。原业务模块未修改，既有真实业务回归复跑；组件 DOM/回调为合成外部端口。截图在工作区输出目录，不提交。

生产 DTO 映射、新鲜度（候选 5 秒）、业务归档证据、静态路由/页面接线需负责人审核后实施；直接 Wi-Fi 64hex trial 不冒充 UUID，未强制统一来源。未做 ARM64/整仓 Rust/设备/生产服务/真实发送或停止进程验收，不改变全局进度、名册、权限、依赖或密码协议。

代码 AGPL-3.0-only，OpenAI Codex 辅助实现与验证，无第三方复制或估造工时。详细合同和主线补验见 docs/web/operation-receipt-integration.md。可独立回退候选包，无迁移；接口未获审时依赖它的生产装配仍保持未完成。
