# 成员B：B-29可直接审核的路由前置补丁

账号wnfhsj88（原wnfhsj，member-b）；Viper-Boss/RustSimHub 分支 codex/wnfhsj88-static-route-proposal，PR到main。
起始main：`81a822e3f916dd1324e2cac24145d3f426159807`。
提案代码提交：`37c73594307792638ef51907b0052d671cc456c4`；最终PR完整HEAD见正文，此后只有本报告。

交付 [具体补丁](../web/member-b-static-routes.patch) 与 [应用/边界说明](../web/member-b-static-route-proposal.md)。仅两个候选修改路径：Edge linux.rs 的五个显式静态GET/include_str路由，以及 linux/tests.rs 一个真实Router回归；生产文件均未修改。实际仓库净diff只有本人docs/web与docs/deliveries材料。

先前通用接线说明没有给负责人可直接应用的diff；本包补齐，不把“已交提案”当作“路由已注册”。T6只开放B的app/HTML/CSS装配，Rust路由由负责人接纳；无用户额外确认请求，没有自行越过文件范围。

## 真实验证

受测源码精确为上述main + SHA256 `c16ab2f98b0f5f92893a52f60497742ec21d5d8a033f431cad893e493af9cbf1` 补丁，位于git archive生成的临时副本。

- 仅附回归、仍用原路由：`cargo test -p rustsimhub-edge --all-features --locked --offline member_b_assets_use_existing_static_boundary` 退出101，实际 GET /test-center.js 为404而期望200。
- 完整提案：`cargo test -p rustsimhub-edge --all-features --locked --offline` 退出0，38通过、0失败、0忽略。
- `cargo clippy -p rustsimhub-edge --all-targets --all-features --locked --offline -- -D warnings`、`cargo fmt --all -- --check` 均退出0。
- `git apply --check docs/web/member-b-static-routes.patch` 在当前仓库通过；反向 `git apply --reverse --check` 在临时受测源码通过；本PR的diff检查通过。

测试使用真实Router/中间件与临时不存在的Core socket，覆盖五个资源的精确源码/MIME、Host拒绝、POST拒绝、未知路径拒绝、CSP/permissions/no-store/nosniff保留。没有实际HTTPS服务器、Core/设备、真实生产路由部署或主页面挂载；不能用此结果替代生产集成/实机验收。日志仅在setup-logs，不提交生成物。

## 接续、来源与许可

主线按精确PR HEAD审查后，在自己的审核分支应用补丁再复验；仅合并本说明不会自动注册路由。无迁移，回退提案的实际应用只移除五个资源及回归，不清数据。路由接受后才能继续按原业务控制器装配；A统计授权provider、D完整恢复backend、Pages路径与设备验收仍需对应负责人提供。#91的CSP修复独立，不夹带。

本批未跑整仓/浏览器/ARM，不借用旧结果填通过；远程CI另按PR精确HEAD回读。未改全局进度、名册、他人实现/分支/作者，无设备/NAS/生产数据访问、短信/拨号/流量、签名/部署/发布。沿用本仓库既有路由/测试模式，AGPL-3.0-only，无新依赖/第三方源码复制；OpenAI Codex辅助编写与实测，没有工时估造。整体进度由主线看板维护。
