# A-17～20：入口盘点与公开产物扫描候选

执行账号wnfhsj88（用户已明确接手原A全部任务）。分支codex/wnfhsj88-public-artifact-scan，起始主线 `80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d`；实现/受测完整HEAD `678e74d52b17b00d01f87cbdd4f179aad7238019`，之后仅本报告；最终PR完整HEAD见正文。

## 实现与边界

A-17对实际HTTPS/首配HTTP、一般/媒体/转发私有IPC、Auth/scope/恢复grant、固定Root转发/AP/升级与私有写入作源码盘点，逐入口给调用者、绑定/期限/输入上限/撤销与缺口，并将20个证据文件与上述主线逐字节比较后记录SHA256。见docs/security/interface-inventory.md。没有将库接口/安装候选当作真实Root或生产BLE验收。

A-18提供实际只读扫描器，默认不读内容/不运行Git；显式公开Git文本白名单或有严格manifest的公开/合成产物才读取。Linux逐级目录句柄与NOFOLLOW/NONBLOCK拒绝链接/特殊文件，普通单硬链接文件受大小/总量/文件数/行长/时间限制。七类有限规则查私钥头、token/字面量/Wi-Fi PSK/疑似个人通信字段；报告只含规则/位置/计数/摘要，不含命中正文或值。文件名若也含命中值会一并脱敏。报告新建0600、不覆盖；没有输入修复/删除/网络/上传。

A-19实际18项测试覆盖全部正例/公开公钥与摘要等反例、同一行占位符后的字面量、秘密在文件名的隐蔽回显、实际Git清单、FIFO/目录/根父叶链接/硬链接、读取故障/文件变化、编码/二进制/未知类型、重复/manifest、所有预算与时钟异常、输出守卫/default plan及CLI三种状态。测试运行生产扫描器与CLI，不用替代状态机自证。个别读取错误与时间边界用注入端口模拟，Linux链接/FIFO/原文件变化/新报告和Git行为均实际执行。

A-20提交实际合成报告、使用/限制/误报说明和现有CI接入建议；不改Actions/生产打包器或许可清单。本包沿用PR115依赖盘点，不声称其已获审，也不另造一套依赖版本。用户持续要求下先推进不接入产品运行时的独立只读候选；全局SEC验收、正式白名单和依赖审批仍由负责人确认。

## 真实验证

```sh
source /workspace/toolchains/activate-rustsimhub.sh
python3 -m unittest discover -s tools/security-inventory -p 'test_*.py' -v
python3 -m py_compile tools/security-inventory/scan.py
python3 tools/security-inventory/scan.py
python3 tools/security-inventory/scan.py --run --tracked --output /tmp/new-public-source-report.json
git diff --check
```

18项回归/default plan/语法/diff退出0。显式合成CLI：公开反例退出0；七类合成匹配退出1且报告无原值；未知二进制产物退出2、仍保留其他匹配。输入前后逐字节相同。docs/security/synthetic-scan-report.json为实际CLI的脱敏合成结果，不是发行物无秘密证明。

受测HEAD的实际公开Git白名单：943个选定输入、50个范围外文件；908个文件完整处理，读取8388609字节（8MiB加1检测字节），33项未完成/未继续。规则候选总数52，分类为{'secret_literal': 35, 'personal_identifier': 12, 'private_key': 4, 'communication_text': 1}。两个生成页长行与总量上限明确产生incomplete，CLI预期退出2。没有把预算中断、范围外/未知输入填“干净”，没有将候选直接认定真实秘密或自动改文件。

日志与真实源库报告只保留工作区setup-logs/member-a-public-scan-*，未上传原文；仅提交合成脱敏结果。未改Rust/网页/设备接口，未重跑无关workspace/ARM/设备或借PR112的899项结果填本包验证。远程CI按最终HEAD单独回读。

## 审核/未完成

正则是有限启发式；缺少全部格式/编码、实际生产私有发行白名单、目标Root/UID、真实证书/媒体/BlueZ/密码/恢复运行和最终安全签认。时间预算是本地协作检查，不能抢占任意文件系统阻塞。不把本包声明为SEC-01/04或A-28整体通过。A后续新Root/密码/协议/数据迁移/SMTP生产接线继续按接口提案审核。

沿用AGPL-3.0-only，Python仅标准库，无新第三方依赖/许可证变更；OpenAI Codex辅助实现/验证，无工时或作者改写。无设备/NAS/私有维护材料、真实NKN/RPC/交易/通信/流量、Root命令、签名/部署/环境发布或全局进度变更。回退仅撤本工具/文档，不删除任何输入/原有报告或业务状态。
