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
err-cannot-kick-self = 您不能踢出自己
err-cannot-kick-admin = 無法踢出管理員使用者
err-cannot-disable-last-admin = 無法停用最後一個管理員

# 聊天主題錯誤
err-topic-contains-newlines = 主題不能包含換行符號

# 訊息驗證錯誤
err-message-empty = 訊息不能為空

# 使用者名稱驗證錯誤
err-username-empty = 使用者名稱不能為空
err-username-invalid = 使用者名稱包含無效字元（允許字母、數字和符號 - 不允許空格或控制字元）

# 動態錯誤訊息（帶參數）
err-broadcast-too-long = 訊息太長（最多{ $max_length }個字元）
err-chat-too-long = 訊息太長（最多{ $max_length }個字元）
err-topic-too-long = 主題不能超過{ $max_length }個字元
err-version-mismatch = 版本不符：伺服器使用{ $server_version }，客戶端使用{ $client_version }
err-kicked-by = 您已被{ $username }踢出
err-username-exists = 使用者名稱「{ $username }」已存在
err-user-not-found = 找不到使用者「{ $username }」
err-user-not-online = 使用者「{ $username }」不在線上
err-failed-to-create-user = 建立使用者「{ $username }」失敗
err-account-disabled = 帳戶「{ $username }」已被停用
err-update-failed = 更新使用者「{ $username }」失敗
err-username-too-long = 使用者名稱太長（最多{ $max_length }個字元）