# P6AB：实际ARM签名启动组、回退与账本恢复

2026-10-23。沿用P6AA固定代码9528712和同源开发签名组，在NAS已有固定ARM64 Debian13镜像中执行生产updater。**第三轮通过**；前两轮失败保留，不将其改为成功。没有访问棒子、打开PCM、连接modem、发短信、拨号、启用蜂窝或修改正式升级账本。整体77%，不重复计算既有升级能力的进展点。

## 实际输入

- 源码提交：`9528712c784d80c9326b99f46b1f72971b5714d2`。
- 源码归档SHA256：`9bbc71d42fb5b10d0a2346579dccc56f6d732828689c2879fed48b1b796a5d39`。
- 八个Rust ARM64程序中的生产updater，以及epoch1/2/3同源完整开发签名组；上传后逐文件复核摘要。
- 基础运行镜像：`debian@sha256:a29215f6a35e51e22adffa17f89e9d2ef06214e64a2bad10d765c46aea49f11f`，实际Architecture为arm64，没有使用x86模拟结果代替。

私有索引`.private/schema-arm-runtime-20261023-third.json`为本轮成功依据，first/second为失败记录。签名私钥从未上传；只上传开发公钥、签名包、updater和试验脚本。

## 通过项目

1. 真实updater输出schema6及匹配的源码身份，三个包均通过签名与payload校验。
2. 在空的试验账本安装factory epoch1并选择它。
3. 暂存epoch2后再次安装同一factory不修改账本；已有candidate时拒绝另一暂存，账本逐字节保持。
4. 通过实际`boot-select ROOT PUBLIC FACTORY RUNTIME VERSIONS`接口选择候选并发布selected链接；下一次调用回选factory，链接一致。
5. 失败回退后floor仍2，拒绝重放同epoch；回退不降低防回退下限。
6. 选择epoch3并显式确认，后续选择保持该组。
7. 删除试验state.json后status拒绝，不创建空账本覆盖。用已保存的完整state.next恢复后，账本逐字节一致，floor仍3、candidate为空，仍选择已确认组。
8. 在试验副本中追加程序字节，真实updater拒绝损坏包；原签名包保持不变。
9. 测试容器正常退出并移除。试验账本位于隔离tmpfs，不绑定现场NSHSTATE或应用versions。

## 失败记录及修正边界

first：无capabilities的容器root无法读取NAS ACL目录中的试验脚本，在updater启动前被拒绝。未修改NAS ACL、未给运行容器添加DAC等能力。

second：将经校验文件通过COPY放入专用临时镜像，解决目录ACL带来的读入问题；真实ARM签名验证和stage已执行通过，随后脚本调用不存在的`select-runtime` CLI而返回Invalid。这是测试脚本错误，不是回退实现的证明，也没有修改产品以接受该命令。

third：改用现有`boot-select`接口，在全新临时目录和空账本执行完整流程，通过。三轮报告独立，未覆盖失败证据。

运行容器均无网络、无host设备挂载、capabilities为空、no-new-privileges、只读根文件系统；业务写入仅在192MiB试验tmpfs。CPU1、内存384MiB、PID32。容器root仅用于生产updater要求的账本操作，不是NAS root目录或棒子业务初始化。

## 仍未证明

本次运行的是NAS ARM环境，不是棒子完整系统启动；三个epoch使用同一固定代码，证明组选择与账本行为，不能证明跨不同代码版本的数据库兼容。

尚未执行v5→v6的实际业务迁移、候选运行期间新增短信保留、完整Core/Edge/Media启动组及掉电矩阵、现场发布身份的交付与正式业务初始化。暂不部署；下一步继续这些任务，原卡数据保持关闭。
