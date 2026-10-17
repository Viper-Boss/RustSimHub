# 内部系统树的板级集成

此层把已锁定Debian基础树、六个Rust生产程序、固定服务账户和UFI003板级文件组合到新的Linux目录，不能直接刷机。`tools/compose-system-tree.py`读取已提交overlay、核对基础锁/完整树及程序摘要，不运行传入程序，不修改基础输入、不访问设备、不生成boot/分区镜像。

```sh
sudo python3 tools/compose-system-tree.py \
  --base /private/verified-base-output \
  --artifacts /private/verified-arm64-programs \
  --out /private/new-system-tree
```

实际可用状态以外部`SYSTEM-CANDIDATE.json`为准。失败保留`COMPOSITION.FAILED`；不复用、修补或自动删除失败目录。输出明确`flash_allowed=false`、`release_ready=false`、`recovery_verified=false`。可选`--signed-application`、`--release-public`和`--updater-artifacts`一起接入签名启动组；再以`--native-artifacts`指定同源Native构建才标记native_media_installed=true，核验源归档/程序摘要及真实签名。媒体仍需每设备显式配置，不从旧包推定；此树不是完整产品验收结果。

组合工具显式设置新建公开程序目录0755、公开来源元数据0644及私有版本目录0700，支持调用者使用umask077；既有基础目录权限保持，不能为解决服务执行失败而递归放开私有目录。新候选必须做实际非Root ARM运行与只读systemd依赖检查，不能仅凭组合/隐私审计通过就称服务可运行。

## 板级启动顺序

`var-lib-rustsimhub.mount`按GPT标签NSHSTATE挂到`/var/lib/rustsimhub`，不格式化。只支持当前实测UFI003布局：mmcblk0p15、264159个512字节扇区、ext4/NSHSTATE、rw且不是根文件系统。检查失败时拒绝进入目录/固件准备，禁止向根文件系统临时目录悄悄降级。

`rustsimhub-firstboot`检查板型/挂载和固定Core986、Edge985、Media984、IPC987。挂载根目录收紧为root:root/0700，避免旧NAS服务UID在新发行版中被其他守护程序复用后访问旧文件；不更改旧文件内容及各自owner。只在`rustsimhub-v1`新命名空间创建私有Core/Edge/Media子目录。已有目录的链接、owner或mode不符直接拒绝，不递归chown/repair，不创建业务身份、设置code或认领窗口。

USB serial在独立root私有目录中生成并同步保存。重启或重建根系统继续使用原值；合法pending续用，损坏/链接/冲突拒绝，不自动重新生成。USB NCM从该serial派生稳定MAC，不再依赖会随rootfs重建变化的machine-id。USB输入先验证，有任何已有gadget都拒绝重配。开发VID/PID沿用原项目开发值，未解决正式产品分配。

固件加载从同一NSHSTATE内原`.private-firmware`验证自有bundle白名单/摘要，禁止任意路径、配置注入、不同既有固件覆盖和NV/校准分区写入；没有bundle时不假造硬件就绪。随后rmtfs/qrtr启动，仅允许两个已知offline remoteproc启动，running不重启、crashed拒绝强制处理。NetworkManager/ModemManager的drop-in按此顺序等待。

NetworkManager的system-connections目录单独绑定到持久分区中root:root/0700的networkmanager目录。首启只从不可变公开模板种入USB profile，原子pending发布并同步；已有合法私有文件保留原内容，包含NM正常补充的UUID/时间戳，不重新覆盖。Wi-Fi口令由NM写入其root私有目录，不进入通用镜像或Core文件。损坏pending和链接拒绝；现有普通蜂窝APN不自动导入。此持久化尚未通过新的完整系统实机启动，不能据此声称掉电或rootfs升级的配网保持已验收。

新树保留空持久挂载点，不打入任何身份、业务库、网络密码、维护公钥、SSH host keys、设置码或厂商固件。原NSHSTATE的数据不由组合器复制到镜像。现有板在换系统前仍必须完成当前恢复门禁；旧系统回退时还要核对挂载根目录owner（原旧系统firstboot会重新设置旧服务owner），不能把rootfs回刷等同于持久数据状态已回退。

来源与原始SHA见`reuse-provenance.json`及顶层NOTICE，复用部分为AGPL-3.0-only。新增目录/标识准备和树组合工具同为AGPL-3.0-only。内核、Debian包和厂商固件不因此重新许可。

SSH维护层新增独立持久主机密钥准备和公钥认证配置；首启只创建空ssh-access，安装器显式提供维护公钥，缺失时SSH拒绝启动。完整pending目录可续用，损坏或冲突拒绝，保留主机身份。可信初始化/Root私有交付及候选namespace中的合成SSH登录/撤销已通过；真实NSHSTATE安装、实际维护钥匙交付与新系统维护登录尚未验收，见docs/persistent-ssh-maintenance.md。原724fc09候选树不原地修改，最新组合以HANDOFF.md为准。

本机隔离测试：`sudo python3 -m unittest discover -s tools/system -p 'test_*.py'`。固件/radio测试复用真实脚本并仅替换固定设备路径；目录准备测试只用一次性目录/合成旧数据，测试挂载守卫失败不写、旧文件内容/owner保持、稳定serial及pending续用、链接/错误owner拒绝。不能代替真实启动、USB链路或恢复验收。

当前共44项隔离测试，含严格umask下服务UID真实执行、既有私有目录保持/链接拒绝、归档守卫、SSH材料、可信初装及应用恢复失败路径。已组合树的程序运行可用`tools/test-base-runtime.py --system-tree --base .../rootfs --artifacts ... --report /private/new-result.json`验证：执行factory的实际安装路径，核对Root0555/摘要/来源，并确认树前后未变。该检查不启动服务组。正式Core/Media/Edge签名组实机硬退出回退证据见docs/VALIDATION-MEDIA-STARTUP-P6K.md；仍未完成板级独立开机。
