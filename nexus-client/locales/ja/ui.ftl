# Nexus BBS Client - Japanese Translations

# =============================================================================
# Buttons
# =============================================================================

button-cancel = キャンセル
button-send = 送信
button-delete = 削除
button-connect = 接続
button-save = 保存
button-create = 作成
button-edit = 編集
button-update = 更新
button-accept-new-certificate = 新しい証明書を承認

# =============================================================================
# Titles
# =============================================================================

title-nexus-bbs = Nexus BBS
title-connect-to-server = サーバーに接続
title-add-bookmark = ブックマークを追加
title-edit-server = サーバーを編集
title-broadcast-message = ブロードキャストメッセージ
title-user-create = ユーザー作成
title-user-edit = ユーザー編集
title-update-user = ユーザー更新
title-connected = 接続中
title-settings = 設定
title-bookmarks = ブックマーク
title-users = ユーザー
title-fingerprint-mismatch = 証明書のフィンガープリントが一致しません！

# =============================================================================
# Placeholders
# =============================================================================

placeholder-username = ユーザー名
placeholder-password = パスワード
placeholder-port = ポート
placeholder-server-address = サーバーアドレス
placeholder-server-name = サーバー名
placeholder-username-optional = ユーザー名（任意）
placeholder-password-optional = パスワード（任意）
placeholder-password-keep-current = パスワード（現在のまま維持する場合は空白）
placeholder-message = メッセージを入力...
placeholder-no-permission = 権限がありません
placeholder-broadcast-message = ブロードキャストメッセージを入力...

# =============================================================================
# Labels
# =============================================================================

label-auto-connect = 自動接続
label-add-bookmark = ブックマークに追加
label-admin = 管理者
label-enabled = 有効
label-permissions = 権限:
label-expected-fingerprint = 期待されるフィンガープリント:
label-received-fingerprint = 受信したフィンガープリント:
label-theme = テーマ
label-chat-font-size = チャットフォントサイズ
label-show-connection-notifications = 接続通知を表示
label-show-timestamps = タイムスタンプを表示
label-use-24-hour-time = 24時間形式を使用
label-show-seconds = 秒を表示

# =============================================================================
# Permission Display Names
# =============================================================================

permission-user_list = ユーザーリスト
permission-user_info = ユーザー情報
permission-chat_send = チャット送信
permission-chat_receive = チャット受信
permission-chat_topic = チャットトピック
permission-chat_topic_edit = チャットトピック編集
permission-user_broadcast = ユーザーブロードキャスト
permission-user_create = ユーザー作成
permission-user_delete = ユーザー削除
permission-user_edit = ユーザー編集
permission-user_kick = ユーザーキック
permission-user_message = ユーザーメッセージ

# =============================================================================
# Tooltips
# =============================================================================

tooltip-chat = チャット
tooltip-broadcast = ブロードキャスト
tooltip-user-create = ユーザー作成
tooltip-user-edit = ユーザー編集
tooltip-settings = 設定
tooltip-hide-bookmarks = ブックマークを隠す
tooltip-show-bookmarks = ブックマークを表示
tooltip-hide-user-list = ユーザーリストを隠す
tooltip-show-user-list = ユーザーリストを表示
tooltip-disconnect = 切断
tooltip-edit = 編集
tooltip-info = 情報
tooltip-message = メッセージ
tooltip-kick = キック
tooltip-close = 閉じる
tooltip-add-bookmark = ブックマークを追加

# =============================================================================
# Empty States
# =============================================================================

empty-select-server = リストからサーバーを選択してください
empty-no-connections = 接続なし
empty-no-bookmarks = ブックマークなし
empty-no-users = オンラインユーザーなし

# =============================================================================
# Chat Tab Labels
# =============================================================================

chat-tab-server = #サーバー

# =============================================================================
# System Message Usernames
# =============================================================================


# =============================================================================
# Chat Message Prefixes
# =============================================================================

chat-prefix-system = [システム]
chat-prefix-error = [エラー]
chat-prefix-info = [情報]
chat-prefix-broadcast = [BROADCAST]

# =============================================================================
# Success Messages
# =============================================================================

msg-user-kicked-success = ユーザーを正常にキックしました
msg-broadcast-sent = ブロードキャストを正常に送信しました
msg-user-created = ユーザーを正常に作成しました
msg-user-deleted = ユーザーを正常に削除しました
msg-user-updated = ユーザーを正常に更新しました
msg-permissions-updated = 権限が更新されました
msg-topic-updated = トピックが正常に更新されました

# =============================================================================
# Dynamic Messages (with parameters)
# =============================================================================

msg-topic-cleared = { $username } がトピックをクリアしました
msg-topic-set = { $username } がトピックを設定しました: { $topic }
msg-topic-display = トピック: { $topic }
msg-user-connected = { $username } が接続しました
msg-user-disconnected = { $username } が切断しました
msg-disconnected = 切断されました: { $error }
msg-connection-cancelled = 証明書の不一致のため接続がキャンセルされました

# =============================================================================
# Error Messages
# =============================================================================

err-connection-broken = 接続エラー
err-user-kick-failed = ユーザーのキックに失敗しました
err-no-shutdown-handle = 接続エラー: シャットダウンハンドルがありません
err-userlist-failed = ユーザーリストの更新に失敗しました
err-port-invalid = ポートは有効な数字である必要があります（1-65535）

# Network connection errors
err-no-peer-certificates = サーバー証明書が見つかりません
err-no-certificates-in-chain = チェーンに証明書がありません
err-unexpected-handshake-response = 予期しないハンドシェイク応答
err-no-session-id = セッションIDを受信できませんでした
err-login-failed = ログインに失敗しました
err-unexpected-login-response = 予期しないログイン応答
err-connection-closed = 接続が閉じられました
err-could-not-determine-config-dir = 設定ディレクトリを特定できませんでした
err-message-too-long = メッセージが長すぎます
err-send-failed = メッセージの送信に失敗しました
err-broadcast-too-long = ブロードキャストメッセージが長すぎます
err-broadcast-send-failed = ブロードキャストの送信に失敗しました
err-name-required = ブックマーク名は必須です
err-address-required = サーバーアドレスは必須です
err-port-required = ポートは必須です
err-username-required = ユーザー名は必須です
err-password-required = パスワードは必須です
err-message-required = メッセージは必須です

# =============================================================================
# Dynamic Error Messages (with parameters)
# =============================================================================

err-failed-save-config = 設定の保存に失敗しました: { $error }
err-failed-save-settings = 設定の保存に失敗しました: { $error }
err-invalid-port-bookmark = ブックマークのポートが無効です: { $name }
err-failed-send-broadcast = ブロードキャストの送信に失敗しました: { $error }
err-failed-send-message = メッセージの送信に失敗しました: { $error }
err-failed-create-user = ユーザーの作成に失敗しました: { $error }
err-failed-delete-user = ユーザーの削除に失敗しました: { $error }
err-failed-update-user = ユーザーの更新に失敗しました: { $error }
err-failed-update-topic = トピックの更新に失敗しました: { $error }
err-message-too-long-details = { $error }（{ $length }文字、最大{ $max }）

# Network connection errors (with parameters)
err-invalid-address = 無効なアドレス '{ $address }': { $error }
err-could-not-resolve = アドレス '{ $address }' を解決できませんでした
err-connection-timeout = { $seconds }秒後に接続がタイムアウトしました
err-connection-failed = 接続に失敗しました: { $error }
err-tls-handshake-failed = TLSハンドシェイクに失敗しました: { $error }
err-failed-send-handshake = ハンドシェイクの送信に失敗しました: { $error }
err-failed-read-handshake = ハンドシェイク応答の読み取りに失敗しました: { $error }
err-handshake-failed = ハンドシェイクに失敗しました: { $error }
err-failed-parse-handshake = ハンドシェイク応答の解析に失敗しました: { $error }
err-failed-send-login = ログインの送信に失敗しました: { $error }
err-failed-read-login = ログイン応答の読み取りに失敗しました: { $error }
err-failed-parse-login = ログイン応答の解析に失敗しました: { $error }
err-failed-create-server-name = サーバー名の作成に失敗しました: { $error }
err-failed-create-config-dir = 設定ディレクトリの作成に失敗しました: { $error }
err-failed-serialize-config = 設定のシリアライズに失敗しました: { $error }
err-failed-write-config = 設定ファイルの書き込みに失敗しました: { $error }
err-failed-read-config-metadata = 設定ファイルのメタデータの読み取りに失敗しました: { $error }
err-failed-set-config-permissions = 設定ファイルの権限の設定に失敗しました: { $error }

# =============================================================================
# Fingerprint Warning
# =============================================================================

fingerprint-warning = これはセキュリティ上の問題（MITM攻撃）またはサーバーの証明書が再生成されたことを示している可能性があります。サーバー管理者を信頼している場合のみ受け入れてください。

# =============================================================================
# User Info Display
# =============================================================================

user-info-header = [{ $username }]
user-info-is-admin = 管理者です
user-info-connected-ago = 接続: { $duration }前
user-info-connected-sessions = 接続: { $duration }前（{ $count }セッション）
user-info-features = 機能: { $features }
user-info-locale = ロケール: { $locale }
user-info-address = アドレス: { $address }
user-info-addresses = アドレス:
user-info-address-item = - { $address }
user-info-created = 作成日: { $created }
user-info-end = ユーザー情報終了
user-info-unknown = 不明
user-info-error = エラー: { $error }

# =============================================================================
# Time Duration
# =============================================================================

time-days = { $count }日
time-hours = { $count }時間
time-minutes = { $count }分
time-seconds = { $count }秒