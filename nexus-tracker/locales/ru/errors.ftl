# Сообщения об ошибках трекера (v0.1.0)
#
# Все ключи используют префикс `err-tracker-*`, чтобы изолировать их от
# пространства имён локализации nexus-server. Ключи соответствуют 1:1
# помощникам `err_tracker_*` в `nexus-tracker/src/errors.rs`.

# Аутентификация
err-tracker-unauthorized = Неверный или отсутствующий пароль

# Валидация полей (error_kind: invalid)
err-tracker-fingerprint-invalid = Неверный формат отпечатка сертификата
err-tracker-name-too-long = Имя сервера слишком длинное (макс. { $max_length } байт)
err-tracker-description-too-long = Описание сервера слишком длинное (макс. { $max_length } байт)
err-tracker-password-too-long = Пароль слишком длинный (макс. { $max_length } байт)
err-tracker-address-too-long = Адрес слишком длинный (макс. { $max_length } байт)
err-tracker-address-invalid = Неверный адрес
err-tracker-version-too-long = Строка версии сервера слишком длинная (макс. { $max_length } байт)
err-tracker-locale-too-long = Код локали слишком длинный (макс. { $max_length } байт)

# Скорость / ёмкость
err-tracker-rate-limited = Превышен лимит скорости; повторите попытку позже
err-tracker-capacity = Трекер достиг максимальной ёмкости; повторите попытку позже
err-tracker-per-ip-capacity = Слишком много записей с вашего IP на этом трекере

# Уровень протокола
err-tracker-malformed-message = Неправильно сформированное сообщение
err-tracker-handshake-required = Перед любым другим сообщением требуется handshake
err-tracker-role-violation = Сообщение не разрешено для роли этого соединения
err-tracker-protocol-version-mismatch = Несовместимая версия протокола трекера (сервер: { $server }, клиент: { $client })
err-tracker-handshake-version-invalid = Неверная версия handshake (должна быть корректной semver)
err-tracker-unknown-message-type = Неизвестный тип сообщения

# Кадр / транспорт
err-tracker-frame-error = Нарушение формата кадра
err-tracker-payload-too-large = Полезная нагрузка превышает ограничение по типу сообщения
