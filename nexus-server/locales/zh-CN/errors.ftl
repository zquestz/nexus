# 身份验证和会话错误
err-not-logged-in = 未登录

# 头像验证错误
err-avatar-invalid-format = 头像格式无效（必须是base64编码的数据URI）
err-avatar-too-large = 头像太大（最多{ $max_length }个字符）
err-avatar-unsupported-type = 不支持的头像类型（仅支持PNG、WebP或SVG）
err-authentication = 身份验证错误
err-invalid-credentials = 用户名或密码无效
err-handshake-required = 需要握手
err-already-logged-in = 已经登录
err-handshake-already-completed = 握手已完成
err-account-deleted = 您的账户已被删除
err-account-disabled-by-admin = 账户已被管理员禁用

# 权限和访问错误
err-permission-denied = 权限被拒绝

# 功能错误
err-chat-feature-not-enabled = 聊天功能未启用

# 数据库错误
err-database = 数据库错误

# 消息格式错误
err-invalid-message-format = 无效的消息格式

# 用户管理错误
err-cannot-delete-last-admin = 无法删除最后一个管理员
err-cannot-delete-self = 您不能删除自己
err-cannot-demote-last-admin = 无法降级最后一个管理员
err-cannot-edit-self = 您不能编辑自己
err-cannot-create-admin = 只有管理员才能创建管理员用户
err-cannot-kick-self = 您无法踢出自己
err-cannot-kick-admin = 无法踢出管理员用户
err-cannot-message-self = 您无法给自己发消息
err-cannot-disable-last-admin = 无法禁用最后一个管理员

# 聊天主题错误
err-topic-contains-newlines = 主题不能包含换行符
err-topic-invalid-characters = 主题包含无效字符

# 版本验证错误
err-version-empty = 版本不能为空
err-version-too-long = 版本太长（最多{ $max_length }个字符）
err-version-invalid-semver = 版本必须是semver格式（MAJOR.MINOR.PATCH）

# 密码验证错误
err-password-empty = 密码不能为空
err-password-too-long = 密码太长（最多{ $max_length }个字符）

# 区域设置验证错误
err-locale-too-long = 区域设置太长（最多{ $max_length }个字符）
err-locale-invalid-characters = 区域设置包含无效字符

# 功能验证错误
err-features-too-many = 功能太多（最多{ $max_count }个）
err-features-empty-feature = 功能名称不能为空
err-features-feature-too-long = 功能名称太长（最多{ $max_length }个字符）
err-features-invalid-characters = 功能名称包含无效字符

# 消息验证错误
err-message-empty = 消息不能为空
err-message-contains-newlines = 消息不能包含换行符
err-message-invalid-characters = 消息包含无效字符

# 用户名验证错误
err-username-empty = 用户名不能为空
err-username-invalid = 用户名包含无效字符（允许字母、数字和符号 - 不允许空格或控制字符）

# 未知权限错误
err-unknown-permission = 未知权限: '{ $permission }'

# 动态错误消息（带参数）
err-broadcast-too-long = 消息太长（最多{ $max_length }个字符）
err-chat-too-long = 消息太长（最多{ $max_length }个字符）
err-topic-too-long = 主题不能超过{ $max_length }个字符
err-version-major-mismatch = 不兼容的协议版本：服务器是版本{ $server_major }.x，客户端是版本{ $client_major }.x
err-version-client-too-new = 客户端版本{ $client_version }比服务器版本{ $server_version }更新。请更新服务器或使用旧版客户端。
err-kicked-by = 您已被{ $username }踢出
err-username-exists = 用户名"{ $username }"已存在
err-user-not-found = 找不到用户"{ $username }"
err-user-not-online = 用户"{ $username }"不在线
err-failed-to-create-user = 创建用户"{ $username }"失败
err-account-disabled = 账户"{ $username }"已被禁用
err-update-failed = 更新用户"{ $username }"失败
err-username-too-long = 用户名太长（最多{ $max_length }个字符）
# 权限验证错误
err-permissions-too-many = 权限太多（最多{ $max_count }个）
err-permissions-empty-permission = 权限名称不能为空
err-permissions-permission-too-long = 权限名称太长（最多{ $max_length }个字符）
err-permissions-contains-newlines = 权限名称不能包含换行符
err-permissions-invalid-characters = 权限名称包含无效字符

# 服务器更新错误
err-admin-required = 需要管理员权限
err-server-name-empty = 服务器名称不能为空
err-server-name-too-long = 服务器名称太长（最多{ $max_length }个字符）
err-server-name-contains-newlines = 服务器名称不能包含换行符
err-server-name-invalid-characters = 服务器名称包含无效字符
err-server-description-too-long = 服务器描述太长（最多{ $max_length }个字符）
err-server-description-contains-newlines = 服务器描述不能包含换行符
err-server-description-invalid-characters = 服务器描述包含无效字符
err-max-connections-per-ip-invalid = 每个IP的最大连接数必须大于0
err-no-fields-to-update = 没有要更新的字段

err-server-image-too-large = 服务器图片太大（最大512KB）
err-server-image-invalid-format = 服务器图片格式无效（必须是base64编码的数据URI）
err-server-image-unsupported-type = 不支持的服务器图片类型（仅支持PNG、WebP、JPEG或SVG）
