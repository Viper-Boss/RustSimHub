# 可信本地首次设置准备

2026-10-15。源码9b5f1d9c0689ab278e5301e54a207b6ef0831c93，精确归档SHA256 19fba055e597a8d4e3b3ae504d84d5f8631d191ab843b02a5b922427ad194504；本机WSL185项测试、六个ARM64生产程序构建、工作区Clippy及CI38752335688成功。增量编译不代表全量或性能对照。Native仍是ca596a7版本，本轮不声明新源完整五文件应用包。

新`--prepare-device-setup`以Core UID在私有空域生成设置码，重复准备保持码与公钥，不输出秘密或开窗。`simhubctl identity SOCKET EXPECTED_CORE_UID`以Edge UID读取本地pin及窗口布尔状态，错误预期UID拒绝。Auth新增两项、CLI新增两项失败路径测试。

`tools/test-wifi-provisioning.py --platform --empty-history --device-first-owner --local-device-preparation`实机通过，私有`.private/local-device-preparation-9b5f1d9-first.json`passed/cleanup_verified=true。首次本地prepare后同码/同pin保持，真实Core开窗、本地CLI与HTTPS公钥/状态一致，错误UID失败。未认领Core真实SIGKILL后本地prepare拒绝，重启正确码也不能重新开窗。

失败域完整保留；另建明确独立身份作为成功夹具。该域本地prepare→同Core进程认领/登录/完整业务成功，窗口关闭，本地CLI看到相同pin和关闭状态。删码后再次真实SIGKILL，原owner/pin/收件箱去重保持，短信/空历史、有限Wi-Fi试连/保存/重连及崩溃回退、通话准备互斥回归通过，没有提交拨号。

临时服务/profile/规则清理完成，旧四服务active、PCMclosed、checkpoint零、WWAN保持；新增通话/SMS/蜂窝测试零。该轮使用主机用户态，不将它称为新RootDirectory或新系统启动。此前候选用户态证明仍见P6G。本轮是可信安装接口准备，不代替正式网页、证书/设置码交付、生产服务组、SSH真实登录或恢复，不增加已计点数，总体69/100。
