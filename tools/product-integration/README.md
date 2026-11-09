# 固定生产组合输入检查

窗口8的第一阶段：冻结输入的完整HEAD及包含关系，不将“PR已提交/已合并/测试报告”当作运行、健康或安装授权。输入清单不自动跟随分支或GitHub最新标题；实际交付变化由负责人回读并审查后更新。

```sh
python3 tools/product-integration/verify.py --repository /private/full-checkout --integration FULL_COMMIT_SHA
python3 tools/product-integration/verify.py --repository /private/full-checkout --integration FULL_COMMIT_SHA --require-complete
python3 -m unittest discover -s tools/product-integration -p 'test_*.py'
```

不联网、执行候选程序、取凭据、写Git/业务文件、安装或确认。Git replace对象禁用；浅仓库、错baseline、非commit、虚假包含关系、重复键/输入、循环依赖拒绝。待交付输入必须明确null，不用预计分支或PR号填空。当前health-confirm、产品安装/数据三项仍待实际交付；网页回归已定位PR207。

默认退出0只表示来源检查执行成功，JSON的 `source_assembly_complete` 可能仍false；严格模式在产品待交/未包含、脏工作树或checkout HEAD不等于冻结组合时退出3，非法输入退出2。报告类交付不满足产品依赖，缺少报告单列但不阻塞产品源码集合。即使完整，tests/cloud/authorization/health/installable均保持false：它只证明固定Git历史和当前工作树一致，不证明字节构建、测试、安装或业务健康。后续完整发行需由真实CLI/维护/安装/数据/健康端到端证明；该脚本不替代其实现。

工作树检查采用Git报告的改动及HEAD；不是逐文件归档内容核验，不能替代现有构建器的完整Git archive/运行工具字节校验。公开报告不提供批准、自动签名或部署功能。

## 隔离生产链（窗口8续接）

`production_chain.py` 仅在一次性容器/ARM虚拟机中运行；不安装宿主服务、
不修改宿主NSS、不挂载设备、无网络。输入必须来自同一已提交归档；正式
Core/worker元数据在创建测试数据前再次核对。使用原Store/Auth创建临时
baseline和候选（fixture epoch 1/2、合成密钥），之后串联固定生产路由。

```sh
python3 tools/product-integration/production_chain.py --mode native \
  --programs PRIVATE_NATIVE_PROGRAMS --test-driver PRIVATE_NATIVE_UPDATE_LIBTEST \
  --revision FULL_COMMIT --runtime-image sha256:EXISTING_PYTHON_RUNTIME_IMAGE
python3 tools/product-integration/production_chain.py --mode arm-vm \
  --programs PRIVATE_ARM_PROGRAMS --test-driver PRIVATE_ARM_UPDATE_LIBTEST \
  --revision FULL_COMMIT --guest-root "$PWD/.local/OWN_ARM_DEPENDENCY_ROOT" \
  --kernel PRIVATE_GENERIC_ARM_KERNEL --qemu PRIVATE_QEMU_SYSTEM_AARCH64
```

Native路径只在`cfg(test)`的driver中适配架构/PID1/cgroup，正式Core、
launcher、媒体和Edge不作替换。ARM路径在真实来宾内核下调用正式updater
CLI和实际cgroup；仅MM和服务控制为隔离fixture。来宾依赖需包含Python3、
busybox、getent、setpriv、dbus-daemon、完整动态库与加载器；它们不是产品
rootfs。QEMU自身依赖由调用方通过其私有库路径提供。两条路径都不能证明
设备/PID1验收或产品固件可启动。原始日志、数据库及临时信任留在`.local/`。
