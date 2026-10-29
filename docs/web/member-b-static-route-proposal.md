# B-29：五个候选模块的最小静态路由补丁

此文件与 [member-b-static-routes.patch](member-b-static-routes.patch) 是供负责人接纳的具体 diff。生产 Rust 文件没有在本 PR 修改，主页面也没有增加会请求404资源的 import。

基线 main：`81a822e3f916dd1324e2cac24145d3f426159807`。
补丁 SHA256：`c16ab2f98b0f5f92893a52f60497742ec21d5d8a033f431cad893e493af9cbf1`。

任务书允许必要时单独提出具体 diff；T6 仅向 B 开放最小 app/HTML/CSS 装配，没有开放 Rust 路由。此前各集成文档只有接线清单，本补丁把这项前置具体化为可应用/编译/测试的代码，仍由主线决定接纳并落到生产文件。

## 精确范围

只为 test-center.js、operation-receipt.js、configuration-state.js、diagnostics.js、usage-stats.js 增加既有 Router 链中的显式 GET/include_str 资源；45行路由代码，不扫描目录，不增加参数化文件读取、设备接口或鉴权面。

附一个真实 Router 回归，直接比对五份静态源码字节/JavaScript MIME，核对与 network.js 相同的 CSP/permissions、no-store/nosniff；错误Host为403，POST为405，三个未列入路径为404。Core socket 指向不存在的临时路径，不依赖真实Core/设备提供任何结果。没有新依赖、CLI选项、状态或迁移。

## 负责人应用与复验

在审核分支中执行，而非直接推送main：

```sh
git apply --check docs/web/member-b-static-routes.patch
git apply docs/web/member-b-static-routes.patch
cargo test -p rustsimhub-edge --all-features --locked --offline
cargo clippy -p rustsimhub-edge --all-targets --all-features --locked --offline -- -D warnings
cargo fmt --all -- --check
```

本次已用该精确 main 的临时源码副本验证：仅加测试时 GET /test-center.js 实际404，新断言失败（退出101）；加入提案路由后Edge 38项全部通过、严格Clippy与fmt退出0。补丁正向 git apply --check 在当前仓库通过，反向检查在临时提案源码通过，证实补丁与受测代码一致。临时副本不是已部署或已修改的生产路由。

这只是路由前置，不会自动挂载任何组件或调用业务。负责人实际接纳路由后，页面装配仍按各 integration 文档的已授权原控制器/精确原ID/失败状态接线；统计授权读取provider尚缺，不能用合成端口替代。TestCenter 的生产CSP样式修复在独立 #91；本补丁不夹带它，也不以模块可下载证明页面可用。

配置器生产认证/安全会话/GATT/Core事务后端、Pages冻结路径、真实设备/Android/双端及最终验收继续保留；本提案没有发布或增加全局完成度。
