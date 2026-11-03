# WEB2-RUN · 固定运行器检查同步

2026-11-03（Asia/Shanghai），真实作者wnfhsj88，Codex辅助，AGPL-3.0-only。按第二批任务书执行主线辅助包；使用从最新main **`8f9cfeff1a4b5a804b8b138e890f7386caf5a006`**建立的独立干净工作树。没有引入第二套运行器、外部命令清单、通用环境赋值或新依赖。

实现/实际受测HEAD：**`a491e083d84b8a8fe901582bd020384f42225609`**。分支`codex/web-mainline-checks-sync-20261103`；后续仅追加本报告，最终40位PR HEAD在PR正文登记，不把报告变更冒称重新执行产品。WEB2-LAN已单独提交PR195，本包从main开始，不含LAN产品修复，也不等待其合并。

## 实际变化与覆盖对照

原CHECKS真实展开86阶段；当前真实展开**92阶段**：ordinary58、native-audio4、runner1、root6、root-netns13、root-systemd10。原86项没有删除，网页固定元组展开的脚本原本已覆盖，未重复增添。新增如下：

| ID | 实际命令/职责 | 组/计数/前置 |
| --- | --- | --- |
| offline-delivery | `python3 -m unittest tools.offline_delivery.test_delivery -v` | ordinary；实际unittest计数，固定文件存在 |
| kernel-artifact-audit | `python3 -m unittest discover -s tools/kernel-artifact-audit -p 'test*.py'` | ordinary；实际unittest计数 |
| nas-control-test | `go test -race -count=1 -timeout=60s -json ./tools/nas-adapter/control ./tools/nas-adapter/cmd/control` | ordinary；Go/CGO/CC及已支持Linux amd64/arm64；阶段240秒；只增加-json记录真实顶层/子测试/skip |
| nas-control-vet | `go vet ./tools/nas-adapter/control ./tools/nas-adapter/cmd/control` | ordinary；Go；阶段240秒；按退出码，不伪造测试数 |
| native-test-relay / native-clippy-relay | 原独立manifest命令加固定`--features turn-relay` | native-audio；与默认分别报告；不运行test=false私有中继驱动 |

两项Go仅在各自子进程复制固定GO111MODULE=off、GOTOOLCHAIN=local、GOPROXY=off、GOSUMDB=off；名称/argv/cwd不匹配即拒绝。没有给Check构造器或CLI开放任意environment字段。后续NKN保持原模块模式和父环境，真实子进程夹具证明隔离；它是外部工具环境夹具，不是本包执行了真实NKN SDK检查。

工作流对照采用**人工审过的固定字节版本**，并非猜测/解析任意YAML：绑定`.github/workflows/rust.yml` SHA256 **`9b746809729d5c60e65db8b72598fb9f1229109ee52c900396f4322739936e21`**，47个实际具名步骤映射到75个固定阶段身份，另17项明确runner-only；外部安装/版本探针/混合polkit准备另列而不冒称运行。92项展开argv/cwd/env/组/超时/依赖/产物元数据逐项比对。新工作流字节/语法、阶段增删/改参会报待人工同步；对照文件从不提供可执行命令，执行始终来自CHECKS。`workflow_coverage.py`与CLI执行/list前均检查。

增加有限`--check`仅选择已提交ID、保留原顺序和组约束，省略依赖仍not_run；不能输入shell/外部清单/命令/环境。原20项进程树/pidfd/源冻结/日志上限/防覆盖等用例保持，新增8项回归含真实子进程的环境隔离、模块方式零项/skip/成功、race缺能力/独立vet，以及工作流未知语法/argv漂移拒绝。Go结构化结果区分测试/子测试/无测试CLI包，零项/未知/重复记录失败，测试skip为not_run。

## 实际验证与失败

环境：Debian x86_64、EUID1000、Rust1.98.0、Node24.19.0、Python3.12.14、Go1.27.3；原库/CC已准备。每shell加载既有工具链；Go仅使用本人已有编译缓存，未清理他人缓存；原始日志留本工作树`.local/`，没有复制他人私有目录/生产材料。

| 实际执行 | 结果 |
| --- | --- |
| `python3 -m unittest discover -s tools/mainline-checks -p 'test_*.py' -v` | 最终28项通过（保留原20项），退出0 |
| `python3 tools/mainline-checks/run.py --group runner --output .local/runner-final/report.json` | 干净源a491e08；runner-unit实际28项/0skip，1阶段通过，退出0 |
| `python3 tools/mainline-checks/workflow_coverage.py` | 92个真实展开身份、47个固定工作流步骤对照通过，退出0 |
| `python3 tools/mainline-checks/run.py --list` | 92阶段及明确排除/审计元数据，退出0；未按字符串grep误报网页脚本遗漏 |
| `python3 tools/mainline-checks/run.py --group ordinary --check offline-delivery --check kernel-artifact-audit --check nas-control-test --check nas-control-vet --output .local/new-offline/report.json` | **干净源a491e08，4过/0败/0未运行，退出0**；各阶段before/after来源保持 |
| 上述四阶段实际计数 | Python24/0skip；原内核21/0skip；Go10顶层+10子测试/0skip，CLI包无测试另记；vet退出0且无测试计数 |
| `git diff --check` | 退出0 |

首轮unit执行于8f9cfeff的未提交候选工作树，28项中1失败：夹具假定零测试返回0，但本Python实际返回5。此轮未冻结干净源码，不赋予虚构的完整候选受测SHA；原始unit-initial.log保留。已只修正测试对Python真实0/5结果的预期，未改运行器的失败分类，随后在上述完整干净HEAD通过28项及实际四阶段。零项从未算通过，fake-Go仅用于环境/计数失败夹具，不替代四项真实执行。

## 未运行与下一步

本包只执行新增四阶段、运行器回归与覆盖对照，不把这个子集叫全仓全绿。未执行其余ordinary/native/Root组；Root探针显示EUID1000且sudo不存在，未借普通UID/Mock替代Root、netns或PID1。保留浏览器专用恢复入口的not_run边界及Native私有test=false驱动边界。真实网页/Go/NKN/业务组合由下一包WEB2-COMBO在固定集成来源执行；本包不跨目录修复产品、workflow、release材料库或NAS客户端。

GitHub CI按最终PR HEAD另外回读，不用本机检查冒称云端通过。提交给Viper-Boss审核后继续WEB2-COMBO，不自行合并、改权限/计费或部署。全局仍**81.5%**、20/169、加权10.6%，92个运行器阶段不是全局动作新增分母。
