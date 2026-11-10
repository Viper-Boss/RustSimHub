# 固定生产组合输入检查

窗口8的第一阶段：冻结输入的完整HEAD及包含关系，不将“PR已提交/已合并/测试报告”当作运行、健康或安装授权。输入清单不自动跟随分支或GitHub最新标题；实际交付变化由负责人回读并审查后更新。

```sh
python3 tools/product-integration/verify.py --repository /private/full-checkout --integration FULL_COMMIT_SHA
python3 tools/product-integration/verify.py --repository /private/full-checkout --integration FULL_COMMIT_SHA --require-complete
python3 -m unittest discover -s tools/product-integration -p 'test_*.py'
```

不联网、执行候选程序、取凭据、写Git/业务文件、安装或确认。Git replace对象禁用；浅仓库、错baseline、非commit、虚假包含关系、重复键/输入、循环依赖拒绝。待交付输入必须明确null，不用预计分支或PR号填空。输入锁已登记206/211、209/213、210/212和208的固定历史及包含关系；网页回归来源保留PR207/208。

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

## 正式维护生产入口

Linux Root updater只接受以下无附加参数的动作：

```sh
rustsimhub-update product-prepare
rustsimhub-update product-apply
rustsimhub-update product-inspect
rustsimhub-update product-recover
rustsimhub-update product-rollback
rustsimhub-update product-activate
rustsimhub-update confirm-running-candidate
rustsimhub-update inspect-state
```

信任固定为`/usr/share/rustsimhub/release.public`；应用账本固定在
`/var/lib/rustsimhub/rustsimhub-v1/system/app`，候选应用位于同级
`app-inbox`，有限Root材料位于`product-inbox`。独立批准角色策略固定为
`/etc/rustsimhub/product-roles.json`，包不能写入信任、策略或批准自己的UID。
`prepare`不启动服务；`apply`停固定业务服务、持一致备份及原数据锁安装，
需要时执行原邮件3→4及恢复6→7→9。安装器本身没有start/enable命令。
`inspect`要求固定业务已停，保留原账本；它不是运行健康检查。

只有`product-recovery-health-v1`提供本轮显式`product-activate`和受限健康
确认链：原选择器先消耗attempt，交出数据库writer给真正Core，保留Root/
信任/槽位/NSS/原数据库inode事实。重复activate在停服务前拒绝。
随后另一次新鲜Root观察通过原Core端点、Auth/SQL lease、flock及最终
Recheck推进原确认账本一次。任何计划/只读JSON均不能作为确认能力。

该profile仅支持forwarding Root模式、实际schema9与worker4/outbox3（或
队列缺失）、NKN/BLE/转发OFF。候选和回退必须实际实现该模式/健康前缀，
单Core profile v1还要求两者Core字节一致。AP及邮件健康组合明确拒绝；
旧baseline缺模式、schema/worker不兼容均须另作显式基线维护，不能改JSON。
邮件适配保留原base6、baseline/candidate/baseline三方SMTP5/outbox4限制；
需要两种迁移时邮件先于恢复。默认启动没有隐式迁移。

`recover/rollback`仅恢复原安装器可证明的有限材料，不恢复旧业务数据或
重置floor/attempted/恢复预算。Executing/Unknown迁移不能自动重试；保留
原journal和后写数据。故障后先查询原状态，不以删除账本或重新prepare消歧。
真实设备、签名/epoch、PID1及安装授权仍需负责人另行核验；上述命令不能
在宿主环境作fixture测试。

runner的`--scenario default`使用同源默认构建，验证原boot-select/launcher、
真实Owner登录/MM协调、schema6保持、无产品profile/健康端点、原floor及
attempted不变；默认`--scenario product`执行上面的恢复健康链。ARM两种场景
均使用正式CLI，Native仅driver内有架构适配。容器开始暂存前要求至少1GiB
可用工作区；ENOSPC专项只能用独立小tmpfs，不填充宿主文件系统。


邮件专项用`--scenario email --legacy-driver ORIGINAL_FORMAT3_FIXTURE`，driver
须编译updater的`email-production-candidate`，正式Core/worker须为真实SMTP5。
`legacy-fixture/`为单独feature图，只在私有UID986内通过原Auth登录和原Queue
写入format3，避免测试feature统一把旧队列初始化成format4。该专项执行实际
`product-prepare/apply/inspect`和保留快照的3→4迁移；不声明邮件自动确认。
Root邮件journal格式2记录实际持锁源摘要，仅为已完成迁移的旧备份来源证据；
不授权迁移或数据恢复。旧格式1邮件记录缺摘要时拒绝此来源复用，保留人工核验。
