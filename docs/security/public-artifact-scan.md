# A-18～20：有界公开资料秘密扫描

工具`tools/security-inventory/scan.py`检查显式公开输入中的有限可疑模式；不联网、不上传、不运行被检查程序，不自动修复或删除输入。默认只打印计划，既不读取内容也不运行Git。当前安全文件适配器要求Linux的目录句柄、NOFOLLOW/NONBLOCK；不在缺少这些能力时假装完成。

## 执行与结果

```sh
python3 tools/security-inventory/scan.py
python3 tools/security-inventory/scan.py --run --tracked --output /tmp/new-public-source-report.json
python3 tools/security-inventory/scan.py --run --artifact /explicit/public-artifact --manifest /explicit/public-manifest.json --output /tmp/new-artifact-report.json
python3 -m unittest discover -s tools/security-inventory -p 'test_*.py' -v
```

产物manifest严格为单个JSON对象：`{"schema":1,"files":["index.html","src/app.js"]}`，只接受这两个键、整数版本1和文件列表；拒绝重复JSON键。路径必须为规范相对路径，不能有绝对路径、点/双点、反斜杠、控制字符或私有目录。manifest本身也按安全文件方式读取，上限128KiB。

`--tracked`只读取固定本仓库的Git已跟踪路径清单，然后选择明确的公开目录和文本类型（见源码常量PUBLIC_PREFIXES/PUBLIC_ROOT_FILES/TEXT_SUFFIXES）。不扫描.git历史、私有维护目录、备份、主机用户目录/系统设备，不接受任意Git命令/runner。未选中的文件计入out_of_scope_count，不能因此给整个仓库“全部干净”的结论。输出必须是新文件，以0600创建，不覆盖已有报告；报告父路径也拒绝链接及系统/私有根。

| 退出码/状态 | 含义 |
|---|---|
| 0 / no_findings_in_scope | 这次明确的所有输入已扫描且无规则命中；不是无任何秘密的证明 |
| 1 / findings | 已检查的明确输入存在潜在匹配，需复核；不输出命中正文/值 |
| 2 / incomplete | 有错误、跳过、未知编码/类型或预算中断；可同时包含匹配，不能当扫描通过 |

文件位置只输出相对路径与行号；若路径自身含可疑token，或含本文件命中的口令/号码/正文值，则改为`redacted-path-`加相对路径SHA256前16位，并同步修正该文件之前的发现项，避免从文件名回显秘密。输出文件摘要/字节数用于绑定被读取的输入，不含绝对根目录、原命中行、密钥/令牌/正文或外部命令stderr。局部摘要不是输入在整个扫描期间不可变的证明；每个句柄读前后还核对inode/大小/修改与变更时间，发现变化按未完成处理。

## 规则与人工复核

| 规则ID | 有限覆盖 |
|---|---|
| private_key | 常见PEM/OpenSSH等PRIVATE KEY头；公开证书/公钥不因此匹配 |
| access_token | GitHub token/PAT形式的有限模式；不验证或使用令牌 |
| secret_literal | 明确password/passphrase/seed/private_key等字段的有界引号字面量 |
| wifi_psk | NetworkManager等psk/wifi_password/wpa_passphrase赋值；含引号或空格的有限口令 |
| personal_identifier | recipient/phone_number/imsi/iccid的明确数字赋值 |
| communication_text | sms_body/message_body/sms_text的明确正文赋值 |
| bearer_token | 有限JWT形状，不尝试解析或使用认证 |

空值、准确的`<redacted>`/`<placeholder>`及变量/Actions secret引用不被当作嵌入的字面值；公钥/摘要/年份/示例字段名不等于私密值。真实规则仍是启发式，不覆盖所有凭据格式/编码/拼接。例外只解释这些明确语法，不提供跳过整个凭据类别的开关；测试中即使合成值带SYNTHETIC前缀也照常匹配。同一行先出现占位符不会遮住其后另一个真实字面量。

若匹配是公开测试向量/明确合成夹具，应按规则ID、相对位置、确切源SHA说明误报原因，经负责人复核；不删除断言、全局忽略password/token类别或对真实秘密生成公开“证明材料”。若需处理真实私有发行资料，由负责人另行确认输入范围，不能指向设备备份/主机私有目录以凑验收。

## 资源与文件边界

单次最多1024文件，每文件512KiB、总读取8MiB（最多多读取1个检测字节），每行16384字符、最多256项发现；扫描阶段以单调时间协作核验5秒，Git清单管道另有3秒/2MiB/8192路径上限。超限/回拨/无效时钟为未完成，不拉长限制或静默截断冒充通过。同步文件系统调用不能被Python时钟抢占，不对任意远程文件系统承诺硬实时；只用于明确公开本地输入。

根和每级目录通过NOFOLLOW目录句柄遍历；叶文件用NOFOLLOW/NONBLOCK打开后fstat，只有单硬链接普通文件可读。FIFO/目录/链接/越界/读取失败、无效UTF-8、NUL二进制、未知扩展名、重复路径、长行均拒绝或明确未完成。即使有发现，后续文件出错也保持incomplete。报告写入失败同样不形成完整结论，已有输入和报告不被改写。

## 实际运行与CI接入建议

18项真实Python回归通过，直接执行生产扫描器及CLI。覆盖全部规则正例、公开字段反例、同一行占位符后的字面量、命中值在文件名中的隐蔽回显、真实Git路径清单、FIFO不阻塞、叶/父/根链接/硬链接、文件变化、读取故障、二进制/编码/超限/时间/重复/manifest及输出不覆盖。

已提交的`synthetic-scan-report.json`来自实际CLI，只含明确合成输入的脱敏结果：7个规则匹配、无问题，退出1。另有真实CLI未知类型退出2和无匹配退出0，原始输入逐字节保持；这些合成结果不是实际产品无秘密证明。工作区原始报告不提交。

在固定main80db24e上的公开跟踪输入实跑曾因8MiB预算和生成页长行明确中断，输出incomplete；存在合成配置/通信字段的候选匹配。未读的输入、排除的二进制等不填“干净”，不把正则候选直接定为真实泄漏。最终受测HEAD/精确统计见交付报告。

建议负责人在现有CI中的公开产物allowlist明确后，以新临时报告路径执行`--artifact`：0只代表本范围无匹配，1和2分别进入人工复核/未完成处理。先保存脱敏统计，不把原始输入/命中片段作为CI artifact上传。此包不改Actions或生产打包器；本地工具/测试通过不替代SEC-01/04、依赖/许可审查或发行签认。
