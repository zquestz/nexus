# Nexus BBS Client - Traditional Chinese Translations

# =============================================================================
# Buttons
# =============================================================================

button-cancel = 取消
button-send = 傳送
button-delete = 刪除
button-connect = 連線
button-save = 儲存
button-create = 建立
button-edit = 編輯
button-update = 更新
button-accept-new-certificate = 接受新憑證
button-close = 關閉
button-choose-avatar = 選擇頭像
button-clear-avatar = 清除

# =============================================================================
# Titles
# =============================================================================

title-nexus-bbs = Nexus BBS
title-connect-to-server = 連線至伺服器
title-add-bookmark = 新增書籤
title-edit-server = 編輯伺服器
title-broadcast-message = 廣播
title-user-create = 建立使用者
title-user-edit = 編輯使用者
title-update-user = 更新使用者
title-user-management = 使用者管理
title-confirm-delete = 確認刪除
title-connected = 已連線
title-settings = 設定
title-bookmarks = 書籤
title-users = 使用者
title-edit-server-info = 編輯伺服器資訊
title-fingerprint-mismatch = 憑證指紋不符！
title-server-info = 伺服器資訊
title-user-info = 使用者資訊
title-about = 關於

# =============================================================================
# Placeholders
# =============================================================================

placeholder-username = 使用者名稱
placeholder-password = 密碼
placeholder-port = 連接埠
placeholder-server-address = 伺服器位址
placeholder-server-name = 伺服器名稱
placeholder-username-optional = 使用者名稱（選填）
placeholder-password-optional = 密碼（選填）
placeholder-password-keep-current = 密碼
placeholder-message = 輸入訊息...
placeholder-no-permission = 無權限
placeholder-broadcast-message = 輸入廣播訊息...
placeholder-server-description = 伺服器描述

# =============================================================================
# Labels
# =============================================================================

label-auto-connect = 自動連線
label-add-bookmark = 新增書籤
label-admin = 管理員
label-enabled = 啟用
label-permissions = 權限：
label-expected-fingerprint = 預期指紋：
label-received-fingerprint = 收到的指紋：
label-theme = 主題
label-chat-font-size = 字型大小：
label-show-connection-notifications = 顯示連線通知
label-show-timestamps = 顯示時間戳記
label-use-24-hour-time = 使用24小時制
label-show-seconds = 顯示秒數
label-server-name = 名稱：
label-server-description = 描述：
label-server-version = 版本：
label-chat-topic = 聊天主題：
label-chat-topic-set-by = 主題設定者：
label-max-connections-per-ip = 每IP最大連線數：
label-avatar = 頭像：
label-details = 技術詳情
label-chat-options = 聊天選項
label-appearance = 外觀
label-image = 圖片
label-general = 一般
label-limits = 限制

# =============================================================================
# Permission Display Names
# =============================================================================

permission-user_list = 使用者清單
permission-user_info = 使用者資訊
permission-chat_send = 傳送聊天
permission-chat_receive = 接收聊天
permission-chat_topic = 聊天主題
permission-chat_topic_edit = 編輯聊天主題
permission-user_broadcast = 使用者廣播
permission-user_create = 建立使用者
permission-user_delete = 刪除使用者
permission-user_edit = 編輯使用者
permission-user_kick = 踢除使用者
permission-user_message = 使用者訊息

# =============================================================================
# Tooltips
# =============================================================================

tooltip-chat = 聊天
tooltip-broadcast = 廣播
tooltip-manage-users = 使用者管理
tooltip-server-info = 伺服器資訊
tooltip-about = 關於
tooltip-settings = 設定
tooltip-hide-bookmarks = 隱藏書籤
tooltip-show-bookmarks = 顯示書籤
tooltip-hide-user-list = 隱藏使用者清單
tooltip-show-user-list = 顯示使用者清單
tooltip-disconnect = 中斷連線
tooltip-edit = 編輯
tooltip-info = 資訊
tooltip-message = 訊息
tooltip-kick = 踢出
tooltip-close = 關閉
tooltip-add-bookmark = 新增書籤
tooltip-create-user = 建立使用者
tooltip-delete = 刪除

# =============================================================================
# Empty States
# =============================================================================

empty-select-server = 從清單中選擇伺服器
empty-no-connections = 無連線
empty-no-bookmarks = 無書籤
empty-no-users = 無線上使用者
user-management-loading = 正在載入使用者...
user-management-no-users = 找不到使用者

# =============================================================================
# Chat Tab Labels
# =============================================================================

chat-tab-server = #伺服器

# =============================================================================
# System Message Usernames
# =============================================================================


# =============================================================================
# Chat Message Prefixes
# =============================================================================

chat-prefix-system = [系統]
chat-prefix-error = [錯誤]
chat-prefix-info = [資訊]
chat-prefix-broadcast = [BROADCAST]

# =============================================================================
# Success Messages
# =============================================================================

msg-user-kicked-success = 使用者已成功踢除
msg-broadcast-sent = 廣播已成功傳送
msg-user-created = 使用者已成功建立
msg-user-deleted = 使用者已成功刪除
msg-user-updated = 使用者更新成功
msg-permissions-updated = 您的權限已更新
msg-topic-updated = 主題更新成功

# =============================================================================
# Dynamic Messages (with parameters)
# =============================================================================

msg-topic-cleared = { $username } 清除了主題
msg-topic-set = { $username } 設定了主題：{ $topic }
msg-server-info-updated = 伺服器設定已更新
msg-topic-display = 主題：{ $topic }
confirm-delete-user = 您確定要刪除使用者 '{ $username }' 嗎？
msg-user-connected = { $username } 已連線
msg-user-disconnected = { $username } 已中斷連線
msg-disconnected = 已中斷連線：{ $error }
msg-connection-cancelled = 由於憑證不符，連線已取消

# =============================================================================
# Error Messages
# =============================================================================

err-connection-broken = 連線錯誤
err-failed-update-server-info = 更新伺服器資訊失敗：{ $error }
err-user-kick-failed = 踢除使用者失敗
err-no-shutdown-handle = 連線錯誤：無關閉控制代碼
err-userlist-failed = 重新整理使用者清單失敗
err-port-invalid = 連接埠必須是有效數字（1-65535）

# Network connection errors
err-no-peer-certificates = 未找到伺服器憑證
err-no-certificates-in-chain = 憑證鏈中沒有憑證
err-unexpected-handshake-response = 意外的握手回應
err-no-session-id = 未收到工作階段ID
err-login-failed = 登入失敗
err-unexpected-login-response = 意外的登入回應
err-connection-closed = 連線已關閉
err-could-not-determine-config-dir = 無法確定設定目錄
err-message-too-long = 訊息過長（{ $length }個字元，最多{ $max }個字元）
err-send-failed = 傳送訊息失敗
err-no-chat-permission = 您沒有傳送訊息的權限
err-broadcast-too-long = 廣播過長（{ $length }個字元，最多{ $max }個字元）
err-broadcast-send-failed = 傳送廣播失敗
err-name-required = 書籤名稱為必填
err-address-required = 伺服器位址為必填
err-port-required = 連接埠為必填
err-username-required = 使用者名稱為必填
err-password-required = 密碼為必填
err-message-required = 訊息為必填

# Validation errors
err-message-empty = 訊息不能為空
err-message-contains-newlines = 訊息不能包含換行符
err-message-invalid-characters = 訊息包含無效字元
err-username-empty = 使用者名稱不能為空
err-username-too-long = 使用者名稱過長（最多{ $max }個字元）
err-username-invalid = 使用者名稱包含無效字元
err-password-too-long = 密碼過長（最多{ $max }個字元）
err-topic-too-long = 主題過長（{ $length }個字元，最多{ $max }個字元）
err-avatar-unsupported-type = 不支援的檔案類型。請使用PNG、WebP、JPEG或SVG。
err-avatar-too-large = 頭像過大。最大大小為{ $max_kb }KB。
err-avatar-decode-failed = 無法解碼頭像。檔案可能已損壞。
err-server-name-empty = 伺服器名稱不能為空
err-server-name-too-long = 伺服器名稱過長（最多{ $max }個字元）
err-server-name-contains-newlines = 伺服器名稱不能包含換行符
err-server-name-invalid-characters = 伺服器名稱包含無效字元
err-server-description-too-long = 描述過長（最多{ $max }個字元）
err-server-description-contains-newlines = 描述不能包含換行符
err-server-description-invalid-characters = 描述包含無效字元
err-failed-send-update = 傳送更新失敗：{ $error }

# =============================================================================
# Dynamic Error Messages (with parameters)
# =============================================================================

err-failed-save-config = 儲存設定失敗：{ $error }
err-failed-save-settings = 儲存設定失敗：{ $error }
err-invalid-port-bookmark = 書籤中的連接埠無效：{ $name }
err-failed-send-broadcast = 傳送廣播失敗：{ $error }
err-failed-send-message = 傳送訊息失敗：{ $error }
err-failed-create-user = 建立使用者失敗：{ $error }
err-failed-delete-user = 刪除使用者失敗：{ $error }
err-failed-update-user = 更新使用者失敗：{ $error }
err-failed-update-topic = 更新主題失敗：{ $error }
err-message-too-long-details = { $error }（{ $length }字元，最大{ $max }）

# Network connection errors (with parameters)
err-invalid-address = 無效位址 '{ $address }'：{ $error }
err-could-not-resolve = 無法解析位址 '{ $address }'
err-connection-timeout = 連線在 { $seconds } 秒後逾時
err-connection-failed = 連線失敗：{ $error }
err-tls-handshake-failed = TLS握手失敗：{ $error }
err-failed-send-handshake = 傳送握手失敗：{ $error }
err-failed-read-handshake = 讀取握手回應失敗：{ $error }
err-handshake-failed = 握手失敗：{ $error }
err-failed-parse-handshake = 解析握手回應失敗：{ $error }
err-failed-send-login = 傳送登入失敗：{ $error }
err-failed-read-login = 讀取登入回應失敗：{ $error }
err-failed-parse-login = 解析登入回應失敗：{ $error }
err-failed-create-server-name = 建立伺服器名稱失敗：{ $error }
err-failed-create-config-dir = 建立設定目錄失敗：{ $error }
err-failed-serialize-config = 序列化設定失敗：{ $error }
err-failed-write-config = 寫入設定檔失敗：{ $error }
err-failed-read-config-metadata = 讀取設定檔中繼資料失敗：{ $error }
err-failed-set-config-permissions = 設定設定檔權限失敗：{ $error }

# =============================================================================
# Fingerprint Warning
# =============================================================================

fingerprint-warning = 這可能表示存在安全問題（中間人攻擊）或伺服器憑證已重新產生。僅在信任伺服器管理員時才接受。

# =============================================================================
# User Info Display
# =============================================================================

user-info-username = 使用者名稱：
user-info-role = 角色：
user-info-role-admin = 管理員
user-info-role-user = 使用者
user-info-connected = 已連線：
user-info-connected-value = { $duration }前
user-info-connected-value-sessions = { $duration }前（{ $count }個工作階段）
user-info-features = 功能：
user-info-features-value = { $features }
user-info-features-none = 無
user-info-locale = 語言：
user-info-address = 位址：
user-info-addresses = 位址：
user-info-created = 建立時間：
user-info-end = 使用者資訊結束
user-info-unknown = 未知
user-info-loading = 正在載入使用者資訊...

# =============================================================================
# Time Duration
# =============================================================================

time-days = { $count }天
time-hours = { $count }小時
time-minutes = { $count }分鐘
time-seconds = { $count } 秒

# =============================================================================
# Command System
# =============================================================================

cmd-unknown = 未知指令：/{ $command }
cmd-help-header = 可用指令：
cmd-help-desc = 顯示可用指令
cmd-help-escape-hint = 提示：使用 // 發送以 / 開頭的訊息
cmd-message-desc = 發送訊息給用戶
cmd-message-usage = 用法：/{ $command } <用戶名> <訊息>
cmd-userinfo-desc = 顯示用戶資訊
cmd-userinfo-usage = 用法：/{ $command } <用戶名>
cmd-kick-desc = 將用戶踢出伺服器
cmd-kick-usage = 用法：/{ $command } <用戶名>
cmd-topic-desc = 查看或管理聊天主題
cmd-topic-usage = 用法：/{ $command } [設定|清除] [主題]
cmd-topic-arg-set = 設定
cmd-topic-arg-clear = 清除
cmd-topic-set-usage = 用法：/{ $command } 設定 <主題>
cmd-topic-none = 未設定主題
cmd-broadcast-desc = 向所有用戶發送廣播
cmd-broadcast-usage = 用法：/{ $command } <訊息>
cmd-clear-desc = 清除當前分頁的聊天記錄
cmd-clear-usage = 用法：/{ $command }
cmd-focus-desc = 聚焦到伺服器聊天或用戶訊息視窗
cmd-focus-usage = 用法：/{ $command } [用戶名]
cmd-focus-not-found = 找不到用戶：{ $name }
cmd-list-desc = 顯示已連線/所有用戶
cmd-list-arg-all = 所有
cmd-list-usage = 用法：/{ $command } [所有]
cmd-list-empty = 沒有已連線的用戶
cmd-list-output = 線上用戶：{ $users }（{ $count }位用戶）
cmd-list-all-no-permission = 您需要 user_edit 或 user_delete 權限才能列出所有用戶
cmd-list-all-output = 用戶：{ $users }（{ $count }位用戶）
cmd-help-usage = 用法：/{ $command } [指令]
cmd-topic-permission-denied = 您沒有編輯主題的權限
cmd-window-desc = 管理聊天分頁
cmd-window-usage = 用法：/{ $command } [下一個|上一個|關閉 [用戶名]]
cmd-window-arg-next = 下一個
cmd-window-arg-prev = 上一個
cmd-window-arg-close = 關閉
cmd-window-list = 開啟的分頁：{ $tabs }（{ $count }個分頁）
cmd-window-close-server = 無法關閉伺服器分頁
cmd-window-not-found = 找不到分頁：{ $name }
cmd-serverinfo-desc = 顯示伺服器資訊
cmd-serverinfo-usage = 用法：/{ $command }
cmd-serverinfo-header = [伺服器]
cmd-serverinfo-end = 伺服器資訊結束

# =============================================================================
# About Panel
# =============================================================================

about-app-name = Nexus BBS
about-copyright = © 2025 Nexus BBS Project
button-choose-image = 選擇圖片
button-clear-image = 清除
label-server-image = 伺服器圖片:
err-server-image-too-large = 伺服器圖片太大（最大512KB）
err-server-image-invalid-format = 伺服器圖片格式無效（必須是base64編碼的資料URI）
err-server-image-unsupported-type = 不支援的伺服器圖片類型（僅支援PNG、WebP、JPEG或SVG）
err-server-image-decode-failed = 無法解碼圖片。檔案可能已損壞。
err-failed-read-image = 讀取圖片失敗: { $error }
