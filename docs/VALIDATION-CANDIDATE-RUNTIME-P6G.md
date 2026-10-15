# 候选用户态实机运行

2026-10-15。板级组合源724fc0922794ad08214b574b9c63bb438d808f1f，基础源452d75e9f647b01ca2badb80b8e980ee99cec96b，应用源ca596a79acd43f968caa749172bd1e6f72fdac07。三个来源分别记录，不将工具提交冒充应用版本。

通用树12303条目，树摘要fbcbade0f7137cec59c0eeead9ec092e2e526e70e62550e0b569c5cdc228dd9b；压缩归档132531369字节、SHA256 a73e1c0f1bb2ebb9aed764b25fe39313a42b60d36a64d732f53d13e827860cf1。`tools/stage-system-tree.py`检查归档路径、链接、重复项和文件类型，核验空间及上传摘要，在新私有目录保留数字所有者及xattr展开，完整tar比较通过。归档边界新增3项合成测试，板级测试共26项。

`tools/test-wifi-provisioning.py --platform --empty-history --device-first-owner --root-directory …`实机通过，私有报告`.private/system-tree-rootdirectory-first.json`的passed/cleanup_verified为true。Core/Edge实际进程根目录inode、执行文件SHA和所映射libc inode与候选树一致。UID986/985，能力集合为零；只读候选树、独立可写数据，Edge屏蔽D-Bus。

同进程首次设置转业务、未认领进程真实SIGKILL后正确码拒绝、已认领进程SIGKILL后owner/pin/短信去重保持通过。实际收件箱及空历史读取、Wi-Fi取消/错误PSK崩溃回退/确认保存/自动重连，以及配网期间拒绝通话准备、恢复后仅准备不提交通过。临时服务/权限/profile清理通过，旧四服务active、PCMclosed、checkpoint零，WWAN保持。运行后全树再次tar比较通过，未改写候选树。

原生音频使用同一应用源归档8bdf8fcb6b9ebce139a8c0c4fd8de742fd3a17b8690d729f3506c0bbdd999f52在NAS离线ARM容器构建。11项测试通过，release耗时2分55秒，仅为本轮已有缓存的结果。编译器Rust1.99.0、ALSA1.2.8，与主程序本机交叉编译的Rust1.98.0分别记录。ELF5320088字节，SHA256 57947d95d26cc075ceb23f65994a4e53ff99eaac497af32a9d792077797d69cc，实际build-info源摘要匹配，GLIBC上限2.34。

此音频ELF在同一候选用户态、Media UID984/空能力/PrivateDevices/PrivateNetwork下运行：合成Opus500帧、双向ICE/DTLS/SRTP各50帧，指纹验证/错误授权拒绝/撤销关闭/超时关闭通过；没有打开PCM。私有报告`.private/native-candidate-first.json`通过且清理核验通过。这是候选用户态兼容性证明，未安装持久媒体工作进程。

本轮新增电话、短信发送、蜂窝测试均零，10010累计仍15次。硬件服务仍来自主机旧MM/NM/rmtfs；未验证板级首启、独立上电、正式GUI/证书、完整媒体、EDL或回刷。候选manifest的flash_allowed、release_ready、recovery_verified、app_service_group_installed、native_media_installed、board_boot_verified仍false。P6增加0.5至4.5/12，总体69/100，不再对原生重建和重复测试计点。
