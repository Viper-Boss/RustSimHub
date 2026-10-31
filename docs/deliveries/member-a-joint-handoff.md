# A-30 离线联合回归与交接候选

账号 wnfhsj88；基线 main `80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d`。
本次联合受测 Rust/Go/网页实现完整 HEAD
`63ec1d85869e0a6a31a4bc6d9b1b10b43d07a3dc`；新增跨组件测试和交接索引的完整 HEAD
`77c02546131f502c01d1251d2cab6ebd0852c44d`。
后者未改变 Rust/Go/网页实现，提交后实际执行其新增 interop 测试及公开源扫描。
本报告提交只增加此文件；PR 最终 HEAD 以 GitHub 回读为准。

## 本包实际修改

正常合并本人 PR108/123/118/119/120/121/124/128/129/133；123 包含112，128包含115。
完整组件 SHA、基线、冻结接口摘要和缺口见 `docs/nkn/member-a-handoff.json`；
所有组件 SHA 都经实际 `git merge-base --is-ancestor <sha> HEAD` 核验。
Cargo.toml 的真实合并冲突仅合并两个新增本地成员；Cargo.lock 同时保留各组件条目，
未升级/删除第三方包、未重写作者、未改其他成员的分支。

新增 `tools/nkn-build/test_status_interop.py`：实际 native 进程的专用管道输出 →
冻结 T5 14 字段 → PR119 实际网页解析器和文本投影。无 SDK、未知/null 的语义保持，
传入年龄 0 也不能把 unavailable 变成零流量/Ready；多字段或少字段真实拒绝。
版本检查用于避免误用旧本地制品，不是对自报版本字符串的密码学出处认证。

新增有限开关/真实支持/故障回选说明 `docs/nkn/member-a-handoff.md`，更新原14步
状态索引：A-25现在有PR133实际代码/31测试，原其余9步仍有实现缺口。
没有改写原报告的测试数，也没有将部分交付标全部完成。

## 实际验证

Linux x86_64，Rust1.98.0、Go1.27.3、Node24.19、Python3.12、系统 Chromium；
启用 `/workspace/toolchains/activate-rustsimhub.sh`。

| 实际命令/范围 | 结果 |
| --- | --- |
| `cargo test --workspace --locked` | **59段，907 passed / 0 failed / 6 ignored** |
| 三个新实现包 nkn-agent/usage-stats/forwarder 的 `cargo clippy --all-targets --all-features --locked -- -D warnings` | 通过 |
| `cargo check --workspace --all-targets --locked` | 通过；既有 proc-macro-error2 future-incompatibility 提示保留 |
| `cargo fmt --all -- --check` / `git diff --check` | 通过 |
| `tools/test-web-*.mjs` 全部26个脚本 + `tools/test-ble-configurator.mjs` | 全部退出0 |
| `tools/security-matrix/test-audit.mjs` | 7通过 / 0失败 |
| Python security-inventory / dependencies / site-build | 分别18 / 6 / 2通过 |
| Go module `go test ./... -count=1` / `go vet ./...`，GOPROXY=off/GOSUMDB=off/GOTOOLCHAIN=local/CGO_ENABLED=0 | 7测试通过、vet通过，无下载 |
| 当前联合源码实际构建 amd64 + `test_native.py` | 9类真实进程/超时/信号/EOF/帧检查通过 |
| `test_status_interop.py` | 4类真实进程/实际网页/冻结契约互操作检查通过 |
| NKN status Chromium1440/390/320 | 实际模块、陈旧计时、键盘/CSP/无溢出通过；外部状态源明确合成 |
| Configurator Chromium1440/390/320 | 实际IDB CAS、离线SW、待定记录/多窗口/会话竞争更新保护通过 |

这里的907是本次组合实际测试结果，不是单包/不同分支数量相加；未运行整个workspace
all-features测试。6项既有特殊Root/tmpfs/子进程worker忽略不记通过；无设备验收。

当前联合原生前端仅本次 amd64 单次构建，不冒称重新做了双架构可复现验证：
4,081,530B，SHA256 `9ba4e79e99c8e15dd606949cfc39670372925e3d2b7d45fbab5c672e14bdcff4`，
内嵌实际联合实现 SHA63ec1d8；未链接 NKN SDK。此前双架构各两次独立构建仍引用
PR128的960d379及其精确摘要，未把旧制品当当前SHA产物。没有ARM运行/签名/安装。

实际公开跟踪源扫描在77c0254运行：退出2/incomplete，56项启发式候选、3问题
（两项line_limit、一项total_limit），8MiB预算后仍90输入未扫描；不称无秘密或已泄漏。
实际固定systemd/polkit静态审计：退出2/incomplete，112个策略格全部matched、
0 finding，但app-select/app-recover两个原服务仍缺地址族声明，2 gap保留。
没有改其他负责人文件或把声明审计当实际OS权限结论。

工作区日志 `setup-logs/member-a-joint-*` 及 `nkn-native-joint-63ec1d8`；浏览器截图和
原始报告只留隔离工作区，不上传未经复核的证据/命中片段。

## 审核、缺口与下一步

Root/密码方案与SMTP兼容仍须负责人按任务书T6审核，组件未因此进入生产入口。
实际SDK/身份/固定出口、A-22真实状态provider、A-26协议/路由及D的Core桥、
A-27真正Rust/浏览器加密互操作、A-29生产兼容接线、完整A-30系统组装仍缺。
这些是具体未实现/未验收项，不以unavailable前端/合成端口替代。

未引入新的第三方包或升级版本；邮件仅新增已锁定base64的直接依赖边及time的formatting特性。
项目AGPL-3.0-only，Go运行时BSD许可按PR128保留。
OpenAI Codex辅助实现、合并冲突处理、实际回归和报告。
本候选供联合审核/回归，主线可按小PR逐一接收；没有自行合入main、发布看板、签名、
部署、连接真实NKN/链/邮箱、修改Root/路由/设备动作。
公开整体进度沿用最后实际核验的 **81.5%**，离线交付不虚增全局完成率。
