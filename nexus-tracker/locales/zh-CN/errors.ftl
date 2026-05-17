# 跟踪器错误消息
#
# 所有键都使用 `err-tracker-*` 前缀，使其与 nexus-server 的本地化命名空间
# 隔离。这些键与 `nexus-tracker/src/errors.rs` 中的 `err_tracker_*`
# 辅助函数一一对应。

# 身份验证
err-tracker-unauthorized = 密码错误或缺失

# 字段验证 (error_kind: invalid)
err-tracker-fingerprint-invalid = 无效的证书指纹格式
err-tracker-name-too-long = 服务器名称过长 (最多 { $max_length } 字符)
err-tracker-name-empty = 服务器名称不能为空
err-tracker-name-contains-newlines = 服务器名称不能包含换行符
err-tracker-name-invalid-characters = 服务器名称包含无效字符
err-tracker-description-too-long = 服务器描述过长 (最多 { $max_length } 字符)
err-tracker-description-contains-newlines = 服务器描述不能包含换行符
err-tracker-description-invalid-characters = 服务器描述包含无效字符
err-tracker-password-too-long = 密码过长 (最多 { $max_length } 字节)
err-tracker-address-too-long = 地址过长 (最多 { $max_length } 字节)
err-tracker-address-invalid = 无效的地址
err-tracker-version-too-long = 服务器版本字符串过长 (最多 { $max_length } 字节)
err-tracker-version-invalid = 无效的版本 (必须是有效的 semver)
err-tracker-locale-too-long = 区域代码过长 (最多 { $max_length } 字节)
err-tracker-locale-invalid = 区域设置包含无效字符
err-tracker-port-zero = 端口不能为零
err-tracker-websocket-port-zero = WebSocket 端口不能为零

# 速率 / 容量
err-tracker-rate-limited = 超出速率限制；请稍后再试
err-tracker-capacity = 跟踪器已达到容量上限；请稍后再试
err-tracker-per-ip-capacity = 来自您 IP 的条目在此跟踪器上过多

# 协议层
err-tracker-malformed-message = 消息格式错误
err-tracker-handshake-required = 在任何其他消息之前需要先握手
err-tracker-role-violation = 此连接的角色不允许该消息
err-tracker-protocol-version-mismatch = 不兼容的跟踪器协议版本 (服务器: { $server }, 客户端: { $client })
err-tracker-handshake-version-invalid = 无效的握手版本 (必须是有效的 semver)
err-tracker-unknown-message-type = 未知的消息类型

# 帧 / 传输
err-tracker-frame-error = 帧格式违规
err-tracker-payload-too-large = 负载超过了每种消息类型的限制
