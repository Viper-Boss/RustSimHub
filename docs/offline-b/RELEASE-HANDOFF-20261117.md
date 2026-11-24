# 离线候选、升级与恢复交接

范围固定在产品源码 `de663743bd726e77f7c560139d0f91ea78e8fe57` 及本次 B
工具提交。当前以 USB 电脑工具为主要配网方向；热点配网待后续开发和验收。
本文件不是安装许可。当前没有生成可安装的完整新候选，也未获得健康确认、
设备 epoch、签名或真实网络授权。整体验收仍为 81.5%。

## 用户支持范围

- USB/共享 Wi-Fi 的状态、授权、试连、确认、取消应沿用原事务和原操作 ID。
  本次网页模型回归不证明 USB 工具与真实设备端到端可用。
- 安装计数表示近期活跃安装的估计，不是自然人数；查询未知/旧缓存不能显示为0。
  保存统计参与许可也不表示已签名、已提交或已上链。
- 当前源码含独立身份、纯交易签名、受限 RPC 和 Root/SDK 候选；完整运行态
  CheckInTransport 的 PR268 尚为草稿，本次没有消费它。不要宣称默认上报可用。
- 默认关闭、Owner/realm/代次、原始 Pending 摘要及最长300秒 Root 权限保持。
  不通过重建身份、延长授权或伪造健康结果来启用候选。
- 短信、转发、通话的 Unknown 不表示未发生。恢复后核对原 ID，不自动再次发送、
  递送、拨号或重新准备/提交链上交易。真实接收端、声音、NAS、运营商等尚待实测。

## 真实发行 profile 矩阵

来源：`tools/product-release/release.py` 和
`tools/product-integration/matrix/profiles.json`。下面的“支持”仅指已有离线代码路径，
不是本轮已构建、签名、安装或真机通过。

| profile | 业务schema | 邮件/队列契约 | 自动健康 |
|---|---|---|---|
| legacy-v1 | 6 | worker4/outbox3 | 不支持 |
| product-v1 | 6 | worker4/outbox3 | 不支持 |
| product-email-v1 | 6 | worker5/outbox4 | 不支持 |
| product-recovery-v1 | 9 | worker4/outbox3 | 不支持 |
| product-email-recovery-v1 | 9 | worker5/outbox4，先邮件、后恢复迁移 | 不支持 |
| product-recovery-health-v1 | 9 | worker4/outbox3 | 仅原限时、同一Core上下文接口 |

健康 profile 仍要求原 forwarding Root 模式、同一 Core 字节、5000ms 最大样本年龄
和250ms进程预算。AP/SMTP5/email-recovery、不同 Core v1 基线不得冒称支持原自动健康。
schema9 后原 v1 Root 回退接口不能保留完整 fallback launch-profile capability，仍拒绝。
不能以回滚二进制推定数据库可回滚。

统计候选功能也不能从源码存在推导为以上 profile 已启用：现有发行器选择
`product-providers-candidate` 等原特性，并未自动加入
`usage-transaction-candidate` / `usage-root-transport-candidate`。Go Root 的对应
候选构建标签和运行态接线同样需要单独审核，不能新增“统计已上线”profile来绕过。
BLE/AirSIM/VoBLE/native音频及手机配置器不在本次实现范围；不修改它们的材料或验收。

## 固定来源和未签名构建

使用完整 Git 祖先和干净 checkout。32项来源锁保留原18项，并追加14项经逐文件和
实际 Git 祖先核对的来源；report 类型不提供产品代码。索引只证明列出的源码组合，
不证明测试、真实程序、SDK链接闭包、签名、Root授权或安装。

```sh
python3 tools/product-integration/verify.py --repository . --integration FULL_40_HEX --require-complete
python3 tools/product-release/release.py --help
```

第一条退出3表示所固定组合不完整、工作区脏或 checkout 不匹配，不能改标签充数。
退出2为输入/Git拒绝。完整来源检查退出0仍没有安装/健康授权。
新输入或代码追加后，原 HEAD 的证据不自动延续。

已有构建器的真实参数如下；参数值必须来自自己的固定提交和已核输入，不能用
报告JSON、上传的二进制或旧应用号冒充新构建。不要传不存在的万能“跳过验证”参数。

```sh
python3 tools/product-release/release.py \
  --profile PROFILE --archive FIXED_GIT_ARCHIVE --repository REPOSITORY \
  --output FRESH_OUTPUT --roles APPROVED_NSS_SNAPSHOT \
  --sysroot MATCHING_SYSROOT --audio-sysroot MATCHING_AUDIO_SYSROOT \
  --revision FULL_40_HEX --source ACTUAL_ARCHIVE_SHA256 \
  --verified-sdk-stage VERIFIED_FIXED_SDK_STAGE
```

非 product 模式不需要 SDK stage；需要的模式缺它就拒绝。只调用原发行器，保留原
`build.json`、`profile-build.json` 和应用包格式。真实Core/Edge/worker/SDK/Root/音频
程序的架构、来源、模式、特性、GLIBC/ABI与许可证必须相符；空程序或 `--version`
假输出不能代替。源码/锁/静态资源/WASM/SDK patch变化必须重新构建。

当前未提供：合格Linux环境、匹配的ARM/音频sysroot、QEMU/隔离客体、独立批准的
NSS角色快照、逐文件核过的SDK stage及实际程序。因此没有运行本轮完整profile构建，
没有以旧制品改标签，也没有生成假 `COMPLETED`。后续只应先产生
`signed=false`、`epoch_assigned=false`、`installable=false`、`hardware_tested=false`、
`health_authorized=false` 的真实未签名候选。

## 负责人安装与恢复前置

以下是待负责人执行/签认的前置，当前任务没有执行这些设备动作：

1. 重新核实现场应用/boot/信任/空间/角色/程序和当前Owner上下文；不能沿用旧日志。
   固定Root目标表、UID/组、只读/可写目录、完整程序组不得扩大或借用其他功能权限。
2. 独立验证候选来源、实际程序元数据、完整组、应用/Root格式和版本契约。只按
   已有包工具验证外部信任，不以候选自带的信任或 `ready` 作为批准。
3. 用合成数据在隔离Linux运行旧/新/回退组合：保留提交后新写、短信回收与疑似重复、
   outbox Unknown、统计Pending和独立参与授权；不能恢复旧快照抹掉新写。
4. 运行真实故障矩阵：SIGKILL、只读/写满、截断、部分发布/同步失败、重启、
   错身份/realm/schema/epoch、坏备份、链接替换、锁冲突、晚回复、子进程未退出。
   原测试忽略或清理未知必须保留，不算通过。
5. 准备新的可读备份与恢复入口，保留原始摘要/操作ID。实际签名、设备epoch、
   Root准入、唯一启动、原进程/上下文健康、确认和回退由负责人另行授权并执行。
6. 二号同型号设备首次安装/恢复、真实断电、长期组合、真实通信与声音、物理灯效
   仍是现场验收。QEMU/合成PID1/交叉构建/短时CPU样本均不能代替。

不会读取主线 `.private`/`.local`、Owner浏览器配置或生产材料来补输入。隔离测试
只能在自己的新建目录/镜像/命名空间中进行；旧工具若固定使用 `.local`，应在专用
干净测试checkout内建立自己的输出，不能指向主线已有维护目录。

## 离线验证、依赖和缺陷追踪

PR270 提供 Windows/Linux 有界入口和29项初始差距表。B1把来源测试接到
主线固定stage `offline-b-source-inputs` 和单独CI；Linux有条件时可显式选择该stage，
不运行主线默认组里的热点/蓝牙/音频专项：

```sh
python3 tools/mainline-checks/run.py --group ordinary --check offline-b-source-inputs \
  --output .local/OWN_FRESH_RUN/report.json
```

当前 `rust.yml` 保持不变。其新增统计UI/Owner测试被精确映射；AirSIM原步骤明确
记录 `excluded_scope/not_run`，不通过别的检查替代。覆盖清单不提供发布许可。

依赖工具生成了**不完整的锁文件声明清单**：Cargo根锁344条、native锁192条，
两者不可相加当作实际链接闭包。离线metadata不可用、未取得SDK stage/Go真实闭包、
ALSA/Opus系统材料和漏洞通告未核；`approved=false`。未升级依赖、换许可证或将
未知许可/风险填通过。必须再对实际构建程序和固定SDK notices核验。

云端CI计费零步骤、Linux/ARM/QEMU/真实浏览器和设备/公网未运行项保留到各自
精确HEAD的报告。单个小PR离线交付不改变总台账，也不表示本任务全部验收完成。
