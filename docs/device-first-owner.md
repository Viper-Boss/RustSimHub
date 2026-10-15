# 设备运行与一次性首次设置窗口

可信本地准备入口（2026-10-15新增）：

```
rustsimhub-core --prepare-device-setup STATE IDENTITY PRIVATE_SETUP_DIRECTORY
simhubctl identity SOCKET EXPECTED_CORE_UID
```

第一个命令由本地安装/维护层以Core账户执行，使用真实业务库与稳定身份，只在principals为空且setup.used完全不存在时准备32字节随机setup.code。现有合法码保持不变；短文件、链接、损坏状态或已使用窗口拒绝，不自动修复。输出仅准备结果、公钥与window_opened=false，不输出设置码，也不启动Core或网络窗口。后续--serve-device仍是唯一正常开窗路径。设置码的可信本地交付、证书与SSH维护接线仍需继续实现。

第二个命令只读本地Unix套接字，要求文件owner及连接peer UID都符合显式指定的Core UID，严格解析32字节公钥/协议2/ed25519/布尔窗口状态，输出公钥hex与当前窗口状态。用于可信USB/SSH维护会话交付pin；它不能把远程网络返回的公钥变成可信身份，不创建会话、授权或所有者。新增Auth两项与CLI两项失败路径测试通过，Auth共30项、CLI共3项及工作区clippy通过；实机验收使用test-wifi-provisioning.py新增--local-device-preparation，当前检查点尚待冻结构建。

新增显式Linux运行模式：

```
rustsimhub-core --serve-device SOCKET EDGE_UID STATE IDENTITY MEDIA_SOCKET MEDIA_UID ARCHIVE ACCESS COMPAT NETWORK_JOURNAL PRIVATE_SETUP_DIRECTORY
```

这是完整Core功能与本地首个所有者窗口的组合，不是开机安装器。它沿用同一Auth、业务库和套接字；设置成功后立即通过原所有者签名登录访问短信/电话控制/历史/NAS/Wi-Fi接口，无需停止Core或切换运行模式。媒体授权接口存在仍不代表媒体产品交付，`media_available`保持false。其他模式不会隐式开启设置窗口。

安装/维护入口必须先创建Core UID私有0700设置目录。只有在目录内显式供给Core所有、0600/0400且不可链接的32字节随机`setup.code`，并且业务库没有任何principal，才可能打开窗口。目录/文件创建及设置码、设备pin、证书的可信本地交付仍由安装/维护层负责，不能把共享设置码打入镜像，不能用未核对的网络identity代替设备pin。

打开窗口前，以CREATE_NEW创建并fsync `setup.used`及父目录。标记含固定版本域和设备公钥，不含设置码。规则：

- 新空库且无code：不开窗口，也不凭空生成code。
- 有任何principal（包括已撤销项）：先持久标记已经观察到所有权，不读取code，不开窗口。
- 已有合法标记：不开窗口；即使code已删除或业务DB丢失也不会重新认领。
- 标记损坏、属于其他设备、权限不当、软/硬链接，或无法持久写入：拒绝启动，保留现场，不能自动删除或回退到旧初始化模式。
- 一个窗口仍为5分钟、最多16次码校验、最后一个挑战、真实客户端签名、空principals事务；成功后内存秘密清零。退出或崩溃会结束这次窗口，重启不能刷新时限/预算。开窗后未完成设置也需要可信本地维护处理，不能通过网络重开。

标记由Core私有目录保护，防止网络请求或业务库丢失导致隐式重开；不是抵抗root/Core任意文件修改的外部防篡改硬件。身份/目录丢失或人工维护必须使用原有受控恢复路径，不能引入远程所有权重置。旧`--serve-local-setup`保留为显式维护模式，不能作为自动恢复/启动fallback。

本机workspace新增3项相关测试，当前177项及Clippy通过：缺码无隐式窗口、一次开窗后不重开、签名初始化、旧身份+新业务库仍关闭、既有所有权无code封闭、损坏/外设备/链接标记拒绝。六程序ARM生产构建与实机验收见VALIDATION-DEVICE-FIRST-OWNER-P6E.md；尚未因此声称正式首启网页、证书交付、开机服务或完整镜像完成。

实机验收入口：`tools/test-wifi-provisioning.py --platform --empty-history --device-first-owner ...`。先在未初始化域真实SIGKILL并验证原正确码也拒绝重开，完整保留该域；成功路径使用明确分开的新测试身份。随后验证同一Core PID初始化/登录/业务可用、删码后的真实SIGKILL重启、原owner与数据保持，继续有限Wi-Fi试连接/恢复。测试域替换是SSH操作的夹具管理，不是产品所有权重置API，不修改旧生产身份。
