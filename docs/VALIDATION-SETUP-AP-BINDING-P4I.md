# P4I Core真实绑定与首次认领材料交付

精确源码d63672e的[GitHub CI40062341541](https://github.com/Viper-Boss/RustSimHub/actions/runs/40062341541)两个任务全部成功，真实Core材料/代码CLI步骤成功；完整结果为setup-ap-binding-ci-20261024-first.json。当前仍未部署。

2026-10-24。已将AP材料准备接到实际Core生成的realm、公钥和运行UID，增加初始认领代码的私有导出及与窗口启用的目录锁。311项工作区Rust测试、严格Clippy、生产检查和三程序/库ARM64构建通过；三个真实Core/Edge/Root命令联合测试通过。本轮没有访问棒子、安装应用、开热点、切网、拨号、发短信、开普通数据或改变证书信任。上次确认业务185ec6e/confirmed43，整体79.5%。

这里的真实Core联调运行在一次性Linux临时目录中，身份和数据库由实际Core二进制生成；不是读取或改变棒子的现有身份。热点自动首次窗口、AP到STA交接和正式安装仍未完成。

## 已实现接口

```text
rustsimhub-core --prepare-device-ap STATE_DIR IDENTITY_DIR SETUP_DIR
rustsimhub-setup-ap --prepare-from-core ROOT_PARENT CORE_SETUP_DIR EDGE_INSTALLATION EDGE_UID CORE_UID
rustsimhub-setup-ap --export-first-setup MATERIAL_INSTALLATION CORE_SETUP_DIR CORE_UID NEW_PRIVATE_EXPORT_DIR
```

Core命令由实际Core用户离线执行。新建空状态时，沿用原有身份/数据库初始化；已有数据时沿用身份绑定规则。输出binding.json中的realm来自实际数据库epoch，公钥来自实际Identity，UID来自当前进程。与原setup.code一起保存在Core的0700目录内、文件0600；不接受外部传入realm、公钥或UID，不打印秘密，也不调用for_device或写setup.used。

Root身份误用该Core命令时，在打开数据库/身份之前拒绝，不产生Root所有的初始化片段。既有绑定只能精确重验；错误/损坏绑定不覆盖，也不为掩盖错误生成新代码。已有所有者或setup.used时拒绝重新准备，原有--prepare-device-setup行为保留。

Root导入验证Core目录/文件真实UID与GID、绑定中的core_uid、AP专用Edge配置及材料记录。与之前手工提供Root绑定目录的方式分开，不能因为Root能读取文件就信任任意用户生成的公钥。

首次导出在P4H的私有connection.json中额外写入setup_code，保留SSID、WPA口令、实际设备公钥、地址及证书摘要。仅允许与材料完全相同的realm/公钥/Core UID，且setup.used必须不存在、setup.code必须为32字节。普通可信导出仍不包含该代码；两种导出均只写新建Root私有目录，标准输出不含代码/口令。

## 与已有首次窗口的边界

Core代码准备、设备for_device窗口消费以及Root初始材料导出共用同一个目录inode的排他flock，防止在窗口写入setup.used时并行导出旧代码。锁被占用时有限拒绝，不等待、不删除标记、不续期。旧窗口的300秒、16次预算和身份绑定的setup.used语义不变。

这一步没有新增匿名开热点接口，也没有让网络请求拿到代码。Root/网页尚不能把这些材料自动转为未认领设备的AP租约；现有setup_ap_start仍只接受已有Owner。下一步需把真实LocalSetup一次性窗口、通话空闲观测及Core网络锁连接到有限Root租约，认领后保留有界配置交接，不能简单伪造Owner或延长窗口。

## 已验证

- 完整工作区311项Rust及严格Clippy、生产Core检查通过；随后增加既有Owner拒绝和Root误用的入口验证，补充Auth定向测试/严格Clippy、Core严格Clippy及重建通过，没有把重复运行累计为新增测试。
- Auth单元证明绑定来自实际Auth、重复准备保持代码、窗口消费后拒绝、错误绑定不创建秘密、目录锁竞争不消费窗口；既有Owner/数据库丢失/预算/签名首次认领测试仍通过。
- 三项真实CLI联合测试覆盖原P4H准备/导出，以及以数值UID/GID 986运行实际Core、实际生成realm和设备公钥、Root导入和初始代码导出。核对代码不在输出、正确私有副本、重复执行保持代码；错误realm/UID、目录锁、已用标记拒绝；Root误调用时三个状态目录保持空。
- Core、Edge、Root热点程序及库ARM64构建和ELF机器类型183核对通过；这些是未签名开发产物，没有冻结源码包嵌入证明，不能用于正式部署。
- CI真实材料步骤加入Core构建和真实身份导出路径。精确提交的远端结果另存，未用前次P4H成功代替。

私有证据：setup-ap-binding-20261024-first.log/.exit、setup-ap-binding-extra-20261024-first.log/.exit、setup-ap-binding-final-20261024-first.log/.exit、setup-ap-binding-cli-20261024-second.json、setup-ap-binding-arm-20261024-second.json及setup-ap-binding-checkpoint-20261024-first.json。

## 下一步

1. 未认领设备的有限AP租约生产者：真实LocalSetup、空闲观测、认领后的配置交接；不伪造Owner，不重新调用已消费的窗口。
2. AP到STA的试连接/确认、错密码/超时恢复AP及隔离规则交接，然后接首次配置网页。
3. 固定来源联合安装、受限服务模板验证和备用维护入口；用户方便时再测热点/手机首次配置、保存后冷启动。
4. Root Unknown明确核对、故障/掉电和后续NAS/远程缺口继续保留在全目标中。

当前无需用户操作；普通数据关闭、关机调查搁置、累计10010仍44。源码在crates/auth/src/setup.rs、crates/core/src/main.rs、crates/setup-ap/src/executor/provision.rs及channel/files.rs。
