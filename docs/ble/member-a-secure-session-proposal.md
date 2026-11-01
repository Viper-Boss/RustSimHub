# A-27：安全会话候选、身份绑定及互操作门槛（未冻结）

对照main80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d的development-plan§7.3、T6 SecureSessionPortV1和现有Auth recovery_capability。**production_ready=false、crypto_wire_frozen=false保持**。这里提交具体方案和明确未知，不把选用标准算法等同协议已通过安全审查。

## 候选标准与实现路径

优先评估标准`Noise_XX_25519_AESGCM_SHA256`，不用自行写ECDH/HKDF/AEAD/Noise状态机。项目原设计明确要求独立X25519传输密钥与Ed25519身份绑定；当前恢复记录只有Ed25519恢复公钥，没有已核验的长期X25519映射，不能伪称已满足Noise IK的预共享静态公钥条件。若主线选择预注册X25519再用IK，须独立审批注册/撤销及数据兼容，而不是将Ed25519 seed临时转换/复用。

拟使用项目原计划提到的Rust snow实现，Rust原生与浏览器WASM复用同一状态机，JS仅做受限胶水/字节传递与既有独立恢复CryptoKey签名；不另写一套JS Noise或自行拼nonce。snow没有正式审计的事实由原设计明确记录，不宣称已审计。浏览器WASM路径仍需构建/大小/实际运行验证，不能用“同一源码”代替跨端互操作结果。

**版本仍供审批，已有可核验评估**：此前公开crates API403及无离线缓存的记录保留为历史限制；2026-11-01同一官方端点已实际返回200。本轮独立评估锁定snow0.10.0，crate SHA256 `599b506ccc4aff8cf7844bc42cf783009a434c1e26c964432560fb6d6ad02d82`，包声明Git `4bb43f50370bdb3e8b1b57814ac662864db2704f`、Apache-2.0 OR MIT。评估在`tools/dependencies/noise-evaluation/`独立workspace，不改生产Cargo/lock，不提供应用签名/授权协议。实际最小AES特性组合因上游缺KeyInit导入而编译失败；评估显式额外启用ChaCha特性，不改上游密码源码，额外依赖及失败样例需负责人审核。该版本/取舍并未获批或进入产品，production_ready/crypto_wire_frozen仍false。原生/WASM标准Noise互通与依赖/随机源/CSP证据详见`docs/dependencies/noise-snow-evaluation.md`；它们不等于下述应用绑定、pin/grant或Core互操作已完成。备选成熟TLS方案仍需独立来源/设备pin与跨端评估。

## 具体身份绑定候选（应用层新协议，须审查）

Noise XX只证明DH传输密钥关系，不自动证明现有Ed25519设备pin/恢复grant。下面的应用身份绑定是本项目**待审新协议组成**，不冒称Noise规范已经定义或安全。

1. 每会话独立X25519静态/临时密钥与32B CSPRNG session nonce，不读Owner/统计/SIM秘密，不作为新Owner。长期Ed25519设备私钥只在实际可信设备签名端使用；浏览器恢复私钥仍是既有独立非导出CryptoKey，不导出到WASM或localStorage。
2. prologue固定版本/purpose及完整上下文：预期device Ed25519 pin、独立client recovery public key、grant generation、session nonce。device pin从原可信管理渠道核对；客户端公钥只是未验证的会话声明，匹配现有grant后仍需持钥签名。其在握手协商中的公开性/可关联性必须披露，不能宣称隐藏一切元数据。
3. 完成标准XX三消息后，双方尚未得到业务权限。先在该Noise加密通道内交换设备身份绑定，再由客户端验证设备pin签名后发送客户端绑定；两者均对**最终Noise handshake hash**签名，避免将未完整握手/不同MITM中间会话混用。
4. 拟定被签固定字节串：ASCII域`RustSimHub/RecoveryBinding/v1`+NUL、version:u16、purpose:u8、role:u8、device_pin:32、client_key:32、grant_generation:u64、session_nonce:32、core_auth_instance_hash:32、core_connection_nonce:32、trusted_transport_nonce:32、final_handshake_hash:32；整数大端，固定字段顺序/长度，不签重建JSON或可变Unicode字符串。新增的三个Core绑定值只能从实际Core/可信运输producer获得，设备在其签名的加密binding record中提供给客户端，客户端验pin签名后才能采用并回签；Core按自己的私有RecoveryProofBinding再比对，不能接受peer自行报的nonce。auth_instance_hash拟为实际Core实例标识UTF-8的SHA256，仅作固定长度绑定，不是授权证明，具体编码须审。role=device/client严格区分，purpose目前仅ble_recovery；普通NKN信令另设域/用途，不自动获得恢复或Owner权限。以上具体域/编码都尚未冻结。
5. 设备端验证客户端Ed25519签名、实际Core实例/连接nonce和当前active grant/generation/期限后，才能通过D的真实trusted verifier产生Core内部能力；close/revoke/begin/confirm/save仍在真实Auth重新核验。双方密文可解、绑定验签通过、蓝牙bonding/广告名/MAC都不是Owner授权。

Core当前公开接点包括`RecoveryRequestVerifierV1`、`RecoveryProofBinding`、`RecoveryConnection`、`prepare_recovery_verification`、`finish_recovery_verification`与`check_recovery_capability`，不新增自报authenticated/channel字符串入口，不重建grant表/改现有schema或Scope。A交密码/会话实现，D接Core可信producer；不得为互操作测试提供生产默认Mock认证。

## 记录/预算与拒绝条件

沿用T5真实上限：1会话/1在途消息、完整消息2048B、最多256片、12B分片头、属性上限244B（初始20B）、组装15秒、结果队列4、D-Bus调用5秒。分片header/message_id**不是密码nonce**。密文AEAD tag、记录方向/序号/类型都计入2048B；具体最大明文在获审格式确定后从该预算扣除，不能把完整2048B当明文再额外附tag。

候选握手总预算10秒（且不得大于Core/传输原有效期），同一截止贯穿各消息，不逐包续期；会话寿命/请求配额服从实际grant和Core更短期限，不能由本提案延长。Noise自身按实现/官方向量产生各方向nonce和密钥；不手工猜AESGCM与其他套件的计数编码、不使用T5 fragment counter或复用同一cipherstate跨方向/传输。消息/nonce空间/队列/期限耗尽关闭，不归零续用；断线必须新握手、新nonce，旧会话输出拒绝。

拒绝矩阵必须直接执行实际Rust/WASM模块：错误pin、错误客户端key、无/撤销/旧grant、错purpose/role/version/context、不同handshake hash签名、篡改/截断/额外字节、重复/乱序/方向交换、旧session/旧连接nonce、计数耗尽、超量分片/记录、期限/迟到、关闭后输出。失败不得向Core交明文，不回退未加密Wi-Fi口令，不记录密码/密钥/原始帧。测试only使用隔离合成身份；不对现有设备/Owner签名或读取生产identity。

WASM需检查Rust1.98目标、浏览器CSPRNG、零化/Drop边界、2048B线预算及PWA每文件512KiB/总2MiB限制；不能声称清除JS引用擦除了全部内存。现有CSP下WASM编译可能被拒绝，若需`wasm-unsafe-eval`必须以单独范围/资源白名单提案审核，绝不直接加通用unsafe-eval或外链CDN。真实Android/第二端与Core集成仍由主线验收。

## 必须先获批的点

套件/实现与可核验锁定依赖；独立DH密钥生命周期和身份绑定组成/域/固定编码；Core真实签名/verifier producer与撤销/会话期限；Rust/浏览器构建/CSP/静态预算。批准后提交真正加解密、官方已核对来源的KAT及Rust→浏览器/反向互操作、失败向量；当前**没有执行或声称这些密码互操作通过**，也没有将此文档标A-27完成。
