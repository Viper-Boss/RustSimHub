# 窗口6 · WEB4-INSTALL：正式有限产品安装与中断恢复

2026-11-04交付状态：第一阶段PR209固定 `42dca70ff82b405b787616def708903bb126bad5`，第二阶段PR213固定 `28c38c0bcb5c802c1f223787fa27efc957410ebd` 已回读为open非草稿，未合并。作者已交有限签名包/执行器、Root/SIGKILL/只读/ENOSPC证据；本机尚未重新验收。状态为“模块交付，生产入口待集成”，由 [新版窗口8](WEB4-INTEGRATE-RESUME-20261104.md) 接 `LiveInstallationFence/OwnedRootChanges`、固定CLI和连续原业务守卫；不重复派本模块，不宣称生产安装完成。

先读[本批总任务书](WEB4-DEVELOPMENT-DELEGATION-20261104.md)。分支建议 `codex/web4-product-install-20261104`；报告 `docs/deliveries/web4-product-install-20261104.md`。目标是把PR205材料变成**有签名/可信前置、固定目标、可实际执行和失败恢复的安装代码**，不是再交一份清单生成器。部署和真实密钥不在任务内。

## 来源和所有权

先核对并正常合入PR205最新固定HEAD；读取 `tools/setup_ap_package.py`、`tools/system/setup_ap_install.py`、实际设备侧固定安装器、`crates/update/src/admission*`、Store、`tools/offline_delivery/rootfs.py` 和真实profile材料。不重复造旧AP包或直接把它的FILES加几个名字。

可写新 `tools/product-install/`、`firmware/system/product-install/`、`crates/update/src/product_materials/`及本包测试/文档。既有update/shared入口由窗口3完成后窗口8接；Core、健康协议、数据迁移、AP、NKN/BLE业务、构建器、旧八目标表只读。独立模块可以用专用test target引用同源文件编译；未接入CLI必须如实记清，不复制替身。8最终必须调用本包真实执行器。

## 交给窗口8的接口

- `ProductMaterialPlanV1`（计划/报告，不是授权）：固定profile、来源提交/归档摘要、实际应用/程序摘要、有限角色/目标ID、材料摘要、NSS/Root基线与所需步骤。JSON中不得接受任意目的路径、shell、unit、UID或“已批准/healthy”。
- 新产品维护包必须有**独立明确的kind/version/签名域及有限目标表**，严格绑定profile、来源与全部文件字节。临时密钥只用于隔离测试；可信key来自调用者已持有的安装信任，不来自包内自带key。旧AP/应用格式原样保留，新包不能被旧解析器当成旧许可。
- 拟用私有类型 `VerifiedProductMaterials` 表达本次打开并核验的材料；执行前后重验实际文件/目标/基线，不能拿离线JSON的 `verified:true` 放行。
- 生产调用由8在原维护准入/停机/备份条件下完成。支持prepare、apply、inspect/recover的明确状态；不提供任意目录生产写入。测试root只能在显式隔离模式使用，不能经网络或普通生产参数变成改写宿主任意位置的能力。
- 实际写集及签名合同在独立提交列明，等待主线合并前审核；本窗开发不是已批准设备启用。

## 十二个开发动作

1. 列出PR205实际产物及安装目标表：Core profile/drop-in、NKN Root/SDK、BLE host/单元/总线/tmpfiles等按profile精确选择。表中写来源/权限/所有者/是否已有/能否回退；未支持的目标拒绝，不接受任意路径。
2. 实现新包编码/解析/外部信任验签及完整payload检查，绑定board/arch/source/profile/应用组/角色；严格字段、重复键、大小与文件集合门禁，旧签名域不可重放。不要把未签名 `profile-build.json` 直接当授权。
3. 实现实际NSS预检与有限账户/目录计划：名称固定，可信数字来源明确，不把PR205合成982/981自动当设备批准；核local及NSS别名、补充组、音频/通用IPC关系。只在自有临时root执行账户变化；冲突不自动选下一个UID。
4. 实现固定文件写入执行器：nofollow、单链接普通文件、权限/属主和预算、同文件系统暂存、写入后回读/fsync、目录持久化。禁止解包路径穿越、跟随软链接、写块设备或通用命令执行。
5. 实现维护事务与备份清单：写前持有准入、绑定实际旧字节/原权限，明确停止/同步顺序和失败状态；不另开业务writer，不将运行中随手复制当一致备份。
6. 实现持久安装journal及幂等inspect/recover，标明已完成/待做/Unknown；在准备、备份、逐文件替换、目录持久化、服务材料切换等边界中断后可解释，不靠删除journal重做。
7. 实现有限回退：只恢复本事务可证明的旧目标；保留后来数据、原身份、floor/attempted/恢复预算。不因清理失败假报原样恢复，不自动回刷数据库或整盘。
8. 接上默认关闭语义：安装NKN/BLE能力不产生开启标记、不自动enable；转发OFF不启worker，不能为通过安装检查开启数据网络。实际服务控制固定白名单、有时限和回执。
9. 明确自更新及两阶段安装顺序：当前执行器/launcher/Root合同发生变化时如何避免先替换自己的信任/准入前提。未支持的跨版本组合明确拒绝，不先关闭守卫再继续写。
10. 临时root中真实安装/升级/重复执行/恢复正例；实际验签与真实文件属主/权限，保留原AP与应用包拒绝新kind。模拟systemd适配只证明调用计划，不声称PID1/cgroup已经验收。
11. 反例覆盖错key/源/应用/profile/文件集合、未知目标/角色、NSS冲突、链接/竞态/只读/写满/中断/旧基线变化、关闭/回退失败。用真实执行器，不只给计划函数喂JSON。
12. 至少提交“新有限包/核验＋执行器/恢复”两个可审阶段；报告完整SHA、测试命令、实际改变的临时文件、旧合同兼容和8所需最小接线清单。不得将生产安装记完成。

## 完成标准

真实执行器已在自有临时root完成有界写入/拒绝/中断恢复，实际签名和固定写集有证据；既有信任/Root门禁保持。不能只交模板、计划或永远拒绝所有安装的壳。共享CLI最终由8接通；本包未与8联通前标“模块交付，生产入口待集成”。不刷写、不使用真实密钥、不改宿主账户/服务。
