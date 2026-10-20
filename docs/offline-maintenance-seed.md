# 离线维护身份资料

`tools/system/prepare-maintenance-seed.py`仅在NAS/构建机的全新Root0700私有目录准备每设备材料，不写实际NSHSTATE，不启动SSH，也不生成业务身份或设置码。输入是已经选定的单行Ed25519维护**公钥**及其准确文件SHA256；客户端私钥继续只留在电脑。

```sh
python3 tools/system/prepare-maintenance-seed.py \
  --maintenance-public-key /absolute/private/operator/maintenance.pub \
  --expected-public-sha256 PRIVATE_REVIEWED_PUBLIC_FILE_SHA256 \
  --output /absolute/private/new-seed
```

准备的`rustsimhub-v1/system`包含Root0600维护公钥、独立Ed25519 SSH主机身份及稳定`usb.serial`，各目录Root0700。主机公私钥由真实ssh-keygen派生核对，USB两端MAC与固件算法一致。私有manifest记录准确公钥摘要、主机公钥指纹、USB摘要/MAC及全部未安装/未验收标记；完整校验和同步后才写complete.marker。资料包含主机私钥，必须整体保密，不进入Git或通用rootfs。

错误摘要、SSH选项、多行/坏公钥、软/硬链接、错owner/权限、既有或部分输出一律拒绝。任何中断保留现场，不自动重新生成主机身份/USB serial；不能通过换目录来掩盖一次已有设备身份，后续须使用原索引选定材料。`validate_seed`重新核对完整目录形状、所有权、主机公私钥、串号和manifest，拒绝额外或修改的内容。

部署前仍需在受控离线状态副本上安装、核对旧内容保持，并验收固定主机公钥下的真实维护登录。不得直接把资料路径式复制进旧业务用户可改名的状态父目录；不能改变旧挂载根owner破坏业务。此工具不代替首启、可信业务初装、恢复演练或任何刷写门禁。

`tools/test-maintenance-seed-login.py`用于进一步验收选定身份。它从严格固定主机密钥的NAS连接读取已校验的材料，将服务器身份暂存到棒子Root私有夹具；实际客户端私钥始终由本机Paramiko使用，通过旧SSH通道访问候选ARM sshd的127.0.0.1随机高端口。独立90秒systemd时限及结束停止限制测试窗口。覆盖正确密钥、错误主机指纹、错误客户端、夹具撤销与恢复，以及选定主机身份保持。它不改现用SSH、不开放外部监听、不安装到实际状态分区；候选的挂载守卫明确替换为true，不能据此声称真实NSHSTATE或完整系统维护入口已经验收。实际结果须以新的私有报告为准，不能只根据工具存在判断通过。

NAS侧使用`prepare-nas-usb-profile.py --seed-directory PRIVATE_SEED --expected-public-sha256 PRIVATE_DIGEST --usb-port RECORDED_PORT --output NEW_PRIVATE_OUTPUT`生成独立Root0600 NetworkManager keyfile和计划。只绑定选定USB host MAC，地址10.55.0.1/29，autoconnect=false、never-default=true、忽略自动DNS且禁用IPv6；工具本身不调用NetworkManager、不加载或激活配置、不改地址/路由。计划要求激活前核对实际USB物理口、固件VID/PID、完整serial和host MAC，并检查网段冲突；不得拿新MAC猜旧接口名。实际加载/激活需要另外保存结果，准备文件不代表网络接入通过。

若现用SSH禁止TCP转发，loopback模式应失败并保留记录，不改变旧SSH策略。`--usb-direct`是另一个显式测试模式：只接受固定10.55.0.2地址，NAS必须已有无网关的物理USB直达路由，临时服务仅绑定该地址，并通过AllowUsers将维护认证限制为NAS的10.55.0.1来源；不监听Wi-Fi或公网。NAS经现有允许的设备接入通道直接连接测试端口，客户端私钥仍留在本机，保留90秒时限、精确主机固定和结束停止。该模式同样只验证候选用户态及所选身份，不代表已安装正式维护入口。

`build-maintenance-state.py`只在私有挂载命名空间构建单独的NSHSTATE候选普通文件。输入为独立验证的准确恢复计划及原始证据、所选完整维护资料；从只读固定FD复制p15并核对摘要，检查ext4/NSHSTATE卷身份，只对新Root0600副本建立可写loop。挂载使用nodev/nosuid/noexec/noatime。旧挂载根必须是101:103:0700且不存在Rust命名空间；安装所选资料后只将副本挂载根改为0:0:0700。对原有每项文件摘要、所有者/模式、硬链接数量、mtime、符号链接目标及xattrs做前后比较，不跟随旧链接或打开特殊文件。

完成后同步、卸载、分离loop，两次只读e2fsck通过，卷label/UUID/长度保持，源整图重新核对，才记录候选摘要。失败保留部分副本；既有输出不覆盖。命令须在`unshare --mount --propagation private`中运行；挂载空间不私有时拒绝。该副本包含设备私有资料，只能用于匹配的新系统，不能用于仅回退旧rootfs；旧系统恢复仍使用原始匹配rootfs+NSHSTATE快照。构建不会安装到棒子、修改源图或授予刷写/发布许可，具体实测以新的NAS私有报告为准。
