# 首次安装与私有交付验收

2026-10-16。首次安装初始化、持久TLS配置与启动完成门禁已在棒子的隔离候选用户态通过。旧系统继续运行，没有写入boot/rootfs/GPT/NV/校准分区，没有新增拨号、短信或蜂窝测试。

## 冻结输入

- 不可变更新器与Edge安装辅助：fbc2cdba4f8453dfe8484caf35e13a077e41da89，源归档SHA256 `9f331a82de78d90008b6c7c81858f48995bf23151986633fe10e40246fd22266`。
- 签名主应用仍为9b5f1d9、源19fba055…，与P6I相同；Native同源输入不变。此次安装辅助源码变化不冒充新主应用。
- 基础系统仍为锁定220包、树c6992751…，组合源码fbc2cdba…；新组合12338条目，树SHA256 `3e3f51ced9e4b377576ccfb7bfb697e1bd6cabad542f7961cff78048a553bf4c`。本机私有位置见HANDOFF.md。
- 棒子上的通用RootDirectory仍是P6F的724fc09树；新工具和签名应用通过隔离绑定接入。没有将它当成新组合整树或新系统启动。

## 验收结果

`tools/test-boot-service-group.py`的私有报告installation-boot-first.json中passed/cleanup_verified均为true。

1. 没有installation.complete时启动target，Core/Edge均不运行，也不消耗一次恢复预算。
2. 错误发布公钥在任何设备身份生成之前被拒绝，未留下新身份或公钥锚点。
3. 正确签名factory初始化成功，Core和Edge分别以986/985、空cap及候选RootDirectory运行。Root私有交付包含设备公钥、证书/pin、origin、一次设置码与维护公钥；码不输出终端。再次初始化拒绝，身份和TLS不重置。
4. 使用交付的公钥/证书/code完成真实首次owner认领，严格TLS及完整Core查询通过。身份、库或码丢失、链接与完成标记损坏的拒绝由板级隔离测试覆盖。
5. 真正Core SIGKILL触发整组回退；原owner/pin与数据保持。第三版显式可信健康确认后重启保持；再次故障耗尽恢复预算即停止，没有循环恢复。台账与payload位于不同实际文件系统，台账无payload。
6. 自有临时单元、规则、挂载和只读输入绑定已清理；旧四服务active、PCMclosed。既有实验目录及原始证据保留。

此实机夹具只在测试副本中替换NSHSTATE守卫和隔离内挂载检查，因此actual_nshstate_guard_run=false。真实分区守卫的只读接受/拒绝证据仍来自P6F；不能将此次结果冒充真实NSHSTATE初始化、掉电保持或维护SSH登录。

## 构建和系统树

本机WSL以非Root构建，workspace191项测试和六程序ARM64生产release通过；Auth31/Edge11相关Clippy通过。CI [38770338860](https://github.com/Viper-Boss/RustSimHub/actions/runs/38770338860)成功，板级38项通过。新树只读绑定factory供systemd-analyze verify检查退出0；仅Debian原装networkd/timesyncd两条旧`!!`兼容提示。验证前后树哈希一致。

所有候选的flash_allowed/release_ready/recovery_verified/board_boot_verified仍为false，automatic_update_health_confirmation=false，媒体未开放。

## 进度与下一步

P6增加0.5点至6/12，总体70.5/100，约71%。增加来自可信初始化、持久TLS私有交付和启动门禁的实际接线，不按测试数、重复回退验收或组合次数计分。

仍需真实持久分区的每设备准备、维护入口、完整镜像/上电、离线一致备份和恢复/掉电验收。正式网页向导、动态多接口及远程relay、媒体等维持原范围；流量测试等待换卡。
