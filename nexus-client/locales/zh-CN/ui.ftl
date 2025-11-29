# Nexus BBS Client - Simplified Chinese Translations

# =============================================================================
# Buttons
# =============================================================================

button-cancel = 取消
button-send = 发送
button-delete = 删除
button-connect = 连接
button-save = 保存
button-create = 创建
button-edit = 编辑
button-update = 更新

button-accept-new-certificate = 接受新证书

# =============================================================================
# Titles
# =============================================================================

title-nexus-bbs = Nexus BBS
title-connect-to-server = 连接到服务器
title-add-server = 添加服务器
title-edit-server = 编辑服务器
title-broadcast-message = 广播消息
title-user-create = 创建用户
title-user-edit = 编辑用户
title-update-user = 更新用户
title-connected = 已连接
title-bookmarks = 书签
title-users = 用户
title-fingerprint-mismatch = 证书指纹不匹配！

# =============================================================================
# Placeholders
# =============================================================================

placeholder-username = 用户名
placeholder-password = 密码
placeholder-port = 端口
placeholder-server-address = 服务器地址
placeholder-server-name = 服务器名称
placeholder-username-optional = 用户名（可选）
placeholder-password-optional = 密码（可选）
placeholder-password-keep-current = 密码（留空保持当前密码）
placeholder-message = 输入消息...
placeholder-no-permission = 无权限
placeholder-broadcast-message = 输入广播消息...

# =============================================================================
# Labels
# =============================================================================

label-auto-connect = 自动连接
label-add-bookmark = 书签
label-admin = 管理员
label-enabled = 已启用
label-permissions = 权限：
label-expected-fingerprint = 预期指纹：
label-received-fingerprint = 收到的指纹：

# =============================================================================
# Permission Display Names
# =============================================================================

permission-user_list = 用户列表
permission-user_info = 用户信息
permission-chat_send = 发送聊天
permission-chat_receive = 接收聊天
permission-chat_topic = 聊天主题
permission-chat_topic_edit = 编辑聊天主题
permission-user_broadcast = 用户广播
permission-user_create = 创建用户
permission-user_delete = 删除用户
permission-user_edit = 编辑用户
permission-user_kick = 踢出用户
permission-user_message = 用户消息

# =============================================================================
# Tooltips
# =============================================================================

tooltip-chat = 聊天
tooltip-broadcast = 广播
tooltip-user-create = 创建用户
tooltip-user-edit = 编辑用户
tooltip-toggle-theme = 切换主题
tooltip-hide-bookmarks = 隐藏书签
tooltip-show-bookmarks = 显示书签
tooltip-hide-user-list = 隐藏用户列表
tooltip-show-user-list = 显示用户列表
tooltip-disconnect = 断开连接
tooltip-edit = 编辑
tooltip-info = 信息
tooltip-message = 消息
tooltip-kick = 踢出
tooltip-close = 关闭
tooltip-add-bookmark = 添加书签

# =============================================================================
# Empty States
# =============================================================================

empty-select-server = 从列表中选择服务器
empty-no-connections = 无连接
empty-no-bookmarks = 无书签
empty-no-users = 无在线用户

# =============================================================================
# Chat Tab Labels
# =============================================================================

chat-tab-server = #服务器

# =============================================================================
# System Message Usernames
# =============================================================================

msg-username-system = 系统
msg-username-error = 错误
msg-username-info = 信息
msg-username-broadcast-prefix = [广播]

# =============================================================================
# Success Messages
# =============================================================================

msg-user-kicked-success = 用户已成功踢出
msg-broadcast-sent = 广播已成功发送
msg-user-created = 用户已成功创建
msg-user-deleted = 用户已成功删除
msg-user-updated = 用户已成功更新
msg-permissions-updated = 您的权限已更新
msg-topic-updated = 主题已成功更新

# =============================================================================
# Dynamic Messages (with parameters)
# =============================================================================

msg-topic-cleared = { $username } 清除了主题
msg-topic-set = { $username } 设置了主题：{ $topic }
msg-topic-display = 主题：{ $topic }
msg-user-connected = { $username } 已连接
msg-user-disconnected = { $username } 已断开连接
msg-disconnected = 已断开连接：{ $error }
msg-connection-cancelled = 由于证书不匹配，连接已取消

# =============================================================================
# Error Messages
# =============================================================================

err-connection-broken = 连接错误
err-user-kick-failed = 踢出用户失败
err-no-shutdown-handle = 连接错误：无关闭句柄
err-userlist-failed = 刷新用户列表失败
err-port-invalid = 端口必须是有效数字（1-65535）

# Network connection errors
err-no-peer-certificates = 未找到服务器证书
err-no-certificates-in-chain = 证书链中没有证书
err-unexpected-handshake-response = 意外的握手响应
err-no-session-id = 未收到会话ID
err-login-failed = 登录失败
err-unexpected-login-response = 意外的登录响应
err-connection-closed = 连接已关闭
err-could-not-determine-config-dir = 无法确定配置目录
err-message-too-long = 消息过长
err-send-failed = 发送消息失败
err-broadcast-too-long = 广播消息过长
err-broadcast-send-failed = 发送广播失败
err-name-required = 书签名称为必填项
err-address-required = 服务器地址为必填项
err-port-required = 端口为必填项

# =============================================================================
# Dynamic Error Messages (with parameters)
# =============================================================================

err-failed-save-config = 保存配置失败：{ $error }
err-failed-save-theme = 保存主题偏好失败：{ $error }
err-bookmark-connection-failed = 书签连接失败：{ $error }
err-invalid-port-bookmark = 书签中的端口无效：{ $name }
err-failed-send-broadcast = 发送广播失败：{ $error }
err-failed-send-message = 发送消息失败：{ $error }
err-failed-create-user = 创建用户失败：{ $error }
err-failed-delete-user = 删除用户失败：{ $error }
err-failed-update-user = 更新用户失败：{ $error }
err-failed-update-topic = 更新主题失败：{ $error }
err-message-too-long-details = { $error }（{ $length }字符，最大{ $max }）

# Network connection errors (with parameters)
err-invalid-address = 无效地址 '{ $address }'：{ $error }
err-could-not-resolve = 无法解析地址 '{ $address }'
err-connection-timeout = 连接在 { $seconds } 秒后超时
err-connection-failed = 连接失败：{ $error }
err-tls-handshake-failed = TLS握手失败：{ $error }
err-failed-send-handshake = 发送握手失败：{ $error }
err-failed-read-handshake = 读取握手响应失败：{ $error }
err-handshake-failed = 握手失败：{ $error }
err-failed-parse-handshake = 解析握手响应失败：{ $error }
err-failed-send-login = 发送登录失败：{ $error }
err-failed-read-login = 读取登录响应失败：{ $error }
err-failed-parse-login = 解析登录响应失败：{ $error }
err-failed-create-server-name = 创建服务器名称失败：{ $error }
err-failed-create-config-dir = 创建配置目录失败：{ $error }
err-failed-serialize-config = 序列化配置失败：{ $error }
err-failed-write-config = 写入配置文件失败：{ $error }
err-failed-read-config-metadata = 读取配置文件元数据失败：{ $error }
err-failed-set-config-permissions = 设置配置文件权限失败：{ $error }

# =============================================================================
# Fingerprint Warning
# =============================================================================

fingerprint-warning = 这可能表示存在安全问题（中间人攻击）或服务器证书已重新生成。仅在信任服务器管理员时才接受。

# =============================================================================
# User Info Display
# =============================================================================

user-info-header = [{ $username }]
user-info-is-admin = 是管理员
user-info-connected-ago = 已连接：{ $duration }前
user-info-connected-sessions = 已连接：{ $duration }前（{ $count }个会话）
user-info-features = 功能：{ $features }
user-info-locale = 语言：{ $locale }
user-info-address = 地址：{ $address }
user-info-addresses = 地址：
user-info-address-item = - { $address }
user-info-created = 创建时间：{ $created }
user-info-end = 用户信息结束
user-info-unknown = 未知
user-info-error = 错误：{ $error }

# =============================================================================
# Time Duration
# =============================================================================

time-days = { $count }天
time-hours = { $count }小时
time-minutes = { $count }分钟
time-seconds = { $count }秒