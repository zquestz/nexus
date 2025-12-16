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
button-close = 关闭
button-choose-avatar = 选择头像
button-clear-avatar = 清除

# =============================================================================
# Titles
# =============================================================================

title-nexus-bbs = Nexus BBS
title-connect-to-server = 连接到服务器
title-add-bookmark = 添加书签
title-edit-server = 编辑服务器
title-broadcast-message = 广播
title-user-create = 创建用户
title-user-edit = 编辑用户
title-update-user = 更新用户
title-user-management = 用户管理
title-confirm-delete = 确认删除
title-connected = 已连接
title-settings = 设置
title-bookmarks = 书签
title-users = 用户
title-edit-server-info = 编辑服务器信息
title-fingerprint-mismatch = 证书指纹不匹配！
title-server-info = 服务器信息
title-user-info = 用户信息
title-about = 关于


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
placeholder-password-keep-current = 密码
placeholder-message = 输入消息...
placeholder-no-permission = 无权限
placeholder-broadcast-message = 输入广播消息...
placeholder-server-description = 服务器描述

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
label-theme = 主题
label-chat-font-size = 字体大小：
label-show-connection-notifications = 显示连接通知
label-show-timestamps = 显示时间戳
label-use-24-hour-time = 使用24小时制
label-show-seconds = 显示秒
label-server-name = 名称：
label-server-description = 描述：
label-server-version = 版本：
label-chat-topic = 聊天主题：
label-chat-topic-set-by = 主题设置者：
label-max-connections-per-ip = 每IP最大连接数：
label-avatar = 头像：
label-details = 技术详情
label-chat-options = 聊天选项
label-appearance = 外观
label-image = 图片
label-general = 常规
label-limits = 限制

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
tooltip-manage-users = 用户管理
tooltip-server-info = 服务器信息
tooltip-about = 关于
tooltip-settings = 设置
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
tooltip-create-user = 创建用户
tooltip-delete = 删除

# =============================================================================
# Empty States
# =============================================================================

empty-select-server = 从列表中选择服务器
empty-no-connections = 无连接
empty-no-bookmarks = 无书签
empty-no-users = 无在线用户
user-management-loading = 正在加载用户...
user-management-no-users = 未找到用户

# =============================================================================
# Chat Tab Labels
# =============================================================================

chat-tab-server = #服务器

# =============================================================================
# System Message Usernames
# =============================================================================


# =============================================================================
# Chat Message Prefixes
# =============================================================================

chat-prefix-system = [系统]
chat-prefix-error = [错误]
chat-prefix-info = [信息]
chat-prefix-broadcast = [BROADCAST]

# =============================================================================
# Success Messages
# =============================================================================

msg-user-kicked-success = 用户已成功踢出
msg-broadcast-sent = 广播已成功发送
msg-user-created = 用户已成功创建
msg-user-deleted = 用户已成功删除
msg-user-updated = 用户已成功更新
msg-permissions-updated = 您的权限已更新
msg-topic-updated = 主题更新成功

# =============================================================================
# Dynamic Messages (with parameters)
# =============================================================================

msg-topic-cleared = { $username } 清除了主题
msg-topic-set = { $username } 设置了主题：{ $topic }
msg-server-info-updated = 服务器配置已更新
msg-topic-display = 主题：{ $topic }
confirm-delete-user = 您确定要删除用户 '{ $username }' 吗？
msg-user-connected = { $username } 已连接
msg-user-disconnected = { $username } 已断开连接
msg-disconnected = 已断开连接：{ $error }
msg-connection-cancelled = 由于证书不匹配，连接已取消

# =============================================================================
# Error Messages
# =============================================================================

err-connection-broken = 连接错误
err-failed-update-server-info = 更新服务器信息失败：{ $error }
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
err-message-too-long = 消息过长（{ $length }个字符，最多{ $max }个字符）
err-send-failed = 发送消息失败
err-no-chat-permission = 您没有发送消息的权限
err-broadcast-too-long = 广播过长（{ $length }个字符，最多{ $max }个字符）
err-broadcast-send-failed = 发送广播失败
err-name-required = 书签名称为必填项
err-address-required = 服务器地址为必填项
err-port-required = 端口为必填项
err-username-required = 用户名为必填项
err-password-required = 密码为必填项
err-message-required = 消息为必填项

# Validation errors
err-message-empty = 消息不能为空
err-message-contains-newlines = 消息不能包含换行符
err-message-invalid-characters = 消息包含无效字符
err-username-empty = 用户名不能为空
err-username-too-long = 用户名过长（最多{ $max }个字符）
err-username-invalid = 用户名包含无效字符
err-password-too-long = 密码过长（最多{ $max }个字符）
err-topic-too-long = 主题过长（{ $length }个字符，最多{ $max }个字符）
err-avatar-unsupported-type = 不支持的文件类型。请使用PNG、WebP、JPEG或SVG。
err-avatar-too-large = 头像过大。最大大小为{ $max_kb }KB。
err-avatar-decode-failed = 无法解码头像。文件可能已损坏。
err-server-name-empty = 服务器名称不能为空
err-server-name-too-long = 服务器名称过长（最多{ $max }个字符）
err-server-name-contains-newlines = 服务器名称不能包含换行符
err-server-name-invalid-characters = 服务器名称包含无效字符
err-server-description-too-long = 描述过长（最多{ $max }个字符）
err-server-description-contains-newlines = 描述不能包含换行符
err-server-description-invalid-characters = 描述包含无效字符
err-failed-send-update = 发送更新失败：{ $error }

# =============================================================================
# Dynamic Error Messages (with parameters)
# =============================================================================

err-failed-save-config = 保存配置失败：{ $error }
err-failed-save-settings = 保存设置失败：{ $error }
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

user-info-username = 用户名：
user-info-role = 角色：
user-info-role-admin = 管理员
user-info-role-user = 用户
user-info-connected = 已连接：
user-info-connected-value = { $duration }前
user-info-connected-value-sessions = { $duration }前（{ $count }个会话）
user-info-features = 功能：
user-info-features-value = { $features }
user-info-features-none = 无
user-info-locale = 语言：
user-info-address = 地址：
user-info-addresses = 地址：
user-info-created = 创建时间：
user-info-end = 用户信息结束
user-info-unknown = 未知
user-info-loading = 正在加载用户信息...

# =============================================================================
# Time Duration
# =============================================================================

time-days = { $count }天
time-hours = { $count }小时
time-minutes = { $count }分钟
time-seconds = { $count }秒

# =============================================================================
# Command System
# =============================================================================

cmd-unknown = 未知命令：/{ $command }
cmd-help-header = 可用命令：
cmd-help-desc = 显示可用命令
cmd-help-escape-hint = 提示：使用 // 发送以 / 开头的消息
cmd-message-desc = 向用户发送消息
cmd-message-usage = 用法：/{ $command } <用户名> <消息>
cmd-userinfo-desc = 显示用户信息
cmd-userinfo-usage = 用法：/{ $command } <用户名>
cmd-kick-desc = 将用户踢出服务器
cmd-kick-usage = 用法：/{ $command } <用户名>
cmd-topic-desc = 查看或管理聊天主题
cmd-topic-usage = 用法：/{ $command } [设置|清除] [主题]
cmd-topic-arg-set = 设置
cmd-topic-arg-clear = 清除
cmd-topic-set-usage = 用法：/{ $command } 设置 <主题>
cmd-topic-none = 未设置主题
cmd-broadcast-desc = 向所有用户发送广播
cmd-broadcast-usage = 用法：/{ $command } <消息>
cmd-clear-desc = 清除当前标签页的聊天记录
cmd-clear-usage = 用法：/{ $command }
cmd-focus-desc = 聚焦到服务器聊天或用户消息窗口
cmd-focus-usage = 用法：/{ $command } [用户名]
cmd-focus-not-found = 未找到用户：{ $name }
cmd-list-desc = 显示已连接/所有用户
cmd-list-arg-all = 所有
cmd-list-usage = 用法：/{ $command } [所有]
cmd-list-empty = 没有已连接的用户
cmd-list-output = 在线用户：{ $users }（{ $count }位用户）
cmd-list-all-no-permission = 您需要 user_edit 或 user_delete 权限才能列出所有用户
cmd-list-all-output = 用户：{ $users }（{ $count }位用户）
cmd-help-usage = 用法：/{ $command } [命令]
cmd-topic-permission-denied = 您没有编辑主题的权限
cmd-window-desc = 管理聊天标签页
cmd-window-usage = 用法：/{ $command } [下一个|上一个|关闭 [用户名]]
cmd-window-arg-next = 下一个
cmd-window-arg-prev = 上一个
cmd-window-arg-close = 关闭
cmd-window-list = 打开的标签页：{ $tabs }（{ $count }个标签页）
cmd-window-close-server = 无法关闭服务器标签页
cmd-window-not-found = 未找到标签页：{ $name }
cmd-serverinfo-desc = 显示服务器信息
cmd-serverinfo-usage = 用法：/{ $command }
cmd-serverinfo-header = [服务器]
cmd-serverinfo-end = 服务器信息结束

# =============================================================================
# About Panel
# =============================================================================

about-app-name = Nexus BBS
about-copyright = © 2025 Nexus BBS Project
button-choose-image = 选择图片
button-clear-image = 清除
label-server-image = 服务器图片:
err-server-image-too-large = 服务器图片太大（最大512KB）
err-server-image-invalid-format = 服务器图片格式无效（必须是base64编码的数据URI）
err-server-image-unsupported-type = 不支持的服务器图片类型（仅支持PNG、WebP、JPEG或SVG）
err-server-image-decode-failed = 无法解码图片。文件可能已损坏。
err-failed-read-image = 读取图片失败: { $error }
