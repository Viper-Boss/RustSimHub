# A1：真实Root/SDK隔离集成与查询开关修复

后续已补九个故障场景并复跑原五个场景，十四场景均通过；见[故障矩阵补充](OFFLINE-A-A1-FAILURE-MATRIX-20261116.md)。
本报告保留第一轮固定来源，下述“未完成”测试边界以新报告为准；A1整体仍未完成。

PR268仍为草稿，整个A包尚未完成。本次补齐本机真实内核隔离链路的五个场景，
并修复上报引用误报为显式查询已开启的问题；不启用生产统计上报，不改变原300秒授权。

## 固定来源

- 实现HEAD：`21d67962e23718635e4cfbdc43ec44009de929f0`。
- 实现tree：`646767c5a81759d36da5b00c414f49fce551e9a8`。
- 完整Git源码tar SHA256：`cc983c8377e51ebfa3a86e4143362af1092f3dfe63bb66f2909d6a98fcd66f36`。
- 父提交：`652139692f2f57711cd5028081ea7ce776dd27d0`；包含main `de663743bd726e77f7c560139d0f91ea78e8fe57`。
- 本次实际Core测试程序来自上述tree。Go SDK/Root复用已构建的固定HEAD
  `18cbd7c405d844d24de15eb115880217cf8f5fad`；已核至父提交的Go/prepare源码无变化，
  不冒称本次重新构建。SDK SHA256：`643e580a321bd29a6021b3a804f2db076e58e30c4867eadbb53ddec82edd77b0`。
- 其后的本报告/状态说明仅为文档变更，最终PR HEAD在PR正文登记，避免循环引用。

## 实际改动和执行边界

`status.rs`的`owners()`与状态回读共同使用显式查询所有权：仅签到持有的底层统计引用不再把查询开关显示为开启。
原未启用候选feature的行为保持；查询、签到、Remote、Manual的底层共享引用继续由原Supervisor管理。

新增被忽略的专项测试`statistics_transport_kernel_sdk`，只有显式设置专项环境并满足已安装角色证明才运行。
复用原Root内核夹具，新增统计合成节点。实际使用固定完整SDK、真实Ed25519签名、真实Root
UID/cgroup/nft、原Auth/许可、Machine/FileStore和原预算；没有伪造Ready或删除内核门禁。
节点独立解析提交字节、重建未签名编码及双SHA摘要，核对持久化的原交易摘要。

环境：WSL2 Ubuntu24.04，Core UID1001/SDK UID1002，Rust1.98.0；每个场景独立Podman容器，
network=none、私有mnt/net/pid/cgroup，容器内仅合成本机TLS节点；容器结束后没有本次运行的残留容器。
专项容器使用privileged以运行真实nft/cgroup夹具，不挂宿主设备或宿主cgroup；这不是生产隔离配置。
17个Ubuntu包只校验并解包到私有目录，没有执行系统安装/维护脚本或启动宿主服务。
官方Ubuntu镜像配置ID：`7da97f743f397971f35dd21cd2d19a7da070a150e0cfa3eda559e71891ec7533`。
没有访问用户设备/NAS、公网NKN、真实链上交易、消息/电话或蜂窝网络。

## 五个实际场景

| 场景 | 实际结果与断言 |
|---|---|
| startup | 退出0；真实启动及准备后先持久化摘要，随后一次提交；Pending保留，退出回收原SDK |
| shared | 退出0；查询/签到/Remote/Manual共享原SDK；独立关闭不抢占其他引用；重启后查询原摘要，未再次预检或提交 |
| lost_ack | 退出0；节点已接收但不返回提交回执，Machine为Unknown；原Pending保留；恢复只查询原摘要，提交总数仍为1 |
| off | 退出0；节点收到一次提交后关闭独立参与，原Pending保留；迟到回执不记成功，其余显式引用按原监督管理 |
| cancel | 退出0；节点收到一次提交后取消原预算，结果Disabled、Pending保留；原SDK最终回收 |

勘误：原版此表把off/cancel写成“未发生提交”，与节点原始记录和测试断言不符；
这两个场景都是已发送后的在途围栏。提交前关闭/取消的零发送证据须由后续独立场景提供，不能用此表代替。

恢复查询用测试逻辑时钟前移来达到原Machine冷却，Auth/Root/AttemptBudget的真实Instant不变。
夹具最多驱动两次原查询步骤并遵守真实Root冷却，最终必须看到节点收到原摘要，不能仅以Unknown判通过。
本次成功恢复日志在第一次查询步骤已观察到请求；没有声称第二次补查分支已被覆盖。
节点未提供获准的可信链证明，因此查询最终仍为Unknown，不冒称链上包含/确认。

## 原代码负对照和失败保留

- tree `4ec64084500aa887e0a4e73e155628fc14153238`：原草稿状态实现配合同样的新断言，
  startup在实际Root/SDK链路中退出101，命中“上报引用不能冒充显式查询开启”。
- tree `d7c72488afdad40ce11d54971f75d24f3f50e8a9`：与上述负对照仅`status.rs`不同；
  相同测试的startup通过。其shared随后出现Unknown但节点查询数为0，原因尚未确定，保留失败。
- 最终646 tree只比d7更改专项测试的有界原查询驱动；产品字节相同。五场景均通过。
- 环境准备中保留WSL镜像拉取超时、私有策略路径、runroot长度、conmon库路径、loader模式、
  slim镜像目录缺失及Root策略字段非规范顺序等失败；修正夹具/环境后才计后续通过，没有放宽产品认证。

## 验证命令和准确范围

固定d7 tree执行以下命令，均退出0；最终646仅专项测试变化，产品字节一致：

```sh
cargo test -p rustsimhub-core --lib --features usage-root-transport-candidate,email-production-candidate --locked --offline -- --nocapture
cargo test -p rustsimhub-core --bin rustsimhub-core --features usage-root-transport-candidate,email-production-candidate --locked --offline -- --nocapture
cargo test -p rustsimhub-nkn-agent --all-features --locked --offline -- --nocapture
cargo test -p rustsimhub-auth --lib --features nkn-session-candidate --locked --offline nkn_session::tests:: -- --nocapture
cargo check -p rustsimhub-core --no-default-features --all-targets --locked --offline
cargo check -p rustsimhub-core --lib --target aarch64-unknown-linux-gnu --features usage-root-transport-candidate,email-production-candidate --locked --offline
```

Core lib129通过/4忽略，Core bin237通过/3忽略，nkn-agent65通过，Auth专项9通过，均0失败。
原签名5及Root2通过/1忽略为重叠专项，不累加总数。忽略项不计通过。
ARM仅`cargo check --lib`，未链接ARM程序、未QEMU/实机运行。
最终646 tree专项Core编译和上述五个真实容器场景通过；最终严格检查与格式检查均退出0：

```sh
cargo clippy -p rustsimhub-core --all-targets --features usage-root-transport-candidate,email-production-candidate --locked --offline -- -D warnings
cargo fmt --all -- --check
```

复验入口为源码中的`tools/offline-a/statistics_kernel_fixture.py`，复用
`tools/nkn-build/sdk-managed/root_kernel_fixture.py`。环境/构建/执行脚本与原始日志保留本机私有证据目录，
不把包含本机路径、命名空间和角色原始记录的日志提交公开仓库。

## 未完成与下一步

本次五场景不能替代全部撤权/到期/迟到/持久化失败/部分启动失败组合，后续继续补齐适配器矩阵。
可信链包含证明、深度/重组及长期Owner策略仍需负责人决策；当前不延长授权、不清Unknown、不启用生产上报。
完整常规NKN Noise/Core业务通道、IPv6/NAT/TLS/TURN矩阵不由统计夹具代为证明。
继续A0剩余27项映射、A2/A3/A4离线缺口及A5最终组合；热点专用部分继续延期，其他成员范围保持。
未改全局台账或权重。AI辅助Codex；无设备部署或发行签名。整体固定验收 **81.5%**，20/169与加权10.6%保持。
