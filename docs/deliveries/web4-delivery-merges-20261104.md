# 六笔已交依赖的正常合并与复核

日期：2026-11-04，Asia/Shanghai。用户明确“能合并就合并”。GitHub状态及真实祖先关系均回读通过，当前main `5304a96d28f2c76c14ab651938a217dbeae67fa3`。六笔依赖及解除冲突的214都已合入；不更改成员原分支、历史作者、保护规则或计费，不直接写main或使用管理员绕过。

| PR | 原/最终完整HEAD | GitHub记录merge commit |
|---|---|---|
| [206](https://github.com/Viper-Boss/RustSimHub/pull/206) | `fe65bf3e1fd02375ed6fcd38a595271c33180caa` | `fe194d38c1e5c9a637ee916e9ab3374c720b234c` |
| [209](https://github.com/Viper-Boss/RustSimHub/pull/209) | `42dca70ff82b405b787616def708903bb126bad5` | `d43b5fbee8172f144537b433c7af03b5ef984206` |
| [210](https://github.com/Viper-Boss/RustSimHub/pull/210) | `19f9844fd76ff1597b69a694b2e86dff3561989b` | `7c4726f561f12c82e2306e5bc5a649a3736782b9` |
| [211](https://github.com/Viper-Boss/RustSimHub/pull/211) | `77b2baf88bc7409c72c31b5b08ea6b2aa8bf7ba9` | `5304a96d28f2c76c14ab651938a217dbeae67fa3` |
| [212](https://github.com/Viper-Boss/RustSimHub/pull/212) | `a148e670f8b6a38c0d7acfe4f9636b32a7eeff8b` | `c7e7264b92b392a678a6794b15ab295a266e02b8` |
| [213](https://github.com/Viper-Boss/RustSimHub/pull/213) | `28c38c0bcb5c802c1f223787fa27efc957410ebd` | `7296f866349b7b4bf7bf99cf1f9fef99191e121f` |
| [214](https://github.com/Viper-Boss/RustSimHub/pull/214) | `ccd128d19ecd83931f53f5bb68fa6c01ad62bf88` | `22042711b67868225a83e4341ff2a706e3e36a80` |

206仅交接文档冲突，214保留两边记录并正常纳入其历史，206随后由GitHub记录为merged。211在依赖进入main后从原堆叠base转向main再正常合并；不能把仍指向206分支的合并误当main已接入。197/200/198的历史随206进入，保留作者，不重复复制；本轮未单独审合195/196/199/201或草稿。

## 实际复核来源与边界

冻结受测源 `5f1d65bce9674cf6b990ea2b80745290c73f36ac`，完整Git归档SHA256 `39a7790e671532fda5032f4cc145e6207f66f8feece5893f37c08baf103e3606`。与最终main的代码/完整树等价核对由本轮最终检查另存；这里不把main的不同归档SHA或未来ARM产物冒充旧源制品。

- 五包默认：497通过/0失败/12专项ignored；全特性：749通过/0失败/17专项ignored。默认与全特性覆盖重叠，不累加。测试在同源临时Linux checkout执行，RUST_TEST_THREADS=1；原安装包约定的文件锁专项串行运行，无删测试/放宽断言。
- 五包all-targets/all-features严格Clippy、fmt及安装非cfg(test)同源库严格Clippy通过；均locked/offline。
- 数据模块四feature模式的同源harness测试及各自all-targets严格Clippy均通过：

| 模式 | 通过 | 专项ignored |
|---|---:|---:|
| default | 8 | 1 |
| product | 8 | 1 |
| product + email | 8 | 1 |
| dev-fixtures | 8 | 1 |

本轮新增修正只有独立harness的Cargo.lock里给rustsimhub-update补已有rustsimhub-wire依赖；逐包比对版本/来源/校验值完全不变，没有升级依赖。原正式入口、DataFence和产品模式接线仍留给网页窗口8，未接管它的生产开发。

## 保留的失败与未运行

初次组合旧harness锁清单使--locked拒绝，修复后同一约束重新验证。并行运行曾有安装RootIO锁重开Busy，以及原admission维护快照释放后Store仍忙两个偶发失败；前轮并行497/749通过不抹掉后续失败，原因尚未独立确证，不称并行全绿或产品缺陷已修复。串行用例全部执行通过只是当前验证结果，不替代该问题的后续定位。具体为 `crates/update/tests/product_materials_rootio.rs::fixed_lock_is_real_exclusive_and_released_and_unsafe_alias_refused` 的drop后重开返回Busy，以及 `admission::tests::actual_main_writer_blocks_observation_and_snapshot_blocks_the_real_main_writer` 的drop后Store::open断言。下一轮连续守卫接线需保留这两项诊断；不能靠删除测试、增加sleep或隐藏重试宣称修复，当前并行CI稳定性仍未证明。

准备锁清单时，第一次工具链PATH缺失；随后离线metadata已写最小锁差异但因无缓存的WASM bumpalo失败。两次都不计测试通过，不因非目标缓存缺项启用联网或改变依赖版本。原日志/准备失败及各完整命令/退出码/摘要仅存私有目录。

云端实际注释为付款/消费限制未启动，不称CI通过。作者既有Root/SIGKILL/只读/ENOSPC、迁移16/9和健康native隔离结果是原报告证据，本轮没有重跑这些专用容器矩阵。新ARM/QEMU、真实PID1/cgroup、强制PID复用、真实ON worker及设备验收未运行；没有生产签名、分配epoch、访问NAS/棒子或部署。

## 下一步

网页窗口8从最新main消费上述代码，按新版任务书继续正式入口、连续持锁及有限Root变更、健康profile兼容、同源发行和真实隔离端到端。原健康只支持有限schema6/forwarding，不因合并变成schema9产品可安装。任务8仍未完成；原P0–P7 **81.5%**、完整动作 **20/169**、加权 **10.6%**保持，PR合并不等于功能/设备验收。
