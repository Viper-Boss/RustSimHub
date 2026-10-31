# 2026-10-31：截图七份 PR 的 main 同步与合并状态复核

本包只处理用户指出的 PR 合并冲突/检查失败，不接管新功能，不自行合入 main。
GitHub 登录及七份 PR 创建账号实际回读为 wnfhsj88；原始提交作者全部保留。

共同 main 基线：`342adaa2fee6f3ea36a82f0145dc7931e12e36de`。
GitHub 初次回读六份为 dirty/conflicting，#127 本来可以合并但检查失败。
对最新 main 实际执行 git merge-tree，七份全部可自动合并，无需手工取舍文件。
本轮生成正常双亲合并提交，保留原 PR HEAD 和共同 main，不变基、不强推、不删原历史。

| PR | 原 HEAD | 同步后的受测合并 HEAD | 本轮验证 |
| --- | --- | --- | --- |
| #125 | `d504895db44c8ef2ec765cd880c3d1607f2a8bb0` | `1ab2e1ce2db3a24dd61aea4c03cc80ecff1ccde3` | fmt及workspace all-targets/all-features check通过；Storage 81通过 |
| #127 | `d94381502d08504209da40bcd0d56509536b627b` | `6e78c636ae595b3b624f67c7495abae295ff53ea` | fmt及workspace all-targets/all-features check通过；Auth/Storage/Core/BLE 490通过 |
| #130 | `7bfdfb121799ca79b82f62f9d222cb8e6b809869` | `6a21d7cc7c39f3674d85c9fa8e09f7840ed37780` | fmt及workspace all-targets/all-features check通过；未额外运行此分支的测试 |
| #131 | `290bcc289e179cd36019d06731b34468f7f2efbc` | `4b5afc1900e4eae3c0ccec2d9de2c032f64dd623` | fmt及workspace all-targets/all-features check通过；BLE 158通过 |
| #136 | `dbaf53f5edd0dbabd47d865429b5c2d656e07e56` | `db59f9ee9945900258b5cd425d15254bba391b6a` | fmt及workspace all-targets/all-features check通过；Auth/Storage 164通过 |
| #137 | `7b682d4f7f31a161957730115dcbb9aafbee07ec` | `14533c032587fe9f3a56775e23b0eca11978e93c` | fmt及workspace all-targets/all-features check通过；Auth/Storage 189通过 |
| #138 | `4c729b3b20446bb67f4c597b5f97e93f6be9692f` | `26db888b44bfd6b8db5a9f83e01876b582cd5b5a` | workspace all-features：1162通过 / 0失败 / 6既有忽略；整仓严格Clippy |

#138的上表合并HEAD为完整受测代码；随后只增加此交付报告，最终完整HEAD以PR回读及PR正文为准。
没有把#138的1162项或不同分支的数量算作每份PR的独立测试数。

## 实际环境及命令

Linux x86_64，Rust1.98.0，Go1.27.3，先启用工作区toolchains/activate-rustsimhub.sh。
本轮独立target关闭debug/incremental减少构建空间，Cargo/Go均保持离线依赖模式，
未下载或升级依赖、未安装系统包。主线带入的Cargo成员/邮件依赖边正常保留。

最上层同步代码实际运行：

```sh
cargo fmt --all -- --check
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
```

全特性59测试段1162通过/0失败/6忽略；严格Clippy、fmt通过。
6项忽略仍为既有有界子进程worker、tmpfs ENOSPC和隔离Root能力项，不计通过。
既有proc-macro-error2 future-incompatibility提示保留，没有降低warnings门禁。

较早六份分支分别对上表精确HEAD运行fmt和整个workspace all-targets/all-features check。
#125/130/131会从main带入已合并的恢复存储变更；#127还带入此前已合并的
Auth/Core/BLE组件，因此分别追加表中实际专项测试。#136/137也单独跑Auth/Storage。

日志只留工作区setup-logs/pr-merge-sync-{all-features,clippy}.log和
pr-merge-sync-<PR>-validation.log；没有提交原始硬件/身份/数据库/业务数据。
本轮不运行Docker跨UID手动入口、native-audio硬件/设备、真实Root/网络/链/邮件或部署。
原报告中的手动试验仍只属于其原受测HEAD，不改写成同步后已经重跑。

## GitHub检查与后续

初始截图中的0/4红叉经实际jobs/annotations回读：任务在启动前因账户账单/支出上限失败，
steps=[]，不是Rust测试报错。合并冲突与CI红叉必须分别处理；更新分支不能修复账单。
当前HEAD的实际合并状态、后续Actions和完整HEAD会回写各原PR正文，不声称CI绿灯。
仅正常快进推送这七个原PR分支；main由负责人按原依赖、精确HEAD及审核门禁合并。

无新增生产协议/权限/迁移设计、Root动作、签名、发行或Pages发布。
新提交/报告由wnfhsj88署名，旧功能作者未重写；AGPL-3.0-only，OpenAI Codex辅助冲突排查及回归。
本次属于维护，整体进度沿用最后实际公开核验的81.5%，不因消除冲突增加验收百分比。


## 后续：main接收#125/#127后的依赖链同步

用户随后提供#130三文件冲突截图。此时main已前进至
`ee29941ad6cd6bc082b647d74fc67364dab151b8`，GitHub对#130重新报告dirty。
原#130与新main存在两个共同祖先（原#125与原#127的HEAD），本地正常合并可完整保留两侧内容。
因此先将新main正常合入#130，再顺序将已更新依赖合入#131→#136→#137→#138。
没有将这五份PR分别孤立地合入同一个旧main，后续分支真实包含前一个更新后的PR提交。

| PR | 更新前 | 新的受核验合并HEAD | 新增依赖父提交 |
| --- | --- | --- | --- |
| #130 | `6a21d7cc7c39f3674d85c9fa8e09f7840ed37780` | `1354a41d75215c915668fc4626c253f285b0379d` | `ee29941ad6cd6bc082b647d74fc67364dab151b8` |
| #131 | `4b5afc1900e4eae3c0ccec2d9de2c032f64dd623` | `9f598f24ea33282deaea49cb707141e2495a0d46` | `1354a41d75215c915668fc4626c253f285b0379d` |
| #136 | `db59f9ee9945900258b5cd425d15254bba391b6a` | `da698954886f06bf226d38c41a7f1362bc615cf6` | `9f598f24ea33282deaea49cb707141e2495a0d46` |
| #137 | `14533c032587fe9f3a56775e23b0eca11978e93c` | `088f4b7d50640472561449be87e88b68bd3bba1d` | `da698954886f06bf226d38c41a7f1362bc615cf6` |
| #138 | `b44c4011d367cba7ce0d3fc49cacf76e4300b1c1` | `e03e71dca2c03c843c2c6d05cf4ab206579fd16a` | `088f4b7d50640472561449be87e88b68bd3bba1d` |

实际逐份核验：五份合并的完整Git树均与其更新前逐字相同；旧HEAD、新main、前一个已更新依赖均为新HEAD祖先；
每份PR对新main的唯一共同祖先就是新main本身。
截图中的以下三个文件在每份更新前后均零差异：

- docs/ble/service-policy-kernel-uids.md
- firmware/system/ble/fixtures/policy_uid_fixture.py
- firmware/system/ble/run_policy_kernel_uids.py

保留#130的--rust-gatt入口、真实Rust子进程夹具/诊断和后续集成说明。
此轮修复提交依赖关系，不改变任何源码树；沿用上表各相同源码树的已通过编译/测试，
没有重复跑Rust测试，也不把旧结果写成新跑。额外执行Python语法检查及启动器--help核验（不启动Docker）。
本节随后仅作为文档提交加入#138，最终完整HEAD及GitHub最新mergeable回读写入各PR正文。
若main今后加入新的代码，仍须针对届时的确切基线重新核验，不能将当前回读视为永久保证。
