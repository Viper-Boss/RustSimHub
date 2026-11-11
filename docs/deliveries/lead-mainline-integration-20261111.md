# 主线整合收口 · 2026-11-11

主线整合完成：215/216已正常合并；195/196/199随215纳入，200旧历史早在main并按已吸收关闭。固定源6f4e14a7与最终main完整树一致；本轮Linux统计79项、workspace全特性1672项通过，严格检查/默认生产检查/系统与发行工具专项通过。前序失败保留：Go PATH、网络绑定权限及测试临时根目录权限；仅后者修正3行测试夹具，经PR225正常合入，未改生产限制。没有新ARM/QEMU/完整Native或浏览器链，不沿用作者旧源结果当本轮通过。四个WEB5窗口继续原分工，本机未改其产品文件。未签名/分配epoch/部署/访问设备；81.5%、20/169、10.6%保持。

## 固定来源与接纳

最终main `c5fd2d082e25d15535f249f11f5b7a4604427fea`；本轮受测源 `6f4e14a75defa42e98a98dcad8cca67305e8ee04`，完整归档 SHA256 `18677e9925c7f23b92c31f27e1ed7f8c2013f7c1beebfce5b37fb6fa7f29209c`。两棵完整Git树逐字节一致，归档提交标识不同不混成同一个制品。源保存在codex/mainline-integration-20261111。本次正常merge、保留作者、没有管理员绕过/直推main/保护修改；全局云端CI仍须按实际状态记录，不把未启动当通过。

| PR | 当前GitHub状态 | 完整HEAD |
|---|---|---|
| 195 | MERGED | `6787addaf4ca0e79f8b8455cbb3175372c9c318c` |
| 196 | MERGED | `395a07eea72a074f3126466d03a2b899787ac9eb` |
| 199 | MERGED | `fb77eaf63a7ef50a8da229664fe37129a9a0bd6f` |
| 200 | CLOSED | `a690213cec950b273632f595b96a0bea8f8e5e64` |
| 215 | MERGED | `610d7fa63b1a0f129a3f146c0595171868f8e059` |
| 216 | MERGED | `35375a5afbada0c947eaffb2cd1b9ea6a7127cf2` |
| 225 | MERGED | `b82b25f779ddab8399d514ce02d5ab4681c163cb` |

195/196/199已由215包含，未再次复制；200已是旧main7535535的祖先，原PR按已吸收closed，GitHub未标merged不伪报。API尝试同时关闭和转base返回422，实际状态已closed而base保持原组件；随后回读确认并补吸收说明，未重复向组件分支合并。

## 本轮实际验证

统计包：79通过/0失败/0忽略。workspace全特性按环境分组：普通用户（除forwarder）(1554, 0, 30)，独立用户/网络命名空间内的forwarder (118, 0, 1)，不重叠合计1672通过/0失败/31忽略。统计包与工作区覆盖重叠不累计。隔离组只启用自己的lo，无外网、无宿主服务/账户变更；命名空间root不是真实宿主Root/PID1验收。Rust测试串行执行，保留原专项ignored；没有削弱断言/预算或重新实现WEB5模块。

- `cargo +1.98.0 fmt --all --check`：exit 0
- `cargo +1.98.0 test -p rustsimhub-usage-stats --locked --offline -j 8`：exit 0
- `cargo +1.98.0 test --workspace --exclude rustsimhub-forwarder --all-features --locked --offline -j 8`：exit 0
- `unshare -Urn sh -c 'ip link set lo up; exec "$@"' sh cargo +1.98.0 test -p rustsimhub-forwarder --all-features --locked --offline -j 8`：exit 0
- `cargo +1.98.0 clippy --workspace --all-features --all-targets --locked --offline -j 8 -- -D warnings`：exit 0
- `cargo +1.98.0 check --workspace --locked --offline -j 8`：exit 0
- `python3 -m unittest discover -s tools/system -p test_*.py`：exit 0
- `python3 -m unittest discover -s tools/product-integration -p test_*.py`：exit 0
- `python3 tools/product-integration/verify.py --repository /home/wzs11/.cache/rustsimhub/lead-nov11-qh9l2bs9/source --integration 6f4e14a75defa42e98a98dcad8cca67305e8ee04 --require-complete`：exit 0
- `python3 -m unittest discover -s tools/product-release -p test_*.py`：exit 0
- `node tools/test-web-test-center.mjs`：exit 0

系统工具unittest汇总205项，其中94项实际执行、111项因专项运行条件跳过；生产组合12项、发行18项通过。Node仅运行真实首页测试中心脚本，不代表全部32个入口或真实浏览器验收。上述工具结果不与Rust数量累计。

PR225最终HEAD为b82b25f779ddab8399d514ce02d5ab4681c163cb；与受测6f4e14a完整树一致，仅合入已接纳的main历史，GitHub最终diff只有测试文件3行。当前两项云端检查130632946839/130632946495的annotations均明确因付款/消费上限未启动，不是代码测试执行失败，也未记为通过。连接器首次合并225遇传输错误；只读确认仍OPEN后才重试，随后正常merge成功，没有不明结果重复提交。

首轮在NAS兼容夹具因PATH缺Go失败；仅添加已有/home/wzs11/.local/go/bin及GOTOOLCHAIN=local到本轮进程，未改锁/全局环境。第二轮普通用户forwarder有5项网卡绑定/DNS用例Forbidden/Elapsed失败，随后将整个forwarder测试组单列进无外网用户/网络命名空间，普通其余包仍为原用户；不将原失败抹成普通用户全绿。隔离能力检查的自动授权审核第一次超时，按工具允许重试一次后正常通过，未绕过权限拒绝。

第三轮两项retained安装器测试在创建私有Root前被Denied。确认原因：tempfile跟随运行器umask，目录未明确700；不改源码仅以umask077对照，两项通过。正式修正仅在测试prepare中增加显式700及解释共3行，经PR225提交；第四轮恢复普通umask完整复验。生产Root权限检查、签名、锁及预算均未放松。原失败日志和补验记录全部保留私有目录。缓存刚清理导致首次编译较慢，不按构建耗时判断产品性能。

## 未运行与后续

新组合未重建ARM/QEMU、完整Native端到端或全浏览器/WASM；未运行真实PID1/设备、外网、生产签名/epoch或安装。215原作者ARM4.494/4.732秒、128MiB/8任务等报告仅证明其固定8470608，不移算为本轮新来源。完整workspace默认测试本轮未跑，默认cargo check不等于默认测试全通过。后续按新来源做ARM候选与限时/资源复核，再按新只读现场证据准备签名/实际验收。

四个WEB5包继续原文件边界，不因主线接纳重派任务8或中途改写他人分支。原源码/私有证据/未提交工作保持；进度81.5%不因合并增加。下一项：四包交付后的接纳与统一来源冻结，再准备新来源候选材料；实机与签名另按实际前置执行。

## 进度维护

五人Git贡献/PR交付统计已按最终main重新读取；未改作者或归属规则。看板build检查、Node页面检查和Python台账12项/成员11项/计划6项均通过（共29项）。原模块任务状态、169分母、权重、20已验收动作和81.5%逐项保持。公开发布仅同步原13文件白名单，最终Pages部署与在线内容摘要核对另保留私有发布记录；本地版同步相同页面。
