# 身份验证和会话错误
err-not-logged-in = 未登录
err-authentication = 身份验证错误
err-invalid-credentials = 用户名或密码无效
err-handshake-required = 需要握手
err-already-logged-in = 已经登录
err-handshake-already-completed = 握手已完成
err-account-deleted = 您的账户已被删除
err-account-disabled-by-admin = 账户已被管理员禁用

# 权限和访问错误
err-permission-denied = 权限被拒绝

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

# 消息验证错误
err-message-empty = 消息不能为空

# 用户名验证错误
err-username-empty = 用户名不能为空
err-username-invalid = 用户名包含无效字符（允许字母、数字和符号 - 不允许空格或控制字符）

# 动态错误消息（带参数）
err-broadcast-too-long = 消息太长（最多{ $max_length }个字符）
err-chat-too-long = 消息太长（最多{ $max_length }个字符）
err-topic-too-long = 主题不能超过{ $max_length }个字符
err-version-mismatch = 版本不匹配：服务器使用{ $server_version }，客户端使用{ $client_version }
err-kicked-by = 您已被{ $username }踢出
err-username-exists = 用户名"{ $username }"已存在
err-user-not-found = 找不到用户"{ $username }"
err-user-not-online = 用户"{ $username }"不在线
err-failed-to-create-user = 创建用户"{ $username }"失败
err-account-disabled = 账户"{ $username }"已被禁用
err-update-failed = 更新用户"{ $username }"失败
err-username-too-long = 用户名太长（最多{ $max_length }个字符）