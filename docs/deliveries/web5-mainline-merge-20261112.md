# WEB5四包主线接纳 · 2026-11-12

四窗口代码4/4已接纳：223控制台、224首次热点、226发行审计正常合并，227矩阵经228整合并自动标已合并，原作者/父链保留。最终main2ec294b9与受测85aa0395完整树一致；本轮storage/update库243通过、19专项ignored，严格检查、5个Node入口及首启/矩阵/发行工具短回归通过。前序Node日期失败按原交付TZ=UTC要求复验通过，未改生产代码。未重跑新完整workspace、ARM/浏览器/WASM、完整镜像或设备；作者固定源证据不移算为新组合。未签名/部署/设备访问；原验收81.5%、20/169、加权10.6%保持。

## 接纳来源

按用户“4个跑完了，合并”的明确授权接纳，不新增第二人批准要求；本轮为来源/草稿/合并状态核对和短组合回归，没有声称全面独立代码审查。四个原PR均非草稿，52/53/56/69草稿保持。

| PR | GitHub实际状态 | 原HEAD | 合并提交 |
|---|---|---|---|
| 223 | MERGED | `665f485f768d3d0fc71cd94dc6c3022f18b5336c` | `5ea7b08cd05122c36cf7838e8138fa3764ca5055` |
| 224 | MERGED | `ab36b762bce521d58567931c6b575fbb3cc06c30` | `f6ef27b0c3a131e9bab7ffbd493cdd4c27e16fa9` |
| 226 | MERGED | `8fa52815056b1da01727ff3161d20b81b6c56302` | `300ca09d491015e1c653bb372e7ec58aade42af4` |
| 227 | MERGED | `2e8023e858ac8b40184acf2217cb8e91b9421cbd` | `85aa039536c5ff373ea1b68f35bc5ecbc79f7ebe` |
| 228 | MERGED | `85aa039536c5ff373ea1b68f35bc5ecbc79f7ebe` | `2ec294b969ef18bc19bc6bf3b8fb396136e43fa1` |

223、224、226正常merge。227在服务器返回405 merge conflicts；负责人从最新main300ca09与固定227HEAD正常合并得到85aa0395，本地ort无文件冲突，没有手工改写产品或成员分支。整合PR228仅带227未入main的13文件，正常merge后原227自动标MERGED。保留原作者与全部双亲历史；没有管理员绕过、保护变更或直接推main。

最终main `2ec294b969ef18bc19bc6bf3b8fb396136e43fa1`；实际受测源码 `85aa039536c5ff373ea1b68f35bc5ecbc79f7ebe`；该受测归档SHA256 `0a81f8efd4d3406745b9c1b809788c0cb676050312e1e4897d2fcbff62814029`。两棵完整Git树一致，提交/归档标识不混为同一制品。作者各包完整ARM/浏览器/隔离链结果继续绑定其各自交付文档，不转算成新组合已经运行。

## 本轮实际验证

- storage140/update103库全特性测试通过，共243，不与各作者的默认集/全特性集或上轮旧源1672叠加；19项专项ignored。
- 两库lib全特性严格Clippy与fmt通过。
- 五个Node入口通过：console fencing、密码、session、daily phone、daily SMS。控制台旧基础护栏和新增12组均通过。
- firstboot-ap单测25项中6执行通过、19因专项条件跳过；矩阵4项、发行审计18项通过。不把跳过的Root/PID1或ARM测试记为本轮通过。

- `cargo +1.98.0 fmt --all --check`：exit 0
- `cargo +1.98.0 test -p rustsimhub-storage -p rustsimhub-update --lib --all-features --locked --offline -j 8`：exit 0
- `cargo +1.98.0 clippy -p rustsimhub-storage -p rustsimhub-update --lib --all-features --locked --offline -j 8 -- -D warnings`：exit 0
- `node tools/test-web-console-fencing.mjs`：exit 0
- `node tools/test-web-password.mjs`：exit 0
- `node tools/test-web-session.mjs`：exit 0
- `node tools/test-web-daily-phone.mjs`：exit 0
- `node tools/test-web-daily-sms.mjs`：exit 0
- `python3 -m unittest discover -s tools/firstboot-ap -p 'test_*.py'`：exit 0
- `python3 -m unittest discover -s tools/product-integration/matrix -p 'test_*.py'`：exit 0
- `python3 -m unittest discover -s tools/release-audit -p 'test_*.py'`：exit 0

首次Node日期断言因运行器Asia/Shanghai相对UTC期望不同失败（实际本地12:00、期望UTC04:00）；交付原命令明确TZ=UTC。仅按其环境要求复跑网页/工具部分通过，未改源码或断言；原失败与复验日志均保留私有目录。相同源码已通过的Rust/fmt/Clippy按日志摘要继承，只计一次。没有重跑完整workspace/默认全套、新ARM/QEMU/真实浏览器/WASM、完整EmailRecovery隔离链、PID1或设备。

本轮PR228两项云端检查131018710033/131018710243实际annotations均为付款/消费上限导致job未启动，未记CI通过；Ubuntu迁移提示不当成代码错误。

## 已交付与剩余边界

四包生产源码/工具已接纳：控制台分页/时间/历史投影、通用首次热点准备与激活、中断EmailRecovery检查/恢复及有限支持矩阵、只读发行审计。不能继续写“这些功能未开发”；作者完整八步报告保留原始验收限制。

下一步：用最终来源统一构建所有目标及完整系统材料，重跑组合/ARM/有限健康/恢复验证，准备完整发行输入；最后进行真实PID1/AP/手机/第二设备及资源/长期边界验收。旧应用74或旧847制品不能改标签充当新系统。AP/SMTP5自动健康仍明确不支持；PMIC继续搁置。

本轮未签名、分配epoch、上传安装、刷分区、操作NAS/棒子或发送真实消息。主工作区原未提交改动保留。进度81.5%、20/169、加权10.6%，原169项与权重/状态逐项未改；32包内步骤不加入分母。贡献/PR按最终main重新统计，作者不改写。

看板build校验、Node页面/成员卡检查及29项Python台账/成员/计划测试通过。公开页面仍仅同步13个白名单文件；Pages部署及在线四文件摘要核对保留独立私有发布记录，本地离线版同步同一快照。
