# A-01～04接手：最新主线Linux集成验证

用户2026-10-30要求wnfhsj88接手fungi1220全部任务；分配调整PR111及公开看板PR1单独处理。本分支codex/wnfhsj88-usage-stats-takeover保留原PR41的三条原作者提交，不改写署名或冒称本人新增统计库。

起始main `80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d`，合入原PR41 HEAD `295c47655af1d69d2eb0568eda09023c4599c16b`；实际合成/受测完整HEAD `4459a39259da0220f2cc5856e616656cce519147`。冲突仅在workspace成员列表：保留最新mdns/ble并加入usage-stats。Cargo.lock正常三方合并，无注册表版本升级；库源码与原PR41一致。之后仅本报告，最终完整HEAD见PR正文。

原第一批Windows/交叉检查与限制见[原作者交付](member-a-usage-stats.md)，本报告增加实际Linux运行证据，不覆写历史结果。

## 真实执行

```sh
source /workspace/toolchains/activate-rustsimhub.sh
cargo test -p rustsimhub-usage-stats --all-features --locked --offline
cargo clippy -p rustsimhub-usage-stats --all-targets --all-features --locked --offline -- -D warnings
cargo check -p rustsimhub-usage-stats --no-default-features --locked --offline
cargo test --workspace --all-features --locked --offline --no-fail-fast
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo fmt --all -- --check
git diff --check
```

全部退出0。统计包47通过、0失败/忽略：协议13、实际文件16、查询缓存18；包含此前未在Linux运行的6项权限/祖先链接/硬链接/锁inode等条件用例。实际OFF零transport/clock、缓存回拨、真实文件重开/拒绝覆盖和单调10秒超时均通过。

当前workspace 899通过、0失败、6忽略（54个结果段，含文档测试）；严格workspace及包级Clippy、生产特性/fmt/diff检查通过。忽略为两个父测试调用的worker、forwarder/private tmpfs写满、Root setuid/setgid和无CAP_KILL环境、setup-ap私有tmpfs写满；未尝试在普通文件系统填盘或改主机Root能力。没有把忽略项、交叉构建或CI排队计作通过。

日志member-a-takeover-usage-{tests,clippy,check}.log、member-a-takeover-workspace-{tests,clippy}.log仅工作区，未提交生成物/身份/生产数据。远程CI按最终完整HEAD单独回读。

## 交付与未验收

本PR为承接#41到最新main的实际集成分支，以当前完整HEAD审核；没有自行合并或关闭原PR。主线可选择本分支完成审核后关闭被承接的#41，不能把旧提交审核套到新HEAD。

A-01～04的原契约尚无负责人技术审核记录。A-05～08的签到/提交确认、禁用代次/共享引擎运行状态机仍未实现；当前库只有预留持久字段/意图DTO，不冒充Prepared/Pending/Confirmed生产流程。后续新协议/依赖/签名域/迁移按具体接口PR先审；A-09～12先做固定上游只读盘点和依赖清单。

没有真实NKN/RPC/交易、Owner/Core命令注册、网络适配、服务启动、Root/ARM/设备/网页完整联调；Windows生产存储与ENOSPC/掉电等原限制保留。统计UI仍无授权读取provider，本PR未擅自补Core/wire或开启默认统计。

AGPL-3.0-only，原作者提交/来源表保留，OpenAI Codex辅助集成与验证，无新增第三方实现或工时估造。不操作设备/NAS/真实通信/流量/链上交易、签名、部署、环境发布或全局进度。
