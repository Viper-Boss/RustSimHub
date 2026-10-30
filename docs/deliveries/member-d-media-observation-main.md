# D-25/26局部交付：将已复现媒体撤权修复独立接回main

2026-10-30。用户要求自主持续推进；#103交付后继续当前可独立验证的媒体失败路径。

- 仓库/分支：Viper-Boss/RustSimHub / `codex/gtw-media-observation-main-20261030`，目标main。
- 起始main：`6b5de5f3ca5ff706adacd8f25ed7f638546739a9`。
- 原代码：`10966baef286f37cb93eea33e0d00c5c29201a13`（旧PR #55两文件代码提交）。
- 本分支代码/整仓受测：`55f806f9eb3d36d0c8c48955c02809d94321097b`。
- 当前完整40位HEAD在PR正文记录；之后只补本人文档。
- [复现、修正与组件边界](../testing/media-observation-main.md)。

原Author Viper-Boss与AuthorDate保留，实际Committer GTW，cherry-pick -x记录来源；
不改旧PR/他人分支，不把这份原修复重新署为本人创作。旧#55的GATT/全局台账依赖未合入。
仅media local.rs与其独立boundary_tests及本人文档，无其他成员实现或全局进度变更。

## 实测

Linux x86_64、UID1000、Rust1.98、锁定离线缓存，真实本地Unix socket与生产Client/Permit/Pump。
只模拟Core对端/PCM。先把runtime临时还原真实origin/main、只保留同一测试接入，6项中
4通过、取消/计数耗尽2项失败（Ok != Denied），退出101。随后恢复已cherry-pick的修复，
媒体28项全部通过；包严格Clippy/fmt/diff通过。不放宽断言、不增时限、不自动重试。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
# 修复前基线：实际main runtime + 同一个boundary_tests，结果4/2/0、退出101
cargo test -p rustsimhub-media --locked --offline local::boundary_tests
# 修复源码恢复后
cargo test -p rustsimhub-media --locked --offline
cargo clippy -p rustsimhub-media --all-targets --locked --offline -- -D warnings
cargo test --workspace --all-features --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo fmt --all -- --check
git diff --check
```

在上述代码HEAD完整整仓 **780通过、0失败、6项继承忽略**，整仓严格Clippy/fmt/diff退出0。
六项ignore继承C06/forwarder/Root/C14，本批无新增；proc-macro-error2既有future-incompat保留。
当前分支来自main774+本批6，不含待审#97/#101/#103。源码实测后只补本人文档。
原日志work/media-observation-main未提交生成物，摘要：

```text
before.log           eba53d634d1caf086d7d18862874ee5dc12cabd20ad076f6ff72a22494bb9808
media.log            096988a30e735abef54c1a43bf1c4f1cd293225b4757df5b3dc91d1c13c4b4a8
clippy.log           dd97462d6bf7e197ceab27dcbdf40ad157966fd894f3d37ace4af75e666a62d2
workspace.log        19dee4d20120ef3ad27552f9b4453781d54bce135c4370570d6a3a019999884d
workspace-clippy.log 2187696ca2bee1c190d74b9763c370ac6c0b5554d7f0aa1ea987a1aed1fa84f0
```
16类回复故障、先发出再取消、未poll/成功/Drop、旧回复不复活以及父目录/UID校验均实测。
没有真实不同UID进程、ARM/native/ALSA/设备/公网/TURN/DTLS/真实电话或听音验证。

## 数量、兼容与下一步

这是旧#55修复的独立main收敛，不重复计旧包：仍 **21/26离线包已提交，剩D-22～26共5步**。
不把本地媒体6入口/16向量当作完整D-25 TURN或D-26联合验收。固定P0–P7约80%、main19/169保持。
原协议/票据/通话租约/500ms RPC/1000ms镜像/采样/队列保持；没有schema/Root/依赖或安装变化。
回退不需数据迁移，但会恢复原两处不能立即撤销的行为，不能把回退当安全增强。

来源AGPL-3.0-only，原作者/许可证保留，无新第三方代码/密码原语。Codex辅助复核/接回/实测，
无作者改写/工时虚报。没有生产秘密/设备/NAS、真实网络业务/电话/短信/流量、签名或部署。
CI按最终HEAD另行回读，不自合并；随后收敛已存在GATT库/清理修复的main依赖。
