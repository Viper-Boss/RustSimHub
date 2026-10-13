# 通话控制验证（P3 初始）

2026-10-13。最终 Rust 源码 `73db36a42c1b88a0b789a0dcd21343b94d087b16`。

## 结果

| 环境 | 验证 | 结果 |
| --- | --- | --- |
| GitHub / Rust 1.98 | 格式、90 项测试、默认 core 检查、Clippy -D warnings | 全部通过 |
| NAS ARM64 / Rust 1.99 | 默认 workspace、90 项测试、release、自检、CLI help | 最终容器退出 0 |
| 私有 peer | 拨号/DTMF 方法超时、重复 submit | 每个动作只调用一次；Unknown 不自动重试 |
| 私有 peer | 两客户端接听及真正同时提交 | 一个拥有控制权，Accept 仅一次 |
| 私有 peer | 错误客户端续租、授权撤销、正常关闭 | 拒绝；仅本核心拥有通话触发一次挂断清理 |
| 私有 peer | 租约过期、挂断失败 | 不伪造 idle、不放行新拨号、不重复清理 |
| 私有 peer | 外部来电、只读模式 | 外部通话阻止拨号，只读模式不能创建控制意图 |
| SQLite | scope、完整 generation、幂等冲突、唯一许可、事务失败 | 通过 |
| SQLite | SIM 切换、断开/重开 | 呼叫观测 Reconciling、提交 Unknown、旧许可不能再次取得 |
| 棒子 nassimhub 非 root | 呼叫目录/状态/事件、calls_read 与只读控制拒绝 | 通过；无真实电话或短信写入 |
| 棒子独立 root 写模式 | 用户授权的一次 10010 拨号 | ACTIVE (modem_state=4) 已观察 |
| 棒子真实挂断 | 同一租约的优先挂断 | Submitted；最终 TERMINATED (state=7) |
| 本机私有备份只读核验 | 一次 dial、一次 hangup，均 resolved/submitted | 通过；无额外拨号/清理动作；SMS 操作数 0 |
| 最终状态 | 挂断后 available=true、active_calls=0 | 通过；临时单元和 socket 已关闭 |
| 旧服务/网络 | nassimhub-agent/MM/NM/rmtfs、Wi-Fi/ethernet/GSM | 四个 active；Wi-Fi/ethernet connected；GSM disconnected |

90 项：auth 21、core 22、ctl 1、domain 9、policy 1、storage 31、wire 5。
CI：[38340600665](https://github.com/Viper-Boss/RustSimHub/actions/runs/38340600665)。
NAS 最终默认检查/测试/release 成功；release 重建 4 分 26 秒，限制 2 CPU / 2 GiB，这是构建耗时。

## 实机边界

只拨打用户明确授权的 10010 一次，观察 ACTIVE 后脚本自动提交挂断；没有拨打个人或紧急号码、发送新短信、
启用蜂窝数据、关闭旧服务或更改分区。此次电话是否计费未通过账单核验，不声明免费。
没有捕获、播放或注入通话声音，media_available=false；ACTIVE 不代表双向声音已验收。

写模式为独立 root 临时单元，系统只读、只放开新私有目录，最长 160 秒。
非 root 只读单元最长 150 秒。生产非 root 写权限和 systemd 安装仍需验证。
真实来电/拒接/DTMF、强制重启/断电、故障挂断及孤儿接管尚未进行硬件验收。

正常停止后用 fixture 重开私有 DB，会按设计将 observation 置为 Reconciling，但保留最后原始 modem_state=7。
核验依据是实时 ACTIVE/final-status 记录、已提交 dial/hangup 和最后原始 Terminated，不能将重开后的隔离状态解释成仍有电话活动。
跨重启的既有通话作为未接管通话处理，不自动挂断或再次拨号，详见 call-control.md。

## 资源与产物

RSS/HWM 样本：非 root 只读 6948 kB，拨号/挂断后 7016 kB（约 6.85 MiB），均 4 线程。
这是 Rust 核心样本，不含整套 Linux/MM/旧服务，也不是和 Go 的性能对比。

| 产物 | 字节 | SHA-256 |
| --- | --- | --- |
| source.tar.gz | — | f5dbf55ce8663ea3eb398f80fad00ca69b694b68d195325bdd0587a4e7b590b3 |
| rustsimhub-core | 5264864 | e57613ad4f30e6f6f71ca32a00f6e65368b1f2bcb086837d5cf76962f53840b8 |
| simhubctl | 857104 | 39cad87bb1d092c0aee001fbf6e84a546ace924750be037e6242fe6b67e33422 |

最终源包逐字节匹配提交中 52 个允许列表文件；ELF aarch64 PIE，构建镜像 digest 及 Rust 1.99 沿用前阶段。
core 含 dev-fixtures；peer 模拟仅 cfg(test)。最终测试辅助布局修正后重建，二进制哈希与先前只读实机测试一致，未额外触发硬件动作。
本机 `.local/artifacts/{rustsimhub-core-calls-p3,simhubctl-calls-p3}`，NAS 当前 `src/target/release/`，
棒子 `/opt/rustsimhub/dev-calls-p3/`。这些是开发产物，不是完整刷机镜像。

## 私有证据与接手

- `.private/nas-calls-final-build.log`、`nas-calls-artifact-final6.log`：最终构建、测试和哈希。
- `.private/device-calls-readonly.log`：非 root 目录、权限与只读验证。
- `.private/device-call-carrier-once.log`：真实一次拨号、ACTIVE、自动挂断和关闭摘要。
- `.private/carrier-call-evidence.tar.gz`：停止后的独立目录、DB、测试身份、会话和 once 标记；禁止公开。
- `.private/carrier-call-readback-summary.json`：只读核验的一次 dial/hangup、状态 4→7、零活动电话、零 SMS 操作及归档哈希。
- 棒子 `/var/tmp/rustsimhub-carrier-call-p3-evidence` 指向私有目录；`/var/tmp/rustsimhub-carrier-call-20261013.once` 已存在。

本轮单次电话授权已使用，不能删除 marker 或重新运行拨号脚本；此前短信一次授权同样已经使用。
中间编译/Clippy 对格式、测试模块布局和字段简写的诊断已修正并保留在私有/NAS history 中。
最后 CI 及 ARM64 均为修正后的源码，未绕过 lint。HTTPS 中断使用既有 API 同步，提交树哈希已验证一致。

下一步：ALSA 双向 PCM/媒体授权 PoC、真实来电与 DTMF、生产权限与 TLS、跨重启通话管理和完整系统恢复/镜像。
