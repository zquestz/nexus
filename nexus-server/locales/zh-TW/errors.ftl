# 身份驗證和會話錯誤
err-not-logged-in = 未登入
err-authentication = 身份驗證錯誤
err-invalid-credentials = 使用者名稱或密碼無效
err-handshake-required = 需要握手
err-already-logged-in = 已經登入
err-handshake-already-completed = 握手已完成
err-account-deleted = 您的帳戶已被刪除
err-account-disabled-by-admin = 帳戶已被管理員停用

# 權限和存取錯誤
err-permission-denied = 權限被拒絕

# 功能錯誤
err-chat-feature-not-enabled = 聊天功能未啟用

# 資料庫錯誤
err-database = 資料庫錯誤

# 訊息格式錯誤
err-invalid-message-format = 無效的訊息格式

# 使用者管理錯誤
err-cannot-delete-last-admin = 無法刪除最後一個管理員
err-cannot-delete-self = 您不能刪除自己
err-cannot-demote-last-admin = 無法降級最後一個管理員
err-cannot-edit-self = 您不能編輯自己
err-cannot-create-admin = 只有管理員可以建立管理員使用者
err-cannot-kick-self = 您不能踢除自己
err-cannot-kick-admin = 無法踢除管理員用戶
err-cannot-message-self = 您不能給自己發送訊息
err-cannot-disable-last-admin = 無法停用最後一位管理員

# 聊天主題錯誤
err-topic-contains-newlines = 主題不能包含換行符號
err-topic-invalid-characters = 主題包含無效字元

# 版本驗證錯誤
err-version-empty = 版本不能為空
err-version-too-long = 版本太長（最多{ $max_length }個字元）
err-version-invalid-semver = 版本必須採用 semver 格式（MAJOR.MINOR.PATCH）

# 密碼驗證錯誤
err-password-empty = 密碼不能為空
err-password-too-long = 密碼太長（最多{ $max_length }個字元）

# 地區設定驗證錯誤
err-locale-too-long = 地區設定太長（最多{ $max_length }個字元）
err-locale-invalid-characters = 地區設定包含無效字元

# 功能驗證錯誤
err-features-too-many = 功能太多（最多{ $max_count }個）
err-features-empty-feature = 功能名稱不能為空
err-features-feature-too-long = 功能名稱太長（最多{ $max_length }個字元）
err-features-invalid-characters = 功能名稱包含無效字元

# 訊息驗證錯誤
err-message-empty = 訊息不能為空
err-message-contains-newlines = 訊息不能包含換行符號
err-message-invalid-characters = 訊息包含無效字元

# 使用者名稱驗證錯誤
err-username-empty = 使用者名稱不能為空
err-username-invalid = 使用者名稱包含無效字元（允許字母、數字和符號 - 不允許空格或控制字元）

# 未知權限錯誤
err-unknown-permission = 未知權限: '{ $permission }'

# 動態錯誤訊息（帶參數）
err-broadcast-too-long = 訊息太長（最多{ $max_length }個字元）
err-chat-too-long = 訊息太長（最多{ $max_length }個字元）
err-topic-too-long = 主題不能超過{ $max_length }個字元
err-version-major-mismatch = 不相容的協定版本：伺服器是版本{ $server_major }.x，客戶端是版本{ $client_major }.x
err-version-client-too-new = 客戶端版本{ $client_version }比伺服器版本{ $server_version }更新。請更新伺服器或使用較舊的客戶端。
err-kicked-by = 您已被{ $username }踢出
err-username-exists = 使用者名稱「{ $username }」已存在
err-user-not-found = 找不到使用者「{ $username }」
err-user-not-online = 使用者「{ $username }」不在線上
err-failed-to-create-user = 建立使用者「{ $username }」失敗
err-account-disabled = 帳戶「{ $username }」已被停用
err-update-failed = 更新使用者「{ $username }」失敗
err-username-too-long = 使用者名稱太長（最多{ $max_length }個字元）
# 權限驗證錯誤
err-permissions-too-many = 權限太多（最多{ $max_count }個）
err-permissions-empty-permission = 權限名稱不能為空
err-permissions-permission-too-long = 權限名稱太長（最多{ $max_length }個字元）
err-permissions-contains-newlines = 權限名稱不能包含換行符
err-permissions-invalid-characters = 權限名稱包含無效字元
