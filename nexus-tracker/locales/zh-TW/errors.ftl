# 追蹤器錯誤訊息 (v0.1.0)
#
# 所有鍵都使用 `err-tracker-*` 前綴，使其與 nexus-server 的本地化命名空間
# 隔離。這些鍵與 `nexus-tracker/src/errors.rs` 中的 `err_tracker_*`
# 輔助函式一對一對應。

# 身份驗證
err-tracker-unauthorized = 密碼錯誤或缺失

# 欄位驗證 (error_kind: invalid)
err-tracker-fingerprint-invalid = 無效的憑證指紋格式
err-tracker-name-too-long = 伺服器名稱過長 (最多 { $max_length } 字元)
err-tracker-name-empty = 伺服器名稱不能為空
err-tracker-name-contains-newlines = 伺服器名稱不能包含換行符號
err-tracker-name-invalid-characters = 伺服器名稱包含無效字元
err-tracker-description-too-long = 伺服器描述過長 (最多 { $max_length } 字元)
err-tracker-description-contains-newlines = 伺服器描述不能包含換行符號
err-tracker-description-invalid-characters = 伺服器描述包含無效字元
err-tracker-password-too-long = 密碼過長 (最多 { $max_length } 位元組)
err-tracker-address-too-long = 地址過長 (最多 { $max_length } 位元組)
err-tracker-address-invalid = 無效的地址
err-tracker-version-too-long = 伺服器版本字串過長 (最多 { $max_length } 位元組)
err-tracker-version-invalid = 無效的版本 (必須為有效的 semver)
err-tracker-locale-too-long = 地區代碼過長 (最多 { $max_length } 位元組)
err-tracker-locale-invalid = 地區設定包含無效字元
err-tracker-port-zero = 連接埠不能為零
err-tracker-websocket-port-zero = WebSocket 連接埠不能為零

# 速率 / 容量
err-tracker-rate-limited = 超出速率限制；請稍後再試
err-tracker-capacity = 追蹤器已達到容量上限；請稍後再試
err-tracker-per-ip-capacity = 來自您 IP 的項目在此追蹤器上過多

# 協定層
err-tracker-malformed-message = 訊息格式錯誤
err-tracker-handshake-required = 在任何其他訊息之前需要先握手
err-tracker-role-violation = 此連線的角色不允許此訊息
err-tracker-protocol-version-mismatch = 不相容的追蹤器協定版本 (伺服器: { $server }, 用戶端: { $client })
err-tracker-handshake-version-invalid = 無效的握手版本 (必須為有效的 semver)
err-tracker-unknown-message-type = 未知的訊息類型

# 框架 / 傳輸
err-tracker-frame-error = 框架格式違規
err-tracker-payload-too-large = 負載超過每種訊息類型的限制
