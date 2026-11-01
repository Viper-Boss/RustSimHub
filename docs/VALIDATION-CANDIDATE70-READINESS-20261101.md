# 应用70与热点维护材料准备

## 固定来源与构建

冻结主线`825a5016e3aa0524190af12139bf908bdd6378e8`，包含#160的SMTP内部解析修复及#161的独立SDK裁剪评估。源码归档SHA-256为`1f0c8a98111d1b8ca22b4cd4db6c369fec7b269d6acdcb0e59cebce46bcbc324`。

在本机普通用户WSL、私有回环网络执行正式构建器，1254个归档文件逐个核对，完整工作区**1353通过/0失败/6项原有专用隔离测试忽略**，12个ARM主程序编译、实际元数据/架构/GLIBC检查及独立输出hash通过。同源原生音频另构建并实际执行ARM QEMU **34通过/0失败/0忽略**，共13程序。

本次复用上一轮已核验的Cargo/native缓存，不声称空缓存重现；音频输出目录仍独占新建，最终产物来源、hash及sysroot导出文件重新核对。程序没有在棒子上编译。旧5d655331的[CI 41294901148](https://github.com/Viper-Boss/RustSimHub/actions/runs/41294901148)已完成且两个job成功；新825a5016的[CI 41296075720](https://github.com/Viper-Boss/RustSimHub/actions/runs/41296075720)按其实际运行状态另记，不能用旧绿勾替代。

## 真实只读检查

通过既有固定主机指纹的NAS/USB维护通道，确认设备仍为已确认应用69、没有待选候选，七个服务active、当前通话空闲、Wi-Fi正常、普通蜂窝数据未连接、转发MainPID为0。公开签名信任与本机既有开发签名材料匹配。没有注册新管理员、拨号、短信、推送、关停或设备文件写入。

随后再次核对同一boot和原更新台账，只读观察维护包8个固定目标的当前字节及权限。结果：**只有两个二进制需要更新**，其余六个静态材料完全相同：

- `/usr/libexec/rustsimhub/rustsimhub-setup-ap`
- `/usr/libexec/rustsimhub/rustsimhub-core-launcher`

Core/Root联合契约相同不表示旧助手已经包含新扫描实现；候选准备仍提供同源8目标维护包，不能只装应用组便宣称主动扫描已交付。PC侧下载的现有文件只是运行时观察副本，**不是停机一致性备份或恢复验证**，不能据此跳过真正维护窗口。

第一次目标差异脚本把WSL路径直接当Windows路径，读取本地manifest即失败，尚未连接设备；修正路径后以独立第二次记录完成，失败与空目录保留。

## 已生成与验证

- 应用70：固定格式2六文件、schema6，生产运行feature，使用与设备信任匹配的既有**开发签名**。不是正式生产发行或新建信任；私钥未复制到包中、未传设备。
- 热点维护包：使用既有`tools/setup_ap_package.py`，固定8目标、独立签名域、同源Core/Edge/AP/launcher真实元数据，不调用安装器。没有接管C的通用`ReleaseMaterialPlanV1`或改变成员任务。
- OpenSSL校验两种签名域通过；真实ARM更新器在本机Root私有网络、POSIX临时目录执行`verify-package`通过，epoch70与清单digest一致。Root仅满足既有验签CLI的UID前提，没有执行stage/choose/confirm。
- 6项反例通过：ARM更新器拒绝错误公钥、错误签名域、损坏worker；Python拒绝旧schema；维护包验证拒绝错误签名域和损坏Root助手。没有把权限拒绝当成这些反例成功。

私有索引保留在主工作目录`.private/`，不提交签名材料、真实日志、设备信任标识或现有文件副本：

```text
official-builder-20261101-825a5016.json
main-build-summary-20261101-825a5016.json
main-native-20261101-825a5016.json
mainline-candidate-device-preflight-20261101-first.json
mainline-candidate70-825a5016-20261101-first.json
mainline-candidate70-verification-20261101-first.json
mainline-candidate70-root-diff-20261101-second.json
```

## 后续主线安装顺序

1. 再核对签名/同源载荷、真实boot/已确认69/候选为空、通话空闲、数据关闭及空间；本次只读结果不充当未来维护准入。
2. 用既有有限维护模式注册有界恢复动作；暂停相关业务、制作当前一致性备份并回读核对；保留已有开发信任、身份、短信/Unknown/流量数据和实际Root文件。
3. 仅在固定维护目标内更新所需Root二进制，其他六文件保持经验证字节；使用实际更新器及其数据锁/Root/应用回选门禁处理候选70，不直接改台账或选定链接。
4. 核对实际选定来源、七个服务、Root助手、原Owner和旧业务记录保持、无普通数据或转发进程意外启动；健康确认必须绑定候选source/epoch及真实结果，不能凭签名成功确认。
5. 验证失败按既有候选回选和Root备份恢复；不通过回滚业务数据库丢弃新短信或Unknown。手机/首次配网视觉、听音与拔插等人工验收继续放最后。

本轮**未上传、未安装、未stage/choose/confirm，设备仍运行69**。PMIC/关机继续搁置；代码合并和离线候选不增加实机验收点，原P0–P7 **81.5%**、完整行动台账 **20/169**保持。
