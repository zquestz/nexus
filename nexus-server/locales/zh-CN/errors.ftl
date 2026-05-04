# 身份验证和会话错误
err-not-logged-in = 未登录

# 昵称验证错误
err-nickname-empty = 昵称不能为空
err-nickname-in-use = 昵称已被使用
err-nickname-invalid = 昵称包含无效字符（允许字母、数字和符号 - 不允许空格或控制字符）
err-nickname-is-username = 昵称不能是已存在的用户名
err-nickname-not-found = 找不到用户"{ $nickname }"
err-nickname-not-online = 用户"{ $nickname }"不在线
err-nickname-required = 共享账户需要昵称
err-nickname-too-long = 昵称太长（最多{ $max_length }个字符）

# 离开消息错误
err-status-too-long = 离开消息太长（最多{ $max_length }个字节）
err-status-contains-newlines = 离开消息不能包含换行符
err-status-invalid-characters = 离开消息包含无效字符

# 共享账户错误
err-shared-cannot-be-admin = 共享账户不能成为管理员
err-shared-cannot-change-password = 无法更改共享账户的密码
err-shared-invalid-permissions = 共享账户不能拥有这些权限：{ $permissions }
err-shared-message-requires-nickname = 共享账户只能通过昵称接收消息
err-shared-kick-requires-nickname = 共享账户只能通过昵称踢出

# 访客账户错误
err-guest-disabled = 此服务器未启用访客访问
err-cannot-rename-guest = 访客账户不能被重命名
err-cannot-change-guest-password = 访客账户的密码不能被更改
err-cannot-delete-guest = 访客账户不能被删除

# 头像验证错误
err-avatar-invalid-format = 头像格式无效（必须是base64编码的数据URI）
err-avatar-too-large = 头像太大（最多{ $max_length }个字节）
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
err-permission-denied-chat-create = 权限被拒绝：您可以加入现有频道，但无法创建新频道

# 功能错误
err-chat-feature-not-enabled = 聊天功能未启用

# 频道错误
err-channel-name-empty = 频道名称不能为空
err-channel-name-too-short = 频道名称在#后必须至少有一个字符
err-channel-name-too-long = 频道名称过长（最多{ $max_length }个字符）
err-channel-name-invalid = 频道名称包含无效字符
err-channel-name-missing-prefix = 频道名称必须以#开头
err-channel-not-found = 未找到频道 '{ $channel }'
err-channel-already-member = 您已经是频道 '{ $channel }' 的成员
err-channel-limit-exceeded = 您不能加入超过 { $max } 个频道
err-channel-list-invalid = 无效频道 '{ $channel }': { $reason }

# 数据库错误
err-database = 数据库错误

# 消息格式错误
err-invalid-message-format = 无效的消息格式
err-message-not-supported = 不支持的消息类型

# 用户管理错误
err-cannot-delete-last-admin = 无法删除最后一个管理员
err-cannot-delete-self = 您不能删除自己
err-cannot-demote-last-admin = 无法降级最后一个管理员
err-cannot-edit-self = 您不能编辑自己
err-current-password-required = 更改密码需要提供当前密码
err-current-password-incorrect = 当前密码不正确
err-cannot-create-admin = 只有管理员才能创建管理员用户
err-cannot-kick-self = 您无法踢出自己
err-cannot-kick-admin = 无法踢出管理员用户
err-cannot-delete-admin = 只有管理员才能删除管理员用户
err-cannot-edit-admin = 只有管理员才能编辑管理员用户
err-cannot-message-self = 您无法给自己发消息
err-cannot-disable-last-admin = 无法禁用最后一个管理员

# 聊天主题错误
err-topic-contains-newlines = 主题不能包含换行符
err-topic-invalid-characters = 主题包含无效字符

# 版本验证错误
err-version-empty = 版本不能为空
err-version-too-long = 版本太长（最多{ $max_length }个字节）
err-version-invalid-semver = 版本必须是semver格式（MAJOR.MINOR.PATCH）

# 密码验证错误
err-password-empty = 密码不能为空
err-password-too-long = 密码太长（最多{ $max_length }个字节）
err-password-too-weak = 密码强度不足，最低要求为 { $required ->
    [0] 弱
    [1] 一般
    [2] 良好
    [3] 强
    [4] 非常强
   *[other] 未知
}

# 区域设置验证错误
err-locale-too-long = 区域设置太长（最多{ $max_length }个字节）
err-locale-invalid-characters = 区域设置包含无效字符

# 功能验证错误
err-features-too-many = 功能太多（最多{ $max_count }个）
err-features-empty-feature = 功能名称不能为空
err-features-feature-too-long = 功能名称太长（最多{ $max_length }个字节）
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
err-broadcast-too-long = 消息太长（最多{ $max_length }个字节）
err-chat-too-long = 消息太长（最多{ $max_length }个字节）
err-topic-too-long = 主题不能超过{ $max_length }个字节
err-version-major-mismatch = 不兼容的协议版本：服务器是版本{ $server_major }.x，客户端是版本{ $client_major }.x
err-version-client-too-new = 客户端版本{ $client_version }比服务器版本{ $server_version }更新。请更新服务器或使用旧版客户端。
err-version-minor-mismatch = 不兼容的协议版本。服务器: { $server_version }，客户端: { $client_version }。双方必须使用相同的次要版本。
err-kicked-by = 您已被{ $username }踢出
err-kicked-by-reason = 您已被{ $username }踢出: { $reason }
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
err-permissions-permission-too-long = 权限名称太长（最多{ $max_length }个字节）
err-permissions-contains-newlines = 权限名称不能包含换行符
err-permissions-invalid-characters = 权限名称包含无效字符

# 服务器更新错误
err-admin-required = 需要管理员权限
err-server-name-empty = 服务器名称不能为空
err-server-name-too-long = 服务器名称太长（最多{ $max_length }个字节）
err-server-name-contains-newlines = 服务器名称不能包含换行符
err-server-name-invalid-characters = 服务器名称包含无效字符
err-server-description-too-long = 服务器描述太长（最多{ $max_length }个字节）
err-server-description-contains-newlines = 服务器描述不能包含换行符
err-server-description-invalid-characters = 服务器描述包含无效字符

err-no-fields-to-update = 没有要更新的字段
err-invalid-password-strength = 无效的密码强度值

err-server-image-too-large = 服务器图片太大（最大512KB）
err-server-image-invalid-format = 服务器图片格式无效（必须是base64编码的数据URI）
err-server-image-unsupported-type = 不支持的服务器图片类型（仅支持PNG、WebP、JPEG或SVG）
err-public-address-too-long = 公开地址太长（最多{ $max_length }个字节）
err-public-address-contains-scheme = 公开地址不能包含URL协议
err-public-address-contains-brackets = 公开地址不能包含方括号
err-public-address-contains-path = 公开地址不能包含路径
err-public-address-contains-userinfo = 公开地址不能包含用户名
err-public-address-contains-whitespace = 公开地址不能包含空格
err-public-address-contains-port = 公开地址不能包含端口
err-public-address-contains-zone-id = 公开地址不能包含IPv6区域标识符
err-public-address-invalid-format = 公开地址不是有效的主机名或IP地址

# 新闻错误
err-news-not-found = 新闻 #{ $id } 未找到
err-news-body-too-long = 新闻内容太长（最多{ $max_length }个字节）
err-news-body-invalid-characters = 新闻内容包含无效字符
err-news-image-too-large = 新闻图片太大（最大512KB）
err-news-image-invalid-format = 新闻图片格式无效（必须是base64编码的数据URI）
err-news-image-unsupported-type = 不支持的新闻图片类型（仅支持PNG、WebP、JPEG或SVG）
err-news-empty-content = 新闻必须包含文字内容或图片
err-cannot-edit-admin-news = 只有管理员可以编辑管理员发布的新闻
err-cannot-delete-admin-news = 只有管理员可以删除管理员发布的新闻

# 文件区域错误
err-file-path-too-long = 文件路径过长（最多{ $max_length }个字符）
err-file-path-invalid = 文件路径包含无效字符
err-file-not-found = 文件或目录未找到
err-file-not-directory = 路径不是目录
err-dir-name-empty = 目录名称不能为空
err-dir-name-too-long = 目录名称过长（最多{ $max_length }个字符）
err-dir-name-invalid = 目录名称包含无效字符
err-dir-already-exists = 同名文件或目录已存在
err-dir-create-failed = 创建目录失败

err-dir-not-empty = 目录不为空
err-delete-failed = 无法删除文件或目录
err-rename-failed = 无法重命名文件或目录
err-rename-target-exists = 同名文件或目录已存在
err-move-failed = 无法移动文件或目录
err-copy-failed = 无法复制文件或目录
err-destination-exists = 目标位置已存在同名文件或目录
err-cannot-move-into-itself = 无法将目录移动到其自身内部
err-cannot-copy-into-itself = 无法将目录复制到其自身内部
err-destination-not-directory = 目标路径不是目录

# Transfer Errors
err-file-area-not-configured = 文件区域未配置
err-file-area-not-accessible = 文件区域无法访问
err-transfer-path-too-long = 路径太长
err-transfer-path-invalid = 路径包含无效字符
err-transfer-access-denied = 访问被拒绝
err-transfer-read-failed = 无法读取文件
err-transfer-path-not-found = 文件或目录未找到
err-transfer-file-failed = 传输 { $path } 失败: { $error }

# Upload Errors
err-upload-destination-not-allowed = 目标文件夹不允许上传
err-upload-write-failed = 文件写入失败
err-upload-hash-mismatch = 文件验证失败 - 哈希值不匹配
err-upload-path-invalid = 上传中的文件路径无效
err-upload-conflict = 另一个上传到此文件名的操作正在进行中或已中断。请尝试使用其他文件名。
err-upload-file-exists = 具有此名称的文件已存在。请选择其他文件名或请求管理员删除现有文件。
err-upload-empty = 上传必须包含至少一个文件
err-upload-protocol-error = 上传协议错误
err-upload-connection-lost = 上传过程中连接丢失

# Ban System Errors
err-ban-self = 您不能封禁自己
err-ban-admin-by-nickname = 无法封禁管理员
err-ban-admin-by-ip = 无法封禁此IP
err-ban-invalid-target = 无效的目标（使用昵称、IP地址或CIDR范围）
err-target-too-long = 目标过长（最多 { $max_length } 个字符）
err-ban-invalid-duration = 无效的时长格式（使用 10m、4h、7d 或 0 表示永久）
err-ban-not-found = 未找到 '{ $target }' 的封禁记录
err-reason-too-long = 封禁原因过长（最多 { $max_length } 个字符）
err-reason-invalid = 封禁原因包含无效字符
err-banned-permanent = 您已被此服务器封禁
err-banned-with-expiry = 您已被此服务器封禁（{ $remaining } 后解除）

# File Search Errors
err-search-query-empty = 搜索查询不能为空
err-search-query-too-short = 搜索查询过短（最少 { $min_length } 个字节）
err-search-query-too-long = 搜索查询过长（最多 { $max_length } 个字符）
err-search-query-invalid = 搜索查询包含无效字符
err-search-failed = 搜索失败
# Trust System Errors
err-trust-invalid-target = 无效的目标（请使用昵称、IP地址或CIDR范围）
err-trust-invalid-duration = 无效的持续时间格式（使用 10m、4h、7d 或 0 表示永久）
err-trust-not-found = 未找到 '{ $target }' 的信任条目

# Voice Errors
err-voice-listen-required = 您需要 voice_listen 权限才能加入语音
err-voice-already-joined = 您已在语音会话中
err-voice-not-joined = 您不在语音会话中
err-voice-not-channel-member = 您必须是 { $channel } 的成员才能加入语音
err-voice-target-not-online = { $nickname } 不在线
err-voice-invalid-target = 无效的语音目标

# 群组错误
err-group-name-empty = 群组名称不能为空
err-group-name-too-long = 群组名称太长（最多{ $max_length }个字符）
err-group-name-invalid = 群组名称包含无效字符
err-group-not-found = 群组未找到
err-group-already-exists = 已存在同名群组
err-group-shared-permission = 共享群组不能拥有此权限
err-group-not-empty-delete = 群组中仍有用户，无法删除
err-group-not-empty-modify = 群组中仍有用户，无法修改共享状态
err-group-no-fields = 没有要更新的字段
err-group-shared-mismatch = 账户类型与群组类型不匹配（共享账户需要共享群组）

# Tracker Errors
err-tracker-not-found = 找不到追踪器
err-tracker-no-pending-fingerprint = 追踪器没有待接受的指纹
err-tracker-name-invalid = 追踪器名称包含无效字符
err-tracker-name-empty = 追踪器名称不能为空
err-tracker-name-contains-newlines = 追踪器名称不能包含换行符
err-tracker-name-too-long = 追踪器名称过长 (最多 { $max_length } 字节)
err-tracker-address-invalid = 追踪器地址无效
err-tracker-address-empty = 追踪器地址不能为空
err-tracker-address-too-long = 追踪器地址太长（最多{ $max_length }个字节）
err-tracker-address-contains-scheme = 追踪器地址不能包含URL协议
err-tracker-address-contains-brackets = 追踪器地址不能包含方括号
err-tracker-address-contains-path = 追踪器地址不能包含路径
err-tracker-address-contains-userinfo = 追踪器地址不能包含用户名
err-tracker-address-contains-whitespace = 追踪器地址不能包含空格
err-tracker-address-contains-port = 追踪器地址不能包含端口
err-tracker-address-contains-zone-id = 追踪器地址不能包含IPv6区域标识符
err-tracker-address-invalid-format = 追踪器地址不是有效的主机名或IP地址
err-tracker-port-invalid = 追踪器端口无效
err-tracker-fingerprint-invalid = 追踪器指纹格式无效
err-tracker-password-too-long = 追踪器密码过长 (最多 { $max_length } 字节)
err-tracker-endpoint-duplicate = 此地址和端口已配置了另一个追踪器
err-tracker-name-duplicate = 已配置了另一个使用此名称的追踪器
err-tracker-too-many = 已达到追踪器上限 (最多 { $max } 个)

# Tracker registration status messages
err-tracker-connection-failed = 无法连接到追踪器
err-tracker-tls-failed = 与追踪器的 TLS 握手失败
err-tracker-handshake-failed = 追踪器握手失败
err-tracker-connection-lost = 与追踪器的连接丢失
err-tracker-db-failed = 更新追踪器状态时出现数据库错误
err-tracker-fingerprint-mismatch = 追踪器证书与已存储的指纹不匹配
err-tracker-fingerprint-intercepted = 追踪器自报的指纹与其 TLS 证书不匹配
err-tracker-unauthorized = 追踪器拒绝了注册
err-tracker-rate-limited = 被追踪器限速
err-tracker-capacity = 追踪器已达到容量上限
err-tracker-invalid = 追踪器以无效为由拒绝了注册
err-tracker-protocol-error = 追踪器发送了格式错误的错误响应
err-tracker-unknown = 追踪器报告了未知错误

# Flood Protection Errors
err-flood-warning = 消息受到限制（警告 { $violation }/{ $max_violations }）。您可以在{ $seconds }秒后再次发送消息。继续发送将导致断开连接。
err-flood-disconnect = 已断开连接：超出聊天速率限制。
