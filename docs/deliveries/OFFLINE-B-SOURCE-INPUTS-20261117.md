# B1 固定来源与有界输入 — 2026-11-17

起始 main：`de663743bd726e77f7c560139d0f91ea78e8fe57`，包含产品基线
`2ae840fbffff04691ccf8cedd49d554093dd4032`。独立分支
`codex/offline-b-source-inputs-20261117-fungi1220`，本仓库PR到main，不依赖PR270的源码。
实现/最终完整HEAD在PR正文绑定，避免报告自引用。当前整体仍81.5%。

## 实际修改

- 原 `inputs.json` 前18项及基线逐字段保留，追加14项为32项，原32项/128KiB上限不变。
  PR251/252/253/255/256/257/258/263/265/266/267/269是代码/工具来源，261/264仅报告。
  每项读取真实HEAD和改动文件，并用实际 `git --no-replace-objects merge-base` 核对，
  `includes` 仅填写真实祖先。报告不能提供产品依赖。逐项证据在
  `docs/offline-b/source-input-review-20261117.json`。
- 新发现PR268为草稿，明确排除；250/262/259/69/52/53/56等其他窗口/延期草稿也不消费。
  没有改作者、邀请/名单、main、草稿或他人分支。
- 实际复现：120001字节、60000层JSON锁在旧CLI触发未捕获RecursionError，退出1并打印
  traceback；初始2项反例1通过/1失败。修复把递归耗尽分类为有限拒绝/退出2。
  文件在打开/分配前检查为单链接普通文件且不超过128KiB，读取最多上限+1，核对
  打开对象及读取前后元数据；空文件、非普通/链接/替换/增长不得获得成功。
- 发现并修复已有主线运行器指纹滞后：旧hash9b7468…不匹配当前 `rust.yml` 的935203…，
  会直接拒绝启动。实际逐项比较新增的3个步骤，原 `rust.yml` 不改。
  统计UI/Owner检查映射到固定argv，AirSIM步骤保留但明确 `excluded_scope/not_run`；
  不能从排除项提供可执行检查或冒称CI通过。没有执行/修改AirSIM业务或测试。
- 新来源回归进入专用CI（完整Git祖先、不安装运行时）及主线固定stage；同步原
  workflow inventory全部96个check身份，覆盖50个步骤的静态记录。其中79个检查有
  原rust.yml映射，其他为runner-only；不是96项已运行或全CI通过。
- 新交接手册使用原六种profile、原包格式/构建器，保留热点延期、schema9回退限制、
  统计候选未完整接线及签名/安装/恢复/设备前置，没有创建另一个“绿色构建manifest”。

## 真实验证

环境：Windows AMD64、Python3.12.14，本机已有Node24.22.1；本包未下载新工具链/固件。
合成数据仅放任务自建临时目录，未访问设备/NAS/生产密钥/浏览器Owner资料或业务网络。

```text
python -m unittest discover -s tools/product-integration -p test_*.py -v
  exit0：20通过、0失败/跳过（原12 + 新8）
python -O -m unittest discover -s tools/product-integration -p test_input_limits.py -v
  exit0：8通过；生产防护不依赖assert
python -m unittest discover -s tools/mainline-checks -p test_coverage_contract.py -v
  exit0：4通过
python tools/mainline-checks/workflow_coverage.py
  精确workflow字节及96个身份审核通过；不是执行这些检查
python tools/product-integration/verify.py --repository . --integration BASE --require-complete
  开发时32项included、无缺项，因工作区脏正确退出3；完整提交后另按实际HEAD回读
python tools/product-release/release.py --help
  exit0：核实真实6个profile和参数；未执行构建
```

同时运行原依赖清单工具：显式指定 `tools/nkn-engine` 作upstream时因缺go.sum拒绝，
退出1，未将它冒充完整SDK stage；未提供upstream的正确调用生成声明清单后退出2，
`completeness=incomplete/approved=false`。本地报告SHA256：
`ed6b1b5436e7b3dea0fc71bf14dfb49e5a352a62fd49ab1da64b4d36e164130c`。
它不是实际编译模块的许可或供应链批准，未知项见交接手册；没有更改锁文件。

## 未运行与前置

主线Linux进程运行器本体/原Linux同步测试在本机未运行；便携的精确身份测试4项已跑。
Rust/ARM/QEMU、实际LinuxUID/PID1/cgroup/nft/ENOSPC/SIGKILL/迁移/签名样本完整组、
真实Chrome/三宽、实际SDK/音频sysroot/角色快照和完整profile构建均缺环境或输入，
没有用旧制品/模拟成功顶替。没有生成可安装候选、生产签名、epoch或健康许可。

CI按新PR实际HEAD单独回读；原PR270的计费零步骤不是本PR编译/测试结果。
没有重跑计费失败或修改计费/保护。B2/B3的真实Linux故障矩阵与B5实际链接闭包、
长期资源样本、发布树扫描仍需合格输入；交接手册明确逐项前置，不宣称全部B任务完成。

源码与许可证沿用AGPL-3.0-only；无新依赖/生产接口，未改A业务、Root写入白名单、
默认开机、现有身份或全局进度。Codex实现并检查；未估造自报工时。主线需审核合并、
按同源输入复验，再独立安排签名安装与现场验收。回退仅撤回本包工具/索引/文档，
不涉及用户数据库、种子、队列或设备。
