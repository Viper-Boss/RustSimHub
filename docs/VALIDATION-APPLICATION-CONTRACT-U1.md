# U1 · 程序实际转发版本能力描述（UPD-01阶段一）

2026-10-27。状态：版本能力生产元数据与严格类型解析已实现并通过本机验证；完整Root/应用兼容准入仍未完成，UPD-01不计done。工作分支codex/application-root-compatibility基于DNS修复PR#4，尚不部署或声称main已有这些字段。

## 实现与边界

`crates/forwarder/src/compatibility.rs`定义有版本的RuntimeCompatibility/OutboxCompatibility。Core和forwarder的真实`--build-info`新增forwarding_runtime_contract：描述格式1、worker协议4、SQLite application_id 0x52534657、直接读取/写入格式3、需迁移来源[1,2]。不把“可迁移1/2”误写成“不迁移直接读取1/2”，不宣称旧程序可反向读取3。

工作协议和队列application_id/当前格式/迁移来源与实际worker IPC、Queue准入及迁移条件共享同一组编译期常量，避免描述与实现分别手写版本。数值与既有行为不变：原格式1/2在绑定/完整性检查后迁移，Unknown及回执保留；未改schema、签名域、权限、Root参数、重试、旧版本选择或数据回退。

严格解析最多4096字节，拒绝未知描述版本/协议、错误字段类型、缺字段、额外/重复字段、不同读写格式和迁移集合，以及过长/空输入。它只接受本实现支持的契约，不是身份验证、真实数据库探针或已安装Root兼容性的证明。维护调用者仍须验证程序来源、实际Root配置及持久数据版本。

## 实际验证

冻结工作归档736文件，SHA-256 `c1cdd51655e916447acb5d50f08ba609c7370d87046f77c9cef14dbc030a2a94`。显式清理两个变更包Core/forwarder的开发产物，其他内容相同依赖缓存复用；只读源码/依赖、UID65534、空能力/NoNewPrivileges、私有网络/设备。

- Core157 + forwarder82 + Wire18，共257项普通测试通过，另外专用ENOSPC默认ignored并单列；含真实旧outbox1/2迁移、Unknown和Owner绑定失败路径。
- 同三包all-targets/all-features严格Clippy、workspace fmt通过。
- 真正构建Core/forwarder的no-default-features生产程序，再以非Root身份逐个执行实际`--build-info`：两者的完整契约与独立期望JSON一致，worker顶层协议与契约协议一致、源码归档一致、development_fixtures=false、输出小于16384字节、3秒内返回，在新临时工作目录未创建文件。
- 实际程序信息与原始日志在`.private/upd-contract-u1-20261027-first/`。它证明本机native生产程序输出，不是ARM/设备验收。新精确提交CI和ARM仍待补，不能把PR#4的成功移作本阶段结果。

## 尚未完成

阶段一后续精确HEAD `7dfd6a007153885eb158c4921bc38c3574d97fec`：push CI40445261493及PR CI40445295658均completed/success，两个运行各自的test/native-audio均success，已独立回读。后续持久数据检查见[U2](VALIDATION-OUTBOX-INSPECTION-U2.md)；这些成功只属于阶段一，不能替代后续提交的CI。

1. 已验证历史档位、缺失新字段及未知构建的严格兼容分类。
2. 将实际Root配置和当前数据格式与候选/回选程序做有界只读比较，检查不匹配并给维护原因。
3. 接入有限维护准入及对应真实CLI/组合测试；签名材料接线归UPD-02，格式1/2/3设备回退归FWD-20。
4. ARM生产构建、主线审核/合并、最后设备交付。

未访问NAS/棒子、真实提供商或数据卡，未产生真实通信或改信任；四位成员新增文件边界保持。原80%、扩展14/169和加权7.3%不变，持续目标保持active。

## 已独立完成的前置CI修复

PR#4完整HEAD `5c6e4e29bcebf616e4c47889f38f3e4bcbb2bea9` 的push CI40443986959和PR CI40444040928，test/native-audio均success；GitHub回读仍open、mergeable=true、mergeable_state=blocked。自动审批拒绝直接推main后已改用PR，未以其他工具绕过门禁。审核/管理员合并的单笔授权已询问，等待用户答复，不据此阻塞其他开发。
