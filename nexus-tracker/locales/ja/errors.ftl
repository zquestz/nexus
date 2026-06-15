# トラッカーのエラーメッセージ
#
# すべてのキーは `err-tracker-*` プレフィックスを使用し、nexus-server の
# ローカライズ名前空間から分離されています。キーは
# `nexus-tracker/src/errors.rs` の `err_tracker_*` ヘルパーと
# 1対1で対応しています。

# 認証
err-tracker-unauthorized = パスワードが間違っているか、指定されていません

# フィールド検証 (error_kind: invalid)
err-tracker-fingerprint-invalid = 証明書のフィンガープリント形式が無効です
err-tracker-name-too-long = サーバー名が長すぎます (最大 { $max_length } 文字)
err-tracker-name-empty = サーバー名を空にすることはできません
err-tracker-name-contains-newlines = サーバー名に改行を含めることはできません
err-tracker-name-invalid-characters = サーバー名に無効な文字が含まれています
err-tracker-description-too-long = サーバーの説明が長すぎます (最大 { $max_length } 文字)
err-tracker-description-contains-newlines = サーバーの説明に改行を含めることはできません
err-tracker-description-invalid-characters = サーバーの説明に無効な文字が含まれています
err-tracker-password-too-long = パスワードが長すぎます (最大 { $max_length } バイト)
err-tracker-address-too-long = アドレスが長すぎます (最大 { $max_length } バイト)
err-tracker-address-invalid = 無効なアドレスです
err-tracker-version-too-long = サーバーのバージョン文字列が長すぎます (最大 { $max_length } バイト)
err-tracker-version-invalid = 無効なバージョンです (有効なsemverである必要があります)
err-tracker-locale-too-long = ロケールコードが長すぎます (最大 { $max_length } バイト)
err-tracker-locale-invalid = ロケールに無効な文字が含まれています
err-tracker-port-zero = ポートを0にすることはできません
err-tracker-websocket-port-zero = WebSocketポートを0にすることはできません

# レート / 容量
err-tracker-rate-limited = レート制限を超えました。後でもう一度お試しください
err-tracker-refresh-unknown = 登録はもう有効ではありません。再登録するには再接続してください
err-tracker-capacity = トラッカーが容量に達しました。後でもう一度お試しください
err-tracker-per-ip-capacity = このトラッカーへのあなたのIPからのエントリが多すぎます

# プロトコルレベル
err-tracker-malformed-message = 不正な形式のメッセージです
err-tracker-handshake-required = 他のメッセージの前にハンドシェイクが必要です
err-tracker-role-violation = この接続のロールではメッセージは許可されていません
err-tracker-protocol-version-mismatch = 互換性のないトラッカープロトコルバージョン (サーバー: { $server }, クライアント: { $client })
err-tracker-handshake-version-invalid = 無効なハンドシェイクバージョンです (有効なsemverである必要があります)
err-tracker-unknown-message-type = 不明なメッセージタイプ
err-tracker-unexpected-message-type = 予期しないメッセージタイプです

# フレーム / トランスポート
err-tracker-frame-error = フレーム形式違反
err-tracker-payload-too-large = ペイロードがメッセージタイプごとの制限を超えています
