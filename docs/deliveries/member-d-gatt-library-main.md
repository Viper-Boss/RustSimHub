# D-17～20/D-26局部收敛：独立GATT库与清理责任修复

2026-10-30，用户要求无需逐次催促持续推进；媒体PR #104之后继续此独立依赖收敛。

- 仓库/分支：Viper-Boss/RustSimHub / `codex/gtw-gatt-library-main-20261030`，目标main。
- 起始main：`6b5de5f3ca5ff706adacd8f25ed7f638546739a9`。
- 原库来源：`be0219119add5517161a8429328b5c0d1740641d`（旧#51）。
- 原清理修复：`99a7f163c23e7afa7cb344bdad4b48b3d66e596d`（旧#59）。
- 接回库：`911f91d96f6817aeb6ccdbdb3da6eeecb0cdac1a`。
- 最终代码/整仓受测：`669d8731fce43f3702686d0c40db400292e523e3`。
- 当前完整40位HEAD见PR正文；受测后只补本人文档。
- [能力/类型/关闭语义与全部限制](../ble/gatt-library-main.md)。

原作者Viper-Boss和作者日期保留，实际Committer GTW，cherry-pick -x记录来源。
不修改旧分支/PR，不把原代码改署本人。只接两个代码提交，不带旧灯控/看板/全局进度依赖。
workspace冲突保留main所有成员含mdns，仅追加ble；锁文件仅增加本地包/已有开发依赖。
没有新第三方运行依赖、密码原语、Auth/Core/wire/schema/Root/安装/默认服务变化。

## 实测

普通Linux x86_64/UID1000/Rust1.98、锁定离线缓存。
42项库测试（原38+原修复4）通过；对应严格Clippy/fmt通过。没有新增或放宽测试断言。
保留同一回归、临时还原原库修正前lifecycle.rs，实际失败退出101，错误为未确认安全清理
仍被误报quiescent；随后恢复已cherry-pick的修复并通过。此基线是原库代码，不冒称main已含它。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
# 修正前原库runtime，保留同一新增回归，实际退出101
cargo test -p rustsimhub-ble --locked --offline failed_secure_close_cannot_report_quiescence_after_application_removal
# 恢复修复后
cargo test -p rustsimhub-ble --locked --offline
cargo clippy -p rustsimhub-ble --all-targets --locked --offline -- -D warnings
cargo test --workspace --all-features --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo fmt --all -- --check
git diff --check
```

在最终代码HEAD整仓 **816通过、0失败、6项继承忽略**，整仓严格Clippy/fmt/diff退出0。
此分支为main774+本库42，不含待审#97/#101/#103/#104。六项ignore继承C06/forwarder/Root/C14，
本批不新增；保留既有proc-macro-error2 future-incompat提示。源码未在实测后改变，仅补本人文档。
本地work/gatt-library-main日志未提交生成物，摘要：

```text
cleanup-before.log   7b547648d0e5f082727f3eda99c4a9ae5b53c7144db53e243e8c50a9a190641f
ble-final.log        49501f2f400978a7712d241fee0502eff234bf686a85cb1c8b469f3b78352428
workspace.log        865b411c5697a03f2d529b9d62fdc0d37526eea81d94efdac39cd025113f2960
workspace-clippy.log 3fe346e7c5baa37acdeb8f41edc8d66ada927d6bdca5011015e2fdfd68e73b70
```
原生产库实际运行，模拟只有外部Gatt/Secure端口（cfg(test)）；None secure拒绝注册。
没有真实BlueZ/D-Bus/HCI、密码/A互操作、Core可信UID桥全链路、ARM/Native/设备或浏览器BLE验收。

## 数量和交接

本批是已提交D-17～20和D-26前置的main收敛，重复完成步骤0；仍 **21/26离线包已提交，剩5步**。
固定P0–P7约80%、main19/169不变，不更新全局台账/成员/他人验收。
下一步先将本轮独立PR作本地联合合并回归，审核/生产启用仍由主线；不把联合编译当安全互操作。
旧#51/#59怎样关闭/收敛由负责人决定，不自动合并或关闭它们。

AGPL-3.0-only/原署名保留，OpenAI Codex辅助核对/冲突收敛/实测，无工时虚报或作者改写。
没有生产秘密/设备/NAS、真实业务网络/电话/短信/流量、Root账户/签名/安装/部署。
原日志只在work/gatt-library-main，CI按最终HEAD另读，未执行不得计通过。
