# 离线加密身份备份与恢复

开发实现待实机验证，不增加进度。加密与恢复独立为 rustsimhub-recovery，常驻 core/edge/ctl 不引入 age 依赖。

使用固定补丁 age 0.11.5 与标准 age X25519 收件人文件格式；默认关闭 SSH、插件及 armor 支持。恢复钥匙为新生成独立用途，不复用 NAS 密码、SSH 钥匙、客户端签名或发布签名钥匙。收件人公钥送设备即可，私钥留在可信维护端。

离线 CLI：keygen PRIVATE_DIR；backup IDENTITY_DIR RECIPIENT_FILE NEW_AGE_FILE；restore PRIVATE_KEY AGE_FILE EXPECTED_DEVICE_PIN EMPTY_PRIVATE_DIR。所有文件均需当前 UID、私有目录/权限、普通文件/单链接及 NOFOLLOW；不通过 API 暴露任意文件读取或远程恢复。

加密明文只含固定 112 字节：版本魔数、设备公钥、72-byte identity.seed。恢复限定 ciphertext <=8192 bytes，禁止 passphrase/scrypt，完整校验 AEAD、魔数、身份 seed 校验和与预期 Ed25519 公钥后才写入全新空目录；拒绝现有身份、链接、错误收件人/公钥、截断与篡改。备份不生成缺失身份，也不覆盖已有输出。

输出只有结果，不显示 private key/plaintext。恢复不写硬件/SIM/NV，也不能替代数据库/授权备份、TLS 私钥备份、完整 rootfs 与引导恢复。设备身份恢复需要与匹配持久数据共同验证。

依据 [age Rust 文档](https://docs.rs/age/0.11.1/age/) 与 [文件格式规范](https://age-encryption.org/v1)。依赖是 1.0 前的测试版，完整发布还需第三方依赖审查；不能把 Rust 封装或测试通过作为完整安全审计。未通过目标板测试前不作为生产恢复证明。
