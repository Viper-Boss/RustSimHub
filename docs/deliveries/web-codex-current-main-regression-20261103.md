# WEB2-COMBO · 固定待审组合的实际结果

2026-11-03（Asia/Shanghai），wnfhsj88，Codex辅助，AGPL-3.0-only。本包只交运行证据/报告，不跨包改产品，不修改看板、权重、依赖锁或实际作者。来源及92个阶段的命令身份、退出码、计数、日志摘要哈希见[脱敏来源索引](web-codex-current-main-regression-20261103-source.json)；原始日志仅保留本人工作树的.local，未上传环境全量、原始通信数据或凭据。

## 来源冻结

开始与结束报告准备时核对main均为 **8f9cfeff1a4b5a804b8b138e890f7386caf5a006**。PR195、196在冻结时尚未合并，因此按任务书在独立临时分支正常合并两个固定HEAD，而不是宣称当前main已包含本包产品：

| 来源 | 完整HEAD / 说明 |
| --- | --- |
| WEB2-LAN / PR195 | 6787addaf4ca0e79f8b8455cbb3175372c9c318c；其产品/测试受测b0e4c23f8be65a85128df004e22c080325260830，随后仅报告 |
| WEB2-RUN / PR196 | 395a07eea72a074f3126466d03a2b899787ac9eb；其实现/实际新增阶段受测a491e083d84b8a8fe901582bd020384f42225609，随后仅报告 |
| 首次临时合并 | a1b36e8b45e11b7a613f95168d56e17322362b0c，parents为main与PR195 |
| 本轮实际受测组合 | **72251acb78f73286ed143f7f46b9780d133174c0**，parents为上述合并与PR196 |
| 受测tree | d1f74cad823cfddecf89d012a9cfce765c1b66fa |
| git archive SHA256 | 4027cf19449c35db76306b8f03641516831eef05e8c8a35510f7334d768c92b9 |

[固定组合源](https://github.com/Viper-Boss/RustSimHub/tree/72251acb78f73286ed143f7f46b9780d133174c0)由个人分支codex/web2-pending-combination-20261103保留，供重现，不是main或部署。两次合并均无冲突，未用ours/theirs遮盖。全程没有切换/编辑该工作树源码或并行占用同一target；全部阶段的before/after来源均保持同一干净提交。报告PR另从main建立codex/web-current-main-regression-20261103，仅新增本报告和索引，不将两包产品再次混入报告PR。最终40位报告HEAD在PR正文回读登记，不能给另一个main或后续代码版本背书。

## 能力与实际执行

Debian Linux x86_64，UID/EUID1000；Rust/Cargo1.98.0、Node24.19.0、Python3.12.14、Go1.27.3、ALSA1.2.14/Opus1.5.2。真实pidfd、子收割器及进程树清理路径已使用。Root、sudo、netns、mount namespace、systemd/cgroup门禁、polkit不可用；没有降成Mock或普通UID代替专项。

overlay约39～40MiB空余，新工作树与target放在本人的可执行tmpfs中；CARGO_INCREMENTAL=0、DEV_DEBUG=0、TEST_DEBUG=0、BUILD_JOBS=2。本人上一轮公共源码编译缓存移至本组合target，随后由Cargo按本次来源构建，未复用生产制品。内存限制8GiB，观察曾接近7.77GiB；只回收本人本轮已完成LAN target的455个静态编译缓存文件、628095534字节，保留全部可执行文件、源码和原始日志，未清理他人缓存。没有实际ENOSPC/OOM或资源失败。

主运行使用原更新运行器：

    python3 tools/mainline-checks/run.py --group ordinary --group native-audio --group root --group root-netns --group root-systemd --group runner --output .local/full-20261103/report.json

**主运行退出1：92阶段，54通过、6失败、32未运行。** 原结果不回填。分组如下：

| 组 | 通过 | 失败 | 未运行 |
| --- | ---: | ---: | ---: |
| ordinary | 49 | 6 | 3 |
| native-audio | 4 | 0 | 0 |
| runner | 1 | 0 | 0 |
| root | 0 | 0 | 6 |
| root-netns | 0 | 0 | 13 |
| root-systemd | 0 | 0 | 10 |

| 实际关键检查 | 本轮结果（不累计为独立总测试数） |
| --- | --- |
| cargo fmt / 全仓严格Clippy / production check | 均退出0 |
| 工作区默认 | 1281通过、0失败、6项既有ignored |
| 工作区all-features | 1539通过、0失败、11项既有ignored；ignored单列，不自动算普通通过 |
| 原生默认 | 48通过、0失败、0ignored |
| 原生turn-relay | 56通过、0失败、0ignored；默认与可选重叠，不相加 |
| 两种原生严格Clippy | 均退出0；本次没有Codec/Bounds失败，不需要重试或单项诊断 |
| 新离线材料/原内核/正常NAS | Python24、内核21、正常NAS10顶层+10子测试均通过；vet退出0，无伪造测试数 |
| NKN默认离线Go | 26个实际顶层测试通过；不是公网NKN验收 |
| 配置器四脚本、其余网页/工具 | 各按索引记录；网页存在下面六项失败，不称全部通过 |
| upgrade-compatibility / worker-inspection | 原有限CLI均退出0，不是正式Root/设备或真实SMTP递送验收 |
| 运行器 | 原20项+新增8项，实际28项通过；工作流固定字节/展开身份审计通过 |

默认Python缺cryptography，导致development-packager、ap-package、ap-install在主运行not_run。随后发现**已预装**Debian Python3.13.5/cryptography43.0.0，在主运行结束后只用该解释器补验这三项，同样固定源72251ac，未安装/升级包或改锁：

    python3 tools/mainline-checks/run.py --group ordinary --check development-packager --check ap-package --check ap-install --output .local/supplemental-crypto/report.json

该补验通过3阶段、退出0：development-packager命令通过（不伪造unittest数）、ap-package10项、ap-install10项。补验独立记录，不覆盖原主运行。**按固定阶段ID去重的覆盖为57通过、6失败、29未运行**；这是派生覆盖表，不是将主运行改写成57通过，也不是全仓全绿。

## 六项真实失败与准确归因

六项均退出1、均在业务断言前被VM加载阻断：

| 阶段/精确命令 | 夹具入口 |
| --- | --- |
| node tools/test-web-daily-phone.mjs | tools/test-web-daily-phone.mjs:149 |
| node tools/test-web-daily-sms.mjs | tools/test-web-daily-sms.mjs:33 |
| node tools/test-web-event-sync.mjs | tools/test-web-event-sync.mjs:105 |
| node tools/test-web-session.mjs | tools/test-web-session.mjs:69 |
| node tools/test-web-test-center.mjs | tools/test-web-test-center.mjs:43 |
| node tools/test-web-wifi.mjs | tools/test-web-wifi.mjs:35 |

固定72251ac的最小实际错误为：evalmachine.<anonymous>:595，**ReferenceError: NknProductPanel is not defined**。app.js:19确实从nkn-product.js导入NknProductPanel/pendingInput/archiveInput，构造点为app.js:595，组件实际export位于nkn-product.js:34。上述旧VM夹具先移除import，再运行app但没有注入这些新增组件依赖。这是可复现的**夹具/源码集成缺口**，不能据此断言浏览器正式页面必崩溃，也不能把被阻断的业务断言计通过。没有因第一项失败停掉其他独立阶段，没有重跑取绿或跨范围修JS/夹具。

建议原网页/夹具维护者按72251ac同步实际NKN组件及必要port注入并重跑这六项；不能用空桩绕过产品断言。主线此前Core准入竞争的独立报告仍保持，本次Rust通过不能证明该历史竞争已修复；没有擅改Auth/Storage/租约。

## 跨包接线与未运行

| 范围 | 本轮证据 | 生产/实机边界 |
| --- | --- | --- |
| WEB2-LAN | 同一组合实际验证解码后60ms丢弃、队列/存活不变、撤权/缺许可/未来/回退及错误Opus形状；发送侧回归保持 | 无ALSA硬件/真实电话/听音/ARM/部署 |
| WEB2-RUN | 92个固定身份、四新阶段真实执行、Go环境隔离与计数回归；原86未删除 | 运行器不是通用YAML执行器/发行准入，不能替代缺能力专项 |
| 已合入的NKN/网页 | 离线Go/组件检查及六项旧VM缺口已分别记录 | 没有NKN公网或浏览器真实目标支持结论 |
| 已合入PR194的C25～C30 | 材料工具/健康库/正常NAS候选和证据模板已在main；本轮相关离线检查通过 | health始终may_confirm/authorization_verified=false；实际Root生产者/confirm、完整可启动发行物、正常NAS批准/跨机音频及真实中继资源仍需主线接纳 |
| BLE/Core恢复、Root/SMTP等成员候选 | 默认/全部特性回归与原有限CLI结果存在；不是仅按“已合并”填绿 | 候选编译/夹具不等于设备启用、Root UID专项、GATT/RF/OTA、正式账号递送或签名安装；本次未访问/推断设备开关 |

29项Root/netns/systemd门禁阶段未运行，索引逐项列出capability/缺可执行文件/依赖原因；不能记成产品失败或Mock通过。原浏览器/WASM恢复入口、test=false私有Coturn/远程worker驱动、真实设备/NAS/SIM/短信/电话/推送、普通蜂窝数据、硬件/GATT/听音/OTA、生产签名与部署均未执行，且不混入92阶段分母。没有修改账户计费、权限、主线或全局完成比例。

主运行60个实际执行阶段和补验3个阶段均报告cleanup_complete=true、source不漂移，没有报告遗留的本人进程；未信号/清理他人进程。原始报告、日志、能力探针与本人缓存回收记录继续在本人.local保存。云端PR195运行41627223923与PR196运行41629916540均两项0步骤，实际注释是付款/消费上限阻止启动；不是云端通过，也不是本地六项VM失败已在云端重现。报告PR的CI另按其实际HEAD回读。

下一步由负责人审核三个PR、协调修复六个旧VM夹具，并在具备实际Root/netns/PID1环境后执行原专项；云端计费恢复后正常回读CI。真实支持/设备验收仍由主线签认。本执行者完成三包交付，不自行合并、扩大权限或部署；整体验收仍**81.5%**、20/169、加权10.6%。
