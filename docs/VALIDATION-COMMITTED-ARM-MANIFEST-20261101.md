# 主线 ARM 构建清单接线

## 解决的问题与范围

正式 `tools/wsl-build-committed.sh` 仍只导出9个程序，漏掉当前系统需要的 setup-ap、Core launcher、mDNS。它还用 Rust 源码的排版/字符串判断数据版本和能力，有能力已经编入程序但清单显示 false 的风险。原归档检查逐文件比较却不核对完整清单，同时拒绝普通 `git archive` 自带的目录项。

本次增加 `tools/arm_build_manifest.py`，接到主线构建流程，不实现成员 C 的 ReleaseMaterialPlan/通用候选组装，也不修改成员的 Root 材料、健康评估库、设备权限或签名域。

## 接口

```text
python3 tools/arm_build_manifest.py verify-source \
  --archive <archive> --repository <repository> \
  --revision <full-sha> --source <archive-sha256> [--extract-to <new-directory>]

python3 tools/arm_build_manifest.py collect \
  --binary-dir <release-directory> --output <new-absolute-directory> \
  --revision <full-sha> --source <archive-sha256> \
  --toolchain-manifest <pinned-manifest> --sysroot <existing-gnu-sysroot>
```

- `verify-source`核对所有Git常规文件的名字、内容和完整性，允许对应目录，拒绝遗漏/额外/重复文件、链接、越界路径和非普通文件。提取时使用同一份已验证内存字节，不重开可变输入。
- `collect`固定导出当前完整工作区的12个主程序；历史九程序版本使用它自己的固定版本构建器，本工具不静默回退为半套输出。
- 每个程序核对ELF64/AArch64与GLIBC≤2.41，输出原消费者需要的平面 `artifact_sha256`、`database_schema` 和能力字段。Core、Edge、update、setup-ap、Core launcher的来源必须一致；Core与update数据版本一致，四个配网角色契约一致，测试开关关闭。能力来自实际QEMU `--build-info`，不再解析Rust源码字符串。
- 输入只接受有界、单硬链接普通文件；单文件24MiB、程序总量128MiB。元数据16KiB、readelf输出512KiB、工具15秒截止；不用shell，退出前按未复用的leader PID清理工具进程组。
- 检查期间不创建最终输出；发布时独占新目录、只复制固定程序，`build.json`最后写入。发布错误保留带 `BUILD-MANIFEST.FAILED` 的现场，不覆盖已有输出，不导出旧日志/本地路径/旧build.json。
- 构建器可显式复用既有 `RUSTSIMHUB_CROSS_HOME`，不重新安装工具链。此工具面向操作者自己构建的产物，不是可执行任意上传文件的服务，也不是编译器证明或生产信任/候选健康授权。

## 已执行验证

- 普通用户WSL执行11项工具测试，含完整/部分Git归档、目录/中文空格名、改字节/错hash路径、重复/额外/遗漏/链接、已有输出保留、FIFO/硬链接拒绝、错来源/架构/schema/能力/契约、磁盘满保留、JSON重复/非有限数及进程截止/输出上限。
- 同11项在 `python3 -O` 模式执行；保护使用显式异常，不依赖可被关闭的assert。
- 构建shell语法与 `git diff --check`。
- 用上一轮固定 `dc3aa493d84190248d26587ca662a197414d2958` 的真实产物运行新CLI：1202个归档文件核对、12程序元数据/架构/GLIBC观察成功，输出现有消费者可读取的规范build.json，schema=6。没有重编译或重新标注它们的来源。
- 调用现有开发打包器，使用实际Core/CLI/Edge/audio/forwarder和许可证生成三个格式2材料；独立临时测试密钥、epochs 1/2/3，production_release=false、ship_allowed=false。
- 在无网络Root命名空间的POSIX临时目录中，OpenSSL和真实ARM更新器只读验证三个材料均通过；改payload和错钥匙被拒绝且不是权限拒绝。

完整GitHub CI需绑定本次工具提交另查，不能把上一轮产品源码的CI扩大到此工具提交。

## 实测中保留的失败

第一次测试输出父目录不存在，未生成材料；随后普通用户调用ARM验签CLI被其Root门禁拒绝。改为POSIX目录的普通用户尝试仍被同一Root门禁拒绝，故不能把早先猜测的NTFS原因写成事实。最后使用既有材料在隔离Root中验证成功；没有放松生产检查，所有失败报告均保留在私有目录。

WSL验证Windows创建的worktree时，其Git指针不能直接当Linux路径使用；实际归档核对选用共享同一对象库的主仓库。未改Git指针、没有重置任何脏工作目录。

这些是主线构建/接线和离线验签证据，尚未生产签名、组装完整rootfs、部署或手机验收。成员C仍负责候选通用组装，主线保留真实签名/安装；人工测试最后，原81.5%及20/169验收保持。

## 正式构建中的Cargo硬链接（2026-11-01后续）

固定f53deda8的完整正式流程实际跑完工作区测试及12程序ARM release编译，随后在清单收集时被单链接检查拒绝。现场所有Cargo release程序nlink=2：它们与Cargo自己的deps命名共享文件。此前规范清单检查用的是已经复制出来的独立产物，没有覆盖这个真实入口差异；该失败保留，不能称此轮正式流程通过。

构建器现在显式传 `collect --cargo-output`。仅此模式允许操作者自己构建的常规文件有多个硬链接；仍以NOFOLLOW/NONBLOCK读取有界字节，核对有限程序集、总量、架构与同源元数据，在新临时目录写出单链接快照后才执行检查。工具链清单及默认collect模式仍拒绝硬链接。最终导出不是硬链接，与原Cargo文件独立；修改原文件不能改变已导出字节。该工具仍只接受操作者自己的构建输入，不是上传执行服务或生产准入。

验证：13项工具测试在正常及`-O`模式全部通过（新增显式硬链接模式/默认仍拒绝、原文件变化不影响输出、链接/FIFO/超限拒绝），shell语法及diff检查通过。用f53deda8刚编译的真实12程序执行新模式成功，schema6、来源ce2535be1e026f32b4375f7b6c2183dae561baee80d01fd3db38e3219eb46345、五角色元数据/ELF/GLIBC和每个输出nlink1/hash核对通过。这是对修正入口的真实验证，不把先前失败的整条流水线改写为成功。修正后新HEAD的正式流程、CI及同源音频另行绑定。

下一轮固定b9425693的正式流程已跑完工作区、ARM编译和12程序清单收集（来源8dbbae2080a2a815d23bedebac32d619465e03669694b4301e96704604b45027），最后复制到仓库导出目录时失败：干净clone没有被Git忽略的`.local`父目录。此前在已有开发目录构建没有触发此条件。构建器入口现在同时准备私有日志/本地输出父目录；仍要求版本输出目录不存在，不覆盖旧材料。该轮仍保留失败，修正版正式执行结果另记；没有把已经通过的子阶段冒称整条流程成功。
