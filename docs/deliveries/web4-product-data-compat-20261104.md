# WEB4-COMPAT：执行模式与数据兼容交付

日期：2026-11-04（Asia/Shanghai）。窗口7；AI辅助实现，保留已有迁移作者与 AGPL-3.0-only 来源。任务书为 `codex/web-product-delegation-20261103` 的 WEB4-COMPAT 与本批总书；不修改全局台账、169项分母或其他窗口入口。

## 阶段一：模式元数据与显式三方比较

起始 main：`8f9cfeff1a4b5a804b8b138e890f7386caf5a006`。正常快进消费 PR205 `c3a67120cb91f8c6095579a89764466a101d9433`，随后正常合入 main `a2044edfef6aeeced3d2c2eb8743791b34e4ccdc`，没有合入整份任务分配分支。元数据代码提交 `a4717f2c932bdf8010810859e4a42cd275fdf83f`；固定受测完整代码 HEAD：`e9de4c2cf95e7edb6d352c8f037406933030e63d`，其 `git archive HEAD` SHA-256 为 `895a2b5fe8b48a7e7b0ac106ff79ea03858b54101bec0152a23ed38f60d2bbee`。报告提交只补证据，最终 PR HEAD 在 PR 描述回读记录。

### 已有行为与有限转换

| 模式/历史 | 实际业务库 | 固定启动前缀 | 维护与回退约束 |
|---|---:|---|---|
| legacy-v1 | 6 | 无 | 原默认行为；不能读取7/9 |
| product-v1 / product-email-v1 | 6 | `--enable-product-providers-candidate` | 由实际 email 编译特性决定唯一产品模式 |
| 历史 recovery-grants | 7 | 不新增常驻运行模式 | 原 Store 6→7，只能作为显式维护中间态 |
| product-recovery-v1 / product-email-recovery-v1 | 9 | 上述前缀＋`--enable-secure-recovery-candidate` | 原 Store 6→7→9 或7→9；候选写9后，回退必须实际支持9 |

原 worker4 读写 outbox3；smtp 候选 worker5 读写 outbox4，原维护子进程负责3→4。候选可能写入的队列版本参与回退比较；旧 worker 不因新请求自报数字而获得新队列能力。原 format1、默认 Core `--build-info` schema6 和无 profile 路径保持。

新增 `crates/core/src/runtime_compatibility.rs` 纯函数读取真实编译特性、真实 source stamp 与 forwarder 合同。固定模式在打开数据库之前判断；只读元数据无数据库/服务/自动迁移副作用。新增 launcher `runtime_compatibility/` 严格解析显式 format2，检查 mode/前缀/schema/编译特性、源码、fixture、worker协议、未知/重复字段与预算。当前、候选及候选写后回退分别评估，保留退出码0/3/4。离线报告始终 `authorization_verified=false`，`mode_metadata_verified=false`；签名及同字节探测由下一阶段适配器提供。

`tools/product-upgrade/harness/` 是独立 `[workspace]` 验证清单，直接编译同一份拟接入模块；没有复制测试实现。其 runtime-probe 是同源原生验证入口，不冒充已接通的正式 Core CLI。

### 固定代码 HEAD 实测

在上述 HEAD 的独立归档上，Rust1.98，`CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0`，将上述归档摘要设为 `RUSTSIMHUB_BUILD_SOURCE_SHA256`；全部使用 `--locked --offline`：

- `cargo test -p rustsimhub-core-launcher`：40通过，0失败/忽略。
- `cargo clippy -p rustsimhub-core-launcher --all-targets -- -D warnings`：退出0。
- `cargo test --manifest-path tools/product-upgrade/harness/Cargo.toml --features FEATURES`：default、product-providers-candidate、product-providers-candidate,email-production-candidate、dev-fixtures 四种分别1通过；fixture模式明确拒绝元数据。
- 同四种 feature 的 harness `cargo clippy --all-targets -- -D warnings`：全部退出0。

原始日志保存在私有 `.local/logs/stage1-fixed-*.log`。未运行 ARM 真程序/设备；本阶段未编译全 Core，未验证已安装 Root、签名或健康自动确认；CI 须以 GitHub 实际回读为准。

### 窗口8最小接续

待窗口3交付后，Core lib 导出纯模块，main 在任何业务/激活解析之前添加唯一有限入口 `--product-runtime-compatibility MODE`，参数必须恰好两项，调用纯函数，输出 JSON；错误有限退出4。该入口不接受路径、schema或数据库输入。用真实签名 ARM Core/worker 的同字节执行输出补端到端证明；不能将 harness 输出文件、metadata JSON 或安装报告当授权。

本阶段是任务动作1～3的模块交付；动作4～12的签名绑定、原守卫与原迁移协调在下一阶段继续。整体正式验收仍81.5%，20/169，加权10.6%；未部署、未签名生产制品、未自行合并 main。
