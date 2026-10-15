# 组合设备模式的首次设置与崩溃边界

2026-10-15，源码 `ca596a79acd43f968caa749172bd1e6f72fdac07`，源归档 SHA256 `8bdf8fcb6b9ebce139a8c0c4fd8de742fd3a17b8690d729f3506c0bbdd999f52`。本机177项workspace测试和Clippy通过；六个ARM64生产程序由本机WSL构建，production fixtures=false、ELF/GLIBC边界/逐制品摘要验证通过。增量release37.34秒，不作为全量构建或性能对照。GitHub CI38734877173 success。

运行模式与边界见 `device-first-owner.md`。新的 `--serve-device` 同时提供一次性本地首个所有者窗口与现有完整Core功能，成功后不需要人工换运行模式。旧显式维护模式保持；没有自动退回旧初始化模式。

## 真实棒子验收

使用 `tools/test-wifi-provisioning.py --platform --empty-history --device-first-owner`，独立临时Core/Edge、私有新数据目录和空历史库，NAS只作为USB严格TLS验收客户端，不提供旧数据库或NAS principal。

1. 未认领域首次显示窗口可用；真实SIGKILL Core后重启，同一设备pin保持，窗口关闭，原正确设置码也被拒绝。原未认领域完整保留。
2. 成功路径使用明确分开的另一新测试域/身份，不是通过产品API重置前一域。无认证请求和错误设置码拒绝；正确客户端签名完成初始化。初始化前后Core MainPID相同，同一进程内正常owner登录，短信/电话控制/历史/NAS/Wi-Fi功能开启；窗口关闭且finish重放拒绝。
3. `setup.used`实际为Core所有、0600、单链接。删除测试code文件后执行后续真实Core SIGKILL，owner登录、device pin与真实收件箱内容保持，窗口仍关闭，其他客户端不能再次初始化。
4. 有限Wi-Fi试连接/取消、错误PSK候选存在时Core真实SIGKILL自动回退、一次性账本跨重启拒绝重放、owner确认保存和Core退出后NM自动重连通过。配网期间短信/空历史/通话状态可读，拨号准备拒绝，恢复后有效line仅prepare成功，没有submit。
5. Core986/Edge985实际UID及CapEff/CapPrm/CapAmb/CapBnd全零。临时单元停止、自有profile删除、原Wi-Fi恢复、临时权限规则按摘要移除，旧四服务active、PCMclosed、NM checkpoint零、WWAN状态保持。

私有报告 `.private/device-first-owner-ca596a7-first.json` 的 `passed`、`cleanup_verified`、`unowned_window_closed_after_real_sigkill_with_correct_code_denied`、`owner_initialization_and_full_platform_share_same_process`、`device_window_stays_closed_after_real_kill_without_setup_code` 等为true。原始标识、业务内容、路径、code、密钥不得提交。

新进程就绪以实际Edge UID完成Identity IPC读取判断，不能仅凭旧socket文件存在；新未认领SIGKILL测试另有限等待服务停止，避免未知作业被重复启动。

## 未完成的范围

没有安装生产开机服务、集成板级rootfs overlay或刷写完整镜像，没有物理断电/独立仅供电、首启网页/证书可信交付、真实浏览器声音或蜂窝验收。没有新增拨号、短信发送、蜂窝流量，10010累计仍15。业务库丢失和标记损坏/外设备/链接拒绝由本机针对性测试验证，不冒称这次实机已破坏/恢复真实业务库。

P6增加0.5点至4/12，补齐组合运行中的首次设置转业务及持久一次性窗口；此前已计的短信/网络集成不重复计点。总体68.5/100，约69%；完整启动和恢复门禁保持。
