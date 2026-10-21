# 同源业务候选准备与隔离验收 P6U

2026-10-21，用户要求“再给我完成一个任务，往前推一点”。本轮完成R01中的一个独立子任务：最新同源Native、已有七ARM程序、沿用原签名身份的应用候选包及隔离验收。没有连接棒子、刷写、初始化业务或发起电话/短信。完整系统重新组合与上机属于后续任务，R01整体尚未结束。

## 输入与制品归属

| 输入/输出 | 精确归属与边界 |
|---|---|
| 冻结源码 | `2b3c49cc469bce5e0f59ae377f9a59db9f9d0a89` |
| 源码归档SHA256 | `5d61a14119580fe216e5922961cf537a0b0673ec9df33e610ed731f23dfb0f61`，431个归档文件逐一核对Git blob与模式 |
| 七个Rust ARM程序 | 既有本机交叉编译制品，Rust1.98；本轮重新核全部摘要，并在固定Debian ARM运行时执行来源信息与Core自检 |
| Native | 本轮ARM构建，21项测试通过，ALSA1.2.8；SHA256 `2939f093edaa755555e7829b273c444e4b5180335b13663f207bd281c8d73e10`，5516704字节 |
| Native编译器 | 固定镜像内已有的Rust1.99.0，显式设置 `RUSTUP_TOOLCHAIN=1.99.0-aarch64-unknown-linux-gnu`；与此前已验证Native环境一致，不宣称与七Rust程序使用同一编译器 |
| 签名包 | epoch7/8/9，同一源码与负载，沿用此前4e6已选开发签名身份；不生成新密钥，不把开发签名包标为正式发行 |
| LED组件 | 从七程序制品复制，摘要保持；补齐composer需要的组件级 `binary_sha256` 元数据，没有重编或伪造来源 |

包仍使用现有固定五文件格式：Core、simhubctl、Edge、Native和Opus许可证。更新器、恢复工具、迁移工具与Root灯控是独立系统输入，没有擅自加入既有应用manifest格式。

## 本轮验证结果

| 检查 | 结果 |
|---|---|
| 七个ARM程序来源与Core无硬件自检 | 固定ARM Debian容器、UID1000、空capability、无网络及硬件绑定，通过 |
| Native构建与测试 | 21项通过，release退出0，来源信息、媒体worker能力及二进制摘要匹配 |
| Native合成Opus | 500帧，解码峰值大于0，PCM未开启，通过 |
| Native合成DTLS/SRTP | 仅容器内UDP loopback，双方各收50帧；指纹、错误授权拒绝、撤销关闭及媒体超时关闭通过 |
| 三包实际ARM更新器验签 | epoch7/8/9全部通过，精确来源一致 |
| 被篡改签名 | 拒绝，Signature |
| 同长度负载首字节被修改 | 拒绝，Integrity |
| 错误公钥 | 拒绝，Signature |
| 候选尚未choose即confirm | 拒绝，Denied，持久状态字节保持 |
| 已有候选时再stage | 拒绝，Busy，持久状态字节保持 |
| 未确认候选再次选择 | 首次选择8，下一次回退到已确认7 |
| 回退后的旧epoch重放 | 拒绝，Rollback，持久状态字节保持 |
| confirm错误manifest摘要 | 拒绝，Denied，持久状态字节保持 |
| 正确选择/确认9后再choose | 保持已确认9，candidate清空、floor9 |
| 已确认epoch9重放 | 拒绝，Rollback，持久状态字节保持 |
| 原始输入与清理 | 本机及NAS全部输入摘要保持；一次性运行容器消失；两个已退出Native构建容器在完整日志保存后移除，源码/输出/缓存保留 |

上述为8条明确拒绝路径，加正常验签、选择、回退与确认流程。不是8条Rust单测，也不是系统掉电测试；此前222项workspace测试是该冻结源码已有证据，本轮没有将其当作再次运行。

## 隔离夹具与保留的失败

初次Native任务因完整归档包含仓库工具链文件而试图选择未装的1.98 ARM工具链；无网络、只读镜像使下载准备失败。保留原目录和日志，另起独立任务，显式选择固定镜像内已验证的1.99工具链成功，不改源码、不下载或覆盖工具链。

Root更新验收最初在NAS绑定目录中出现访问拒绝，即使只读观察的UID/模式符合预期；该现象尚未确定唯一原因。后改为把逐项核摘要的输入通过stdin装入容器私有tmpfs，Root夹具不绑定任何宿主目录。临时文件系统需显式允许已核验程序执行，补上exec后完整通过。失败报告first/second/third均保留，最终成功报告为fourth；未修改生产更新器的所有权、签名、版本或路径检查，也没有给容器增加capability。

成功Root夹具：UID0、CapEff/CapPrm/CapAmb/CapBnd均0、只读容器根、无外部网络、无宿主文件绑定，唯一读写对象是容器私有tmpfs。签名私钥始终在PC，本轮没有上传到NAS或容器。临时状态随容器退出消失，脱敏结果与原始私有日志另存。

## 接手索引

- 总索引：`.private/r01-candidate-preparation-20261021-first.json`，passed=true。
- Native成功记录：`.private/r01-native-build-20261021-second.json`，21项；失败first保留。
- Native制品：`.local/native-led-candidate-2b3c49c-second/`；规范化build.json与原collection-record.json同时保存。
- 七ARM运行：`.private/r01-arm-nas-runtime-20261021-first.json`。
- 签名包：`.private/boot-application-led-candidate-2b3c49c-first/package7`、package8、package9；公钥与provenance同目录，无新增私钥。
- LED组件输入：`.local/status-led-candidate-2b3c49c-first/`。
- 成功更新与合成媒体验收：`.private/r01-signed-runtime-20261021-fourth.json`；Native原始合成结果引用第三报告，引用前核对全部输入摘要一致。
- 清理：`.private/r01-native-cleanup-20261021-second.log`；第一次清理脚本语法错误发生在任何移除前，失败日志保留。

## 尚未完成与进度

尚未重新组合完整系统树/镜像、在该系统用户态验证全部动态库/单元、交付新业务、初始化owner或验证冷启动。新灯光业务联动、真实声音上下行、短信和蜂窝均没有新增实机结果；累计10010保持38次。旧3da镜像仍不能直接刷写。

下一项应使用冻结2b的Git树、同源七Rust程序/Native、package7与LED组件元数据重新组成，纳入音频23位守卫、恢复条件及灯控；或者选择新源码后全部重新构建。composer要求灯控revision等于组成源码，不能把后续文档HEAD冒充2b，也不能只改build.json的revision满足检查。

本轮已形成并实际验证同源交付输入，P6从6.5/12计至7/12，增加0.5点离线候选准备进展；其他里程碑不变，整体 **74.5/100**。完整系统组合、部署和实机验收仍保留相应剩余点数，不因成功打包视为已交付。完成此单项后保存，未安排设备操作。
