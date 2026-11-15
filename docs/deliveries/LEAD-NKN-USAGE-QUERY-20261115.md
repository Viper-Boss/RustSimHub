# 主线交付：NKN 活跃安装汇总查询

实际入口和剩余项见 [查询接线说明](../nkn/usage-query-integration.md)。复用原统计库与受限 NKN 提供者；不重记成员代码作者，不接管 AirSIM 蓝牙。

## 验证

- `cargo test --offline -p rustsimhub-core -p rustsimhub-wire -p rustsimhub-usage-stats --features rustsimhub-core/product-providers-candidate`：449 项通过，6 项因原显式隔离条件忽略。含真实 Auth/Store/UDS 的默认关闭、Owner/Client 拒绝，以及窗口和字段约束。
- `cargo clippy --workspace --all-features --all-targets --locked --offline -- -D warnings`：通过。原依赖 `proc-macro-error2` 的未来兼容提示保留。
- `cargo build -p rustsimhub-core -p rustsimhub-edge --features rustsimhub-core/product-providers-candidate --target aarch64-unknown-linux-gnu --release --locked --offline`：通过。只是编译，不等于候选打包、验签或实机启动。
- Go 固定 1.27.3 和原逐文件、补丁、module sum 验证的 SDK 源码：`go test -race -mod=readonly -tags=nkn_sdk_candidate github.com/Viper-Boss/RustSimHub/tools/nkn-engine/...` 通过；真实 SDK、Root 的 AMD64 和 ARM64 均构建成功，未执行 ARM 程序或公网 SDK。
- Go 新夹具覆盖三个窗口的实际 HTTP 请求、真实 0 与缺失/null/重复/超限等错误、取消/超时/重定向拒绝、原管道连续查询统计与状态、限频及不退出共享桥、缺 Root 策略不产生生命周期变化。
- `node tools/test-web-usage-stats.mjs`、`node tools/test-web-nkn-product.mjs`、`node tools/test-web-test-center.mjs` 通过；覆盖单独统计引用、原 CAS 回执及退出/页面隐藏后禁止查询。
- `node tools/test-web-app-assembly.mjs`：实际 Chrome 临时配置，320/390/1280px，原 ESM/CSP/IndexedDB/签名 Owner 登录；显式统计读取、无被动查询、退出清空并禁用通过。缺真实模块的负对照失败于原挂载检查。外部设备 API 仅为回环夹具。

私有证明保留第一次 WSL Git 路径失败及后续成功，没有覆盖历史失败。原始日志/制品仅在 `.private` 和本机缓存，不提交。

## 验收边界

查询开发交付不等于“本设备已参加统计”。参与同意、独立身份/真实签名、订阅提交/回执及公网/设备验收尚缺，不标统计模块完成。原 P0–P7 整体验收仍为 **81.5%**。
