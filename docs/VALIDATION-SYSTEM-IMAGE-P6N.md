# P6N 内部系统镜像与完整源码核验

2026-10-18 保存。构建源 f2703b4667f6a648c2cf8e1c21566170db8a40a9；应用仍为4e6fea6，组合树仍为P6M/075b68e，不重标既有制品。

`tools/verify-kernel-source-tree.py` 对固定上游源码归档流式计算全部Git对象，86859项、1479589504字节，根tree为72d4e8e42649f763dd969a1de893051ed64387a1，与93a71ee9468dd8d47a9e082e5d818608f08712e3一致。板级记录已固定该提交，完整基础内核源码归属已闭合；独立重编译完整内核仍未完成。

`tools/build-system-image.py` 从P6M的12367项、树SHA c621fdbdedee685d0f6a930a949d36f2cb6dea29953d8240b4dda7bb17b95376生成全新普通文件。rootfs.img长3436183552字节，SHA 3ac9bc81ceaaa4b0807217b49de4c1622040d11165373100e289d8975d656d88。只读e2fsck通过；隔离只读挂载逐项核对内容、权限、数值属主及能力通过，源树保持，未包含私有固件/状态。实际私有报告system-image-p6n-4e6fea6-first-manifest.json与mounted.json均通过。

严格umask077下62项板级工具测试、34项rootfs测试通过；本轮没有改Rust代码，不声称重跑此前209项Rust测试。f270本轮CI尚未核验；此前598f273完整CI通过。

以上仅为内部离线候选：flash_allowed/release_ready/recovery_verified/board_boot_verified均false，未写设备分区。完整新OS冷启动、真实驱动加载、实际状态初始化和回刷仍需验收，不能把镜像构建成功当作已刷机。

## 每设备安装前的现场发现

实际NSHSTATE目前挂载在旧路径/var/lib/nassimhub，挂载根属主101:103、模式0700；包含既有私有固件/基带文件。RustSimHub新命名空间尚不存在。新首启会将同一挂载根改为root:root；因此仅回退rootfs不足以恢复旧业务访问，还需恢复匹配的状态备份或原属主。不得格式化或丢弃旧状态来绕过这一问题。维护公钥/真实初始化仍未完成。

用户已确认可配合物理恢复。新的私有恢复尝试为recovery-edl-20261018-first.json，复用经过SHA核验的有限只读工具、全新NAS输出目录，禁止重用旧尝试。正常poweroff后观察到旧OS自动重新启动；记录新boot_id并重新核验身份、boot、四服务和空闲呼叫一致。随后仅请求一次systemctl halt，等待停机后才给物理步骤。准备脚本最后一次状态断言失败已保留，不能据此重发关机或启动第二个探针。后续必须核实当前探针实际phase/PID，捕获完成后独立回读、两文件系统检查与正常返回；当前尚不声称新捕获成功。

本检查点无新拨号、短信、普通蜂窝测试或原始分区写入，累计10010仍38。固定评分仍73.5/100，约74%。
