# 身份驗證和會話錯誤
err-not-logged-in = 未登入

# 暱稱驗證錯誤
err-nickname-empty = 暱稱不能為空
err-nickname-in-use = 暱稱已被使用
err-nickname-invalid = 暱稱包含無效字元（允許字母、數字和符號 - 不允許空格或控制字元）
err-nickname-is-username = 暱稱不能是已存在的使用者名稱
err-nickname-not-found = 找不到使用者「{ $nickname }」
err-nickname-not-online = 使用者「{ $nickname }」不在線上
err-nickname-required = 共享帳戶需要暱稱
err-nickname-too-long = 暱稱太長（最多{ $max_length }個字元）

# 共享帳戶錯誤
err-shared-cannot-be-admin = 共享帳戶不能成為管理員
err-shared-cannot-change-password = 無法更改共享帳戶的密碼
err-shared-invalid-permissions = 共享帳戶不能擁有這些權限：{ $permissions }
err-shared-message-requires-nickname = 共享帳戶只能通過暱稱接收訊息
err-shared-kick-requires-nickname = 共享帳戶只能通過暱稱踢出

# 訪客帳戶錯誤
err-guest-disabled = 此伺服器未啟用訪客存取
err-cannot-rename-guest = 訪客帳戶無法重新命名
err-cannot-change-guest-password = 訪客帳戶的密碼無法變更
err-cannot-delete-guest = 訪客帳戶無法刪除

# 頭像驗證錯誤
err-avatar-invalid-format = 頭像格式無效（必須是base64編碼的資料URI）
err-avatar-too-large = 頭像太大（最多{ $max_length }個字元）
err-avatar-unsupported-type = 不支援的頭像類型（僅支援PNG、WebP或SVG）
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
err-current-password-required = 變更密碼需要提供目前密碼
err-current-password-incorrect = 目前密碼不正確
err-cannot-create-admin = 只有管理員可以建立管理員使用者
err-cannot-kick-self = 您不能踢除自己
err-cannot-kick-admin = 無法踢除管理員用戶
err-cannot-delete-admin = 只有管理員才能刪除管理員用戶
err-cannot-edit-admin = 只有管理員才能編輯管理員用戶
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

# 伺服器更新錯誤
err-admin-required = 需要管理員權限
err-server-name-empty = 伺服器名稱不能為空
err-server-name-too-long = 伺服器名稱太長（最多{ $max_length }個字元）
err-server-name-contains-newlines = 伺服器名稱不能包含換行符號
err-server-name-invalid-characters = 伺服器名稱包含無效字元
err-server-description-too-long = 伺服器描述太長（最多{ $max_length }個字元）
err-server-description-contains-newlines = 伺服器描述不能包含換行符號
err-server-description-invalid-characters = 伺服器描述包含無效字元
err-max-connections-per-ip-invalid = 每個IP的最大連線數必須大於0
err-no-fields-to-update = 沒有要更新的欄位

err-server-image-too-large = 伺服器圖片太大（最大512KB）
err-server-image-invalid-format = 伺服器圖片格式無效（必須是base64編碼的資料URI）
err-server-image-unsupported-type = 不支援的伺服器圖片類型（僅支援PNG、WebP、JPEG或SVG）

# 新聞錯誤
err-news-not-found = 找不到新聞 #{ $id }
err-news-body-too-long = 新聞內容太長（最多{ $max_length }個字元）
err-news-body-invalid-characters = 新聞內容包含無效字元
err-news-image-too-large = 新聞圖片太大（最大512KB）
err-news-image-invalid-format = 新聞圖片格式無效（必須是base64編碼的資料URI）
err-news-image-unsupported-type = 不支援的新聞圖片類型（僅支援PNG、WebP、JPEG或SVG）
err-news-empty-content = 新聞必須包含文字內容或圖片
err-cannot-edit-admin-news = 只有管理員可以編輯管理員發布的新聞
err-cannot-delete-admin-news = 只有管理員可以刪除管理員發布的新聞

# 檔案區域錯誤
err-file-path-too-long = 檔案路徑過長（最多{ $max_length }個字元）
err-file-path-invalid = 檔案路徑包含無效字元
err-file-not-found = 檔案或目錄未找到
err-file-not-directory = 路徑不是目錄
err-dir-name-empty = 目錄名稱不能為空
err-dir-name-too-long = 目錄名稱過長（最多{ $max_length }個字元）
err-dir-name-invalid = 目錄名稱包含無效字元
err-dir-already-exists = 已存在同名的檔案或目錄
err-dir-create-failed = 建立目錄失敗

err-dir-not-empty = 目錄不為空
err-delete-failed = 無法刪除檔案或目錄
